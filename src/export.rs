//! Organización de los ficheros que salen de la app.
//!
//! Todo queda bajo `export/<empresa>/...` y separado por tipo y por persona:
//! cada cliente tiene su carpeta de facturas, presupuestos o proformas, y cada
//! empleado la suya de nóminas. Así se puede abrir la carpeta de un cliente o
//! de un trabajador sin mezclar documentos de terceros.

use crate::empleados::{Empleado, Nomina};
use crate::model::{Documento, Empresa};
use std::path::PathBuf;

/// Quita lo que Windows no admite en un nombre de carpeta y deja el nombre
/// legible, con acentos y mayúsculas. No se usa para el nombre del fichero:
/// para eso está `pdf::slug`, que además lo deja sin acentos y en minúsculas.
pub fn nombre_visible(texto: &str) -> String {
    let mut limpio = String::with_capacity(texto.len());
    let mut espacio = false;
    for c in texto.trim().chars() {
        let c = if c.is_control()
            || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        {
            ' '
        } else {
            c
        };
        if c.is_whitespace() {
            if !limpio.is_empty() && !espacio {
                limpio.push(' ');
                espacio = true;
            }
        } else {
            limpio.push(c);
            espacio = false;
        }
    }
    let recortado = limpio
        .trim()
        .trim_matches(|c: char| c == '.')
        .trim();
    let mut salida: String = recortado.chars().take(64).collect();
    while salida.ends_with(' ') || salida.ends_with('.') {
        salida.pop();
    }
    if es_nombre_reservado(&salida) {
        salida.push_str("-carpeta");
    }
    salida
}

fn es_nombre_reservado(nombre: &str) -> bool {
    let base = nombre
        .split('.')
        .next()
        .unwrap_or(nombre)
        .to_ascii_uppercase();
    matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5"
            | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4"
            | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

/// Nombre de la carpeta raíz de la empresa. Si no hay nombre se usa el NIF y,
/// como última red, un nombre fijo.
pub fn segmento_empresa(empresa: &Empresa) -> String {
    let nombre = nombre_visible(&empresa.nombre);
    if !nombre.is_empty() {
        return nombre;
    }
    let nif = nombre_visible(&empresa.nif);
    if !nif.is_empty() {
        return format!("Empresa-{nif}");
    }
    "Mi-empresa".to_string()
}

/// Nombre de la carpeta del cliente dentro de su tipo de documento.
pub fn segmento_cliente(doc: &Documento) -> String {
    let nombre = nombre_visible(&doc.cliente.nombre_completo());
    if !nombre.is_empty() {
        return nombre;
    }
    let id = if doc.cliente_id != 0 {
        doc.cliente_id
    } else {
        doc.id
    };
    format!("Cliente-{id}")
}

/// Nombre de la carpeta del empleado dentro de `Nominas`.
pub fn segmento_empleado(empleado: &Empleado) -> String {
    let nombre = nombre_visible(&empleado.nombre_completo());
    if !nombre.is_empty() {
        return nombre;
    }
    format!("Empleado-{}", empleado.id)
}

/// Carpeta raíz de las exportaciones.
pub fn raiz() -> PathBuf {
    crate::store::dir_base().join("export")
}

/// Carpeta de todo lo de una empresa concreta.
pub fn raiz_empresa(empresa: &Empresa) -> PathBuf {
    raiz().join(segmento_empresa(empresa))
}

/// `export/<empresa>/Facturas/<cliente>` (o Presupuestos/Proformas).
pub fn dir_documento(doc: &Documento, empresa: &Empresa) -> PathBuf {
    raiz_empresa(empresa)
        .join(carpeta_tipo(doc))
        .join(segmento_cliente(doc))
}

pub fn carpeta_tipo(doc: &Documento) -> &'static str {
    match doc.tipo {
        crate::model::TipoDoc::Factura => "Facturas",
        crate::model::TipoDoc::Presupuesto => "Presupuestos",
        crate::model::TipoDoc::Proforma => "Proformas",
    }
}

/// `export/<empresa>/Nominas/<empleado>`.
pub fn dir_nomina(empresa: &Empresa, empleado: &Empleado) -> PathBuf {
    raiz_empresa(empresa)
        .join("Nominas")
        .join(segmento_empleado(empleado))
}

