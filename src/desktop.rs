use futures::StreamExt;
use std::sync::Mutex;
use tauri::{plugin::PluginApi, AppHandle, Emitter, Manager, Runtime};
use zbus::{
    zvariant::{ObjectPath, OwnedValue, Value as ZbusValue},
    Connection, MessageStream, MessageType, Proxy,
};
use std::collections::HashMap;
use std::convert::TryFrom;

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

use crate::commands::{get_adapter_state, get_device_info};
use crate::models::*;
use crate::Result as CrateResult;

pub struct BluetoothManager {
    pub conn: Connection,
    pub initialized: Mutex<bool>,
}

pub async fn init<R: Runtime>(app: AppHandle<R>, _api: PluginApi<R, ()>) -> CrateResult<()> {
    let conn = Connection::system().await?;

    let manager = BluetoothManager {
        conn: conn.clone(),
        initialized: Mutex::new(false),
    };

    app.manage(manager);

    // Suscribirse explícitamente a las señales antes de iniciar el listener
    setup_dbus_subscriptions(&conn).await?;

    tauri::async_runtime::spawn(run_signal_listener(conn, app));

    Ok(())
}

async fn setup_dbus_subscriptions(conn: &Connection) -> CrateResult<()> {
    
    // Suscribirse a las señales del ObjectManager de BlueZ
    let proxy = Proxy::new(
        conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    ).await?;

    match proxy.call_method("GetManagedObjects", &()).await {
        Ok(_) => println!("[bluetooth-plugin] Successfully connected to BlueZ ObjectManager"),
        Err(e) => {
            eprintln!("[bluetooth-plugin] Failed to connect to BlueZ ObjectManager: {:?}", e);
            return Err(e.into());
        }
    }

    // Configurar filtros de señales más específicos
    let dbus_proxy = Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ).await?;

    // Agregar reglas de coincidencia para señales específicas
    let rules = vec![
        "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.ObjectManager'",
        "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties'",
        "type='signal',sender='org.bluez',interface='org.bluez.Adapter1'",
        "type='signal',sender='org.bluez',interface='org.bluez.Device1'",
    ];

    for rule in rules {
        match dbus_proxy.call_method("AddMatch", &(rule,)).await {
            Ok(_) => println!("[bluetooth-plugin] Added D-Bus match rule: {}", rule),
            Err(e) => eprintln!("[bluetooth-plugin] Failed to add match rule '{}': {:?}", rule, e),
        }
    }

    Ok(())
}

fn helper_adapter_info_from_props(
    path: String,
    props: &HashMap<String, OwnedValue>,
) -> AdapterInfo {
    AdapterInfo {
        path,
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
    }
}

fn helper_device_info_from_props(path: String, props: &HashMap<String, OwnedValue>) -> DeviceInfo {
    DeviceInfo {
        path,
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
        uuids: get_prop_vec(props, "UUIDs"),
        adapter: props.get("Adapter")
            .and_then(|v| ObjectPath::try_from(&**v).ok())
            .map(|p: ObjectPath| p.to_string())
            .unwrap_or_default(),
        services_resolved: get_prop!(props, "ServicesResolved", bool, false),
    }
}

