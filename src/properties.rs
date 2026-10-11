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

/// Lee un array de bytes (`ay`) desde el valor de una entrada de
/// `ManufacturerData`/`ServiceData`.
///
/// Gotcha de zvariant: en el mapa `a{sv}` de nivel superior (lo que entrega
/// `GetAll`) el variant ya viene desenvuelto, pero el variant anidado dentro de
/// un `a{qv}`/`a{sv}` **no**: al iterar el `Dict`, cada valor suele llegar como
/// `Value::Value(Box<Value>)` que hay que desenvolver una vez antes de llegar
/// al `Value::Array` de `U8`. Este ayudante tolera las dos formas. Devuelve
/// `None` si no es un array o si alguno de sus elementos no es un `U8`.
fn variant_bytes(value: &Value<'_>) -> Option<Vec<u8>> {
    let inner = match value {
        Value::Value(boxed) => boxed.as_ref(),
        other => other,
    };
    match inner {
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::U8(byte) => Some(*byte),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Lee `ManufacturerData` (`a{qv}`: company ID → bytes). `None` cuando la
/// propiedad no está, no es un dict, o el mapa quedaría vacío.
fn read_manufacturer_data(props: &Properties) -> Option<HashMap<u16, Vec<u8>>> {
    let Value::Dict(dict) = &**props.get("ManufacturerData")? else {
        return None;
    };
    let mut map = HashMap::new();
    for (key, value) in dict.iter() {
        let Value::U16(id) = key else {
            continue;
        };
        if let Some(bytes) = variant_bytes(value) {
            map.insert(*id, bytes);
        }
    }
    (!map.is_empty()).then_some(map)
}

/// Lee `ServiceData` (`a{sv}`: UUID del servicio → bytes). `None` cuando la
/// propiedad no está, no es un dict, o el mapa quedaría vacío.
fn read_service_data(props: &Properties) -> Option<HashMap<String, Vec<u8>>> {
    let Value::Dict(dict) = &**props.get("ServiceData")? else {
        return None;
    };
    let mut map = HashMap::new();
    for (key, value) in dict.iter() {
        let Ok(uuid) = String::try_from(key) else {
            continue;
        };
        if let Some(bytes) = variant_bytes(value) {
            map.insert(uuid, bytes);
        }
    }
    (!map.is_empty()).then_some(map)
}

/// Resuelve el nombre del fabricante desde `manufacturer_data`. Si hay varias
/// entradas se toma la de **menor** company ID, para que el resultado sea
/// determinista, y se busca en la tabla de Bluetooth SIG. `None` si no hay
/// datos o si el id no está en la tabla.
fn manufacturer_name(manufacturer_data: Option<&HashMap<u16, Vec<u8>>>) -> Option<String> {
    let min_id = manufacturer_data?.keys().min()?;
    crate::company_identifiers::company_name(*min_id).map(str::to_string)
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
    let manufacturer_data = read_manufacturer_data(device);
    let manufacturer = manufacturer_name(manufacturer_data.as_ref());
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
        manufacturer,
        service_data: read_service_data(device),
        modalias: read(device, "Modalias"),
        manufacturer_data,
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

    use zbus::zvariant::{Dict, Signature};

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

    // --- fabricante y datos crudos (ManufacturerData / ServiceData) ---------

    /// Un company ID que no está en la tabla de Bluetooth SIG. El id más alto
    /// de la tabla generada es `0x1130`, así que `0xF000` no está y no se
    /// vuelve conocido cuando se regenere la tabla.
    const UNKNOWN_COMPANY_ID: u16 = 0xF000;

    /// Arma el valor de `ManufacturerData` (`a{qv}`) tal como lo entrega BlueZ:
    /// el byte-array de cada entrada va envuelto en un variant anidado
    /// (`Value::Value`), que es el gotcha de zvariant 4 que `variant_bytes`
    /// desenvuelve. `value_override` deja meter un valor que no es `ay` para
    /// probar el descarte.
    fn manufacturer_data(entries: Vec<(u16, Value<'static>)>) -> Value<'static> {
        let mut dict = Dict::new(
            Signature::try_from("q").unwrap(),
            Signature::try_from("v").unwrap(),
        );
        for (id, value) in entries {
            dict.append(Value::U16(id), Value::Value(Box::new(value)))
                .unwrap();
        }
        Value::Dict(dict)
    }

    /// Un `ay` (array de bytes) como el que va dentro de cada entrada.
    fn bytes(raw: Vec<u8>) -> Value<'static> {
        Value::from(raw)
    }

    /// Arma el valor de `ServiceData` (`a{sv}`: UUID → bytes), con el byte-array
    /// envuelto igual que arriba.
    fn service_data(entries: Vec<(&str, Vec<u8>)>) -> Value<'static> {
        let mut dict = Dict::new(
            Signature::try_from("s").unwrap(),
            Signature::try_from("v").unwrap(),
        );
        for (uuid, raw) in entries {
            dict.append(
                Value::from(uuid.to_string()),
                Value::Value(Box::new(Value::from(raw))),
            )
            .unwrap();
        }
        Value::Dict(dict)
    }

    // --- variant_bytes: el gotcha de zvariant 4 ----------------------------

    #[test]
    fn variant_bytes_desenvuelve_el_variant_anidado() {
        let wrapped = Value::Value(Box::new(bytes(vec![0x02, 0x15, 0xFF])));
        assert_eq!(variant_bytes(&wrapped), Some(vec![0x02, 0x15, 0xFF]));
    }

    #[test]
    fn variant_bytes_tolera_el_array_sin_envolver() {
        // Por si alguna ruta entregara el `ay` ya desenvuelto.
        assert_eq!(
            variant_bytes(&bytes(vec![0x01, 0x02])),
            Some(vec![0x01, 0x02])
        );
    }

    #[test]
    fn variant_bytes_rechaza_el_doble_envuelto() {
        let doubly = Value::Value(Box::new(Value::Value(Box::new(bytes(vec![0x01])))));
        assert_eq!(variant_bytes(&doubly), None);
    }

    #[test]
    fn variant_bytes_rechaza_lo_que_no_es_un_array() {
        assert_eq!(variant_bytes(&Value::U16(5)), None);
        let wrapped_scalar = Value::Value(Box::new(Value::U16(5)));
        assert_eq!(variant_bytes(&wrapped_scalar), None);
    }

    #[test]
    fn variant_bytes_rechaza_un_array_que_no_es_de_u8() {
        let not_u8 = Value::Value(Box::new(Value::from(vec![1u32, 2u32])));
        assert_eq!(variant_bytes(&not_u8), None);
    }

    // --- manufacturer_data y el nombre resuelto ----------------------------

    #[test]
    fn el_fabricante_conocido_se_resuelve_por_su_id() {
        for (id, name) in [(0x004C_u16, "Apple, Inc."), (0x0006, "Microsoft")] {
            let mut device = device_props();
            device.insert(
                "ManufacturerData".to_string(),
                owned(manufacturer_data(vec![(id, bytes(vec![0x02, 0x15]))])),
            );
            let info = device_info_from_props(path(), &device, None);
            assert_eq!(info.manufacturer.as_deref(), Some(name), "id {id:#06x}");
            assert_eq!(
                info.manufacturer_data,
                Some(HashMap::from([(id, vec![0x02, 0x15])])),
                "el crudo de id {id:#06x}"
            );
        }
    }

    #[test]
    fn con_varios_fabricantes_gana_el_id_menor() {
        // Dos entradas: Microsoft (0x0006) y Apple (0x004C). El menor manda,
        // para que el resultado no dependa del orden del mapa.
        let mut device = device_props();
        device.insert(
            "ManufacturerData".to_string(),
            owned(manufacturer_data(vec![
                (0x004C, bytes(vec![0x02, 0x15])),
                (0x0006, bytes(vec![0x01])),
            ])),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.manufacturer.as_deref(), Some("Microsoft"));
        assert_eq!(
            info.manufacturer_data,
            Some(HashMap::from([
                (0x004C, vec![0x02, 0x15]),
                (0x0006, vec![0x01]),
            ]))
        );
    }

    #[test]
    fn un_id_desconocido_no_da_nombre_pero_si_crudo() {
        let mut device = device_props();
        device.insert(
            "ManufacturerData".to_string(),
            owned(manufacturer_data(vec![(
                UNKNOWN_COMPANY_ID,
                bytes(vec![0xAB, 0xCD]),
            )])),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.manufacturer, None);
        assert_eq!(
            info.manufacturer_data,
            Some(HashMap::from([(UNKNOWN_COMPANY_ID, vec![0xAB, 0xCD])]))
        );
    }

    #[test]
    fn manufacturer_data_vacio_queda_ausente_y_no_en_mapa_vacio() {
        let mut device = device_props();
        device.insert(
            "ManufacturerData".to_string(),
            owned(manufacturer_data(vec![])),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.manufacturer_data, None);
        assert_eq!(info.manufacturer, None);
    }

    #[test]
    fn sin_manufacturer_data_ambos_quedan_ausentes() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.manufacturer_data, None);
        assert_eq!(info.manufacturer, None);
    }

    #[test]
    fn una_entrada_cuyos_bytes_no_son_u8_se_descarta() {
        // Única entrada con un valor que no es `ay`: el mapa queda vacío y el
        // campo ausente, no un mapa vacío.
        let mut device = device_props();
        device.insert(
            "ManufacturerData".to_string(),
            owned(manufacturer_data(vec![(
                0x004C,
                Value::from(vec![1u32, 2u32]),
            )])),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.manufacturer_data, None);
        assert_eq!(info.manufacturer, None);
    }

    #[test]
    fn la_entrada_rota_se_cae_pero_la_sana_queda() {
        // Dos entradas, una con bytes válidos y otra con un valor que no es
        // `ay`: sobrevive sólo la sana.
        let mut device = device_props();
        device.insert(
            "ManufacturerData".to_string(),
            owned(manufacturer_data(vec![
                (0x0006, bytes(vec![0x09])),
                (0x004C, Value::from("no soy bytes")),
            ])),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(
            info.manufacturer_data,
            Some(HashMap::from([(0x0006, vec![0x09])]))
        );
        assert_eq!(info.manufacturer.as_deref(), Some("Microsoft"));
    }

    // --- service_data ------------------------------------------------------

    #[test]
    fn service_data_con_dos_entradas_sale_crudo() {
        let mut device = device_props();
        device.insert(
            "ServiceData".to_string(),
            owned(service_data(vec![
                ("0000fe0f-0000-1000-8000-00805f9b34fb", vec![0x00, 0x01]),
                ("0000feaa-0000-1000-8000-00805f9b34fb", vec![0x10]),
            ])),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(
            info.service_data,
            Some(HashMap::from([
                (
                    "0000fe0f-0000-1000-8000-00805f9b34fb".to_string(),
                    vec![0x00, 0x01]
                ),
                (
                    "0000feaa-0000-1000-8000-00805f9b34fb".to_string(),
                    vec![0x10]
                ),
            ]))
        );
    }

    #[test]
    fn service_data_vacio_queda_ausente() {
        let mut device = device_props();
        device.insert(
            "ServiceData".to_string(),
            owned(Value::Dict(Dict::new(
                Signature::try_from("s").unwrap(),
                Signature::try_from("v").unwrap(),
            ))),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.service_data, None);
    }

    #[test]
    fn sin_service_data_queda_ausente() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.service_data, None);
    }

    // --- modalias del dispositivo ------------------------------------------

    #[test]
    fn el_dispositivo_lee_modalias_cuando_esta() {
        let mut device = device_props();
        device.insert(
            "Modalias".to_string(),
            owned(Value::from("bluetooth:v004Cp2000d0A00")),
        );
        let info = device_info_from_props(path(), &device, None);
        assert_eq!(info.modalias.as_deref(), Some("bluetooth:v004Cp2000d0A00"));
    }

    #[test]
    fn el_dispositivo_sin_modalias_queda_ausente() {
        let info = device_info_from_props(path(), &device_props(), None);
        assert_eq!(info.modalias, None);
    }

    // --- serialización JSON de los campos nuevos ---------------------------

    #[test]
    fn el_json_de_manufacturer_data_lleva_claves_string_y_arrays_de_numeros() {
        // guest-js promete `Record<string, number[]>`: la clave u16 del mapa
        // sale como texto (0x4C → "76") y los bytes como un array de números.
        let mut device = device_props();
        device.insert(
            "ManufacturerData".to_string(),
            owned(manufacturer_data(vec![(0x004C, bytes(vec![0x02, 0x15]))])),
        );
        device.insert(
            "ServiceData".to_string(),
            owned(service_data(vec![(
                "0000fe0f-0000-1000-8000-00805f9b34fb",
                vec![0x00],
            )])),
        );
        let info = device_info_from_props(path(), &device, None);
        let json = serde_json::to_value(info).unwrap();
        assert_eq!(
            json["manufacturerData"],
            serde_json::json!({ "76": [2, 21] }),
            "{json}"
        );
        assert_eq!(json["manufacturer"], serde_json::json!("Apple, Inc."));
        assert_eq!(
            json["serviceData"],
            serde_json::json!({ "0000fe0f-0000-1000-8000-00805f9b34fb": [0] }),
            "{json}"
        );
    }

    #[test]
    fn sin_los_campos_nuevos_el_json_no_trae_esas_claves() {
        // skip_serializing_if: ausente es ausente, no `null` ni mapa vacío.
        let info = device_info_from_props(path(), &device_props(), None);
        let json = serde_json::to_value(info).unwrap();
        assert!(json.get("manufacturer").is_none(), "{json}");
        assert!(json.get("manufacturerData").is_none(), "{json}");
        assert!(json.get("serviceData").is_none(), "{json}");
        assert!(json.get("modalias").is_none(), "{json}");
    }
}
