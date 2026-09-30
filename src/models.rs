use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub value: Option<String>,
}

/// Todo lo que sale hacia el frontend va en camelCase, igual que en el resto de
/// los complementos del taller y que en `guest-js/index.ts`. Las pruebas de
/// abajo comparan las claves serializadas con las que declara ese archivo.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    pub path: String,
    pub address: String, // MAC address
    pub name: String,
    pub alias: String,
    pub class: u32, // Class of device
    pub powered: bool,
    pub discoverable: bool,
    pub discoverable_timeout: u32,
    pub pairable: bool,
    pub pairable_timeout: u32,
    pub discovering: bool,
    pub uuids: Vec<String>,
    pub modalias: Option<String>, // Ejemplo: "usb:v1D6Bp0246d0540"
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub path: String,
    pub address: String, // MAC address
    pub name: Option<String>,
    pub alias: Option<String>,
    pub class: Option<u32>,
    pub appearance: Option<u16>,
    pub icon: Option<String>,
    pub paired: bool,
    pub trusted: bool,
    pub blocked: bool,
    pub legacy_pairing: bool,
    pub rssi: Option<i16>,
    pub tx_power: Option<i16>, // TxPower
    pub connected: bool,
    pub uuids: Vec<String>,
    pub adapter: String, // ObjectPath del adaptador al que pertenece
    pub services_resolved: bool,
    /// Porcentaje de batería (0–100) de `org.bluez.Battery1`.
    ///
    /// Sólo lo publican los dispositivos que informan batería —la mayoría de
    /// los auriculares y los periféricos nuevos—. Cuando no la publican el
    /// campo no va en el JSON: ausente es «no lo sé», y no es lo mismo que
    /// cero, que es «descargado».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<u8>,
    // Podríamos añadir `manufacturer_data: Option<HashMap<u16, Vec<u8>>>`
    // y `service_data: Option<HashMap<String, Vec<u8>>>` si es necesario.
}

/// La carga del evento `bluetooth-change`.
///
/// Sale con `changeType` y además con `change_type`, el nombre de hasta la
/// 2.1: `vasak-desktop` y `vasak-settings` leen `change_type`, y un evento
/// que perdiera esa clave les dejaría el panel sin enterarse de nada, sin un
/// solo error. La clave vieja se va en la próxima mayor.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "BluetoothChangeWire")]
pub struct BluetoothChange {
    pub change_type: String,
    pub data: serde_json::Value,
}

/// Lo que llega por el cable: una clave, la otra o las dos, que es lo que
/// manda `Serialize`. Con `#[serde(alias)]` las dos serían el mismo campo y el
/// evento que emite este mismo plugin no se podría volver a leer
/// («duplicate field»).
#[derive(Deserialize)]
struct BluetoothChangeWire {
    #[serde(rename = "changeType")]
    camel: Option<String>,
    #[serde(rename = "change_type")]
    snake: Option<String>,
    data: serde_json::Value,
}

impl TryFrom<BluetoothChangeWire> for BluetoothChange {
    type Error = &'static str;

    fn try_from(wire: BluetoothChangeWire) -> Result<Self, Self::Error> {
        let change_type = wire
            .camel
            .or(wire.snake)
            .ok_or("falta `changeType` (o `change_type`)")?;
        Ok(Self {
            change_type,
            data: wire.data,
        })
    }
}

