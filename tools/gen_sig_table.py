#!/usr/bin/env python3
"""Genera la tabla Rust de company identifiers de Bluetooth SIG desde la YAML.

Uso: python3 -I gen_sig_table.py <company_identifiers.yaml> <salida.rs> <fecha>

No importa PyYAML a propósito: el formato es regular y parsearlo a mano evita
una dependencia y cualquier sorpresa de un import desde el dir del dato.
"""
import re
import sys

src, out, fecha = sys.argv[1], sys.argv[2], sys.argv[3]

val_re = re.compile(r"^\s*-\s*value:\s*(0x[0-9A-Fa-f]+)\s*$")
name_re = re.compile(r"^\s*name:\s*(.*?)\s*$")

entries = {}
pending_id = None
with open(src, encoding="utf-8") as f:
    for line in f:
        m = val_re.match(line)
        if m:
            pending_id = int(m.group(1), 16)
            continue
        if pending_id is not None:
            nm = name_re.match(line)
            if nm:
                raw = nm.group(1)
                # YAML: quita comillas simples envolventes y des-escapa '' -> '
                if len(raw) >= 2 and raw[0] == "'" and raw[-1] == "'":
                    raw = raw[1:-1].replace("''", "'")
                elif len(raw) >= 2 and raw[0] == '"' and raw[-1] == '"':
                    raw = raw[1:-1]
                entries[pending_id] = raw
                pending_id = None

if not entries:
    sys.exit("no se parseó ninguna entrada; cambió el formato de la YAML")


def rust_escape(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


rows = "".join(
    f'    ({cid:#06x}, "{rust_escape(entries[cid])}"),\n'
    for cid in sorted(entries)
)

count = len(entries)
header = f"""//! Identificadores de compañía de Bluetooth SIG → nombre de fabricante.
//!
//! GENERADO automáticamente, no se edita a mano. Fuente: Bluetooth SIG Assigned
//! Numbers, `assigned_numbers/company_identifiers/company_identifiers.yaml`,
//! bajada el {fecha}. Para regenerar, volver a correr el generador del PR.
//!
//! La tabla está ordenada por id para resolver con búsqueda binaria sin ningún
//! costo de arranque ni dependencia de parseo.

/// El nombre del fabricante para un company ID de Bluetooth SIG, si está en la
/// lista. El id es el que BlueZ publica como clave de `ManufacturerData`.
pub(crate) fn company_name(id: u16) -> Option<&'static str> {{
    COMPANIES
        .binary_search_by_key(&id, |&(key, _)| key)
        .ok()
        .map(|index| COMPANIES[index].1)
}}

/// {count} fabricantes, ordenados por id.
static COMPANIES: &[(u16, &str)] = &[
{rows}];
"""

with open(out, "w", encoding="utf-8") as f:
    f.write(header)

print(f"{len(entries)} entradas -> {out}")
