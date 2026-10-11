//! Identificadores de compañía de Bluetooth SIG → nombre de fabricante.
//!
//! La tabla no vive acá: es dato, no código. Está vendorizada como
//! `data/company_identifiers.tsv` y `build.rs` la convierte en un array estático
//! ordenado dentro de `OUT_DIR`, que se incluye abajo. Así no hay un `.rs` de
//! cuatro mil tuplas que las herramientas lean como «duplicado», ni un generador
//! en otro lenguaje que mantener, y el runtime queda igual: búsqueda binaria
//! sobre un slice estático, sin costo de arranque ni dependencias de parseo.
//!
//! El `include!` trae `company_name(id: u16) -> Option<&'static str>` y la tabla
//! `COMPANIES`. Para actualizarla, reemplazar el `.tsv` (ver `build.rs`).

include!(concat!(env!("OUT_DIR"), "/company_identifiers_table.rs"));
