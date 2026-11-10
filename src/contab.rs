//! Contabilidad sencilla para autónomo y pequeño negocio.
//!
//! No es un programa de contabilidad: calcula lo necesario para las
//! declaraciones trimestrales (IVA del modelo 303 e IRPF del modelo 130) a
//! partir de lo que ya hay en la app —las facturas emitidas— y de los gastos
//! que se registran aquí.
//!
//! Criterio de devengo: se cuenta por la **fecha de la factura o del gasto**,
//! que es el que se usa en esos modelos, no por la fecha de cobro.

use crate::model::*;
use crate::store::Db;
use chrono::{Datelike, NaiveDate};

/// Trimestre natural (1-4) de una fecha.
pub fn trimestre(fecha: NaiveDate) -> u32 {
    ((fecha.month() - 1) / 3) + 1
}

/// `trimestre` = 0 significa "todo el año".
pub fn en_rango(fecha: NaiveDate, anyo: i32, trimestre: u32) -> bool {
    fecha.year() == anyo && (trimestre == 0 || self::trimestre(fecha) == trimestre)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Resumen {
    /// Base imponible de las facturas emitidas (sin IVA ni retenciones).
    pub ingresos: i64,
    pub iva_repercutido: i64,
    /// IRPF que los clientes han retenido al emitir la factura.
    pub retenido_por_clientes: i64,
    /// Base imponible de los gastos (sin IVA ni retenciones).
    pub gastos: i64,
    pub iva_soportado: i64,
    pub retenido_a_proveedores: i64,
    pub n_facturas: usize,
    pub n_gastos: usize,
}

impl Resumen {
    /// Rendimiento neto del periodo.
    pub fn resultado(&self) -> i64 {
        self.ingresos - self.gastos
    }

    /// 303: IVA repercutido menos IVA soportado deducible. Puede salir a
    /// compensar (negativo) o a devolver.
    pub fn iva_a_ingresar(&self) -> i64 {
        self.iva_repercutido - self.iva_soportado
    }

    /// 130: 20 % del rendimiento neto, menos las retenciones ya practicadas.
    /// Es una estimación: el modelo real aplica acumulados del año.
    pub fn irpf_130(&self) -> i64 {
        div_round(self.resultado() * 2000, 10_000) - self.retenido_por_clientes
    }
}

pub fn resumen(db: &Db, anyo: i32, tri: u32) -> Resumen {
    let mut r = Resumen::default();

    for d in &db.documentos {
        // Sólo facturas: ni presupuestos ni proformas son documentos fiscales,
        // y los borradores o anulados tampoco cuentan.
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if !en_rango(d.fecha, anyo, tri) {
            continue;
        }
        let t = d.totales();
        r.ingresos += t.base_total;
        r.iva_repercutido += t.cuota_iva;
        r.retenido_por_clientes += t.retencion;
        r.n_facturas += 1;
    }

    for g in &db.gastos {
        if !en_rango(g.fecha, anyo, tri) {
            continue;
        }
        r.gastos += g.base_cents;
        r.iva_soportado += g.cuota_iva();
        r.retenido_a_proveedores += g.retencion();
        r.n_gastos += 1;
    }

    r
}

/// Texto seguro para CSV: sin separadores ni saltos de línea.
fn limpia(s: &str) -> String {
    s.replace(';', ",")
        .replace(['\n', '\r'], " ")
        .replace('"', "'")
        .trim()
        .to_string()
}

/// Libro de ingresos y gastos del año en CSV con `;` (lo que espera Excel en
/// español) y BOM UTF-8, para que los acentos salgan bien al abrirlo.
pub fn csv_libro(db: &Db, anyo: i32) -> String {
    let mut filas: Vec<(NaiveDate, String)> = Vec::new();

    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha.year() != anyo {
            continue;
        }
        let t = d.totales();
        filas.push((
            d.fecha,
            format!(
                "Ingreso;{};{};{};{};{};{};{};{};{};{}",
                d.fecha.format("%d/%m/%Y"),
                limpia(&d.etiqueta_numero()),
                limpia(&d.cliente.nif),
                limpia(&d.cliente.nombre_completo()),
                limpia(if d.notas.is_empty() {
                    "Factura"
                } else {
                    d.notas.as_str()
                }),
                fmt_cents(t.base_total),
                fmt_pb(d.lineas.first().map(|l| l.iva_pb).unwrap_or(0)),
                fmt_cents(t.cuota_iva),
                fmt_cents(t.retencion),
                fmt_cents(t.total),
            ),
        ));
    }

    for g in &db.gastos {
        if g.fecha.year() != anyo {
            continue;
        }
        filas.push((
            g.fecha,
            format!(
                "Gasto;{};{};{};{};{};{};{};{};{};{}",
                g.fecha.format("%d/%m/%Y"),
                limpia(&g.concepto),
                limpia(&g.nif),
                limpia(&g.proveedor),
                limpia(&g.categoria),
                fmt_cents(g.base_cents),
                fmt_pb(g.iva_pb),
                fmt_cents(g.cuota_iva()),
                fmt_cents(g.retencion()),
                fmt_cents(g.total()),
            ),
        ));
    }

    filas.sort_by(|a, b| a.0.cmp(&b.0));

    let mut salida = String::from("\u{feff}");
    salida.push_str(
        "Clase;Fecha;Documento;NIF;Contraparte;Concepto;Base;IVA %;Cuota IVA;Retencion;Total\r\n",
    );
    for (_, linea) in filas {
        salida.push_str(&linea);
        salida.push_str("\r\n");
    }
    salida
}

