//! Gráficas de contabilidad.
//!
//! Dos capas: funciones puras que preparan las series (probadas con tests) y
//! dibujo directo con el pintor de egui. No se usa ninguna biblioteca de
//! gráficas: así no hay dependencias nuevas, el estilo es el de la app y las
//! cifras se pueden comprobar sin abrir la ventana.
//!
//! Criterio de devengo, el mismo que el resto de la contabilidad: se cuenta por
//! la fecha de la factura o del gasto, no por la fecha de cobro.

use crate::contab;
use crate::store::Db;
use chrono::Datelike;
use crate::tema;
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke};

pub const MESES: [&str; 12] = [
    "Ene", "Feb", "Mar", "Abr", "May", "Jun", "Jul", "Ago", "Sep", "Oct", "Nov", "Dic",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mes {
    pub ingresos: i64,
    pub gastos: i64,
}

impl Mes {
    pub fn resultado(&self) -> i64 {
        self.ingresos - self.gastos
    }
}

/// Doce meses del año, con la base imponible de facturas y gastos.
pub fn serie_mensual(db: &Db, anyo: i32) -> [Mes; 12] {
    let mut serie = [Mes::default(); 12];
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == crate::model::Estado::Anulado {
            continue;
        }
        if d.fecha.year() != anyo {
            continue;
        }
        let i = d.fecha.month0() as usize;
        serie[i].ingresos += d.totales().base_total;
    }
    for g in &db.gastos {
        if g.fecha.year() != anyo || g.es_pago_personal() {
            continue;
        }
        let i = g.fecha.month0() as usize;
        serie[i].gastos += g.base_cents;
    }
    for n in &db.nominas {
        if n.anyo != anyo {
            continue;
        }
        let i = (n.mes.saturating_sub(1) as usize).min(11);
        serie[i].gastos += n.coste_empresa_cents;
    }
    serie
}

/// Resultado acumulado mes a mes: la línea que enseña la tendencia del año.
pub fn resultado_acumulado(serie: &[Mes; 12]) -> [i64; 12] {
    let mut salida = [0i64; 12];
    let mut suma = 0i64;
    for (i, m) in serie.iter().enumerate() {
        suma += m.resultado();
        salida[i] = suma;
    }
    salida
}

/// IVA repercutido y soportado de cada trimestre.
pub fn serie_iva_trimestral(db: &Db, anyo: i32) -> [(i64, i64); 4] {
    let mut salida = [(0i64, 0i64); 4];
    for (i, celda) in salida.iter_mut().enumerate() {
        let r = contab::resumen(db, anyo, i as u32 + 1);
        *celda = (r.iva_repercutido, r.iva_soportado);
    }
    salida
}

/// Clientes que más han facturado en el periodo, de mayor a menor.
pub fn top_clientes(db: &Db, anyo: i32, tri: u32, cuantos: usize) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = contab::rentabilidad_clientes(db, anyo, tri)
        .into_iter()
        .filter(|r| r.facturado > 0)
        .map(|r| (r.cliente, r.facturado))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(cuantos);
    v
}

/// Techo redondeado a 1, 2 o 5 por una potencia de diez: las guías del eje
/// quedan en cifras que se leen de un vistazo.
/// Elige un paso «redondo» (1, 2, 2,5, 3 o 5 por una potencia de diez) para
/// que la guía superior quede por encima del dato sin aplastar las barras.
pub fn techo(maximo: i64) -> i64 {
    if maximo <= 0 {
        return 300;
    }
    let objetivo = maximo as f64 / 2.5;
    let mut potencia = 1.0f64;
    while potencia * 10.0 <= objetivo {
        potencia *= 10.0;
    }
    let mut paso = potencia * 10.0;
    for factor in [1.0f64, 2.0, 2.5, 3.0, 5.0] {
        let candidato = potencia * factor;
        if candidato >= objetivo {
            paso = candidato;
            break;
        }
    }
    ((paso * 3.0).round() as i64).max(3)
}

/// Importe corto para el eje: `850 €`, `1,2 mil €`, `3,4 M €`.
pub fn compacto(v: i64) -> String {
    let euros = v as f64 / 100.0;
    let abs = euros.abs();
    let (n, sufijo) = if abs >= 1_000_000.0 {
        (euros / 1_000_000.0, " M €")
    } else if abs >= 1_000.0 {
        (euros / 1_000.0, " mil €")
    } else {
        (euros, " €")
    };
    if n.abs() < 10.0 && n.fract().abs() > 0.05 {
        format!("{:.1}", n).replace('.', ",") + sufijo
    } else {
        format!("{:.0}", n) + sufijo
    }
}

