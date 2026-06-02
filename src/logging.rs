use std::sync::OnceLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use std::path::PathBuf;

static INIT_LOGGING: OnceLock<()> = OnceLock::new();

pub fn init_logging() {
    INIT_LOGGING.get_or_init(|| {
        let log_file_path = get_log_path();

        // Crear directorio si no existe
        if let Some(parent) = log_file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .expect("Failed to open log file");

        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("tauri_plugin_bluetooth_manager=info"));

        let file_layer = fmt::layer()
            .with_writer(file)
            .with_target(true)
            .with_thread_ids(false)
            .with_ansi(false)
            .pretty()
            .boxed();

        let stdout_layer = fmt::layer()
            .with_writer(std::io::stdout)
            .with_target(true)
            .with_ansi(true)
            .boxed();

        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(stdout_layer)
            .init();
    });
}

fn get_log_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".logs/vasak/bluetooth.log")
}