/// `export/<empresa>/Modelos`.
pub fn dir_modelos(empresa: &Empresa) -> PathBuf {
    raiz_empresa(empresa).join("Modelos")
}

/// `export/<empresa>/Contabilidad`.
pub fn dir_contabilidad(empresa: &Empresa) -> PathBuf {
    raiz_empresa(empresa).join("Contabilidad")
}

fn slug_empresa(empresa: &Empresa) -> String {
    let slug = crate::pdf::slug(&segmento_empresa(empresa));
    if slug.is_empty() {
        "empresa".to_string()
    } else {
        slug
    }
}

fn slug_empleado(empleado: &Empleado) -> String {
    let slug = crate::pdf::slug(&segmento_empleado(empleado));
    if slug.is_empty() {
        format!("empleado-{}", empleado.id)
    } else {
        slug
    }
}

/// `Nomina-2026-01-0003-ana-garcia-ruiz.pdf`.
pub fn nombre_nomina(n: &Nomina, empleado: &Empleado) -> String {
    format!(
        "Nomina-{:04}-{:02}-{:04}-{}.pdf",
        n.anyo,
        n.mes,
        n.id,
        slug_empleado(empleado)
    )
}

/// `Modelo-303-2026-1T-estudio-nandu-s-l.pdf` o `.json`.
pub fn nombre_modelo(codigo: &str, anyo: i32, tri: u32, extension: &str, empresa: &Empresa) -> String {
    let periodo = if tri == 0 {
        "anual".to_string()
    } else {
        format!("{tri}T")
    };
    format!(
        "Modelo-{codigo}-{anyo}-{periodo}-{}.{}",
        slug_empresa(empresa),
        extension
    )
}

/// `Libro-2026-estudio-nandu-s-l.csv`.
pub fn nombre_libro(anyo: i32, empresa: &Empresa) -> String {
    format!("Libro-{anyo}-{}.csv", slug_empresa(empresa))
}

/// `Informe-2026-estudio-nandu-s-l.txt`.
pub fn nombre_informe(anyo: i32, empresa: &Empresa) -> String {
    format!("Informe-{anyo}-{}.txt", slug_empresa(empresa))
}

/// `datos-estudio-nandu-s-l-20261112-143000.json`.
pub fn nombre_copia(empresa: &Empresa, sello: &str) -> String {
    format!("datos-{}-{sello}.json", slug_empresa(empresa))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cliente, Documento, Empresa, TipoDoc};

    fn empresa() -> Empresa {
        Empresa {
            nombre: "Estudio Ñandú S.L.".into(),
            nif: "B12345678".into(),
            ..Default::default()
        }
    }

    fn factura() -> Documento {
        let cliente = Cliente {
            id: 7,
            nombre: "Agencia Creativa S.L.".into(),
            ..Default::default()
        };
        Documento::nuevo(TipoDoc::Factura, cliente, 0, "2026".into())
    }

    #[test]
    fn deja_los_nombres_legibles_y_sin_caracteres_prohibidos() {
        assert_eq!(nombre_visible("  Estudio Ñandú S.L.  "), "Estudio Ñandú S.L");
        assert_eq!(nombre_visible("A/B:C*?\"<>|"), "A B C");
        assert_eq!(nombre_visible(".."), "");
        assert_eq!(nombre_visible("CON"), "CON-carpeta");
    }

    #[test]
    fn organiza_por_empresa_tipo_y_cliente() {
        let p = dir_documento(&factura(), &empresa());
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "Agencia Creativa S.L");
        assert_eq!(
            p.parent().unwrap().file_name().unwrap().to_string_lossy(),
            "Facturas"
        );
        assert_eq!(
            p.parent()
                .unwrap()
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "Estudio Ñandú S.L"
        );
    }

    #[test]
    fn los_modelos_y_listados_llevan_la_empresa() {
        assert_eq!(
            nombre_modelo("303", 2026, 1, "pdf", &empresa()),
            "Modelo-303-2026-1T-estudio-nandu-s-l.pdf"
        );
        assert_eq!(
            nombre_libro(2026, &empresa()),
            "Libro-2026-estudio-nandu-s-l.csv"
        );
        assert_eq!(
            nombre_informe(2026, &empresa()),
            "Informe-2026-estudio-nandu-s-l.txt"
        );
    }
}