/// Título y subtítulo de una tarjeta de gráfica.
pub fn cabecera(ui: &mut egui::Ui, titulo: &str, sub: &str) {
    ui.label(
        egui::RichText::new(titulo)
            .size(13.0)
            .strong()
            .color(tema::TEXTO),
    );
    if !sub.is_empty() {
        ui.label(egui::RichText::new(sub).size(10.5).color(tema::TENUE));
    }
    ui.add_space(4.0);
}

// ---------------------------------------------------------------------------
// Dibujo
// ---------------------------------------------------------------------------

const BORDE_GRAFICA: Color32 = Color32::from_rgb(0xEA, 0xEE, 0xF5);

struct Lienzo {
    rect: Rect,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
}

fn lienzo(ui: &mut egui::Ui, alto: f32, izq: f32) -> (Lienzo, egui::Response) {
    let ancho = ui.available_width().max(260.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ancho, alto), Sense::hover());
    let l = Lienzo {
        rect,
        x0: rect.left() + izq,
        x1: rect.right() - 6.0,
        y0: rect.top() + 16.0,
        y1: rect.bottom() - 16.0,
    };
    (l, resp)
}

fn guias(p: &egui::Painter, l: &Lienzo, maximo: i64) {
    for i in 0..=3 {
        let v = maximo * i as i64 / 3;
        let y = l.y1 - (l.y1 - l.y0) * i as f32 / 3.0;
        p.line_segment(
            [egui::pos2(l.x0, y), egui::pos2(l.x1, y)],
            Stroke::new(1.0, BORDE_GRAFICA),
        );
        p.text(
            egui::pos2(l.x0 - 6.0, y),
            Align2::RIGHT_CENTER,
            compacto(v),
            FontId::proportional(9.0),
            tema::TENUE,
        );
    }
}

fn leyenda(p: &egui::Painter, l: &Lienzo, series: &[(&str, Color32)]) {
    let mut x = l.x0;
    for (nombre, color) in series {
        p.rect_filled(
            Rect::from_min_size(egui::pos2(x, l.rect.top() + 3.0), egui::vec2(7.0, 7.0)),
            egui::Rounding::same(2.0),
            *color,
        );
        p.text(
            egui::pos2(x + 11.0, l.rect.top() + 6.5),
            Align2::LEFT_CENTER,
            *nombre,
            FontId::proportional(9.5),
            tema::SUAVE,
        );
        x += 16.0 + nombre.chars().count() as f32 * 5.0;
    }
}

/// Barras agrupadas por mes: ingresos y gastos del año.
pub fn barras_mensuales(ui: &mut egui::Ui, serie: &[Mes; 12], alto: f32) {
    let (l, resp) = lienzo(ui, alto, 46.0);
    let p = ui.painter();
    let maximo = techo(
        serie
            .iter()
            .map(|m| m.ingresos.max(m.gastos))
            .max()
            .unwrap_or(0),
    );
    guias(&p, &l, maximo);
    leyenda(
        &p,
        &l,
        &[("Ingresos", tema::ACENTO), ("Gastos", tema::AMBAR)],
    );

    let paso = (l.x1 - l.x0) / 12.0;
    let ancho = (paso * 0.32).clamp(3.0, 22.0);
    for (i, m) in serie.iter().enumerate() {
        let centro = l.x0 + paso * (i as f32 + 0.5);
        for (j, valor) in [m.ingresos, m.gastos].iter().enumerate() {
            let h = (valor.abs() as f32 / maximo as f32) * (l.y1 - l.y0);
            let x = centro - ancho - 1.0 + ancho * j as f32 * 2.0;
            if h > 0.5 {
                p.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x, l.y1 - h),
                        egui::pos2(x + ancho, l.y1),
                    ),
                    egui::Rounding::same(2.0),
                    if j == 0 { tema::ACENTO } else { tema::AMBAR },
                );
            }
        }
        p.text(
            egui::pos2(centro, l.y1 + 3.0),
            Align2::CENTER_TOP,
            MESES[i],
            FontId::proportional(8.5),
            tema::TENUE,
        );
    }

    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
        if pos.x >= l.x0 && pos.x <= l.x1 && pos.y >= l.y0 && pos.y <= l.y1 {
            let idx = (((pos.x - l.x0) / paso) as usize).min(11);
            let m = serie[idx];
            resp.on_hover_ui(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} {}", MESES[idx], idx as u32 + 1))
                        .strong(),
                );
                ui.label(format!("Ingresos: {}", crate::model::fmt_cents_eur(m.ingresos)));
                ui.label(format!("Gastos: {}", crate::model::fmt_cents_eur(m.gastos)));
                ui.label(format!(
                    "Resultado: {}",
                    crate::model::fmt_cents_eur(m.resultado())
                ));
            });
        }
    }
}

