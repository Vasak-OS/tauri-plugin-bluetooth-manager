use tauri::{
  AppHandle,
  Runtime,
  plugin::{Builder, TauriPlugin, PluginApi}, // Removed Setup, PluginApi should be sufficient
  Manager,
};

// Define a placeholder Config struct. Replace with your actual config if needed.
#[derive(Default, serde::Deserialize, Clone)]
pub struct Config {}

pub use models::*;

#[cfg(desktop)]
mod desktop;
mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::BluetoothManager;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the bluetooth-manager APIs.
pub trait BluetoothManagerExt<R: Runtime> {
  fn bluetooth_manager(&self) -> &BluetoothManager<R>;
}

impl<R: Runtime, T: Manager<R>> crate::BluetoothManagerExt<R> for T {
  fn bluetooth_manager(&self) -> &BluetoothManager<R> {
    self.state::<BluetoothManager<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime + Clone>(
    app: AppHandle<R>,
    api: PluginApi<R, Config>, // desktop::init will now take PluginApi
) -> crate::Result<BluetoothManager<R>> {
    #[cfg(desktop)]
    let bluetooth_manager = tauri::async_runtime::block_on(desktop::init(app.clone(), api))?;

    app.manage(bluetooth_manager.clone());

    Ok(bluetooth_manager)
}

// Tauri plugin builder
pub fn init_plugin<R: Runtime + Clone>(_app: AppHandle<R>) -> TauriPlugin<R, Config> { // Added R: Clone
    Builder::<R, Config>::new("bluetooth-manager")
        .invoke_handler(tauri::generate_handler![
            commands::list_adapters,
            commands::set_adapter_powered,
            commands::get_adapter_state,
            commands::start_scan,
            commands::stop_scan,
            commands::list_devices,
            commands::list_paired_devices,
            commands::connect_device,
            commands::disconnect_device,
            commands::get_device_info
            // ping is a method on BluetoothManager, not a global command here
        ])
        .setup(|app_handle, api| {
            init(app_handle.clone(), api)?; // Pass PluginApi directly
            Ok(())
        })
        .build()
}