impl Serialize for BluetoothChange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("BluetoothChange", 3)?;
        state.serialize_field("changeType", &self.change_type)?;
        state.serialize_field("change_type", &self.change_type)?;
        state.serialize_field("data", &self.data)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Los tipos que ve el frontend. Se lee el archivo de verdad y no una
    /// copia de la lista: una copia se separa igual que se separaron los tipos.
    ///
    /// `guest-js/` no viaja en el crate publicado (`exclude` en `Cargo.toml`),
    /// pero esto sólo se compila para las pruebas del repositorio.
    const GUEST_JS: &str = include_str!("../guest-js/index.ts");

    /// Las claves que declara `export interface <name>` en `guest-js/index.ts`.
    /// El segundo conjunto son las opcionales (`clave?:`).
    fn guest_js_keys(name: &str) -> (BTreeSet<String>, BTreeSet<String>) {
        let header = format!("export interface {name} {{");
        let start = GUEST_JS
            .find(&header)
            .unwrap_or_else(|| panic!("guest-js/index.ts no declara `{name}`"))
            + header.len();
        let body = &GUEST_JS[start..];
        let body = &body[..body.find("\n}").expect("la interfaz no cierra")];

        let mut all = BTreeSet::new();
        let mut optional = BTreeSet::new();
        let mut in_doc_comment = false;
        for line in body.lines().map(str::trim) {
            if in_doc_comment {
                in_doc_comment = !line.contains("*/");
                continue;
            }
            if line.starts_with("/*") {
                in_doc_comment = !line.contains("*/");
                continue;
            }
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let Some(colon) = line.find(':') else {
                continue;
            };
            let key = &line[..colon];
            match key.strip_suffix('?') {
                Some(key) => {
                    optional.insert(key.to_string());
                    all.insert(key.to_string());
                }
                None => {
                    all.insert(key.to_string());
                }
            }
        }
        assert!(
            !all.is_empty(),
            "`{name}` salió sin claves: el lector no la entendió"
        );
        (all, optional)
    }

    fn serialized_keys<T: Serialize>(value: &T) -> BTreeSet<String> {
        match serde_json::to_value(value).expect("serializa") {
            serde_json::Value::Object(map) => map.keys().cloned().collect(),
            other => panic!("se esperaba un objeto y salió {other}"),
        }
    }

    /// Compara con todos los campos llenos: así sale cada clave posible.
    fn assert_matches_guest_js<T: Serialize>(name: &str, value: &T) {
        let (declared, _) = guest_js_keys(name);
        assert_eq!(
            serialized_keys(value),
            declared,
            "las claves que manda Rust para `{name}` no son las que declara guest-js/index.ts"
        );
    }

    fn full_adapter() -> AdapterInfo {
        AdapterInfo {
            path: "/org/bluez/hci0".into(),
            address: "00:11:22:33:44:55".into(),
            name: "hci0".into(),
            alias: "Equipo".into(),
            class: 0x6c010c,
            powered: true,
            discoverable: false,
            discoverable_timeout: 180,
            pairable: true,
            pairable_timeout: 0,
            discovering: false,
            uuids: vec!["0000110a".into()],
            modalias: Some("usb:v1D6Bp0246d0540".into()),
        }
    }

    fn full_device() -> DeviceInfo {
        DeviceInfo {
            path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: Some("Auriculares".into()),
            alias: Some("Auriculares".into()),
            class: Some(0x240404),
            appearance: Some(0x0941),
            icon: Some("audio-headset".into()),
            paired: true,
            trusted: true,
            blocked: false,
            legacy_pairing: false,
            rssi: Some(-60),
            tx_power: Some(4),
            connected: true,
            uuids: vec!["0000110b".into()],
            adapter: "/org/bluez/hci0".into(),
            services_resolved: true,
            battery: Some(80),
        }
    }

    #[test]
    fn adapter_info_sale_con_las_claves_de_guest_js() {
        assert_matches_guest_js("AdapterInfo", &full_adapter());
    }

    #[test]
    fn device_info_sale_con_las_claves_de_guest_js() {
        assert_matches_guest_js("DeviceInfo", &full_device());
    }

    #[test]
    fn device_info_sin_bateria_sigue_trayendo_las_obligatorias() {
        // La batería es la única clave que puede faltar del JSON; las que
        // guest-js declara obligatorias tienen que estar siempre.
        let device = DeviceInfo {
            battery: None,
            ..full_device()
        };
        let (declared, optional) = guest_js_keys("DeviceInfo");
        let required: BTreeSet<_> = declared.difference(&optional).cloned().collect();
        let sent = serialized_keys(&device);
        assert!(
            required.is_subset(&sent),
            "faltan obligatorias: {:?}",
            required.difference(&sent).collect::<Vec<_>>()
        );
        assert!(!sent.contains("battery"));
    }

    #[test]
    fn bluetooth_change_sale_con_las_claves_de_guest_js() {
        let change = BluetoothChange {
            change_type: "device-added".into(),
            data: serde_json::json!({ "path": "/org/bluez/hci0" }),
        };
        assert_matches_guest_js("BluetoothChange", &change);
    }

    #[test]
    fn bluetooth_change_manda_el_nombre_viejo_con_el_mismo_valor() {
        let change = BluetoothChange {
            change_type: "device-removed".into(),
            data: serde_json::Value::Null,
        };
        let json = serde_json::to_value(&change).unwrap();
        assert_eq!(json["changeType"], "device-removed");
        assert_eq!(json["change_type"], "device-removed");
    }

    #[test]
    fn bluetooth_change_se_lee_con_cualquiera_de_los_dos_nombres() {
        for key in ["changeType", "change_type"] {
            let json = serde_json::json!({ key: "error", "data": {} });
            let change: BluetoothChange = serde_json::from_value(json).unwrap();
            assert_eq!(change.change_type, "error", "con `{key}`");
        }
    }

    #[test]
    fn bluetooth_change_se_vuelve_a_leer_tal_como_sale() {
        let change = BluetoothChange {
            change_type: "device-connected".into(),
            data: serde_json::json!({ "path": "/org/bluez/hci0/dev_AA" }),
        };
        let json = serde_json::to_value(&change).unwrap();
        let back: BluetoothChange = serde_json::from_value(json).unwrap();
        assert_eq!(back.change_type, "device-connected");
        assert_eq!(back.data, change.data);
    }

    #[test]
    fn bluetooth_change_sin_ninguno_de_los_dos_nombres_es_un_error() {
        let json = serde_json::json!({ "data": {} });
        assert!(serde_json::from_value::<BluetoothChange>(json).is_err());
    }

    #[test]
    fn ping_sale_con_las_claves_de_guest_js() {
        let value = Some("hola".to_string());
        assert_matches_guest_js(
            "PingRequest",
            &PingRequest {
                value: value.clone(),
            },
        );
        assert_matches_guest_js("PingResponse", &PingResponse { value });
    }

    #[test]
    fn el_lector_de_guest_js_separa_las_opcionales() {
        let (all, optional) = guest_js_keys("DeviceInfo");
        assert!(all.contains("legacyPairing"));
        assert!(optional.contains("battery"));
        assert!(!optional.contains("paired"));
        // Lo que hay dentro del comentario de `battery` no son claves.
        assert!(all
            .iter()
            .all(|k| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')));
    }
}
