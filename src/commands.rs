use crate::models::{AdapterInfo, DeviceInfo};
use crate::Result;
use std::collections::HashMap;
use std::convert::TryFrom;
use tauri::{State};
use zbus::{
    zvariant::{
        from_slice, EncodingContext, ObjectPath, OwnedObjectPath, OwnedValue, Value as ZbusValue,
    },
    Connection, Proxy,
};
use crate::desktop::BluetoothManager;

#[tauri::command]
pub async fn list_adapters() -> Result<Vec<AdapterInfo>> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;

    let reply_message = proxy.call_method("GetManagedObjects", &()).await?;

    // Decodificar correctamente como HashMap<ObjectPath, HashMap<String, HashMap<String, OwnedValue>>>
    let managed_objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        reply_message.body()?;

    let mut adapters = Vec::new();

    for (object_path, interfaces) in managed_objects {
        if let Some(props) = interfaces.get("org.bluez.Adapter1") {
            adapters.push(AdapterInfo {
                path: object_path.to_string(),
                address: props
                    .get("Address")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                name: props
                    .get("Name")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                alias: props
                    .get("Alias")
                    .and_then(|v| String::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                class: props
                    .get("Class")
                    .and_then(|v| u32::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                powered: props
                    .get("Powered")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false),
                discoverable: props
                    .get("Discoverable")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false),
                discoverable_timeout: props
                    .get("DiscoverableTimeout")
                    .and_then(|v| u32::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                pairable: props
                    .get("Pairable")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false),
                pairable_timeout: props
                    .get("PairableTimeout")
                    .and_then(|v| u32::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                discovering: props
                    .get("Discovering")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false),
                uuids: props
                    .get("UUIDs")
                    .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
                    .unwrap_or_default(),
                modalias: props
                    .get("Modalias")
                    .and_then(|v| String::try_from(v.clone()).ok()),
            });
        }
    }
    Ok(adapters)
}

#[tauri::command]
pub async fn set_adapter_powered(adapter_path: String, powered: bool) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        adapter_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await?;

    proxy
        .call_method(
            "Set",
            &("org.bluez.Adapter1", "Powered", ZbusValue::from(powered)),
        )
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_adapter_state(adapter_path: String) -> Result<AdapterInfo> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        adapter_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await?;

    let reply_message = proxy
        .call_method("GetAll", &("org.bluez.Adapter1",))
        .await?;

    let body_bytes_owned: Vec<u8> = reply_message.body_as_bytes()?.to_vec();
    let ctxt = EncodingContext::<byteorder::NativeEndian>::new_dbus(0);
    let props: HashMap<String, OwnedValue> = from_slice(&body_bytes_owned, ctxt)?;

    Ok(AdapterInfo {
        path: adapter_path,
        address: props
            .get("Address")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default(),
        name: props
            .get("Name")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default(),
        alias: props
            .get("Alias")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default(),
        class: props
            .get("Class")
            .and_then(|v| u32::try_from(v.clone()).ok())
            .unwrap_or_default(),
        powered: props
            .get("Powered")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        discoverable: props
            .get("Discoverable")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        discoverable_timeout: props
            .get("DiscoverableTimeout")
            .and_then(|v| u32::try_from(v.clone()).ok())
            .unwrap_or_default(),
        pairable: props
            .get("Pairable")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        pairable_timeout: props
            .get("PairableTimeout")
            .and_then(|v| u32::try_from(v.clone()).ok())
            .unwrap_or_default(),
        discovering: props
            .get("Discovering")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        uuids: props
            .get("UUIDs")
            .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
            .unwrap_or_default(),
        modalias: props
            .get("Modalias")
            .and_then(|v| String::try_from(v.clone()).ok()),
    })
}

#[tauri::command]
pub async fn start_scan(adapter_path: String) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        adapter_path.as_str(),
        "org.bluez.Adapter1",
    )
    .await?;
    proxy.call_method("StartDiscovery", &()).await?;
    Ok(())
}

