const COMMANDS: &[&str] = &[
    "list_adapters",
    "set_adapter_powered",
    "get_adapter_state",
    "start_scan",
    "stop_scan",
    "list_devices",
    "list_paired_devices",
    "connect_device",
    "disconnect_device",
    "get_device_info",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
