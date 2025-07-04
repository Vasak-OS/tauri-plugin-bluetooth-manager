use futures::StreamExt;
use tauri::{plugin::PluginApi, AppHandle, Emitter, Manager, Runtime};
use zbus::{
    zvariant::{ObjectPath, OwnedValue, Value as ZbusValue},
    Connection, MessageStream, MessageType,
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

    tauri::async_runtime::spawn(run_signal_listener(conn, app));

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
    let mut stream = MessageStream::from(conn);
    println!("[bluetooth-plugin] MessageStream created. Waiting for signals...");

    while let Some(msg_res) = stream.next().await {
        match msg_res {
          Ok(msg) => {
            let header = match msg.header() {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("[bluetooth-plugin] Failed to get message header: {:?}", e);
                    continue;
                }
            };

            let sender_res_opt_ref = header.sender();
            let sender_opt_str = match sender_res_opt_ref {
                Ok(Some(unique_name_ref)) => Some(unique_name_ref.as_str()),
                Ok(None) => None,
                Err(e) => {
                    eprintln!("[bluetooth-plugin] Error getting sender from header: {:?}", e);
                    None
                }
            };
            
            if msg.message_type() == MessageType::Signal && sender_opt_str == Some("org.bluez") {
              let path_opt_string = match header.path() {
                Ok(Some(p_ref)) => Some(p_ref.as_str().to_string()),
                Ok(None) => None,
                Err(e) => {
                    eprintln!("[bluetooth-plugin] Error getting path from header: {:?}", e);
                    None
                }
              };
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

              match (interface_opt_string.as_deref(), member_opt_string.as_deref()) {
            (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesAdded")) => {
              match msg.body::<(ObjectPath<'_>, HashMap<String, HashMap<String, OwnedValue>>)>() {
                Ok((object_path, interfaces_and_properties)) => {
                  let path_string = object_path.to_string();
                  if let Some(adapter_props) = interfaces_and_properties.get("org.bluez.Adapter1") {
                    let adapter_info = helper_adapter_info_from_props(path_string.clone(), adapter_props);
                    app.emit("adapter-added", adapter_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-added: {}", e));
                  }
                  if let Some(device_props) = interfaces_and_properties.get("org.bluez.Device1") {
                    let device_info = helper_device_info_from_props(path_string, device_props);
                    app.emit("device-added", device_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-added: {}", e));
                  }
                }
                Err(e) => {
                  eprintln!("[bluetooth-plugin] Error decoding InterfacesAdded body: {:?}, signature: {:?}", e, msg.body_signature());
                  app.emit("bluetooth-error", format!("Error decoding InterfacesAdded body: {:?}", e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                }
              }
            }
            (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesRemoved")) => {
              match msg.body::<(ObjectPath<'_>, Vec<String>)>() {
                Ok((object_path, interfaces_removed)) => {
                  let path_string = object_path.to_string();
                  if interfaces_removed.contains(&"org.bluez.Adapter1".to_string()) {
                    app.emit("adapter-removed", path_string.clone()).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-removed: {}", e));
                  }
                  if interfaces_removed.contains(&"org.bluez.Device1".to_string()) {
                    app.emit("device-removed", path_string).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-removed: {}", e));
                  }
                }
                Err(e) => {
                  eprintln!("[bluetooth-plugin] Error decoding InterfacesRemoved body: {:?}, signature: {:?}", e, msg.body_signature());
                  app.emit("bluetooth-error", format!("Error decoding InterfacesRemoved body: {:?}", e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                }
              }
            }
            (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
              if let Some(p_str) = path_opt_string { // p_str is String
                match msg.body::<(String, HashMap<String, ZbusValue<'_>>, Vec<String>)>() {
                  Ok((changed_interface_name, _changed_properties, _invalidated_properties)) => {
                    if changed_interface_name == "org.bluez.Adapter1" {
                      match get_adapter_state(p_str.clone()).await {
                        Ok(adapter_info) => {
                          app.emit("adapter-property-changed", adapter_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-property-changed: {}", e));
                        }
                        Err(e) => {
                          eprintln!("[bluetooth-plugin] Error getting adapter state for {}: {:?}", p_str, e);
                          app.emit("bluetooth-error", format!("Error getting adapter state for {}: {:?}", p_str, e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                        }
                      }
                    } else if changed_interface_name == "org.bluez.Device1" {
                      match get_device_info(p_str.clone()).await {
                        Ok(device_info) => {
                          app.emit("device-property-changed", device_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-property-changed: {}", e));
                        }
                        Err(e) => {
                          eprintln!("[bluetooth-plugin] Error getting device info for {}: {:?}", p_str, e);
                          app.emit("bluetooth-error", format!("Error getting device info for {}: {:?}", p_str, e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                        }
                      }
                    }
                  }
                  Err(e) => {
                    eprintln!("[bluetooth-plugin] Error decoding PropertiesChanged body: {:?}, signature: {:?}", e, msg.body_signature());
                    app.emit("bluetooth-error", format!("Error decoding PropertiesChanged body: {:?}", e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                  }
                }
              } else {
                eprintln!("[bluetooth-plugin] PropertiesChanged signal received without a valid path (or error fetching path).");
                app.emit("bluetooth-error", "PropertiesChanged signal without path").unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
              }
            }
            _ => { /* No relevante */ }
          }
                }
            }
            Err(e) => {
                eprintln!(
                    "[bluetooth-plugin] Error reading from D-Bus message stream: {:?}",
                    e
                );
                app.emit(
                    "bluetooth-dbus-error",
                    format!("D-Bus stream error: {:?}", e),
                )
                .unwrap_or_else(|err| {
                    eprintln!(
                        "[bluetooth-plugin] Failed to emit bluetooth-dbus-error: {}",
                        err
                    )
                });
                break;
            }
        }
    }
    println!("[bluetooth-plugin] D-Bus signal listener terminated.");
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