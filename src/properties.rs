//! Lectura de las propiedades que BlueZ publica por D-Bus.
//!
//! BlueZ entrega cada propiedad envuelta en un variant, y la misma lectura
//! —«sacá `Address` como texto, `RSSI` como i16, y si no está o no es de ese
//! tipo dejalo vacío»— se repetía a mano en cuatro lugares: tres comandos y el
//! oyente de señales. Acá vive una sola vez, sin tocar D-Bus, para que se pueda
//! probar con mapas armados a mano.
//!
//! Las interfaces que se leen son `org.bluez.Adapter1`, `org.bluez.Device1` y
//! `org.bluez.Battery1`. Esta última es opcional: sólo la publican los
//! dispositivos que informan batería, y cuando no está el dato queda ausente,
//! que no es lo mismo que cero.

use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::models::{AdapterInfo, DeviceInfo};

/// Las propiedades de una interfaz, tal como llegan de `GetAll` o de
/// `GetManagedObjects`.
pub(crate) type Properties = HashMap<String, OwnedValue>;

/// Las interfaces de un objeto con sus propiedades, tal como llegan de
/// `GetManagedObjects` y de `InterfacesAdded`.
pub(crate) type Interfaces = HashMap<String, Properties>;

pub(crate) const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
pub(crate) const DEVICE_INTERFACE: &str = "org.bluez.Device1";
pub(crate) const BATTERY_INTERFACE: &str = "org.bluez.Battery1";

/// Lee una propiedad como el tipo pedido; `None` si falta o si el variant trae
/// otro tipo.
fn read<'a, T>(props: &'a Properties, key: &str) -> Option<T>
where
    T: TryFrom<&'a Value<'a>>,
{
    props.get(key).and_then(|value| T::try_from(value).ok())
}

/// Como [`read`], con un valor de fábrica cuando la propiedad no está.
fn read_or<'a, T>(props: &'a Properties, key: &str, default: T) -> T
where
    T: TryFrom<&'a Value<'a>>,
{
    read(props, key).unwrap_or(default)
}

/// Lee una propiedad que es un array de textos (`as`), como `UUIDs`. Si falta,
/// no es un array o alguno de sus elementos no es texto, queda vacía.
fn read_strings(props: &Properties, key: &str) -> Vec<String> {
    props
        .get(key)
        .and_then(|value| match &**value {
            Value::Array(items) => items
                .iter()
                .map(|item| String::try_from(item).ok())
                .collect::<Option<Vec<_>>>(),
            _ => None,
        })
        .unwrap_or_default()
}

/// Lee una propiedad que es una ruta de objeto (`o`), como `Adapter`, y la
/// devuelve como texto; vacía si falta.
fn read_object_path(props: &Properties, key: &str) -> String {
    read::<ObjectPath<'_>>(props, key)
        .map(|path| path.to_string())
        .unwrap_or_default()
}

/// El porcentaje de batería que publica `org.bluez.Battery1`, si el
/// dispositivo la publica y trae `Percentage` con el tipo que corresponde (`y`).
pub(crate) fn battery_percentage(battery: Option<&Properties>) -> Option<u8> {
    battery.and_then(|props| read::<u8>(props, "Percentage"))
}

/// Arma un [`AdapterInfo`] con las propiedades de `org.bluez.Adapter1`.
pub(crate) fn adapter_info_from_props(path: String, props: &Properties) -> AdapterInfo {
    AdapterInfo {
        path,
        address: read_or(props, "Address", String::new()),
        name: read_or(props, "Name", String::new()),
        alias: read_or(props, "Alias", String::new()),
        class: read_or(props, "Class", 0),
        powered: read_or(props, "Powered", false),
        discoverable: read_or(props, "Discoverable", false),
        discoverable_timeout: read_or(props, "DiscoverableTimeout", 0),
        pairable: read_or(props, "Pairable", false),
        pairable_timeout: read_or(props, "PairableTimeout", 0),
        discovering: read_or(props, "Discovering", false),
        uuids: read_strings(props, "UUIDs"),
        modalias: read(props, "Modalias"),
    }
}

