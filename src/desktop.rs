use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{plugin::PluginApi, AppHandle, Emitter, Manager, Runtime};
use zbus::{
    zvariant::{ObjectPath, Value as ZbusValue},
    Connection, MessageStream, MessageType, Proxy,
};

use crate::commands::{get_adapter_state, get_device_info};
use crate::models::*;
use crate::properties::{
    adapter_info_from_props, device_info_from_interfaces, Interfaces, ADAPTER_INTERFACE,
    BATTERY_INTERFACE, DEVICE_INTERFACE,
};
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
    let proxy = Proxy::new(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;

    match proxy.call_method("GetManagedObjects", &()).await {
        Ok(_) => println!("[bluetooth-plugin] Successfully connected to BlueZ ObjectManager"),
        Err(e) => {
            eprintln!(
                "[bluetooth-plugin] Failed to connect to BlueZ ObjectManager: {:?}",
                e
            );
            return Err(e.into());
        }
    }

    // Configurar filtros de señales más específicos
    let dbus_proxy = Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;

    // Agregar reglas de coincidencia para señales específicas.
    //
    // Las reglas filtran por la interfaz de la *señal*, no por la interfaz
    // cuyas propiedades cambian: un cambio de batería llega como
    // `PropertiesChanged` de `org.freedesktop.DBus.Properties` con
    // `org.bluez.Battery1` adentro del cuerpo, y la interfaz nueva llega como
    // `InterfacesAdded` del ObjectManager. Las dos ya están acá, así que la
    // batería no necesita una regla propia.
    let rules = vec![
        "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.ObjectManager'",
        "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties'",
        "type='signal',sender='org.bluez',interface='org.bluez.Adapter1'",
        "type='signal',sender='org.bluez',interface='org.bluez.Device1'",
    ];

    for rule in rules {
        match dbus_proxy.call_method("AddMatch", &(rule,)).await {
            Ok(_) => println!("[bluetooth-plugin] Added D-Bus match rule: {}", rule),
            Err(e) => eprintln!(
                "[bluetooth-plugin] Failed to add match rule '{}': {:?}",
                rule, e
            ),
        }
    }

    Ok(())
}

/// Emite un cambio hacia el frontend; si no se puede, lo deja anotado y sigue.
fn emit_change<R: Runtime>(app: &AppHandle<R>, change_type: &str, data: serde_json::Value) {
    app.emit(
        "bluetooth-change",
        BluetoothChange {
            change_type: change_type.to_string(),
            data,
        },
    )
    .unwrap_or_else(|e| eprintln!("[bluetooth-plugin] Failed to emit {}: {}", change_type, e));
}

/// Emite un error interno del plugin como evento.
fn emit_error<R: Runtime>(app: &AppHandle<R>, message: String) {
    emit_change(app, "error", serde_json::json!({ "message": message }));
}

/// Vuelve a leer el dispositivo y emite `device-property-changed` con lo que
/// haya ahora. Es lo que se hace cuando cambia una propiedad de `Device1` o de
/// `Battery1`, o cuando la batería aparece después de conectar.
async fn emit_device_property_changed<R: Runtime>(app: &AppHandle<R>, path: &str) {
    match get_device_info(path.to_string()).await {
        Ok(device_info) => emit_change(
            app,
            "device-property-changed",
            serde_json::to_value(device_info).unwrap_or_default(),
        ),
        Err(e) => {
            eprintln!(
                "[bluetooth-plugin] Error getting device info for {}: {:?}",
                path, e
            );
            emit_error(app, format!("Error getting device info: {:?}", e));
        }
    }
}