#[tauri::command]
pub async fn stop_scan(adapter_path: String) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        adapter_path.as_str(),
        "org.bluez.Adapter1",
    )
    .await?;
    proxy.call_method("StopDiscovery", &()).await?;
    Ok(())
}

#[tauri::command]
pub async fn list_devices(adapter_path: String) -> Result<Vec<DeviceInfo>> {
    let conn = Connection::system().await?;
    let object_manager_proxy = Proxy::new(
        &conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;

    let reply_message = object_manager_proxy
        .call_method("GetManagedObjects", &())
        .await?;

    // Decodificar correctamente como HashMap
    let managed_objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        reply_message.body()?;

    let mut devices = Vec::new();

    for (object_path, interfaces) in managed_objects {
        if object_path.as_str().starts_with(&adapter_path) {
            if let Some(props) = interfaces.get("org.bluez.Device1") {
                devices.push(DeviceInfo {
                    path: object_path.to_string(),
                    address: props
                        .get("Address")
                        .and_then(|v| String::try_from(v.clone()).ok())
                        .unwrap_or_default(),
                    name: props
                        .get("Name")
                        .and_then(|v| String::try_from(v.clone()).ok()),
                    alias: props
                        .get("Alias")
                        .and_then(|v| String::try_from(v.clone()).ok()),
                    class: props
                        .get("Class")
                        .and_then(|v| u32::try_from(v.clone()).ok()),
                    appearance: props
                        .get("Appearance")
                        .and_then(|v| u16::try_from(v.clone()).ok()),
                    icon: props
                        .get("Icon")
                        .and_then(|v| String::try_from(v.clone()).ok()),
                    paired: props
                        .get("Paired")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false),
                    trusted: props
                        .get("Trusted")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false),
                    blocked: props
                        .get("Blocked")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false),
                    legacy_pairing: props
                        .get("LegacyPairing")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false),
                    rssi: props
                        .get("RSSI")
                        .and_then(|v| i16::try_from(v.clone()).ok()),
                    tx_power: props
                        .get("TxPower")
                        .and_then(|v| i16::try_from(v.clone()).ok()),
                    connected: props
                        .get("Connected")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false),
                    uuids: props
                        .get("UUIDs")
                        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
                        .unwrap_or_default(),
                    adapter: props
                        .get("Adapter")
                        .and_then(|v| ObjectPath::try_from(v.clone()).ok())
                        .map(|p: ObjectPath| p.to_string())
                        .unwrap_or_default(),
                    services_resolved: props
                        .get("ServicesResolved")
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false),
                });
            }
        }
    }
    Ok(devices)
}

#[tauri::command]
pub async fn get_device_info(device_path: String) -> Result<DeviceInfo> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        device_path.as_str(),
        "org.freedesktop.DBus.Properties",
    )
    .await?;

    let reply_message = proxy.call_method("GetAll", &("org.bluez.Device1",)).await?;

    let body_bytes_owned: Vec<u8> = reply_message.body_as_bytes()?.to_vec();
    let ctxt = EncodingContext::<byteorder::NativeEndian>::new_dbus(0);
    let props: HashMap<String, OwnedValue> = from_slice(&body_bytes_owned, ctxt)?;

    Ok(DeviceInfo {
        path: device_path,
        address: props
            .get("Address")
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default(),
        name: props
            .get("Name")
            .and_then(|v| String::try_from(v.clone()).ok()),
        alias: props
            .get("Alias")
            .and_then(|v| String::try_from(v.clone()).ok()),
        class: props
            .get("Class")
            .and_then(|v| u32::try_from(v.clone()).ok()),
        appearance: props
            .get("Appearance")
            .and_then(|v| u16::try_from(v.clone()).ok()),
        icon: props
            .get("Icon")
            .and_then(|v| String::try_from(v.clone()).ok()),
        paired: props
            .get("Paired")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        trusted: props
            .get("Trusted")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        blocked: props
            .get("Blocked")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        legacy_pairing: props
            .get("LegacyPairing")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        rssi: props
            .get("RSSI")
            .and_then(|v| i16::try_from(v.clone()).ok()),
        tx_power: props
            .get("TxPower")
            .and_then(|v| i16::try_from(v.clone()).ok()),
        connected: props
            .get("Connected")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
        uuids: props
            .get("UUIDs")
            .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
            .unwrap_or_default(),
        adapter: props
            .get("Adapter")
            .and_then(|v| ObjectPath::try_from(v.clone()).ok())
            .map(|p: ObjectPath| p.to_string())
            .unwrap_or_default(),
        services_resolved: props
            .get("ServicesResolved")
            .and_then(|v| bool::try_from(v.clone()).ok())
            .unwrap_or(false),
    })
}