/// Área del resultado acumulado, con la línea y el último valor destacado.
pub fn area_acumulada(ui: &mut egui::Ui, serie: &[Mes; 12], alto: f32) {
    let acumulado = resultado_acumulado(serie);
    let (l, resp) = lienzo(ui, alto, 52.0);
    let p = ui.painter();

    let max = (*acumulado.iter().max().unwrap_or(&0)).max(0);
    let min = (*acumulado.iter().min().unwrap_or(&0)).min(0);
    let techo_v = techo(max.max(1));
    let suelo_v = -techo(-min.max(1)) as f32;
    let arriba = techo_v as f32;
    let escala = |v: i64| -> f32 {
        let t = (v as f32 - suelo_v) / (arriba - suelo_v).max(1.0);
        l.y1 - t * (l.y1 - l.y0)
    };
    let base = escala(0);

    // Guías en el cero y en los extremos.
    for v in [0, techo_v, -(suelo_v as i64)] {
        let y = escala(v);
        p.line_segment(
            [egui::pos2(l.x0, y), egui::pos2(l.x1, y)],
            Stroke::new(1.0, BORDE_GRAFICA),
        );
        p.text(
            egui::pos2(l.x0 - 6.0, y),
            Align2::RIGHT_CENTER,
            compacto(v),
            FontId::proportional(9.0),
            tema::TENUE,
        );
    }

    let paso = (l.x1 - l.x0) / 11.0;
    let punto = |i: usize| {
        egui::pos2(l.x0 + paso * i as f32, escala(acumulado[i]))
    };

    // Relleno por columnas finas: funciona con curvas que suben y bajan.
    let columnas = ((l.x1 - l.x0) / 3.0).max(12.0) as usize;
    for c in 0..columnas {
        let x = l.x0 + (l.x1 - l.x0) * c as f32 / columnas as f32;
        let i = ((c as f32 / columnas as f32) * 11.0).round() as usize;
        let y = escala(acumulado[i.min(11)]);
        let (top, bottom) = if y < base { (y, base) } else { (base, y) };
        p.rect_filled(
            Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + 3.5, bottom)),
            egui::Rounding::ZERO,
            tema::ACENTO.gamma_multiply(0.14),
        );
    }

    for i in 0..11 {
        p.line_segment(
            [punto(i), punto(i + 1)],
            Stroke::new(1.8, tema::ACENTO),
        );
    }
    for i in 0..12 {
        p.circle_filled(punto(i), 2.0, tema::ACENTO);
        p.text(
            egui::pos2(punto(i).x, l.y1 + 3.0),
            Align2::CENTER_TOP,
            MESES[i],
            FontId::proportional(8.5),
            tema::TENUE,
        );
    }
    let ultimo = punto(11);
    p.circle_filled(ultimo, 4.0, tema::ACENTO);
    p.text(
        ultimo + egui::vec2(0.0, -8.0),
        Align2::CENTER_BOTTOM,
        compacto(acumulado[11]),
        FontId::proportional(9.5),
        tema::ACENTO,
    );

    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
        if pos.x >= l.x0 && pos.x <= l.x1 && pos.y >= l.y0 && pos.y <= l.y1 {
            let idx = (((pos.x - l.x0) / paso).round() as usize).min(11);
            resp.on_hover_ui(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} · acumulado", MESES[idx])).strong(),
                );
                ui.label(crate::model::fmt_cents_eur(acumulado[idx]));
                ui.label(
                    egui::RichText::new(format!(
                        "Este mes: {}",
                        crate::model::fmt_cents_eur(serie[idx].resultado())
                    ))
                    .small(),
                );
            });
        }
    }
}

