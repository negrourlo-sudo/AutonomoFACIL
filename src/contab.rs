//! Contabilidad sencilla para autónomo y pequeño negocio.
//!
//! No es un programa de contabilidad: calcula lo necesario para las
//! declaraciones trimestrales (IVA del modelo 303 e IRPF del modelo 130) a
//! partir de lo que ya hay en la app —las facturas emitidas— y de los gastos
//! que se registran aquí.
//!
//! Criterio de devengo: se cuenta por la **fecha de la factura, del gasto o
//! del mes de la nómina**, que es el que se usa en esos modelos, no por la
//! fecha de cobro.

use crate::empleados::Nomina;
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

fn nomina_en_rango(n: &Nomina, anyo: i32, trimestre: u32) -> bool {
    n.anyo == anyo && (trimestre == 0 || n.trimestre() == trimestre)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Resumen {
    /// Base imponible de las facturas emitidas (sin IVA ni retenciones).
    pub ingresos: i64,
    pub iva_repercutido: i64,
    /// IRPF que los clientes han retenido al emitir la factura.
    pub retenido_por_clientes: i64,
    /// Gastos sin IVA: gastos manuales más el coste de personal de las
    /// nóminas. Los pagos bancarios marcados como personal no se suman aquí.
    pub gastos: i64,
    /// Parte de `gastos` que corresponde al coste de empresa de las nóminas.
    pub gastos_personal: i64,
    pub iva_soportado: i64,
    pub retenido_a_proveedores: i64,
    pub n_facturas: usize,
    pub n_gastos: usize,
    /// Nóminas incluidas en el coste de personal.
    pub n_nominas: usize,
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
        if !en_rango(g.fecha, anyo, tri) || g.es_pago_personal() {
            continue;
        }
        r.gastos += g.base_cents;
        r.iva_soportado += g.cuota_iva();
        r.retenido_a_proveedores += g.retencion();
        r.n_gastos += 1;
    }

    // El coste de empresa de la nómina (bruto + SS de la empresa) es el gasto
    // deducible de personal. Se cuenta por devengo, aunque aún no esté pagada.
    for n in &db.nominas {
        if !nomina_en_rango(n, anyo, tri) {
            continue;
        }
        r.gastos += n.coste_empresa_cents;
        r.gastos_personal += n.coste_empresa_cents;
        r.n_nominas += 1;
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
        if g.fecha.year() != anyo || g.es_pago_personal() {
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

    // El coste de personal se añade como un apunte propio del libro. El pago
    // bancario marcado como personal no se duplica.
    for n in &db.nominas {
        if n.anyo != anyo {
            continue;
        }
        let empleado = db.empleados.iter().find(|e| e.id == n.empleado_id);
        let nombre = if !n.empleado_nombre.trim().is_empty() {
            n.empleado_nombre.clone()
        } else {
            empleado
                .map(|e| e.nombre_completo())
                .unwrap_or_else(|| "Trabajador".to_string())
        };
        let nif = if !n.empleado_nif.trim().is_empty() {
            n.empleado_nif.clone()
        } else {
            empleado.map(|e| e.nif.clone()).unwrap_or_default()
        };
        let fecha = NaiveDate::from_ymd_opt(n.anyo, n.mes, 1).unwrap_or_default();
        filas.push((
            fecha,
            format!(
                "Personal;{};{};{};{};{};{};0;0;0;{}",
                fecha.format("%d/%m/%Y"),
                format!("Nómina {:02}/{}", n.mes, n.anyo),
                limpia(&nif),
                limpia(&nombre),
                "Coste de personal",
                fmt_cents(n.coste_empresa_cents),
                fmt_cents(n.coste_empresa_cents),
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
    s.push_str(&format!(
        "  · de personal        {:>15}\n",
        fmt_cents(anual.gastos_personal)
    ));
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
    s.push_str(&format!("  Nóminas incluidas    {}\n", anual.n_nominas));

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
        if !en_rango(g.fecha, anyo, tri) || g.es_pago_personal() {
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
    let mut personal = 0i64;
    let mut n_personal = 0usize;
    for n in &db.nominas {
        if nomina_en_rango(n, anyo, tri) {
            personal += n.coste_empresa_cents;
            n_personal += 1;
        }
    }
    if n_personal > 0 {
        m.insert("Personal (nóminas)".to_string(), (personal, n_personal));
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
        if !en_rango(g.fecha, anyo, tri) || g.es_pago_personal() {
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
    /// Gastos deducibles acumulados, incluido el coste de personal.
    pub gastos: i64,
    /// Parte de `gastos` correspondiente a las nóminas.
    pub gastos_personal: i64,
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
        if g.fecha.year() != anyo || trimestre(g.fecha) > tope || g.es_pago_personal() {
            continue;
        }
        a.gastos += g.base_cents;
    }
    for n in &db.nominas {
        if n.anyo != anyo || n.trimestre() > tope {
            continue;
        }
        a.gastos += n.coste_empresa_cents;
        a.gastos_personal += n.coste_empresa_cents;
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
        if g.fecha.year() != anyo || g.es_pago_personal() {
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
    use crate::empleados::{Empleado, Nomina};
    use crate::model::{Cliente, Documento, Empresa, Estado, Linea, TipoDoc};

    fn nomina(id: u64, anyo: i32, mes: u32) -> Nomina {
        let empleado = Empleado {
            id: 1,
            nombre: "Ana".into(),
            apellidos: "García".into(),
            nif: "12345678Z".into(),
            fecha_alta: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        };
        Nomina::calcular(&empleado, id, anyo, mes)
    }

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
    fn el_personal_entra_en_contabilidad_y_en_el_130_sin_duplicar_pagos() {
        let mut db = Db::default();
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 2, 10).unwrap(),
            500_000,
            2100,
            0,
        ));
        let n = nomina(10, 2026, 1);
        let coste = n.coste_empresa_cents;
        db.nominas.push(n);
        // Pago bancario marcado explícitamente: no debe sumarse otra vez.
        db.gastos.push(Gasto {
            id: 2,
            fecha: NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            proveedor: "Banco".into(),
            base_cents: 157_040,
            pago_personal: true,
            ..Default::default()
        });
        // Un pago antiguo que sólo se reconoce por la categoría.
        db.gastos.push(Gasto {
            id: 3,
            fecha: NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            proveedor: "Banco".into(),
            base_cents: 157_040,
            categoria: "Nóminas".into(),
            ..Default::default()
        });
        db.gastos.push(Gasto {
            id: 4,
            fecha: NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            proveedor: "Papelería".into(),
            base_cents: 10_000,
            iva_pb: 2100,
            ..Default::default()
        });

        let r = resumen(&db, 2026, 1);
        assert_eq!(r.gastos, coste + 10_000);
        assert_eq!(r.gastos_personal, coste);
        assert_eq!(r.n_gastos, 1);
        assert_eq!(r.n_nominas, 1);
        assert_eq!(r.iva_soportado, 2_100);
        assert_eq!(r.resultado(), 500_000 - coste - 10_000);

        let a = acumulado_130(&db, 2026, 1);
        assert_eq!(a.gastos, coste + 10_000);
        assert_eq!(a.gastos_personal, coste);
        let esperado = div_round((500_000 - coste - 10_000) * 2000, 10_000);
        assert_eq!(a.cuota, esperado);
        assert_eq!(r.irpf_130(), esperado);

        let categorias = gastos_por_categoria(&db, 2026, 1);
        assert!(categorias
            .iter()
            .any(|(c, base, n)| c == "Personal (nóminas)" && *base == coste && *n == 1));
        let csv = csv_libro(&db, 2026);
        assert!(csv.contains("Personal;"));
        assert!(csv.contains("Coste de personal"));
        assert!(!csv.contains("Banco"));
    }

    #[test]
    fn el_347_ignora_los_pagos_de_personal() {
        let mut db = Db::default();
        db.gastos.push(Gasto {
            id: 1,
            fecha: NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            proveedor: "Banco".into(),
            nif: "B12345678".into(),
            base_cents: 400_000,
            categoria: "Nóminas".into(),
            ..Default::default()
        });
        db.gastos.push(Gasto {
            id: 2,
            fecha: NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            proveedor: "Banco".into(),
            nif: "B12345678".into(),
            base_cents: 100_000,
            ..Default::default()
        });
        assert!(modelo_347(&db, 2026).is_empty());
    }

    #[test]
    fn el_csv_lleva_bom_y_cabecera() {
        let db = Db::default();
        let csv = csv_libro(&db, 2026);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Clase;Fecha;Documento"));
    }
}
