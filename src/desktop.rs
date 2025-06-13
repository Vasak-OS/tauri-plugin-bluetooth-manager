use futures::StreamExt;
use tauri::{plugin::PluginApi, AppHandle, Manager, Runtime}; // Manager y PluginApi pueden no ser necesarios
use zbus::{Connection, MessageStream, message::MessageType, zvariant::{ObjectPath, OwnedValue, Value as ZbusValue}};
use std::collections::HashMap;
use std::convert::TryInto; // Para .try_into()

use crate::models::*;
use crate::{Error, Result as CrateResult}; // Renombrar Result para evitar conflicto con std::result::Result
use crate::commands::{get_adapter_state, get_device_info};

// Asumiendo que PingRequest y PingResponse están definidos en models.rs o en otro lugar
// Si no, necesitarás definirlos o eliminar la función ping.
// pub struct PingRequest { pub value: Option<String> }
// pub struct PingResponse { pub value: Option<String> }


pub struct BluetoothManager<R: Runtime> {
  conn: Connection,
  app: AppHandle<R>,
}

pub fn init<R: Runtime>(app: &tauri::AppHandle<R>, _api: &tauri::PluginSetup<R>) -> CrateResult<BluetoothManager<R>> {
  // Conectar al bus del sistema
  let conn = Connection::system().blocking_or_else(|e| Error::Zbus(e))?; // Usar blocking_or_else para contexto síncrono
                                                                          // o hacer init async
  let manager = BluetoothManager {
    conn: conn.clone(),
    app: app.clone(),
  };
  // Spawn tarea de escucha de señales
  // Asegúrate de que tokio esté en tu Cargo.toml y tengas una feature como "rt-multi-thread"
  #[cfg(feature = "tokio-runtime")] // Ejemplo de cómo podrías manejar esto
  tokio::spawn(run_signal_listener(conn, app.clone()));
  #[cfg(not(feature = "tokio-runtime"))]
  app.run_on_main_thread(move || { // Alternativa si no usas tokio directamente para spawnear
      let _ = tauri::async_runtime::spawn(run_signal_listener(conn, app.clone()));
  }).map_err(|e| Error::CommandError(format!("Failed to spawn listener: {}", e)))?;


  Ok(manager)
}

fn helper_adapter_info_from_props(path: String, props: &HashMap<String, OwnedValue>) -> AdapterInfo {
    AdapterInfo {
        path,
        address: props.get("Address").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        name: props.get("Name").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        alias: props.get("Alias").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        class: props.get("Class").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        powered: props.get("Powered").and_then(|v| v.try_into().ok()).unwrap_or(false),
        discoverable: props.get("Discoverable").and_then(|v| v.try_into().ok()).unwrap_or(false),
        discoverable_timeout: props.get("DiscoverableTimeout").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        pairable: props.get("Pairable").and_then(|v| v.try_into().ok()).unwrap_or(false),
        pairable_timeout: props.get("PairableTimeout").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        discovering: props.get("Discovering").and_then(|v| v.try_into().ok()).unwrap_or(false),
        uuids: props.get("UUIDs")
            .and_then(|v| v.try_into().ok())
            .unwrap_or_default(),
        modalias: props.get("Modalias").and_then(|v| v.try_into().ok()),
    }
}

fn helper_device_info_from_props(path: String, props: &HashMap<String, OwnedValue>) -> DeviceInfo {
    DeviceInfo {
        path,
        address: props.get("Address").and_then(|v| v.try_into().ok()).unwrap_or_default(),
        name: props.get("Name").and_then(|v| v.try_into().ok()),
        alias: props.get("Alias").and_then(|v| v.try_into().ok()),
        class: props.get("Class").and_then(|v| v.try_into().ok()),
        appearance: props.get("Appearance").and_then(|v| v.try_into().ok()),
        icon: props.get("Icon").and_then(|v| v.try_into().ok()),
        paired: props.get("Paired").and_then(|v| v.try_into().ok()).unwrap_or(false),
        trusted: props.get("Trusted").and_then(|v| v.try_into().ok()).unwrap_or(false),
        blocked: props.get("Blocked").and_then(|v| v.try_into().ok()).unwrap_or(false),
        legacy_pairing: props.get("LegacyPairing").and_then(|v| v.try_into().ok()).unwrap_or(false),
        rssi: props.get("RSSI").and_then(|v| v.try_into().ok()),
        tx_power: props.get("TxPower").and_then(|v| v.try_into().ok()),
        connected: props.get("Connected").and_then(|v| v.try_into().ok()).unwrap_or(false),
        uuids: props.get("UUIDs")
            .and_then(|v| v.try_into().ok())
            .unwrap_or_default(),
        adapter: props.get("Adapter")
            .and_then(|v| v.try_into::<ObjectPath>().ok())
            .map(|p| p.to_string())
            .unwrap_or_default(),
        services_resolved: props.get("ServicesResolved").and_then(|v| v.try_into().ok()).unwrap_or(false),
    }
}