/// IVA repercutido y soportado por trimestre.
pub fn barras_iva(ui: &mut egui::Ui, serie: &[(i64, i64); 4], alto: f32) {
    let (l, resp) = lienzo(ui, alto, 46.0);
    let p = ui.painter();
    let maximo = techo(
        serie
            .iter()
            .map(|(r, s)| (*r).max(*s))
            .max()
            .unwrap_or(0),
    );
    guias(&p, &l, maximo);
    leyenda(
        &p,
        &l,
        &[("Repercutido", tema::ACENTO), ("Soportado", tema::VERDE)],
    );

    let paso = (l.x1 - l.x0) / 4.0;
    let ancho = (paso * 0.2).clamp(8.0, 34.0);
    for (i, (rep, sop)) in serie.iter().enumerate() {
        let centro = l.x0 + paso * (i as f32 + 0.5);
        for (j, valor) in [*rep, *sop].iter().enumerate() {
            let h = (*valor as f32 / maximo as f32) * (l.y1 - l.y0);
            let x = centro - ancho - 2.0 + ancho * j as f32 * 2.0;
            if h > 0.5 {
                p.rect_filled(
                    Rect::from_min_max(egui::pos2(x, l.y1 - h), egui::pos2(x + ancho, l.y1)),
                    egui::Rounding::same(2.0),
                    if j == 0 { tema::ACENTO } else { tema::VERDE },
                );
            }
        }
        p.text(
            egui::pos2(centro, l.y1 + 3.0),
            Align2::CENTER_TOP,
            format!("{}T", i + 1),
            FontId::proportional(9.0),
            tema::TENUE,
        );
    }

    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
        if pos.x >= l.x0 && pos.x <= l.x1 && pos.y >= l.y0 && pos.y <= l.y1 {
            let idx = (((pos.x - l.x0) / paso) as usize).min(3);
            let (rep, sop) = serie[idx];
            resp.on_hover_ui(|ui| {
                ui.label(egui::RichText::new(format!("{}T", idx + 1)).strong());
                ui.label(format!("Repercutido: {}", crate::model::fmt_cents_eur(rep)));
                ui.label(format!("Soportado: {}", crate::model::fmt_cents_eur(sop)));
                ui.label(format!(
                    "A ingresar: {}",
                    crate::model::fmt_cents_eur(rep - sop)
                ));
            });
        }
    }
}

/// Barras horizontales con nombre, importe y porcentaje del total.
pub fn barras_horizontales(ui: &mut egui::Ui, filas: &[(String, i64)], color: Color32) {
    let total: i64 = filas.iter().map(|f| f.1.max(0)).sum();
    let maximo = filas.iter().map(|f| f.1).max().unwrap_or(0).max(1);
    for (nombre, valor) in filas {
        let ancho = ui.available_width().max(260.0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ancho, 22.0), Sense::hover());
        let p = ui.painter();
        let etiqueta = if nombre.chars().count() > 26 {
            nombre.chars().take(25).collect::<String>() + "…"
        } else {
            nombre.clone()
        };
        p.text(
            egui::pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            etiqueta,
            FontId::proportional(10.0),
            tema::TEXTO,
        );
        let barra_x = rect.left() + (rect.width() * 0.34).min(190.0);
        let barra_w = (rect.right() - barra_x - 96.0).max(40.0);
        let lleno = (*valor as f32 / maximo as f32) * barra_w;
        p.rect_filled(
            Rect::from_min_size(
                egui::pos2(barra_x, rect.center().y - 4.0),
                egui::vec2(barra_w, 8.0),
            ),
            egui::Rounding::same(4.0),
            Color32::from_rgb(0xF0, 0xF3, 0xF8),
        );
        if lleno > 1.0 {
            p.rect_filled(
                Rect::from_min_size(
                    egui::pos2(barra_x, rect.center().y - 4.0),
                    egui::vec2(lleno, 8.0),
                ),
                egui::Rounding::same(4.0),
                color,
            );
        }
        let pct = if total > 0 {
            *valor as f32 * 100.0 / total as f32
        } else {
            0.0
        };
        p.text(
            egui::pos2(rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            format!(
                "{}  ·  {:.0} %",
                crate::model::fmt_cents_eur(*valor),
                pct
            ),
            FontId::proportional(9.5),
            tema::SUAVE,
        );
        resp.on_hover_text(format!(
            "{}: {} ({:.0} % del total)",
            nombre,
            crate::model::fmt_cents_eur(*valor),
            pct
        ));
    }
}

