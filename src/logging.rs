use std::path::PathBuf;
use std::sync::OnceLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT_LOGGING: OnceLock<()> = OnceLock::new();

/// Abre el archivo de registro, o `None` si no se puede.
///
/// Devuelve `None` en lugar de paniquear. Esto era un `expect`, y un plugin que
/// paniquea al inicializarse **se lleva puesta la aplicación entera**: alcanzaba
/// con un `$HOME` que no se pudiera escribir —montado de sólo lectura, una cuota
/// llena, un `HOME` que apunta a algo que no existe— para que el escritorio no
/// arrancara y el motivo fuera un panic dentro de un plugin de bluetooth.
fn abrir_archivo_de_registro(ruta: &std::path::Path) -> Option<std::fs::File> {
    if let Some(padre) = ruta.parent() {
        let _ = std::fs::create_dir_all(padre);
    }

    match std::fs::OpenOptions::new().create(true).append(true).open(ruta) {
        Ok(archivo) => Some(archivo),
        Err(error) => {
            eprintln!(
                "[bluetooth] no se pudo abrir {} ({error}); el registro va sólo a la salida estándar",
                ruta.display()
            );
            None
        }
    }
}

pub fn init_logging() {
    INIT_LOGGING.get_or_init(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("tauri_plugin_bluetooth_manager=info"));

        let capa_de_archivo = abrir_archivo_de_registro(&get_log_path()).map(|archivo| {
            fmt::layer()
                .with_writer(archivo)
                .with_target(true)
                .with_thread_ids(false)
                .with_ansi(false)
                .pretty()
                .boxed()
        });

        let capa_de_salida = fmt::layer()
            .with_writer(std::io::stdout)
            .with_target(true)
            .with_ansi(true)
            .boxed();

        let suscriptor = tracing_subscriber::registry()
            .with(env_filter)
            .with(capa_de_archivo)
            .with(capa_de_salida);

        // `set_global_default` y no `try_init()`, que es lo que había.
        //
        // `try_init()` hace dos cosas y no una: instala el suscriptor de tracing
        // y **además** instala `LogTracer`, el puente que desvía el crate `log`
        // hacia tracing. Lo segundo pisa el registro del anfitrión, y si el
        // anfitrión ya tiene uno instalado —vasak-desktop lo tiene— falla.
        //
        // Con `init()` eso paniqueaba y la aplicación no arrancaba; con
        // `try_init()` no paniquea, pero el orden dentro de la biblioteca es
        // instalar el suscriptor primero y el puente después, así que el error
        // llegaba **con el suscriptor ya instalado** y el mensaje decía que el
        // registro no se había podido instalar cuando sí estaba funcionando.
        //
        // Un plugin no tiene por qué quedarse con el puente de `log`: eso es del
        // anfitrión. Instalando sólo el suscriptor, este plugin deja de pelear
        // por ese lugar y las dos cosas conviven sin importar en qué orden se
        // inicialicen.
        if let Err(error) = tracing::subscriber::set_global_default(suscriptor) {
            eprintln!(
                "[bluetooth] el registro propio no se pudo instalar ({error}); \
                 los mensajes van al del anfitrión"
            );
        }
    });
}

fn get_log_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".logs/vasak/bluetooth.log")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lo que antes era un `expect` y tiraba la aplicación entera.
    #[test]
    fn una_ruta_que_no_se_puede_abrir_no_paniquea() {
        // /proc no acepta archivos nuevos, así que sirve de ruta imposible sin
        // depender de permisos ni de montar nada.
        let imposible = std::path::Path::new("/proc/no-se-puede/bluetooth.log");
        assert!(abrir_archivo_de_registro(imposible).is_none());
    }

    #[test]
    fn una_ruta_normal_se_abre_y_se_crea_el_directorio() {
        let base = std::env::temp_dir().join(format!("vsk-bt-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ruta = base.join("anidado").join("bluetooth.log");

        assert!(abrir_archivo_de_registro(&ruta).is_some());
        assert!(ruta.exists(), "el directorio intermedio se crea solo");

        // Y dos veces seguidas: se abre en modo agregar, no truncando, porque el
        // registro de la sesión anterior tiene que seguir ahí.
        assert!(abrir_archivo_de_registro(&ruta).is_some());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn la_ruta_del_registro_cuelga_del_home() {
        let ruta = get_log_path();
        assert!(ruta.ends_with(".logs/vasak/bluetooth.log"));
        assert!(ruta.is_absolute() || ruta.starts_with("."));
    }
}
