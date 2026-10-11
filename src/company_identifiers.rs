//! Identificadores de compañía de Bluetooth SIG → nombre de fabricante.
//!
//! La tabla es **dato, no código**: vive en `data/company_identifiers.tsv` (una
//! línea `0xID\tNombre` por fabricante) y se embebe en el binario con
//! `include_str!`. Se parsea una sola vez, perezosamente, a un mapa que se
//! cachea; de ahí en más la resolución es O(1). Los nombres son rebanadas de los
//! datos embebidos, así que no se copia ni se asigna texto por fabricante.
//!
//! Por qué `.tsv` y no un `.rs` con cuatro mil tuplas: un `.rs` generado de ese
//! tamaño es, para las herramientas de análisis, cuatro mil líneas «duplicadas»
//! (misma forma, otro literal), y además habría que mantener un generador
//! aparte. Como dato no se analiza ni se duplica, y el parseo —diez líneas— sí
//! lo cubren las pruebas. Para actualizar la tabla se reemplaza el `.tsv` desde
//! los Assigned Numbers de Bluetooth SIG
//! (`assigned_numbers/company_identifiers/company_identifiers.yaml`).

use std::collections::HashMap;
use std::sync::OnceLock;

/// El `.tsv` embebido, ya ordenado por id, tal como viene de Bluetooth SIG.
const RAW: &str = include_str!("../data/company_identifiers.tsv");

/// El mapa `id → nombre`, construido una sola vez a partir de [`RAW`].
fn table() -> &'static HashMap<u16, &'static str> {
    static TABLE: OnceLock<HashMap<u16, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        RAW.lines()
            .filter_map(|line| {
                let (id, name) = line.split_once('\t')?;
                let id = id.strip_prefix("0x").unwrap_or(id);
                let id = u16::from_str_radix(id, 16).ok()?;
                Some((id, name))
            })
            .collect()
    })
}

/// El nombre del fabricante para un company ID de Bluetooth SIG, si está en la
/// lista. El id es el que BlueZ publica como clave de `ManufacturerData`.
pub(crate) fn company_name(id: u16) -> Option<&'static str> {
    table().get(&id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resuelve_fabricantes_conocidos() {
        assert_eq!(company_name(0x0006), Some("Microsoft"));
        assert_eq!(company_name(0x004C), Some("Apple, Inc."));
    }

    #[test]
    fn un_id_que_no_esta_no_da_nombre() {
        // 0xF000 no está asignado (el id más alto de la tabla es 0x1130).
        assert_eq!(company_name(0xF000), None);
    }

    #[test]
    fn la_tabla_se_parsea_entera_y_sin_colisiones() {
        // Tantas entradas en el mapa como líneas no vacías en el dato: si una
        // línea no parsea o dos comparten id, el conteo no cerraría.
        let lineas = RAW.lines().filter(|l| !l.trim().is_empty()).count();
        assert_eq!(table().len(), lineas);
    }
}
