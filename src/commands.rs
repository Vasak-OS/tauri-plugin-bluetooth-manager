use crate::models::{AdapterInfo, DeviceInfo};
use crate::Result;
use std::collections::HashMap;
use std::convert::TryFrom;
use tauri::State;
use zbus::{
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value as ZbusValue},
    Connection, Proxy,
};
use crate::desktop::BluetoothManager;
use tracing::{info, error};

fn get_prop_vec(props: &HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
    props.get(key).and_then(|v| {
        match &**v {
            ZbusValue::Array(arr) => arr.iter()
                .map(|e| String::try_from(e).ok())
                .collect::<Option<Vec<_>>>(),
            _ => None,
        }
    }).unwrap_or_default()
}

macro_rules! get_prop {
    ($props:expr, $key:expr, $ty:ty) => {
        $props.get($key).and_then(|v| <$ty>::try_from(&**v).ok())
    };
    ($props:expr, $key:expr, $ty:ty, $default:expr) => {
        $props.get($key).and_then(|v| <$ty>::try_from(&**v).ok()).unwrap_or($default)
    };
}

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
        reply_message.body().deserialize()?;

    let mut adapters = Vec::new();

    for (object_path, interfaces) in managed_objects {
        if let Some(props) = interfaces.get("org.bluez.Adapter1") {
            adapters.push(AdapterInfo {
                path: object_path.to_string(),
                address: get_prop!(props, "Address", String, String::new()),
                name: get_prop!(props, "Name", String, String::new()),
                alias: get_prop!(props, "Alias", String, String::new()),
                class: get_prop!(props, "Class", u32, 0),
                powered: get_prop!(props, "Powered", bool, false),
                discoverable: get_prop!(props, "Discoverable", bool, false),
                discoverable_timeout: get_prop!(props, "DiscoverableTimeout", u32, 0),
                pairable: get_prop!(props, "Pairable", bool, false),
                pairable_timeout: get_prop!(props, "PairableTimeout", u32, 0),
                discovering: get_prop!(props, "Discovering", bool, false),
                uuids: get_prop_vec(props, "UUIDs"),
                modalias: get_prop!(props, "Modalias", String),
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

    let props: HashMap<String, OwnedValue> = reply_message.body().deserialize()?;

    Ok(AdapterInfo {
        path: adapter_path,
        address: get_prop!(props, "Address", String, String::new()),
        name: get_prop!(props, "Name", String, String::new()),
        alias: get_prop!(props, "Alias", String, String::new()),
        class: get_prop!(props, "Class", u32, 0),
        powered: get_prop!(props, "Powered", bool, false),
        discoverable: get_prop!(props, "Discoverable", bool, false),
        discoverable_timeout: get_prop!(props, "DiscoverableTimeout", u32, 0),
        pairable: get_prop!(props, "Pairable", bool, false),
        pairable_timeout: get_prop!(props, "PairableTimeout", u32, 0),
        discovering: get_prop!(props, "Discovering", bool, false),
        uuids: get_prop_vec(&props, "UUIDs"),
        modalias: get_prop!(props, "Modalias", String),
    })
}

#[tauri::command]
pub async fn start_scan(adapter_path: String) -> Result<()> {
    info!("Starting scan on adapter: {}", adapter_path);
    
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        adapter_path.as_str(),
        "org.bluez.Adapter1",
    )
    .await?;
    
    match proxy.call_method("StartDiscovery", &()).await {
        Ok(_) => {
            info!("Scan started successfully");
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            error!("StartDiscovery error: {}", msg);
            if msg.contains("org.bluez.Error.InProgress") || msg.contains("InProgress") {
                info!("Scan already in progress, continuing...");
                Ok(())
            } else {
                error!("Error starting scan: {}", msg);
                Err(e.into())
            }
        }
    }
}

#[tauri::command]
pub async fn stop_scan(adapter_path: String) -> Result<()> {
    info!("Stopping scan on adapter: {}", adapter_path);
    
    let conn = Connection::system().await?;
    let proxy = Proxy::new(
        &conn,
        "org.bluez",
        adapter_path.as_str(),
        "org.bluez.Adapter1",
    )
    .await?;
    
    match proxy.call_method("StopDiscovery", &()).await {
        Ok(_) => {
            info!("Scan stopped successfully");
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            error!("StopDiscovery error: {}", msg);
            if msg.contains("No discovery started") 
                || msg.contains("org.bluez.Error.Failed")
                || msg.contains("org.bluez.Error.NotReady") {
                info!("No active scan to stop, continuing...");
                Ok(())
            } else {
                error!("Error stopping scan: {}", msg);
                Err(e.into())
            }
        }
    }
}

#[tauri::command]
pub async fn list_devices(adapter_path: String) -> Result<Vec<DeviceInfo>> {
    info!("Listing devices for adapter: {}", adapter_path);
    
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

    let managed_objects: HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>> =
        reply_message.body().deserialize()?;

    info!("Total managed objects: {}", managed_objects.len());

    let mut devices = Vec::new();

    for (object_path, interfaces) in managed_objects {
        let path_str = object_path.as_str();
        let has_device_interface = interfaces.contains_key("org.bluez.Device1");
        
        info!(
            "Object path: {} | has Device1: {} | starts_with adapter: {}",
            path_str,
            has_device_interface,
            path_str.starts_with(&adapter_path)
        );
        
        if path_str.starts_with(&adapter_path) {
            if let Some(props) = interfaces.get("org.bluez.Device1") {
                let device_name = get_prop!(props, "Name", String, "Unknown".to_string());
                let device_address = get_prop!(props, "Address", String, "Unknown".to_string());

                info!("Found device: {} ({})", device_name, device_address);

                devices.push(DeviceInfo {
                    path: object_path.to_string(),
                    address: device_address,
                    name: get_prop!(props, "Name", String),
                    alias: get_prop!(props, "Alias", String),
                    class: get_prop!(props, "Class", u32),
                    appearance: get_prop!(props, "Appearance", u16),
                    icon: get_prop!(props, "Icon", String),
                    paired: get_prop!(props, "Paired", bool, false),
                    trusted: get_prop!(props, "Trusted", bool, false),
                    blocked: get_prop!(props, "Blocked", bool, false),
                    legacy_pairing: get_prop!(props, "LegacyPairing", bool, false),
                    rssi: get_prop!(props, "RSSI", i16),
                    tx_power: get_prop!(props, "TxPower", i16),
                    connected: get_prop!(props, "Connected", bool, false),
                    uuids: get_prop_vec(props, "UUIDs"),
                    adapter: props
                        .get("Adapter")
                        .and_then(|v| ObjectPath::try_from(&**v).ok())
                        .map(|p: ObjectPath| p.to_string())
                        .unwrap_or_default(),
                    services_resolved: get_prop!(props, "ServicesResolved", bool, false),
                });
            }
        }
    }
    
    info!("Total devices found: {}", devices.len());
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

    let props: HashMap<String, OwnedValue> = reply_message.body().deserialize()?;

    Ok(DeviceInfo {
        path: device_path,
        address: get_prop!(props, "Address", String, String::new()),
        name: get_prop!(props, "Name", String),
        alias: get_prop!(props, "Alias", String),
        class: get_prop!(props, "Class", u32),
        appearance: get_prop!(props, "Appearance", u16),
        icon: get_prop!(props, "Icon", String),
        paired: get_prop!(props, "Paired", bool, false),
        trusted: get_prop!(props, "Trusted", bool, false),
        blocked: get_prop!(props, "Blocked", bool, false),
        legacy_pairing: get_prop!(props, "LegacyPairing", bool, false),
        rssi: get_prop!(props, "RSSI", i16),
        tx_power: get_prop!(props, "TxPower", i16),
        connected: get_prop!(props, "Connected", bool, false),
        uuids: get_prop_vec(&props, "UUIDs"),
        adapter: props
            .get("Adapter")
            .and_then(|v| ObjectPath::try_from(&**v).ok())
            .map(|p: ObjectPath| p.to_string())
            .unwrap_or_default(),
        services_resolved: get_prop!(props, "ServicesResolved", bool, false),
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
        reply_message.body().deserialize()?;

    let mut paired_devices = Vec::new();

    for (object_path, interfaces) in managed_objects {
        if object_path.as_str().starts_with(&adapter_path) {
            if let Some(props) = interfaces.get("org.bluez.Device1") {
                let paired = get_prop!(props, "Paired", bool, false);
                if paired {
                    paired_devices.push(DeviceInfo {
                        path: object_path.to_string(),
                        address: get_prop!(props, "Address", String, String::new()),
                        name: get_prop!(props, "Name", String),
                        alias: get_prop!(props, "Alias", String),
                        class: get_prop!(props, "Class", u32),
                        appearance: get_prop!(props, "Appearance", u16),
                        icon: get_prop!(props, "Icon", String),
                        paired,
                        trusted: get_prop!(props, "Trusted", bool, false),
                        blocked: get_prop!(props, "Blocked", bool, false),
                        legacy_pairing: get_prop!(props, "LegacyPairing", bool, false),
                        rssi: get_prop!(props, "RSSI", i16),
                        tx_power: get_prop!(props, "TxPower", i16),
                        connected: get_prop!(props, "Connected", bool, false),
                        uuids: get_prop_vec(props, "UUIDs"),
                        adapter: props
                            .get("Adapter")
                            .and_then(|v| ObjectPath::try_from(&**v).ok())
                            .map(|p: ObjectPath| p.to_string())
                            .unwrap_or_default(),
                        services_resolved: get_prop!(props, "ServicesResolved", bool, false),
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

    match proxy.call_method("Connect", &()).await {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("InProgress")
                || msg.contains("br-connection-busy")
                || msg.contains("AlreadyConnected")
                || msg.contains("br-connection-already-connected")
            {
                info!("Device already connecting or connected, continuing...");
                Ok(())
            } else {
                error!("Error connecting to device: {}", msg);
                Err(e.into())
            }
        }
    }
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

    match proxy.call_method("Disconnect", &()).await {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("NotConnected")
                || msg.contains("br-connection-not-connected")
                || msg.contains("br-connection-already-disconnected")
            {
                info!("Device already disconnected, continuing...");
                Ok(())
            } else {
                error!("Error disconnecting device: {}", msg);
                Err(e.into())
            }
        }
    }
}

#[tauri::command]
pub async fn bluetooth_plugin_status(
    state: State<'_, BluetoothManager>
) -> Result<bool> {
    Ok(*state.initialized.lock().unwrap())
}