async fn run_signal_listener<R: Runtime>(conn: Connection, app: AppHandle<R>) {
  println!("[bluetooth-plugin] Initializing D-Bus signal listener...");
  let mut stream = MessageStream::from(conn);
  println!("[bluetooth-plugin] MessageStream created. Waiting for signals...");

  while let Some(msg_res) = stream.next().await {
    match msg_res {
      Ok(msg) => {
        if msg.message_type() == MessageType::Signal && msg.sender().as_deref() == Some("org.bluez") {
          let path_str_opt = msg.path().map(|p| p.to_string());
          let interface_name_opt = msg.interface().map(|s| s.to_string());
          let member_name_opt = msg.member().map(|s| s.to_string());

          match (interface_name_opt.as_deref(), member_name_opt.as_deref()) {
            (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesAdded")) => {
              match msg.body::<(ObjectPath<'_>, HashMap<String, HashMap<String, OwnedValue>>)>() {
                Ok((object_path, interfaces_and_properties)) => {
                  let path_string = object_path.to_string();
                  if let Some(adapter_props) = interfaces_and_properties.get("org.bluez.Adapter1") {
                    let adapter_info = helper_adapter_info_from_props(path_string.clone(), adapter_props);
                    app.emit_all("adapter-added", adapter_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-added: {}", e));
                  }
                  if let Some(device_props) = interfaces_and_properties.get("org.bluez.Device1") {
                    let device_info = helper_device_info_from_props(path_string, device_props);
                    app.emit_all("device-added", device_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-added: {}", e));
                  }
                }
                Err(e) => {
                  eprintln!("[bluetooth-plugin] Error decoding InterfacesAdded body: {:?}, signature: {:?}", e, msg.body_signature());
                  app.emit_all("bluetooth-error", format!("Error decoding InterfacesAdded body: {:?}", e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                }
              }
            }
            (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesRemoved")) => {
              match msg.body::<(ObjectPath<'_>, Vec<String>)>() {
                Ok((object_path, interfaces_removed)) => {
                  let path_string = object_path.to_string();
                  if interfaces_removed.contains(&"org.bluez.Adapter1".to_string()) {
                    app.emit_all("adapter-removed", path_string.clone()).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-removed: {}", e));
                  }
                  if interfaces_removed.contains(&"org.bluez.Device1".to_string()) {
                    app.emit_all("device-removed", path_string).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-removed: {}", e));
                  }
                }
                Err(e) => {
                  eprintln!("[bluetooth-plugin] Error decoding InterfacesRemoved body: {:?}, signature: {:?}", e, msg.body_signature());
                  app.emit_all("bluetooth-error", format!("Error decoding InterfacesRemoved body: {:?}", e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                }
              }
            }
            (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
              if let Some(p_str) = path_str_opt {
                match msg.body::<(String, HashMap<String, ZbusValue<'_>>, Vec<String>)>() {
                  Ok((changed_interface_name, _changed_properties, _invalidated_properties)) => {
                    if changed_interface_name == "org.bluez.Adapter1" {
                      match get_adapter_state(p_str.clone()).await {
                        Ok(adapter_info) => {
                          app.emit_all("adapter-property-changed", adapter_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit adapter-property-changed: {}", e));
                        }
                        Err(e) => {
                          eprintln!("[bluetooth-plugin] Error getting adapter state for {}: {:?}", p_str, e);
                          app.emit_all("bluetooth-error", format!("Error getting adapter state for {}: {:?}", p_str, e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                        }
                      }
                    } else if changed_interface_name == "org.bluez.Device1" {
                      match get_device_info(p_str.clone()).await {
                        Ok(device_info) => {
                          app.emit_all("device-property-changed", device_info).unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit device-property-changed: {}", e));
                        }
                        Err(e) => {
                          eprintln!("[bluetooth-plugin] Error getting device info for {}: {:?}", p_str, e);
                          app.emit_all("bluetooth-error", format!("Error getting device info for {}: {:?}", p_str, e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                        }
                      }
                    }
                  }
                  Err(e) => {
                    eprintln!("[bluetooth-plugin] Error decoding PropertiesChanged body: {:?}, signature: {:?}", e, msg.body_signature());
                    app.emit_all("bluetooth-error", format!("Error decoding PropertiesChanged body: {:?}", e)).unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
                  }
                }
              } else {
                eprintln!("[bluetooth-plugin] PropertiesChanged signal received without a valid path.");
                app.emit_all("bluetooth-error", "PropertiesChanged signal without path").unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-error: {}", err));
              }
            }
            _ => { /* No relevante */ }
          }
        }
      }
      Err(e) => {
        eprintln!("[bluetooth-plugin] Error reading from D-Bus message stream: {:?}", e);
        app.emit_all("bluetooth-dbus-error", format!("D-Bus stream error: {:?}", e))
          .unwrap_or_else(|err| eprintln!("[bluetooth-plugin] Failed to emit bluetooth-dbus-error: {}", err));
        break;
      }
    }
  }
  println!("[bluetooth-plugin] D-Bus signal listener terminated.");
}


impl<R: Runtime> BluetoothManager<R> {
  // Asumiendo que PingRequest y PingResponse están definidos en models.rs
  pub fn ping(&self, payload: crate::models::PingRequest) -> CrateResult<crate::models::PingResponse> {
    Ok(crate::models::PingResponse {
      value: payload.value,
    })
  }
}

impl<R: Runtime> BluetoothManager<R> {
  pub fn ping(&self, payload: PingRequest) -> crate::Result<PingResponse> {
    Ok(PingResponse {
      value: payload.value,
    })
  }
}