/// Informe en texto con el resumen anual y por trimestre, pensado para
/// entregarlo junto al libro al gestor o para revisarlo antes de presentar.
pub fn informe_anual(db: &Db, anyo: i32) -> String {
    let mut s = String::new();
    s.push_str(&format!("RESUMEN FISCAL {}\n", anyo));
    s.push_str("========================\n\n");
    s.push_str(&format!("Empresa: {}\n", db.empresa.nombre));
    s.push_str(&format!("NIF:     {}\n\n", db.empresa.nif));

    let anual = resumen(db, anyo, 0);
    let a130 = acumulado_130(db, anyo, 0);
    s.push_str("AÑO COMPLETO\n");
    s.push_str(&format!("  Ingresos (base)      {:>15}\n", fmt_cents(anual.ingresos)));
    s.push_str(&format!("  Gastos (base)        {:>15}\n", fmt_cents(anual.gastos)));
    s.push_str(&format!("  Resultado            {:>15}\n", fmt_cents(anual.resultado())));
    s.push_str(&format!(
        "  IVA repercutido      {:>15}\n",
        fmt_cents(anual.iva_repercutido)
    ));
    s.push_str(&format!(
        "  IVA soportado        {:>15}\n",
        fmt_cents(anual.iva_soportado)
    ));
    s.push_str(&format!(
        "  IVA a ingresar       {:>15}\n",
        fmt_cents(anual.iva_a_ingresar())
    ));
    s.push_str(&format!(
        "  IRPF 130 estimado    {:>15}\n",
        fmt_cents(a130.cuota)
    ));
    s.push_str(&format!("  Facturas emitidas    {}\n", anual.n_facturas));
    s.push_str(&format!("  Gastos registrados   {}\n", anual.n_gastos));

    for tri in 1..=4 {
        let r = resumen(db, anyo, tri);
        let a = acumulado_130(db, anyo, tri);
        s.push_str(&format!("\nTRIMESTRE {}\n", tri));
        s.push_str(&format!("  Ingresos             {:>15}\n", fmt_cents(r.ingresos)));
        s.push_str(&format!("  Gastos               {:>15}\n", fmt_cents(r.gastos)));
        s.push_str(&format!("  303 a ingresar       {:>15}\n", fmt_cents(r.iva_a_ingresar())));
        s.push_str(&format!("  130 acumulado        {:>15}\n", fmt_cents(a.cuota)));
    }

    let ops = modelo_347(db, anyo);
    if !ops.is_empty() {
        s.push_str("\nMODELO 347 (más de 3.005,06 EUR)\n");
        for o in ops {
            s.push_str(&format!(
                "  {:<10} {:<20} {:>15}  {}\n",
                o.tipo,
                o.nif,
                fmt_cents(o.importe),
                o.nombre
            ));
        }
    }

    let ops349 = crate::fiscal::resumen_349(db, anyo);
    if !ops349.is_empty() {
        s.push_str("\nMODELO 349 (empresas de la UE)\n");
        for f in ops349 {
            s.push_str(&format!(
                "  {:<14} {:<24} {:>15}\n",
                f.nif,
                f.nombre,
                fmt_cents(f.base)
            ));
        }
    }

    s.push_str("\nDocumento informativo: revisa las casillas antes de presentar.\n");
    s
}

