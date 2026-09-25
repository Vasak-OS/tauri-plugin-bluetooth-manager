use crate::desktop::BluetoothManager;
use crate::models::{AdapterInfo, DeviceInfo};
use crate::properties::{
    adapter_info_from_props, device_info_from_interfaces, device_info_from_props, Interfaces,
    Properties, ADAPTER_INTERFACE, BATTERY_INTERFACE, DEVICE_INTERFACE,
};
use crate::Result;
use std::collections::HashMap;
use tauri::State;
use tracing::{debug, error, info};
use zbus::{
    zvariant::{OwnedObjectPath, Value as ZbusValue},
    Connection, Proxy,
};

/// Todos los objetos que BlueZ administra, con sus interfaces y propiedades.
async fn managed_objects(conn: &Connection) -> Result<HashMap<OwnedObjectPath, Interfaces>> {
    let proxy = Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;
    let reply = proxy.call_method("GetManagedObjects", &()).await?;
    Ok(reply.body().deserialize()?)
}

/// El proxy de `org.freedesktop.DBus.Properties` de un objeto de BlueZ.
async fn properties_proxy<'a>(conn: &Connection, path: &'a str) -> Result<Proxy<'a>> {
    Ok(Proxy::new(conn, "org.bluez", path, "org.freedesktop.DBus.Properties").await?)
}

/// `GetAll` de una interfaz sobre un objeto.
async fn get_all(proxy: &Proxy<'_>, interface: &str) -> Result<Properties> {
    let reply = proxy.call_method("GetAll", &(interface,)).await?;
    Ok(reply.body().deserialize()?)
}

#[tauri::command]
pub async fn list_adapters() -> Result<Vec<AdapterInfo>> {
    let conn = Connection::system().await?;
    let adapters = managed_objects(&conn)
        .await?
        .into_iter()
        .filter_map(|(object_path, interfaces)| {
            interfaces
                .get(ADAPTER_INTERFACE)
                .map(|props| adapter_info_from_props(object_path.to_string(), props))
        })
        .collect();
    Ok(adapters)
}

#[tauri::command]
pub async fn set_adapter_powered(adapter_path: String, powered: bool) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = properties_proxy(&conn, &adapter_path).await?;

    proxy
        .call_method(
            "Set",
            &(ADAPTER_INTERFACE, "Powered", ZbusValue::from(powered)),
        )
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_adapter_state(adapter_path: String) -> Result<AdapterInfo> {
    let conn = Connection::system().await?;
    let proxy = properties_proxy(&conn, &adapter_path).await?;
    let props = get_all(&proxy, ADAPTER_INTERFACE).await?;
    Ok(adapter_info_from_props(adapter_path, &props))
}

#[tauri::command]
pub async fn start_scan(adapter_path: String) -> Result<()> {
    info!("Starting scan on adapter: {}", adapter_path);

    let conn = Connection::system().await?;
    let proxy = Proxy::new(&conn, "org.bluez", adapter_path.as_str(), ADAPTER_INTERFACE).await?;

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
    let proxy = Proxy::new(&conn, "org.bluez", adapter_path.as_str(), ADAPTER_INTERFACE).await?;

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
                || msg.contains("org.bluez.Error.NotReady")
            {
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
    let managed_objects = managed_objects(&conn).await?;

    info!("Total managed objects: {}", managed_objects.len());

    let mut devices = Vec::new();

    for (object_path, interfaces) in managed_objects {
        let path_str = object_path.as_str();
        let has_device_interface = interfaces.contains_key(DEVICE_INTERFACE);

        info!(
            "Object path: {} | has Device1: {} | starts_with adapter: {}",
            path_str,
            has_device_interface,
            path_str.starts_with(&adapter_path)
        );

        if !path_str.starts_with(&adapter_path) {
            continue;
        }

        if let Some(device) = device_info_from_interfaces(object_path.to_string(), &interfaces) {
            info!(
                "Found device: {} ({})",
                device.name.as_deref().unwrap_or("Unknown"),
                device.address
            );
            devices.push(device);
        }
    }

    info!("Total devices found: {}", devices.len());
    Ok(devices)
}

#[tauri::command]
pub async fn get_device_info(device_path: String) -> Result<DeviceInfo> {
    let conn = Connection::system().await?;
    let proxy = properties_proxy(&conn, &device_path).await?;

    let device = get_all(&proxy, DEVICE_INTERFACE).await?;

    // La batería es opcional: sólo los dispositivos que la informan tienen
    // `org.bluez.Battery1`, y para los demás BlueZ contesta con un error
    // (`InvalidArgs`) en vez de con un mapa vacío. Ese error no es un problema
    // del dispositivo, es «no publica batería», así que se traga y el campo
    // queda ausente. El de `Device1`, arriba, sigue propagándose como siempre.
    let battery = match get_all(&proxy, BATTERY_INTERFACE).await {
        Ok(props) => Some(props),
        Err(e) => {
            debug!("{} no publica batería: {}", device_path, e);
            None
        }
    };

    Ok(device_info_from_props(
        device_path,
        &device,
        battery.as_ref(),
    ))
}

#[tauri::command]
pub async fn list_paired_devices(adapter_path: String) -> Result<Vec<DeviceInfo>> {
    let conn = Connection::system().await?;
    let paired_devices = managed_objects(&conn)
        .await?
        .into_iter()
        .filter(|(object_path, _)| object_path.as_str().starts_with(&adapter_path))
        .filter_map(|(object_path, interfaces)| {
            device_info_from_interfaces(object_path.to_string(), &interfaces)
        })
        .filter(|device| device.paired)
        .collect();
    Ok(paired_devices)
}

#[tauri::command]
pub async fn connect_device(device_path: String) -> Result<()> {
    let conn = Connection::system().await?;
    let proxy = Proxy::new(&conn, "org.bluez", device_path.as_str(), DEVICE_INTERFACE).await?;

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
    let proxy = Proxy::new(&conn, "org.bluez", device_path.as_str(), DEVICE_INTERFACE).await?;

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
pub async fn bluetooth_plugin_status(state: State<'_, BluetoothManager>) -> Result<bool> {
    Ok(*state.initialized.lock().unwrap())
}
