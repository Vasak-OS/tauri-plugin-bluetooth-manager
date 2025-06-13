use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

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
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("bluetooth-manager")
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
    ])
    .setup(|app, api| {
      #[cfg(desktop)]
      let bluetooth_manager = desktop::init(app, api)?;
      app.manage(bluetooth_manager);
      Ok(())
    })
    .build()
}