/// Gasto acumulado por categoría (vacía = "Sin categoría").
pub fn gastos_por_categoria(db: &Db, anyo: i32, tri: u32) -> Vec<(String, i64, usize)> {
    use std::collections::BTreeMap;
    let mut m: BTreeMap<String, (i64, usize)> = BTreeMap::new();
    for g in &db.gastos {
        if !en_rango(g.fecha, anyo, tri) {
            continue;
        }
        let clave = if g.categoria.trim().is_empty() {
            "Sin categoría".to_string()
        } else {
            g.categoria.trim().to_string()
        };
        let e = m.entry(clave).or_insert((0, 0));
        e.0 += g.base_cents;
        e.1 += 1;
    }
    let mut v: Vec<(String, i64, usize)> = m
        .into_iter()
        .map(|(categoria, (base, n))| (categoria, base, n))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

/// Desglose del 303 por tipo de IVA.
#[derive(Clone, Copy, Debug)]
pub struct TipoIva {
    pub pb: i32,
    pub base: i64,
    pub cuota: i64,
}

/// (repercutido, soportado) agrupados por tipo de IVA.
pub fn desglose_iva(db: &Db, anyo: i32, tri: u32) -> (Vec<TipoIva>, Vec<TipoIva>) {
    use std::collections::BTreeMap;
    let mut rep: BTreeMap<i32, (i64, i64)> = BTreeMap::new();
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if !en_rango(d.fecha, anyo, tri) {
            continue;
        }
        for (pb, base, cuota) in d.totales().bases {
            let e = rep.entry(pb).or_insert((0, 0));
            e.0 += base;
            e.1 += cuota;
        }
    }
    let mut sop: BTreeMap<i32, (i64, i64)> = BTreeMap::new();
    for g in &db.gastos {
        if !en_rango(g.fecha, anyo, tri) {
            continue;
        }
        let e = sop.entry(g.iva_pb).or_insert((0, 0));
        e.0 += g.base_cents;
        e.1 += g.cuota_iva();
    }
    let conv = |m: BTreeMap<i32, (i64, i64)>| -> Vec<TipoIva> {
        m.into_iter()
            .map(|(pb, (base, cuota))| TipoIva { pb, base, cuota })
            .collect()
    };
    (conv(rep), conv(sop))
}

/// Modelo 130: acumulado del año hasta el trimestre indicado (0 = año entero).
#[derive(Clone, Copy, Debug, Default)]
pub struct Acumulado {
    pub ingresos: i64,
    pub gastos: i64,
    pub retenciones: i64,
    pub cuota: i64,
}

pub fn acumulado_130(db: &Db, anyo: i32, hasta: u32) -> Acumulado {
    let tope = if hasta == 0 { 4 } else { hasta };
    let mut a = Acumulado::default();
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha.year() != anyo || trimestre(d.fecha) > tope {
            continue;
        }
        let t = d.totales();
        a.ingresos += t.base_total;
        a.retenciones += t.retencion;
    }
    for g in &db.gastos {
        if g.fecha.year() != anyo || trimestre(g.fecha) > tope {
            continue;
        }
        a.gastos += g.base_cents;
    }
    let rendimiento = a.ingresos - a.gastos;
    a.cuota = (div_round(rendimiento * 2000, 10_000) - a.retenciones).max(0);
    a
}

/// Operación del modelo 347 (por tercero y por año).
#[derive(Clone, Debug)]
pub struct Operacion347 {
    pub nif: String,
    pub nombre: String,
    pub importe: i64,
    pub tipo: &'static str,
}

/// Operaciones del año que se declaran en el 347: más de 3.005,06 EUR por
/// persona o entidad, contando base más IVA.
pub fn modelo_347(db: &Db, anyo: i32) -> Vec<Operacion347> {
    use std::collections::BTreeMap;
    const LIMITE: i64 = 300_506;
    let mut acc: BTreeMap<(String, &'static str), (String, i64)> = BTreeMap::new();

    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha.year() != anyo {
            continue;
        }
        // El 347 sólo recoge operaciones españolas.
        if d.operacion != TipoOperacion::Nacional {
            continue;
        }
        let nif = d.cliente.nif.trim().to_uppercase();
        if nif.is_empty() {
            continue;
        }
        let e = acc
            .entry((nif, "Cliente"))
            .or_insert((d.cliente.nombre_completo(), 0));
        e.1 += d.totales().total;
    }

    for g in &db.gastos {
        if g.fecha.year() != anyo {
            continue;
        }
        let nif = g.nif.trim().to_uppercase();
        if nif.is_empty() {
            continue;
        }
        let e = acc
            .entry((nif, "Proveedor"))
            .or_insert((g.proveedor.clone(), 0));
        e.1 += g.total();
    }

    let mut v: Vec<Operacion347> = acc
        .into_iter()
        .filter(|(_, (_, importe))| *importe > LIMITE)
        .map(|((nif, tipo), (nombre, importe))| Operacion347 {
            nif,
            nombre,
            importe,
            tipo,
        })
        .collect();
    v.sort_by(|a, b| b.importe.cmp(&a.importe));
    v
}

