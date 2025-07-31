use futures::StreamExt;
use tauri::{plugin::PluginApi, AppHandle, Emitter, Manager, Runtime};
use zbus::{
    zvariant::{ObjectPath, OwnedValue, Value as ZbusValue},
    Connection, MessageStream, MessageType, Proxy,
};
use std::collections::HashMap;
use std::convert::TryFrom;

use crate::commands::{get_adapter_state, get_device_info};
use crate::models::*;
use crate::Result as CrateResult;

#[derive(Clone)]
pub struct BluetoothManager {
    conn: Connection,
}

pub async fn init<R: Runtime>(app: AppHandle<R>, _api: PluginApi<R, ()>) -> CrateResult<()> {
    let conn = Connection::system().await?;

    let manager = BluetoothManager {
        conn: conn.clone(),
    };

    app.manage(manager);

    // Suscribirse explícitamente a las señales antes de iniciar el listener
    setup_dbus_subscriptions(&conn).await?;

    tauri::async_runtime::spawn(run_signal_listener(conn, app));

    Ok(())
}

async fn setup_dbus_subscriptions(conn: &Connection) -> CrateResult<()> {
    println!("[bluetooth-plugin] Setting up D-Bus subscriptions...");
    
    // Suscribirse a las señales del ObjectManager de BlueZ
    let proxy = Proxy::new(
        conn,
        "org.bluez",
        "/",
        "org.freedesktop.DBus.ObjectManager",
    ).await?;

    // Intentar llamar a GetManagedObjects para verificar la conectividad
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

    println!("[bluetooth-plugin] D-Bus subscriptions setup complete");
    Ok(())
}

fn helper_adapter_info_from_props(
    path: String,
    props: &HashMap<String, OwnedValue>,
) -> AdapterInfo {
    AdapterInfo {
        path,
        address: props.get("Address").and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default(),
        name: props.get("Name").and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default(),
        alias: props.get("Alias").and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default(),
        class: props.get("Class").and_then(|v| u32::try_from(v.clone()).ok()).unwrap_or_default(),
        powered: props.get("Powered").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        discoverable: props.get("Discoverable").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        discoverable_timeout: props.get("DiscoverableTimeout").and_then(|v| u32::try_from(v.clone()).ok()).unwrap_or_default(),
        pairable: props.get("Pairable").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        pairable_timeout: props.get("PairableTimeout").and_then(|v| u32::try_from(v.clone()).ok()).unwrap_or_default(),
        discovering: props.get("Discovering").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        uuids: props.get("UUIDs")
            .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
            .unwrap_or_default(),
        modalias: props.get("Modalias").and_then(|v| String::try_from(v.clone()).ok()),
    }
}

fn helper_device_info_from_props(path: String, props: &HashMap<String, OwnedValue>) -> DeviceInfo {
    DeviceInfo {
        path,
        address: props.get("Address").and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default(),
        name: props.get("Name").and_then(|v| String::try_from(v.clone()).ok()),
        alias: props.get("Alias").and_then(|v| String::try_from(v.clone()).ok()),
        class: props.get("Class").and_then(|v| u32::try_from(v.clone()).ok()),
        appearance: props.get("Appearance").and_then(|v| u16::try_from(v.clone()).ok()),
        icon: props.get("Icon").and_then(|v| String::try_from(v.clone()).ok()),
        paired: props.get("Paired").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        trusted: props.get("Trusted").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        blocked: props.get("Blocked").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        legacy_pairing: props.get("LegacyPairing").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        rssi: props.get("RSSI").and_then(|v| i16::try_from(v.clone()).ok()),
        tx_power: props.get("TxPower").and_then(|v| i16::try_from(v.clone()).ok()),
        connected: props.get("Connected").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
        uuids: props.get("UUIDs")
            .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
            .unwrap_or_default(),
        adapter: props.get("Adapter")
            .and_then(|v| ObjectPath::try_from(v.clone()).ok())
            .map(|p: ObjectPath| p.to_string())
            .unwrap_or_default(),
        services_resolved: props.get("ServicesResolved").and_then(|v| bool::try_from(v.clone()).ok()).unwrap_or(false),
    }
}

