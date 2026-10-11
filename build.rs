use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

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
    "bluetooth_plugin_status",
];

/// Ruta de la tabla de fabricantes de Bluetooth SIG, relativa al crate.
const COMPANY_TABLE: &str = "data/company_identifiers.tsv";

/// Genera la tabla de identificadores de compañía de Bluetooth SIG a partir del
/// `.tsv` vendorizado, hacia `OUT_DIR`, para que `company_identifiers.rs` la
/// incluya con `include!`.
///
/// El dato vive como `.tsv` y no como un `.rs` con cuatro mil tuplas a propósito:
/// un `.rs` generado de ese tamaño es, para las herramientas de análisis, cuatro
/// mil líneas «duplicadas» (misma forma, otro literal), y además había que
/// mantener un generador aparte. Como `.tsv` es dato, no código: no se analiza,
/// no se duplica, y el array estático termina igual en el binario —búsqueda
/// binaria, costo cero de arranque, sin dependencias de parseo en runtime—.
///
/// Para actualizar la tabla se reemplaza el `.tsv` (una línea `0xID\tNombre` por
/// fabricante, ordenada por id) desde los Assigned Numbers de Bluetooth SIG
/// (`assigned_numbers/company_identifiers/company_identifiers.yaml`).
fn generate_company_table() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let tsv_path = Path::new(&manifest).join(COMPANY_TABLE);
    let tsv = fs::read_to_string(&tsv_path)
        .unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", tsv_path.display()));

    let mut rows: Vec<(u16, &str)> = tsv
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (id, name) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("línea sin tabulador en {}: {line:?}", COMPANY_TABLE));
            let id = id.strip_prefix("0x").unwrap_or(id);
            let id = u16::from_str_radix(id, 16).unwrap_or_else(|e| {
                panic!("id no hexadecimal en {}: {line:?} ({e})", COMPANY_TABLE)
            });
            (id, name)
        })
        .collect();
    rows.sort_by_key(|&(id, _)| id);

    let mut out = String::with_capacity(rows.len() * 48 + 512);
    out.push_str(
        "// GENERADO por build.rs desde data/company_identifiers.tsv. No editar.\n\n\
         /// El nombre del fabricante para un company ID de Bluetooth SIG, si está\n\
         /// en la lista. El id es el que BlueZ publica como clave de\n\
         /// `ManufacturerData`.\n\
         pub(crate) fn company_name(id: u16) -> Option<&'static str> {\n\
         \x20   COMPANIES\n\
         \x20       .binary_search_by_key(&id, |&(key, _)| key)\n\
         \x20       .ok()\n\
         \x20       .map(|index| COMPANIES[index].1)\n\
         }\n\n",
    );
    let _ = write!(
        out,
        "/// {} fabricantes de Bluetooth SIG, ordenados por id.\n\
         static COMPANIES: &[(u16, &str)] = &[\n",
        rows.len()
    );
    for (id, name) in &rows {
        // Escape de Rust: sólo `\` y `"` pueden aparecer en un nombre y romper el
        // literal; el `.tsv` ya tiene los tabuladores fuera de los nombres.
        let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(out, "    ({id:#06x}, \"{escaped}\"),");
    }
    out.push_str("];\n");

    let dest =
        Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("company_identifiers_table.rs");
    fs::write(&dest, out).unwrap_or_else(|e| panic!("no se pudo escribir {}: {e}", dest.display()));

    println!("cargo:rerun-if-changed={COMPANY_TABLE}");
}

fn main() {
    generate_company_table();

    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
