use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use std::path::PathBuf;

pub fn init_logging() {
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

    let env_filter = if cfg!(debug_assertions) {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("bluetooth_manager=info"))
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("bluetooth_manager=error"))
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(file)
                .with_target(true)
                .with_thread_ids(false)
                .pretty()
        )
        .init();
}

fn get_log_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".logs/vasak/bluetooth.log")
}