#[tauri::command]
pub async fn list_paired_devices(adapter_path: String) -> Result<Vec<DeviceInfo>> {
    let conn = Connection::system().await?;
    let object_manager_proxy = Proxy::new(
        &conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;

    let reply_message = object_manager_proxy
        .call_method("GetManagedObjects", &())
        .await?;

    // Decodificar correctamente como HashMap
    let managed_objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        reply_message.body()?;

    let mut paired_devices = Vec::new();

    for (object_path, interfaces) in managed_objects {
        if object_path.as_str().starts_with(&adapter_path) {
            if let Some(props) = interfaces.get("org.bluez.Device1") {
                let paired = props
                    .get("Paired")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false);
                if paired {
                    paired_devices.push(DeviceInfo {
                        path: object_path.to_string(),
                        address: props
                            .get("Address")
                            .and_then(|v| String::try_from(v.clone()).ok())
                            .unwrap_or_default(),
                        name: props
                            .get("Name")
                            .and_then(|v| String::try_from(v.clone()).ok()),
                        alias: props
                            .get("Alias")
                            .and_then(|v| String::try_from(v.clone()).ok()),
                        class: props
                            .get("Class")
                            .and_then(|v| u32::try_from(v.clone()).ok()),
                        appearance: props
                            .get("Appearance")
                            .and_then(|v| u16::try_from(v.clone()).ok()),
                        icon: props
                            .get("Icon")
                            .and_then(|v| String::try_from(v.clone()).ok()),
                        paired,
                        trusted: props
                            .get("Trusted")
                            .and_then(|v| bool::try_from(v.clone()).ok())
                            .unwrap_or(false),
                        blocked: props
                            .get("Blocked")
                            .and_then(|v| bool::try_from(v.clone()).ok())
                            .unwrap_or(false),
                        legacy_pairing: props
                            .get("LegacyPairing")
                            .and_then(|v| bool::try_from(v.clone()).ok())
                            .unwrap_or(false),
                        rssi: props
                            .get("RSSI")
                            .and_then(|v| i16::try_from(v.clone()).ok()),
                        tx_power: props
                            .get("TxPower")
                            .and_then(|v| i16::try_from(v.clone()).ok()),
                        connected: props
                            .get("Connected")
                            .and_then(|v| bool::try_from(v.clone()).ok())
                            .unwrap_or(false),
                        uuids: props
                            .get("UUIDs")
                            .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
                            .unwrap_or_default(),
                        adapter: props
                            .get("Adapter")
                            .and_then(|v| ObjectPath::try_from(v.clone()).ok())
                            .map(|p: ObjectPath| p.to_string())
                            .unwrap_or_default(),
                        services_resolved: props
                            .get("ServicesResolved")
                            .and_then(|v| bool::try_from(v.clone()).ok())
                            .unwrap_or(false),
                    });
                }
            }
        }
    }
    Ok(paired_devices)
}

#[tauri::command]
pub async fn connect_device(device_path: String) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        device_path.as_str(),
        "org.bluez.Device1",
    )
    .await?;

    proxy.call_method("Connect", &()).await?;
    Ok(())
}

#[tauri::command]
pub async fn disconnect_device(device_path: String) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        device_path.as_str(),
        "org.bluez.Device1",
    )
    .await?;

    proxy.call_method("Disconnect", &()).await?;
    Ok(())
}

#[tauri::command]
pub async fn bluetooth_plugin_status(
    state: State<'_, BluetoothManager>
) -> Result<bool> {
    Ok(*state.initialized.lock().unwrap())
}