/// Fechas orientativas de las obligaciones del año.
pub fn vencimientos(anyo: i32) -> Vec<(NaiveDate, &'static str, &'static str)> {
    let fecha = |m: u32, d: u32| {
        NaiveDate::from_ymd_opt(anyo, m, d)
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(anyo, 1, 1).expect("1 de enero"))
    };
    vec![
        (fecha(1, 20), "303 y 130", "4T del año anterior"),
        (fecha(1, 30), "390", "Resumen anual de IVA"),
        (fecha(2, 28), "347", "Operaciones con terceros"),
        (fecha(4, 20), "303 y 130", "1T"),
        (fecha(7, 20), "303 y 130", "2T"),
        (fecha(10, 20), "303 y 130", "3T"),
    ]
}

/// Qué aporta cada cliente: facturado, cobrado y lo que sigue pendiente.
#[derive(Clone, Debug)]
pub struct Rentabilidad {
    pub cliente: String,
    pub facturado: i64,
    pub cobrado: i64,
    pub pendiente: i64,
    pub n: usize,
}

pub fn rentabilidad_clientes(db: &Db, anyo: i32, tri: u32) -> Vec<Rentabilidad> {
    use std::collections::BTreeMap;
    let mut m: BTreeMap<String, Rentabilidad> = BTreeMap::new();
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if !en_rango(d.fecha, anyo, tri) {
            continue;
        }
        let clave = d.cliente.nombre_completo();
        let e = m.entry(clave.clone()).or_insert(Rentabilidad {
            cliente: clave,
            facturado: 0,
            cobrado: 0,
            pendiente: 0,
            n: 0,
        });
        let total = d.totales().total;
        e.facturado += total;
        e.cobrado += d.cobrado_cents().min(total);
        e.pendiente += d.pendiente_cents();
        e.n += 1;
    }
    let mut v: Vec<Rentabilidad> = m.into_values().collect();
    v.sort_by(|a, b| b.facturado.cmp(&a.facturado));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cliente, Documento, Empresa, Estado, Linea, TipoDoc};

    fn factura(fecha: NaiveDate, base: i64, iva: i32, irpf: i32) -> Documento {
        let cliente = Cliente {
            id: 1,
            nombre: "Cliente".into(),
            nif: "B12345678".into(),
            ..Default::default()
        };
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente, irpf, "2026".into());
        d.numero = 1;
        d.estado = Estado::Emitido;
        d.fecha = fecha;
        d.lineas = vec![Linea {
            descripcion: "Servicio".into(),
            cantidad_milli: 1000,
            precio_cents: base,
            descuento_pb: 0,
            iva_pb: iva,
        }];
        d
    }

    #[test]
    fn trimestres_naturales() {
        assert_eq!(trimestre(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()), 1);
        assert_eq!(trimestre(NaiveDate::from_ymd_opt(2026, 3, 31).unwrap()), 1);
        assert_eq!(trimestre(NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()), 2);
        assert_eq!(trimestre(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()), 4);
    }

    #[test]
    fn resumen_suma_iva_e_irpf() {
        let mut db = Db::default();
        db.empresa = Empresa {
            nombre: "Yo".into(),
            nif: "12345678Z".into(),
            persona_fisica: false,
            ..Default::default()
        };
        // 1.000 de base, 21 % IVA, 15 % IRPF en el primer trimestre.
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 2, 10).unwrap(),
            100_000,
            2100,
            1500,
        ));
        // Fuera del trimestre.
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 7, 10).unwrap(),
            50_000,
            2100,
            0,
        ));

        let r = resumen(&db, 2026, 1);
        assert_eq!(r.ingresos, 100_000);
        assert_eq!(r.iva_repercutido, 21_000);
        assert_eq!(r.retenido_por_clientes, 15_000);
        assert_eq!(r.n_facturas, 1);
        // 20 % de 1.000 = 200, menos 150 retenidos = 50.
        assert_eq!(r.irpf_130(), 20_000 - 15_000);

        let todo = resumen(&db, 2026, 0);
        assert_eq!(todo.ingresos, 150_000);
        assert_eq!(todo.n_facturas, 2);
    }

    #[test]
    fn gastos_restan_y_su_iva_se_deduce() {
        let mut db = Db::default();
        db.gastos.push(Gasto {
            id: 1,
            fecha: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            proveedor: "Papelería".into(),
            base_cents: 10_000,
            iva_pb: 2100,
            ..Default::default()
        });
        let r = resumen(&db, 2026, 1);
        assert_eq!(r.gastos, 10_000);
        assert_eq!(r.iva_soportado, 2_100);
        assert_eq!(r.iva_a_ingresar(), -2_100);
        assert_eq!(r.resultado(), -10_000);
    }

    #[test]
    fn el_csv_lleva_bom_y_cabecera() {
        let db = Db::default();
        let csv = csv_libro(&db, 2026);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Clase;Fecha;Documento"));
    }
}