/// `InterfacesAdded`: apareció un adaptador, un dispositivo, o una interfaz
/// nueva sobre un objeto que ya existía.
async fn handle_interfaces_added<R: Runtime>(
    app: &AppHandle<R>,
    path: String,
    interfaces: &Interfaces,
) {
    // Detectar cambios de adaptadores
    if let Some(adapter_props) = interfaces.get(ADAPTER_INTERFACE) {
        let adapter_info = adapter_info_from_props(path.clone(), adapter_props);
        emit_change(
            app,
            "adapter-added",
            serde_json::to_value(adapter_info).unwrap_or_default(),
        );
    }

    // Detectar cambios de dispositivos. Si la batería viene en el mismo
    // mensaje, `device_info_from_interfaces` ya la toma.
    if let Some(device_info) = device_info_from_interfaces(path.clone(), interfaces) {
        emit_change(
            app,
            "device-added",
            serde_json::to_value(device_info).unwrap_or_default(),
        );
    } else if interfaces.contains_key(BATTERY_INTERFACE) {
        // BlueZ suma `Battery1` a un dispositivo que ya existía, un rato
        // después de conectarlo: el mensaje trae la batería sola, sin
        // `Device1`. Para el frontend es el mismo dispositivo con un dato
        // nuevo, así que se lo lee entero y se avisa como cambio de propiedad.
        emit_device_property_changed(app, &path).await;
    }
}

/// `InterfacesRemoved`: se fue un adaptador o un dispositivo.
fn handle_interfaces_removed<R: Runtime>(
    app: &AppHandle<R>,
    path: String,
    interfaces_removed: &[String],
) {
    if interfaces_removed.iter().any(|i| i == ADAPTER_INTERFACE) {
        emit_change(
            app,
            "adapter-removed",
            serde_json::json!({ "path": path.clone() }),
        );
    }

    if interfaces_removed.iter().any(|i| i == DEVICE_INTERFACE) {
        emit_change(app, "device-removed", serde_json::json!({ "path": path }));
    }
}

