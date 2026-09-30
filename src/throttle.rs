//! El freno de los cambios de un dispositivo: a lo sumo un aviso por ventana,
//! y el último cambio nunca se pierde.
//!
//! BlueZ manda `PropertiesChanged` seguidos —el RSSI cambia varias veces por
//! segundo mientras se busca— y cada aviso relee el dispositivo entero, así que
//! sin freno la pantalla recibe una ráfaga. El freno que había **descartaba**
//! lo que llegaba dentro de la ventana: si la batería cambiaba dos veces en
//! menos de medio segundo, el segundo valor se perdía y la pantalla mostraba el
//! primero hasta el próximo cambio (Vasak-OS/tauri-plugin-bluetooth-manager#10).
//!
//! Éste **pospone**. Lo que llega dentro de la ventana agenda un aviso para
//! cuando termina, y como ese aviso relee el dispositivo, lleva el último valor
//! de todo lo que cambió mientras tanto. Vale para cualquier propiedad, no sólo
//! para la batería.
//!
//! Está aparte del bucle de señales y sin reloj propio a propósito: la decisión
//! se prueba con instantes inventados, sin bus y sin esperar.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Qué hacer con un cambio.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Avisar ya.
    EmitNow,
    /// Avisar dentro de este tiempo, cuando termina la ventana.
    EmitIn(Duration),
    /// Ya hay un aviso agendado para este dispositivo, y va a llevar este
    /// cambio porque relee todo: no hace falta otro.
    AlreadyScheduled,
}

/// La ventana de cada dispositivo, por su ruta en el bus.
pub struct Throttle {
    window: Duration,
    last: HashMap<String, Instant>,
    scheduled: HashSet<String>,
}

impl Throttle {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last: HashMap::new(),
            scheduled: HashSet::new(),
        }
    }

    /// Un cambio de `path` a las `now`.
    ///
    /// Los críticos —conectado, emparejado, el nombre— se avisan siempre en el
    /// acto: son los que la persona está mirando cambiar.
    pub fn on_change(&mut self, path: &str, now: Instant, critical: bool) -> Decision {
        if critical {
            return Decision::EmitNow;
        }
        if self.scheduled.contains(path) {
            return Decision::AlreadyScheduled;
        }
        match self.last.get(path) {
            Some(last) if now.saturating_duration_since(*last) < self.window => {
                self.scheduled.insert(path.to_owned());
                Decision::EmitIn(self.window - now.saturating_duration_since(*last))
            }
            _ => {
                self.last.insert(path.to_owned(), now);
                Decision::EmitNow
            }
        }
    }

    /// Llegó la hora del aviso agendado para `path`: la ventana vuelve a
    /// empezar desde `now`.
    ///
    /// Devuelve si el aviso sigue haciendo falta. No hace falta si el
    /// dispositivo se fue mientras tanto ([`Throttle::forget`]): releerlo daría
    /// un error por algo que ya no está.
    pub fn fired(&mut self, path: &str, now: Instant) -> bool {
        if !self.scheduled.remove(path) {
            return false;
        }
        self.last.insert(path.to_owned(), now);
        true
    }

    /// El dispositivo se fue: se olvida su ventana.
    pub fn forget(&mut self, path: &str) {
        self.scheduled.remove(path);
        self.last.remove(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_millis(500);
    const DEVICE: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn el_primer_cambio_se_avisa_en_el_acto() {
        let mut throttle = Throttle::new(WINDOW);
        assert_eq!(
            throttle.on_change(DEVICE, Instant::now(), false),
            Decision::EmitNow
        );
    }

    #[test]
    fn un_cambio_dentro_de_la_ventana_se_pospone_y_no_se_pierde() {
        // El caso del issue: la batería cambia dos veces en menos de medio
        // segundo. Antes el segundo se descartaba.
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        assert_eq!(throttle.on_change(DEVICE, start, false), Decision::EmitNow);
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(200), false),
            Decision::EmitIn(ms(300)),
            "tiene que avisar cuando termina la ventana, no descartarlo"
        );
    }

    #[test]
    fn una_rafaga_dentro_de_la_ventana_da_un_solo_aviso_agendado() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(100), false),
            Decision::EmitIn(ms(400))
        );
        for later in [150, 300, 450] {
            assert_eq!(
                throttle.on_change(DEVICE, start + ms(later), false),
                Decision::AlreadyScheduled,
                "a los {later} ms ya hay un aviso que va a releer el dispositivo"
            );
        }
    }

    #[test]
    fn despues_del_aviso_agendado_la_ventana_empieza_de_nuevo() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        throttle.on_change(DEVICE, start + ms(100), false);
        assert!(throttle.fired(DEVICE, start + ms(500)));

        assert_eq!(
            throttle.on_change(DEVICE, start + ms(600), false),
            Decision::EmitIn(ms(400)),
            "la ventana cuenta desde el aviso agendado, no desde el primero"
        );
        assert!(throttle.fired(DEVICE, start + ms(1000)));
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(1600), false),
            Decision::EmitNow
        );
    }

    #[test]
    fn pasada_la_ventana_se_avisa_en_el_acto() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(500), false),
            Decision::EmitNow
        );
    }

    #[test]
    fn los_criticos_no_esperan_ni_cuentan_para_la_ventana() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(10), true),
            Decision::EmitNow
        );
        // Y un no crítico después sigue midiendo desde el último no crítico.
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(20), false),
            Decision::EmitIn(ms(480))
        );
    }

    #[test]
    fn cada_dispositivo_tiene_su_ventana() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        assert_eq!(
            throttle.on_change(
                "/org/bluez/hci0/dev_11_22_33_44_55_66",
                start + ms(10),
                false
            ),
            Decision::EmitNow
        );
    }

    #[test]
    fn olvidar_un_dispositivo_saca_su_aviso_pendiente() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        throttle.on_change(DEVICE, start + ms(100), false);
        throttle.forget(DEVICE);
        assert!(
            !throttle.fired(DEVICE, start + ms(500)),
            "el aviso de un dispositivo que se fue no sale"
        );
        assert_eq!(
            throttle.on_change(DEVICE, start + ms(600), false),
            Decision::EmitNow
        );
    }

    #[test]
    fn el_aviso_agendado_sale_si_el_dispositivo_sigue() {
        let mut throttle = Throttle::new(WINDOW);
        let start = Instant::now();
        throttle.on_change(DEVICE, start, false);
        throttle.on_change(DEVICE, start + ms(100), false);
        assert!(throttle.fired(DEVICE, start + ms(500)));
        assert!(
            !throttle.fired(DEVICE, start + ms(501)),
            "sale una sola vez"
        );
    }
}
