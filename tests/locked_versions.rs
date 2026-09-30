//! Que el bloqueo no vuelva a quedarse en versiones con avisos de seguridad.
//!
//! Con `incompatible-rust-versions = "fallback"` (`.cargo/config.toml`), el
//! resolver baja a la última versión que entra en `rust-version`. Con 1.77.2
//! eso dejaba `time` en 0.3.41 y `serde_with` en 3.16.1, las dos con aviso de
//! Dependabot, y `cargo update` no las movía: los arreglos piden Rust 1.88. Si
//! alguien vuelve a bajar `rust-version`, estas pruebas lo dicen.

use std::path::PathBuf;

fn locked_version(name: &str) -> Vec<u64> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", path.display()));
    let header = format!("name = \"{name}\"\nversion = \"");
    let start = text
        .find(&header)
        .unwrap_or_else(|| panic!("{name} no está en Cargo.lock"))
        + header.len();
    let end = start + text[start..].find('"').expect("versión sin cerrar");
    text[start..end]
        .split('.')
        .map(|part| part.parse().expect("versión numérica"))
        .collect()
}

#[test]
fn time_esta_fuera_del_aviso_de_desborde_de_pila() {
    assert!(locked_version("time") >= vec![0, 3, 47]);
}

#[test]
fn serde_with_esta_fuera_del_aviso_de_keyvaluemap() {
    assert!(locked_version("serde_with") >= vec![3, 21, 0]);
}