/// Marco de tarjeta con el estilo de la aplicación.
pub fn tarjeta(ui: &mut egui::Ui, contenido: impl FnOnce(&mut egui::Ui)) {
    tema::tarjeta(ui, contenido);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use chrono::NaiveDate;

    fn factura(fecha: NaiveDate, base: i64, iva: i32) -> Documento {
        let cliente = Cliente {
            id: 1,
            nombre: "Cliente".into(),
            nif: "B12345678".into(),
            ..Default::default()
        };
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente, 0, "2026".into());
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

    fn gasto(fecha: NaiveDate, base: i64) -> Gasto {
        Gasto {
            id: 1,
            fecha,
            proveedor: "Proveedor".into(),
            base_cents: base,
            iva_pb: 2100,
            ..Default::default()
        }
    }

    #[test]
    fn la_serie_mensual_cuadra_con_el_resumen_anual() {
        let mut db = Db::default();
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            100_000,
            2100,
        ));
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 7, 2).unwrap(),
            50_000,
            2100,
        ));
        db.gastos
            .push(gasto(NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(), 20_000));
        use crate::empleados::{Empleado, Nomina};
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
        let nomina = Nomina::calcular(&empleado, 9, 2026, 1);
        let coste_personal = nomina.coste_empresa_cents;
        db.nominas.push(nomina);

        let serie = serie_mensual(&db, 2026);
        assert_eq!(serie.len(), 12);
        assert_eq!(serie[0].ingresos, 100_000);
        assert_eq!(serie[6].ingresos, 50_000);
        assert_eq!(serie[0].gastos, coste_personal);
        assert_eq!(serie[2].gastos, 20_000);

        let anual = contab::resumen(&db, 2026, 0);
        assert_eq!(
            serie.iter().map(|m| m.ingresos).sum::<i64>(),
            anual.ingresos
        );
        assert_eq!(serie.iter().map(|m| m.gastos).sum::<i64>(), anual.gastos);
    }

    #[test]
    fn el_acumulado_termina_en_el_resultado_del_ano() {
        let mut db = Db::default();
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 5, 5).unwrap(),
            80_000,
            2100,
        ));
        db.gastos
            .push(gasto(NaiveDate::from_ymd_opt(2026, 6, 5).unwrap(), 30_000));
        let serie = serie_mensual(&db, 2026);
        let acumulado = resultado_acumulado(&serie);
        assert_eq!(acumulado[4], 80_000);
        assert_eq!(acumulado[5], 50_000);
        assert_eq!(acumulado[11], 50_000);
    }

    #[test]
    fn el_iva_trimestral_suma_como_el_303() {
        let mut db = Db::default();
        db.documentos.push(factura(
            NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            100_000,
            2100,
        ));
        db.gastos
            .push(gasto(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(), 10_000));
        let serie = serie_iva_trimestral(&db, 2026);
        assert_eq!(serie[0], (21_000, 2_100));
        assert_eq!(serie[1], (0, 0));
        for (i, (rep, sop)) in serie.iter().enumerate() {
            let r = contab::resumen(&db, 2026, i as u32 + 1);
            assert_eq!(*rep, r.iva_repercutido);
            assert_eq!(*sop, r.iva_soportado);
        }
    }

    #[test]
    fn el_techo_da_cifras_redondas() {
        assert_eq!(techo(0), 300);
        assert_eq!(techo(90), 150);
        assert_eq!(techo(1_234), 1_500);
        assert_eq!(techo(9_000), 15_000);
        assert_eq!(techo(1_000_000), 1_500_000);
        // La guía superior nunca queda por debajo del dato.
        for v in [1, 7, 99, 1_001, 45_678, 7_654_321] {
            assert!(techo(v) >= v, "{v}");
        }
    }

    #[test]
    fn el_importe_compacto_se_lee_de_un_vistazo() {
        assert_eq!(compacto(0), "0 €");
        assert_eq!(compacto(85_000), "850 €");
        assert_eq!(compacto(120_000), "1,2 mil €");
        assert_eq!(compacto(1_200_000), "12 mil €");
        assert_eq!(compacto(340_000_000), "3,4 M €");
    }

    #[test]
    fn los_clientes_se_ordenan_y_se_recortan() {
        let mut db = Db::default();
        for (i, (nombre, base)) in [("Uno", 30_000), ("Dos", 90_000), ("Tres", 60_000)]
            .iter()
            .enumerate()
        {
            let mut f = factura(
                NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
                *base,
                2100,
            );
            f.cliente.nombre = (*nombre).to_string();
            f.numero = i as u64 + 1;
            db.documentos.push(f);
        }
        let top = top_clientes(&db, 2026, 0, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "Dos");
        assert_eq!(top[1].0, "Tres");
    }
}