async fn run_signal_listener<R: Runtime>(conn: Connection, app: AppHandle<R>) {
    use std::time::{Duration, Instant};
    let mut stream = MessageStream::from(conn.clone());

    // Throttling state
    let mut device_last_update: HashMap<String, Instant> = HashMap::new();
    const UPDATE_THROTTLE: Duration = Duration::from_millis(500);

    // Obtener el nombre único de org.bluez para comparación
    let mut bluez_unique_name: Option<String> = None;
    
    // Intentar obtener el nombre único del servicio org.bluez
    let _dbus_proxy = match Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ).await {
        Ok(proxy) => {
                match proxy.call_method("GetNameOwner", &("org.bluez",)).await {
                        Ok(reply) => {
                            if let Ok(unique_name) = reply.body().deserialize::<String>() {
                        bluez_unique_name = Some(unique_name);
                    }
                }
                Err(e) => eprintln!("[bluetooth-plugin] Failed to get org.bluez unique name: {:?}", e),
            }
            Some(proxy)
        }
        Err(_e) => {
            None
        }
    };

    while let Some(msg_res) = stream.next().await {
        match msg_res {
          Ok(msg) => {
            if msg.message_type() == MessageType::Signal {
                let header = msg.header();

                let sender_opt_str = header.sender().map(|s| s.to_string());

                let is_bluez_signal = sender_opt_str.as_deref() == Some("org.bluez") || 
                    (bluez_unique_name.is_some() && sender_opt_str == bluez_unique_name);

                if is_bluez_signal {
                    let interface_opt_string = header.interface()
                        .map(|i| i.as_str().to_string());

                    let member_opt_string = header.member()
                        .map(|m| m.as_str().to_string());

                    let path_opt_string = header.path()
                        .map(|p| p.as_str().to_string());
                    
                    match (interface_opt_string.as_deref(), member_opt_string.as_deref()) {
                        (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesAdded")) => {
                            match msg.body().deserialize::<(ObjectPath<'_>, HashMap<String, HashMap<String, OwnedValue>>)>() {
                                Ok((object_path, interfaces_and_properties)) => {
                                  let path_string = object_path.to_string();
                                  
                                  // Detectar cambios de adaptadores
                                  if let Some(adapter_props) = interfaces_and_properties.get("org.bluez.Adapter1") {
                                    let adapter_info = helper_adapter_info_from_props(path_string.clone(), adapter_props);
                                    
                                    app.emit("bluetooth-change", BluetoothChange {
                                        change_type: "adapter-added".to_string(),
                                        data: serde_json::to_value(adapter_info).unwrap_or_default(),
                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-added: {}", e));
                                  }
                                  
                                  // Detectar cambios de dispositivos
                                  if let Some(device_props) = interfaces_and_properties.get("org.bluez.Device1") {
                                    let device_info = helper_device_info_from_props(path_string.clone(), device_props);
                                    
                                    app.emit("bluetooth-change", BluetoothChange {
                                        change_type: "device-added".to_string(),
                                        data: serde_json::to_value(device_info).unwrap_or_default(),
                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-added: {}", e));
                                  }
                                }
                                Err(e) => {
                                  eprintln!("[bluetooth-plugin] Error decoding InterfacesAdded body: {:?}", e);
                                  app.emit("bluetooth-change", BluetoothChange {
                                      change_type: "error".to_string(),
                                      data: serde_json::json!({ "message": format!("Error decoding InterfacesAdded: {:?}", e) }),
                                  }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit error: {}", err));
                                }
                              }
                        }
                        (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesRemoved")) => {
                            match msg.body().deserialize::<(ObjectPath<'_>, Vec<String>)>() {
                                Ok((object_path, interfaces_removed)) => {
                                  let path_string = object_path.to_string();
                                  
                                  if interfaces_removed.contains(&"org.bluez.Adapter1".to_string()) {
                                    app.emit("bluetooth-change", BluetoothChange {
                                        change_type: "adapter-removed".to_string(),
                                        data: serde_json::json!({ "path": path_string.clone() }),
                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-removed: {}", e));
                                  }
                                  
                                  if interfaces_removed.contains(&"org.bluez.Device1".to_string()) {
                                    
                                    app.emit("bluetooth-change", BluetoothChange {
                                        change_type: "device-removed".to_string(),
                                        data: serde_json::json!({ "path": path_string }),
                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-removed: {}", e));
                                  }
                                }
                                Err(e) => {
                                  eprintln!("[bluetooth-plugin] Error decoding InterfacesRemoved body: {:?}", e);
                                  app.emit("bluetooth-change", BluetoothChange {
                                      change_type: "error".to_string(),
                                      data: serde_json::json!({ "message": format!("Error decoding InterfacesRemoved: {:?}", e) }),
                                  }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit error: {}", err));
                                }
                              }
                        }
                        (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
                            
                            if let Some(p_str) = path_opt_string {
                                match msg.body().deserialize::<(String, HashMap<String, ZbusValue<'_>>, Vec<String>)>() {
                                    Ok((changed_interface_name, changed_properties, _invalidated_properties)) => {
                                        if changed_interface_name == "org.bluez.Adapter1" {
                                            match get_adapter_state(p_str.clone()).await {
                                                Ok(adapter_info) => {
                                                    app.emit("bluetooth-change", BluetoothChange {
                                                        change_type: "adapter-property-changed".to_string(),
                                                        data: serde_json::to_value(adapter_info).unwrap_or_default(),
                                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-property-changed: {}", e));
                                                }
                                                Err(e) => {
                                                    eprintln!("[bluetooth-plugin] Error getting adapter state for {}: {:?}", p_str, e);
                                                    app.emit("bluetooth-change", BluetoothChange {
                                                        change_type: "error".to_string(),
                                                        data: serde_json::json!({ "message": format!("Error getting adapter state: {:?}", e) }),
                                                    }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit error: {}", err));
                                                }
                                            }
                                        } 
                                        else if changed_interface_name == "org.bluez.Device1" {
                                            // Throttling logic for device properties
                                            let critical_keys = ["Connected", "Paired", "Trusted", "Blocked", "Name", "Alias"];
                                            let is_critical = changed_properties.keys().any(|k| critical_keys.contains(&k.as_str()));

                                            if !is_critical {
                                                if let Some(last) = device_last_update.get(&p_str) {
                                                    if last.elapsed() < UPDATE_THROTTLE {
                                                        // Skip this update
                                                        continue;
                                                    }
                                                }
                                                device_last_update.insert(p_str.clone(), Instant::now());
                                            }

                                            match get_device_info(p_str.clone()).await {
                                                Ok(device_info) => {
                                                    // println!("[bluetooth-plugin] Device property changed: {}", p_str);
                                                    app.emit("bluetooth-change", BluetoothChange {
                                                        change_type: "device-property-changed".to_string(),
                                                        data: serde_json::to_value(device_info).unwrap_or_default(),
                                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-property-changed: {}", e));
                                                }
                                                Err(e) => {
                                                    eprintln!("[bluetooth-plugin] Error getting device info for {}: {:?}", p_str, e);
                                                    app.emit("bluetooth-change", BluetoothChange {
                                                        change_type: "error".to_string(),
                                                        data: serde_json::json!({ "message": format!("Error getting device info: {:?}", e) }),
                                                    }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit error: {}", err));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("[bluetooth-plugin] Error decoding PropertiesChanged body: {:?}", e);
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "error".to_string(),
                                            data: serde_json::json!({ "message": format!("Error decoding PropertiesChanged: {:?}", e) }),
                                        }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit error: {}", err));
                                    }
                                }
                            } else {
                                eprintln!("[bluetooth-plugin] PropertiesChanged signal received without a valid path.");
                                app.emit("bluetooth-change", BluetoothChange {
                                    change_type: "error".to_string(),
                                    data: serde_json::json!({ "message": "PropertiesChanged signal without path" }),
                                }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit error: {}", err));
                            }
                        }
                        (Some("org.bluez.Device1"), Some("Disconnected")) => {
                            if let Some(p_str) = path_opt_string {
                                match get_device_info(p_str.clone()).await {
                                    Ok(device_info) => {
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "device-disconnected".to_string(),
                                            data: serde_json::to_value(device_info).unwrap_or_default(),
                                        }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-disconnected: {}", e));
                                    }
                                    Err(e) => {
                                        eprintln!("[bluetooth-plugin] Error getting device info for disconnected device {}: {:?}", p_str, e);
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "device-disconnected".to_string(),
                                            data: serde_json::json!({ "path": p_str }),
                                        }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit fallback device-disconnected: {}", err));
                                    }
                                }
                            }
                        }
                        (Some("org.bluez.Device1"), Some("Connected")) => {
                            if let Some(p_str) = path_opt_string {
                                match get_device_info(p_str.clone()).await {
                                    Ok(device_info) => {
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "device-connected".to_string(),
                                            data: serde_json::to_value(device_info).unwrap_or_default(),
                                        }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-connected: {}", e));
                                    }
                                    Err(e) => {
                                        eprintln!("[bluetooth-plugin] Error getting device info for connected device {}: {:?}", p_str, e);
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "device-connected".to_string(),
                                            data: serde_json::json!({ "path": p_str, "connected": true }),
                                        }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit fallback device-connected: {}", err));
                                    }
                                }
                            }
                        }
                        _ => {
                        }
                    }
                }
            }
          }
          Err(e) => {
            eprintln!("[bluetooth-plugin] Error reading from D-Bus message stream: {:?}", e);
            app.emit("bluetooth-change", BluetoothChange {
                change_type: "dbus-error".to_string(),
                data: serde_json::json!({ "message": format!("D-Bus stream error: {:?}", e) }),
            }).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit dbus-error: {}", err));
            break;
          }
        }
    }
}

impl BluetoothManager {
    pub fn ping(
        &self,
        payload: crate::models::PingRequest,
    ) -> CrateResult<crate::models::PingResponse> {
        Ok(crate::models::PingResponse {
            value: payload.value,
        })
    }
}