async fn run_signal_listener<R: Runtime>(conn: Connection, app: AppHandle<R>) {
    println!("[bluetooth-plugin] Initializing D-Bus signal listener...");
    let mut stream = MessageStream::from(conn.clone());
    println!("[bluetooth-plugin] MessageStream created. Waiting for signals...");

    // Obtener el nombre único de org.bluez para comparación
    let mut bluez_unique_name: Option<String> = None;
    
    // Intentar obtener el nombre único del servicio org.bluez
    let dbus_proxy = match Proxy::new(
        &conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    ).await {
        Ok(proxy) => {
            match proxy.call_method("GetNameOwner", &("org.bluez",)).await {
                Ok(reply) => {
                    if let Ok(unique_name) = reply.body::<String>() {
                        println!("[bluetooth-plugin] org.bluez unique name: {}", unique_name);
                        bluez_unique_name = Some(unique_name);
                    }
                }
                Err(e) => eprintln!("[bluetooth-plugin] Failed to get org.bluez unique name: {:?}", e),
            }
            Some(proxy)
        }
        Err(e) => {
            eprintln!("[bluetooth-plugin] Failed to create D-Bus proxy: {:?}", e);
            None
        }
    };

    let mut signal_count = 0;

    while let Some(msg_res) = stream.next().await {
        signal_count += 1;
        
        if signal_count % 10 == 1 {
            println!("[bluetooth-plugin] Received {} signals so far", signal_count);
        }

        match msg_res {
          Ok(msg) => {
            if msg.message_type() == MessageType::Signal {
                let header = match msg.header() {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("[bluetooth-plugin] Failed to get message header: {:?}", e);
                        continue;
                    }
                };

                let sender_opt_str = match header.sender() {
                    Ok(Some(unique_name_ref)) => Some(unique_name_ref.as_str()),
                    Ok(None) => None,
                    Err(e) => {
                        eprintln!("[bluetooth-plugin] Error getting sender from header: {:?}", e);
                        None
                    }
                };

                // Verificar si la señal viene de BlueZ (nombre del servicio o nombre único)
                let is_bluez_signal = sender_opt_str == Some("org.bluez") || 
                    (bluez_unique_name.is_some() && sender_opt_str == bluez_unique_name.as_deref());

                if is_bluez_signal {
                    let interface_opt_string = match header.interface() {
                        Ok(Some(i_ref)) => Some(i_ref.as_str().to_string()),
                        Ok(None) => None,
                        Err(e) => {
                            eprintln!("[bluetooth-plugin] Error getting interface from header: {:?}", e);
                            None
                        }
                    };

                    let member_opt_string = match header.member() {
                        Ok(Some(m_ref)) => Some(m_ref.as_str().to_string()),
                        Ok(None) => None,
                        Err(e) => {
                            eprintln!("[bluetooth-plugin] Error getting member from header: {:?}", e);
                            None
                        }
                    };

                    let path_opt_string = match header.path() {
                        Ok(Some(p_ref)) => Some(p_ref.as_str().to_string()),
                        Ok(None) => None,
                        Err(e) => {
                            eprintln!("[bluetooth-plugin] Error getting path from header: {:?}", e);
                            None
                        }
                    };

                    println!("[bluetooth-plugin] Processing BlueZ signal: interface={:?}, member={:?}, path={:?}", 
                             interface_opt_string, member_opt_string, path_opt_string);
                    
                    match (interface_opt_string.as_deref(), member_opt_string.as_deref()) {
                        (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesAdded")) => {
                            println!("[bluetooth-plugin] Processing InterfacesAdded signal");
                            match msg.body::<(ObjectPath<'_>, HashMap<String, HashMap<String, OwnedValue>>)>() {
                                Ok((object_path, interfaces_and_properties)) => {
                                  let path_string = object_path.to_string();
                                  
                                  // Detectar cambios de adaptadores
                                  if let Some(adapter_props) = interfaces_and_properties.get("org.bluez.Adapter1") {
                                    let adapter_info = helper_adapter_info_from_props(path_string.clone(), adapter_props);
                                    println!("[bluetooth-plugin] Adapter added: {}", path_string);
                                    app.emit("bluetooth-change", BluetoothChange {
                                        change_type: "adapter-added".to_string(),
                                        data: serde_json::to_value(adapter_info).unwrap_or_default(),
                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-added: {}", e));
                                  }
                                  
                                  // Detectar cambios de dispositivos
                                  if let Some(device_props) = interfaces_and_properties.get("org.bluez.Device1") {
                                    let device_info = helper_device_info_from_props(path_string.clone(), device_props);
                                    println!("[bluetooth-plugin] Device added: {}", path_string);
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
                            println!("[bluetooth-plugin] Processing InterfacesRemoved signal");
                            match msg.body::<(ObjectPath<'_>, Vec<String>)>() {
                                Ok((object_path, interfaces_removed)) => {
                                  let path_string = object_path.to_string();
                                  
                                  // Detectar remoción de adaptadores
                                  if interfaces_removed.contains(&"org.bluez.Adapter1".to_string()) {
                                    println!("[bluetooth-plugin] Adapter removed: {}", path_string);
                                    app.emit("bluetooth-change", BluetoothChange {
                                        change_type: "adapter-removed".to_string(),
                                        data: serde_json::json!({ "path": path_string.clone() }),
                                    }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-removed: {}", e));
                                  }
                                  
                                  // Detectar remoción de dispositivos
                                  if interfaces_removed.contains(&"org.bluez.Device1".to_string()) {
                                    println!("[bluetooth-plugin] Device removed: {}", path_string);
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
                            println!("[bluetooth-plugin] Processing PropertiesChanged signal");
                            if let Some(p_str) = path_opt_string {
                                match msg.body::<(String, HashMap<String, ZbusValue<'_>>, Vec<String>)>() {
                                    Ok((changed_interface_name, _changed_properties, _invalidated_properties)) => {
                                        println!("[bluetooth-plugin] PropertiesChanged for interface: {} on path: {}", changed_interface_name, p_str);
                                        
                                        if changed_interface_name == "org.bluez.Adapter1" {
                                            match get_adapter_state(p_str.clone()).await {
                                                Ok(adapter_info) => {
                                                    println!("[bluetooth-plugin] Adapter property changed: {}", p_str);
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
                                            match get_device_info(p_str.clone()).await {
                                                Ok(device_info) => {
                                                    println!("[bluetooth-plugin] Device property changed: {}", p_str);
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
                            println!("[bluetooth-plugin] Processing Device Disconnected signal");
                            if let Some(p_str) = path_opt_string {
                                match get_device_info(p_str.clone()).await {
                                    Ok(device_info) => {
                                        println!("[bluetooth-plugin] Device disconnected: {}", p_str);
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "device-disconnected".to_string(),
                                            data: serde_json::to_value(device_info).unwrap_or_default(),
                                        }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-disconnected: {}", e));
                                    }
                                    Err(e) => {
                                        eprintln!("[bluetooth-plugin] Error getting device info for disconnected device {}: {:?}", p_str, e);
                                    }
                                }
                            }
                        }
                        (Some("org.bluez.Device1"), Some("Connected")) => {
                            println!("[bluetooth-plugin] Processing Device Connected signal");
                            if let Some(p_str) = path_opt_string {
                                match get_device_info(p_str.clone()).await {
                                    Ok(device_info) => {
                                        println!("[bluetooth-plugin] Device connected: {}", p_str);
                                        app.emit("bluetooth-change", BluetoothChange {
                                            change_type: "device-connected".to_string(),
                                            data: serde_json::to_value(device_info).unwrap_or_default(),
                                        }).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-connected: {}", e));
                                    }
                                    Err(e) => {
                                        eprintln!("[bluetooth-plugin] Error getting device info for connected device {}: {:?}", p_str, e);
                                    }
                                }
                            }
                        }
                        _ => {
                            // Solo mostrar señales no manejadas ocasionalmente para evitar spam
                            if signal_count % 50 == 0 {
                                println!("[bluetooth-plugin] Unhandled BlueZ signal: interface={:?}, member={:?}", 
                                       interface_opt_string, member_opt_string);
                            }
                        }
                    }
                } else {
                    // Debug: mostrar señales de otros servicios muy ocasionalmente
                    if signal_count % 100 == 0 {
                        println!("[bluetooth-plugin] Non-BlueZ signal from: {:?}", sender_opt_str);
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
    println!("[bluetooth-plugin] D-Bus signal listener terminated after {} signals", signal_count);
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