async fn run_signal_listener<R: Runtime>(conn: Connection, app: AppHandle<R>) {
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
    )
    .await
    {
        Ok(proxy) => {
            match proxy.call_method("GetNameOwner", &("org.bluez",)).await {
                Ok(reply) => {
                    if let Ok(unique_name) = reply.body().deserialize::<String>() {
                        bluez_unique_name = Some(unique_name);
                    }
                }
                Err(e) => eprintln!(
                    "[bluetooth-plugin] Failed to get org.bluez unique name: {:?}",
                    e
                ),
            }
            Some(proxy)
        }
        Err(_e) => None,
    };

    while let Some(msg_res) = stream.next().await {
        let msg = match msg_res {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!(
                    "[bluetooth-plugin] Error reading from D-Bus message stream: {:?}",
                    e
                );
                emit_change(
                    &app,
                    "dbus-error",
                    serde_json::json!({ "message": format!("D-Bus stream error: {:?}", e) }),
                );
                break;
            }
        };

        if msg.message_type() != MessageType::Signal {
            continue;
        }

        let header = msg.header();
        let sender_opt_str = header.sender().map(|s| s.to_string());

        let is_bluez_signal = sender_opt_str.as_deref() == Some("org.bluez")
            || (bluez_unique_name.is_some() && sender_opt_str == bluez_unique_name);

        if !is_bluez_signal {
            continue;
        }

        let interface_opt_string = header.interface().map(|i| i.as_str().to_string());
        let member_opt_string = header.member().map(|m| m.as_str().to_string());
        let path_opt_string = header.path().map(|p| p.as_str().to_string());

        match (
            interface_opt_string.as_deref(),
            member_opt_string.as_deref(),
        ) {
            (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesAdded")) => {
                match msg.body().deserialize::<(ObjectPath<'_>, Interfaces)>() {
                    Ok((object_path, interfaces)) => {
                        handle_interfaces_added(&app, object_path.to_string(), &interfaces).await;
                    }
                    Err(e) => {
                        eprintln!(
                            "[bluetooth-plugin] Error decoding InterfacesAdded body: {:?}",
                            e
                        );
                        emit_error(&app, format!("Error decoding InterfacesAdded: {:?}", e));
                    }
                }
            }
            (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesRemoved")) => {
                match msg.body().deserialize::<(ObjectPath<'_>, Vec<String>)>() {
                    Ok((object_path, interfaces_removed)) => {
                        handle_interfaces_removed(
                            &app,
                            object_path.to_string(),
                            &interfaces_removed,
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "[bluetooth-plugin] Error decoding InterfacesRemoved body: {:?}",
                            e
                        );
                        emit_error(&app, format!("Error decoding InterfacesRemoved: {:?}", e));
                    }
                }
            }
            (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
                let Some(p_str) = path_opt_string else {
                    eprintln!(
                        "[bluetooth-plugin] PropertiesChanged signal received without a valid path."
                    );
                    emit_error(&app, "PropertiesChanged signal without path".to_string());
                    continue;
                };

                let body = msg.body();
                let (changed_interface_name, changed_properties, _invalidated_properties) =
                    match body
                        .deserialize::<(String, HashMap<String, ZbusValue<'_>>, Vec<String>)>()
                    {
                        Ok(decoded) => decoded,
                        Err(e) => {
                            eprintln!(
                                "[bluetooth-plugin] Error decoding PropertiesChanged body: {:?}",
                                e
                            );
                            emit_error(&app, format!("Error decoding PropertiesChanged: {:?}", e));
                            continue;
                        }
                    };

                if changed_interface_name == ADAPTER_INTERFACE {
                    match get_adapter_state(p_str.clone()).await {
                        Ok(adapter_info) => emit_change(
                            &app,
                            "adapter-property-changed",
                            serde_json::to_value(adapter_info).unwrap_or_default(),
                        ),
                        Err(e) => {
                            eprintln!(
                                "[bluetooth-plugin] Error getting adapter state for {}: {:?}",
                                p_str, e
                            );
                            emit_error(&app, format!("Error getting adapter state: {:?}", e));
                        }
                    }
                } else if changed_interface_name == DEVICE_INTERFACE
                    || changed_interface_name == BATTERY_INTERFACE
                {
                    // Throttling logic for device properties.
                    //
                    // Un cambio de batería es un cambio más del dispositivo:
                    // no es crítico, así que entra por el mismo freno que el
                    // RSSI, y se avisa con el dispositivo entero releído.
                    let critical_keys =
                        ["Connected", "Paired", "Trusted", "Blocked", "Name", "Alias"];
                    let is_critical = changed_interface_name == DEVICE_INTERFACE
                        && changed_properties
                            .keys()
                            .any(|k| critical_keys.contains(&k.as_str()));

                    if !is_critical {
                        if let Some(last) = device_last_update.get(&p_str) {
                            if last.elapsed() < UPDATE_THROTTLE {
                                // Skip this update
                                continue;
                            }
                        }
                        device_last_update.insert(p_str.clone(), Instant::now());
                    }

                    emit_device_property_changed(&app, &p_str).await;
                }
            }
            (Some("org.bluez.Device1"), Some("Disconnected")) => {
                if let Some(p_str) = path_opt_string {
                    match get_device_info(p_str.clone()).await {
                        Ok(device_info) => emit_change(
                            &app,
                            "device-disconnected",
                            serde_json::to_value(device_info).unwrap_or_default(),
                        ),
                        Err(e) => {
                            eprintln!(
                                "[bluetooth-plugin] Error getting device info for disconnected device {}: {:?}",
                                p_str, e
                            );
                            emit_change(
                                &app,
                                "device-disconnected",
                                serde_json::json!({ "path": p_str }),
                            );
                        }
                    }
                }
            }
            (Some("org.bluez.Device1"), Some("Connected")) => {
                if let Some(p_str) = path_opt_string {
                    match get_device_info(p_str.clone()).await {
                        Ok(device_info) => emit_change(
                            &app,
                            "device-connected",
                            serde_json::to_value(device_info).unwrap_or_default(),
                        ),
                        Err(e) => {
                            eprintln!(
                                "[bluetooth-plugin] Error getting device info for connected device {}: {:?}",
                                p_str, e
                            );
                            emit_change(
                                &app,
                                "device-connected",
                                serde_json::json!({ "path": p_str, "connected": true }),
                            );
                        }
                    }
                }
            }
            _ => {}
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
