use tauri::{
    async_runtime,
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

mod commands;
mod desktop;
mod error;
mod models;

pub use error::{Error, Result};

use desktop::BluetoothManager;

pub trait BluetoothManagerExt<R: Runtime> {
    fn bluetooth_manager(&self) -> &BluetoothManager;
}

impl<R: Runtime, T: Manager<R>> BluetoothManagerExt<R> for T {
    fn bluetooth_manager(&self) -> &BluetoothManager {
        self.state::<BluetoothManager>().inner()
    }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R>::new("bluetooth-manager")
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
            commands::get_device_info,
            commands::bluetooth_plugin_status,
        ])
        .setup(|app_handle, api| {
            let result = async_runtime::block_on(desktop::init(app_handle.clone(), api));
            let initialized = result.is_ok();
            if let Some(manager) = app_handle.try_state::<desktop::BluetoothManager>() {
                let mut guard = manager.inner().initialized.lock().unwrap();
                *guard = initialized;
            }
            if let Err(e) = result {
                eprintln!("Bluetooth service not available: {e}");
            }
            Ok(())
        })
        .build()
}