/// Arma un [`DeviceInfo`] con las propiedades de `org.bluez.Device1` y, si el
/// dispositivo la publica, las de `org.bluez.Battery1`.
pub(crate) fn device_info_from_props(
    path: String,
    device: &Properties,
    battery: Option<&Properties>,
) -> DeviceInfo {
    DeviceInfo {
        path,
        address: read_or(device, "Address", String::new()),
        name: read(device, "Name"),
        alias: read(device, "Alias"),
        class: read(device, "Class"),
        appearance: read(device, "Appearance"),
        icon: read(device, "Icon"),
        paired: read_or(device, "Paired", false),
        trusted: read_or(device, "Trusted", false),
        blocked: read_or(device, "Blocked", false),
        legacy_pairing: read_or(device, "LegacyPairing", false),
        rssi: read(device, "RSSI"),
        tx_power: read(device, "TxPower"),
        connected: read_or(device, "Connected", false),
        uuids: read_strings(device, "UUIDs"),
        adapter: read_object_path(device, "Adapter"),
        services_resolved: read_or(device, "ServicesResolved", false),
        battery: battery_percentage(battery),
    }
}

/// Arma un [`DeviceInfo`] con todas las interfaces de un objeto: la de
/// dispositivo, y la de batería si viene en el mismo mapa. `None` si el objeto
/// no es un dispositivo (no trae `org.bluez.Device1`).
pub(crate) fn device_info_from_interfaces(
    path: String,
    interfaces: &Interfaces,
) -> Option<DeviceInfo> {
    let device = interfaces.get(DEVICE_INTERFACE)?;
    let battery = interfaces.get(BATTERY_INTERFACE);
    Some(device_info_from_props(path, device, battery))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("un valor sin descriptores siempre se puede poseer")
    }

    fn props(entries: Vec<(&str, Value<'_>)>) -> Properties {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), owned(value)))
            .collect()
    }

    fn device_props() -> Properties {
        props(vec![
            ("Address", Value::from("AA:BB:CC:DD:EE:FF")),
            ("Name", Value::from("Auriculares")),
            ("Connected", Value::Bool(true)),
            ("RSSI", Value::I16(-60)),
            ("UUIDs", Value::from(vec!["0000110b", "0000110e"])),
            (
                "Adapter",
                Value::from(ObjectPath::try_from("/org/bluez/hci0").unwrap()),
            ),
        ])
    }

    fn path() -> String {
        "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".to_string()
    }

    // --- batería -----------------------------------------------------------

    #[test]
    fn bateria_presente_da_el_porcentaje() {
        let battery = props(vec![("Percentage", Value::U8(87))]);
        let info = device_info_from_props(path(), &device_props(), Some(&battery));
        assert_eq!(info.battery, Some(87));
    }

    #[test]
    fn sin_interfaz_de_bateria_el_dato_queda_ausente() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.battery, None);
    }

    #[test]
    fn bateria_sin_porcentaje_queda_ausente() {
        let battery = props(vec![("Source", Value::from("HFP 1.7"))]);
        assert_eq!(battery_percentage(Some(&battery)), None);
    }

    #[test]
    fn porcentaje_con_otro_tipo_queda_ausente_y_no_en_cero() {
        // BlueZ lo declara `y` (u8). Si llegara como otro entero no se
        // adivina: ausente, que no es lo mismo que descargado.
        let battery = props(vec![("Percentage", Value::U32(87))]);
        assert_eq!(battery_percentage(Some(&battery)), None);
    }

    #[test]
    fn el_porcentaje_cero_es_un_dato_y_no_una_ausencia() {
        let battery = props(vec![("Percentage", Value::U8(0))]);
        assert_eq!(battery_percentage(Some(&battery)), Some(0));
    }

    #[test]
    fn sin_bateria_el_json_no_trae_la_clave() {
        let info = device_info_from_props(path(), &device_props(), None);
        let json = serde_json::to_value(info).unwrap();
        assert!(json.get("battery").is_none(), "{json}");
    }

    #[test]
    fn con_bateria_el_json_trae_el_numero() {
        let battery = props(vec![("Percentage", Value::U8(42))]);
        let info = device_info_from_props(path(), &device_props(), Some(&battery));
        let json = serde_json::to_value(info).unwrap();
        assert_eq!(json["battery"], serde_json::json!(42));
    }

    // --- desde las interfaces de un objeto --------------------------------

    #[test]
    fn un_objeto_sin_device1_no_es_un_dispositivo() {
        let mut interfaces = Interfaces::new();
        interfaces.insert(
            BATTERY_INTERFACE.to_string(),
            props(vec![("Percentage", Value::U8(50))]),
        );
        assert!(device_info_from_interfaces(path(), &interfaces).is_none());
    }

    #[test]
    fn la_bateria_se_toma_del_mismo_objeto_si_viene() {
        let mut interfaces = Interfaces::new();
        interfaces.insert(DEVICE_INTERFACE.to_string(), device_props());
        interfaces.insert(
            BATTERY_INTERFACE.to_string(),
            props(vec![("Percentage", Value::U8(50))]),
        );
        let info = device_info_from_interfaces(path(), &interfaces).unwrap();
        assert_eq!(info.battery, Some(50));
        assert_eq!(info.address, "AA:BB:CC:DD:EE:FF");
    }

    #[test]
    fn un_dispositivo_solo_con_device1_sale_sin_bateria() {
        let mut interfaces = Interfaces::new();
        interfaces.insert(DEVICE_INTERFACE.to_string(), device_props());
        let info = device_info_from_interfaces(path(), &interfaces).unwrap();
        assert_eq!(info.battery, None);
        assert_eq!(info.path, path());
    }

    // --- lo que ya existía y nadie probaba ---------------------------------

    #[test]
    fn el_rssi_se_lee_como_i16() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.rssi, Some(-60));
    }

    #[test]
    fn un_rssi_con_otro_tipo_queda_ausente() {
        let mut device = device_props();
        device.insert("RSSI".to_string(), owned(Value::I32(-60)));
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.rssi, None);
    }

    #[test]
    fn los_uuids_salen_del_array_de_textos() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.uuids, vec!["0000110b", "0000110e"]);
    }

    #[test]
    fn un_array_que_no_es_de_textos_deja_los_uuids_vacios() {
        let mut device = device_props();
        device.insert("UUIDs".to_string(), owned(Value::from(vec![1u32, 2u32])));
        let info = device_info_from_props(path(), &device, None);
        assert!(info.uuids.is_empty());
    }

    #[test]
    fn el_adaptador_sale_de_la_ruta_de_objeto() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.adapter, "/org/bluez/hci0");
    }

    #[test]
    fn sin_adaptador_la_ruta_queda_vacia() {
        let mut device = device_props();
        device.remove("Adapter");
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.adapter, "");
    }

    #[test]
    fn los_booleanos_ausentes_valen_falso() {
        let info = device_info_from_props(path(), &props(vec![]), None);
        assert!(!info.paired);
        assert!(!info.trusted);
        assert!(!info.blocked);
        assert!(!info.legacy_pairing);
        assert!(!info.connected);
        assert!(!info.services_resolved);
        assert_eq!(info.address, "");
        assert_eq!(info.name, None);
    }

    #[test]
    fn los_opcionales_del_dispositivo_se_leen_con_su_tipo() {
        let device = props(vec![
            ("Class", Value::U32(0x240404)),
            ("Appearance", Value::U16(0x0941)),
            ("Icon", Value::from("audio-headset")),
            ("TxPower", Value::I16(4)),
            ("Alias", Value::from("Mis auriculares")),
        ]);
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.class, Some(0x240404));
        assert_eq!(info.appearance, Some(0x0941));
        assert_eq!(info.icon.as_deref(), Some("audio-headset"));
        assert_eq!(info.tx_power, Some(4));
        assert_eq!(info.alias.as_deref(), Some("Mis auriculares"));
    }

    #[test]
    fn el_adaptador_se_arma_con_modalias_ausente() {
        let adapter = props(vec![
            ("Address", Value::from("00:11:22:33:44:55")),
            ("Name", Value::from("hci0")),
            ("Powered", Value::Bool(true)),
            ("DiscoverableTimeout", Value::U32(180)),
            ("UUIDs", Value::from(vec!["00001200"])),
        ]);
        let info = adapter_info_from_props("/org/bluez/hci0".to_string(), &adapter);
        assert_eq!(info.modalias, None);
        assert_eq!(info.address, "00:11:22:33:44:55");
        assert_eq!(info.name, "hci0");
        assert!(info.powered);
        assert!(!info.discovering);
        assert_eq!(info.discoverable_timeout, 180);
        assert_eq!(info.pairable_timeout, 0);
        assert_eq!(info.class, 0);
        assert_eq!(info.uuids, vec!["00001200"]);
    }

    #[test]
    fn el_adaptador_lee_modalias_cuando_esta() {
        let adapter = props(vec![("Modalias", Value::from("usb:v1D6Bp0246d0540"))]);
        let info = adapter_info_from_props("/org/bluez/hci0".to_string(), &adapter);
        assert_eq!(info.modalias.as_deref(), Some("usb:v1D6Bp0246d0540"));
    }
}
