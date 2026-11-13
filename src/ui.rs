//! Interfaz de escritorio (egui/eframe).
//!
//! Sin framework web ni WebView: una ventana nativa, un lienzo inmediato y
//! estado en memoria. La regla de la casa es que **una factura emitida no se
//! edita**: se anula, porque el registro encadenado ya está generado.

use crate::empleados::*;
use crate::model::*;
use crate::perf::{Benchmark, Perfil};
use crate::store::Db;
use crate::{export, facturae, pdf, tema, verifactu};
use chrono::{Datelike, Local, NaiveDate};
// `egui` llega reexportado por eframe: no es una dependencia directa.
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Vista {
    Inicio,
    Presupuestos,
    Proformas,
    Facturas,
    Clientes,
    Empleados,
    Articulos,
    Contabilidad,
    Modelos,
    Guia,
    Ajustes,
}

/// Acciones que se difieren hasta que termina el préstamo mutable del editor.
const MESES_NOMINA: [&str; 12] = [
    "Enero",
    "Febrero",
    "Marzo",
    "Abril",
    "Mayo",
    "Junio",
    "Julio",
    "Agosto",
    "Septiembre",
    "Octubre",
    "Noviembre",
    "Diciembre",
];

enum Accion {
    Nada,
    GuardarBorrador,
    EmitirYGuardar,
    Cancelar,
    Bloqueada(String),
}

struct LineaBuf {
    descripcion: String,
    cantidad: String,
    precio: String,
    dto: String,
    iva: String,
    quitar: bool,
}

impl LineaBuf {
    fn desde(l: &Linea) -> Self {
        Self {
            descripcion: l.descripcion.clone(),
            cantidad: fmt_qty(l.cantidad_milli),
            precio: fmt_cents(l.precio_cents),
            dto: fmt_pb(l.descuento_pb),
            iva: fmt_pb(l.iva_pb),
            quitar: false,
        }
    }
    fn a_linea(&self) -> Linea {
        Linea {
            descripcion: self.descripcion.clone(),
            cantidad_milli: parse_qty(&self.cantidad).unwrap_or(0),
            precio_cents: parse_cents(&self.precio).unwrap_or(0),
            descuento_pb: parse_pb(&self.dto).unwrap_or(0),
            iva_pb: parse_pb(&self.iva).unwrap_or(0),
        }
    }
    fn valida(&self) -> bool {
        parse_qty(&self.cantidad).is_some()
            && parse_cents(&self.precio).is_some()
            && parse_pb(&self.dto).is_some()
            && parse_pb(&self.iva).is_some()
    }
}

/// Gasto en edición; los importes van como texto mientras se teclean.
struct GastoBuf {
    gasto: Gasto,
    fecha: String,
    base: String,
    iva: String,
    irpf: String,
}

/// Empleado en edición. Las fechas y los importes se guardan como texto
/// mientras se teclean, igual que en el resto de fichas.
struct EmpleadoBuf {
    empleado: Empleado,
    alta: String,
    baja: String,
    salario: String,
    irpf: String,
}

impl EmpleadoBuf {
    fn desde(empleado: Empleado) -> Self {
        Self {
            alta: empleado.fecha_alta.format("%d/%m/%Y").to_string(),
            baja: empleado
                .fecha_baja
                .map(|f| f.format("%d/%m/%Y").to_string())
                .unwrap_or_default(),
            salario: fmt_cents(empleado.salario_bruto_anual_cents),
            irpf: fmt_pb(empleado.irpf_pb),
            empleado,
        }
    }

    fn nuevo(empleado: Empleado) -> Self {
        Self {
            alta: empleado.fecha_alta.format("%d/%m/%Y").to_string(),
            baja: String::new(),
            salario: String::new(),
            irpf: fmt_pb(empleado.irpf_pb),
            empleado,
        }
    }
}

impl GastoBuf {
    fn desde(gasto: Gasto) -> Self {
        Self {
            fecha: gasto.fecha.format("%d/%m/%Y").to_string(),
            base: fmt_cents(gasto.base_cents),
            iva: fmt_pb(gasto.iva_pb),
            irpf: fmt_pb(gasto.irpf_pb),
            gasto,
        }
    }

    fn nuevo(gasto: Gasto) -> Self {
        Self {
            fecha: gasto.fecha.format("%d/%m/%Y").to_string(),
            base: String::new(),
            iva: fmt_pb(gasto.iva_pb),
            irpf: fmt_pb(gasto.irpf_pb),
            gasto,
        }
    }
}

/// Artículo en edición. Los precios se guardan como texto mientras se teclean:
/// si se reconstruyen desde el número en cada fotograma, lo escrito se pierde y
/// el campo se queda siempre a cero.
struct ArticuloBuf {
    art: Articulo,
    precio: String,
    iva: String,
}

impl ArticuloBuf {
    fn desde(art: Articulo) -> Self {
        Self {
            precio: fmt_cents(art.precio_cents),
            iva: fmt_pb(art.iva_pb),
            art,
        }
    }

    /// Artículo nuevo: el precio empieza vacío para que se vea la pista.
    fn nuevo(art: Articulo) -> Self {
        Self {
            precio: String::new(),
            iva: fmt_pb(art.iva_pb),
            art,
        }
    }
}

/// Datos que se piden para emitir una R1 por diferencias. Los importes van
/// como texto porque se teclean.
#[derive(Clone)]
struct RectificarBuf {
    original_id: u64,
    base: String,
    iva: String,
    motivo: String,
}

struct Editor {
    doc: Documento,
    serie: String,
    fecha: String,
    vencimiento: String,
    irpf: String,
    cliente_id: u64,
    lineas: Vec<LineaBuf>,
    nuevo: bool,
    /// Huella de cómo estaba al abrirlo: sirve para detectar cambios sin guardar.
    firma_inicial: String,
}

impl Editor {
    fn desde(doc: Documento, nuevo: bool) -> Self {
        let mut e = Self {
            serie: doc.serie.clone(),
            fecha: doc.fecha.format("%d/%m/%Y").to_string(),
            vencimiento: doc.vencimiento.format("%d/%m/%Y").to_string(),
            irpf: fmt_pb(doc.irpf_pb),
            cliente_id: doc.cliente_id,
            lineas: doc.lineas.iter().map(LineaBuf::desde).collect(),
            doc,
            nuevo,
            firma_inicial: String::new(),
        };
        e.firma_inicial = e.firma();
        e
    }

    /// Huella de lo que hay en pantalla. Incluye los textos tal cual, no los
    /// números ya convertidos, para que teclear una fecha a medias también
    /// cuente como cambio.
    fn firma(&self) -> String {
        let mut s = format!(
            "{}|{}|{}|{}|{}|{}|{}|{:?}|",
            self.serie,
            self.fecha,
            self.vencimiento,
            self.irpf,
            self.cliente_id,
            self.doc.forma_pago,
            self.doc.notas,
            self.doc.operacion
        );
        for l in &self.lineas {
            s.push_str(&format!(
                "{};{};{};{};{}#",
                l.descripcion, l.cantidad, l.precio, l.dto, l.iva
            ));
        }
        s
    }

    fn modificado(&self) -> bool {
        self.firma() != self.firma_inicial
    }

    /// Reconstruye el documento con lo que hay en pantalla.
    fn volcar(&mut self) {
        self.doc.serie = self.serie.trim().to_string();
        self.doc.fecha = parse_fecha(&self.fecha).unwrap_or(self.doc.fecha);
        self.doc.vencimiento = parse_fecha(&self.vencimiento).unwrap_or(self.doc.vencimiento);
        self.doc.irpf_pb = parse_pb(&self.irpf).unwrap_or(0);
        self.doc.lineas = self.lineas.iter().map(LineaBuf::a_linea).collect();
    }

    fn problemas(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.serie.trim().is_empty() {
            v.push("La serie no puede estar vacía.".to_string());
        }
        if parse_fecha(&self.fecha).is_none() {
            v.push(format!("Fecha «{}» no válida (usa dd/mm/aaaa).", self.fecha));
        }
        if parse_fecha(&self.vencimiento).is_none() {
            v.push(format!("Vencimiento «{}» no válido.", self.vencimiento));
        }
        if parse_pb(&self.irpf).is_none() {
            v.push("El IRPF debe ser un número (0–100).".to_string());
        }
        if self.lineas.is_empty() {
            v.push("Añade al menos una línea.".to_string());
        }
        for (i, l) in self.lineas.iter().enumerate() {
            if !l.valida() {
                v.push(format!("Línea {}: cantidad, precio o porcentaje no válidos.", i + 1));
            }
            if l.descripcion.trim().is_empty() {
                v.push(format!("Línea {}: falta la descripción.", i + 1));
            }
        }
        v
    }

    /// Sólo los fallos que impiden guardar: fechas y números mal escritos.
    /// Un borrador puede quedarse sin serie, sin líneas o sin descripción.
    fn errores(&self) -> Vec<String> {
        let mut v = Vec::new();
        if parse_fecha(&self.fecha).is_none() {
            v.push(format!("Fecha «{}» no válida (dd/mm/aaaa).", self.fecha));
        }
        if parse_fecha(&self.vencimiento).is_none() {
            v.push(format!("Vencimiento «{}» no válido.", self.vencimiento));
        }
        if parse_pb(&self.irpf).is_none() {
            v.push("El IRPF debe ser un número entre 0 y 100.".to_string());
        }
        for (i, l) in self.lineas.iter().enumerate() {
            if !l.valida() {
                v.push(format!(
                    "Línea {}: cantidad, precio o porcentaje no válidos.",
                    i + 1
                ));
            }
        }
        v
    }
}

fn parse_fecha(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    NaiveDate::parse_from_str(t, "%d/%m/%Y")
        .ok()
        .or_else(|| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok())
}

/// Abre un fichero con la aplicación asociada del sistema.
#[cfg(windows)]
fn abrir_fichero(ruta: &std::path::Path) {
    // `explorer` con una ruta de fichero delega en la asociación del sistema
    // (el lector de PDF que tenga el usuario).
    let _ = std::process::Command::new("explorer").arg(ruta).spawn();
}

#[cfg(not(windows))]
fn abrir_fichero(ruta: &std::path::Path) {
    let _ = std::process::Command::new("xdg-open").arg(ruta).spawn();
}

/// Abre una página web en el navegador del sistema.
#[cfg(windows)]
fn abrir_url(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}

#[cfg(not(windows))]
fn abrir_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Texto y color del próximo plazo de una obligación, listo para pintar.
fn plazo_texto(
    db: &Db,
    ob: &crate::fiscal::Obligacion,
    anyo: i32,
    hoy: NaiveDate,
) -> Option<(String, egui::Color32)> {
    let (periodo, fecha, dias) = crate::fiscal::proximo_plazo(db, ob, anyo, hoy)?;
    Some(if dias < 0 {
        (
            format!(
                "⚠ Sin presentar: {periodo} · el plazo venció el {} (hace {} días)",
                fecha.format("%d/%m/%Y"),
                -dias
            ),
            tema::ROJO,
        )
    } else if dias == 0 {
        (format!("Hoy vence {periodo}"), tema::ROJO)
    } else {
        (
            format!(
                "Siguiente: {periodo} · hasta el {} (quedan {dias} días)",
                fecha.format("%d/%m/%Y")
            ),
            tema::ACENTO,
        )
    })
}

/// Abre la carpeta con el fichero ya seleccionado.
#[cfg(windows)]
fn mostrar_en_carpeta(ruta: &std::path::Path) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", ruta.display()))
        .spawn();
}

#[cfg(not(windows))]
fn mostrar_en_carpeta(ruta: &std::path::Path) {
    if let Some(dir) = ruta.parent() {
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

/// Enseña el PDF recién generado: abre el documento y su carpeta.
pub fn abrir_pdf_y_carpeta(ruta: &std::path::Path) {
    abrir_fichero(ruta);
    mostrar_en_carpeta(ruta);
}

/// Asistente de puesta en marcha: las preguntas y el paso actual.
struct Asistente {
    paso: usize,
    nombre: String,
    nif: String,
    es_sociedad: bool,
    actividad: Actividad,
    regimen_irpf: RegimenIrpf,
    facturacion: String,
    a_particulares: bool,
    a_empresas: bool,
    a_profesionales: bool,
    a_minoristas: bool,
    a_ue: bool,
    a_isp: bool,
    recargo: bool,
    sii: bool,
    empleados: bool,
    alquiler_local: bool,
    verifactu: bool,
    /// Pide abrir la ayuda de Verifactu desde el asistente.
    ayuda: bool,
}

impl Asistente {
    const TITULOS: [&'static str; 4] = ["Tú", "Tu actividad", "Tus clientes", "Tu plan"];

    fn desde(e: &Empresa) -> Self {
        Self {
            paso: 0,
            nombre: e.nombre.clone(),
            nif: e.nif.clone(),
            es_sociedad: !e.persona_fisica && !e.nombre.trim().is_empty(),
            actividad: e.actividad,
            regimen_irpf: e.regimen_irpf,
            facturacion: String::new(),
            a_particulares: true,
            a_empresas: true,
            a_profesionales: false,
            a_minoristas: false,
            a_ue: false,
            a_isp: false,
            recargo: e.recargo_equivalencia,
            sii: e.sii,
            empleados: false,
            alquiler_local: false,
            verifactu: e.verifactu_activo,
            ayuda: false,
        }
    }

    fn perfil(&self) -> crate::asistente::Perfil {
        crate::asistente::Perfil {
            nombre: self.nombre.clone(),
            nif: self.nif.clone(),
            es_sociedad: self.es_sociedad,
            actividad: self.actividad,
            regimen_irpf: self.regimen_irpf,
            facturacion_cents: parse_cents(&self.facturacion).unwrap_or(0),
            a_particulares: self.a_particulares,
            a_empresas: self.a_empresas,
            a_profesionales: self.a_profesionales,
            a_minoristas: self.a_minoristas,
            a_ue: self.a_ue,
            a_isp: self.a_isp,
            recargo_equivalencia: self.recargo,
            sii: self.sii,
            empleados: self.empleados,
            alquiler_local: self.alquiler_local,
            verifactu: self.verifactu,
        }
    }

    fn paso_quien(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("¿Quién factura?")
                .size(16.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new(
                "Tu nombre y NIF salen en cada factura y son los que aparecen como declarante en los modelos.",
            )
            .size(12.0)
            .color(tema::SUAVE),
        );
        ui.add_space(10.0);
        egui::Grid::new("asis_quien")
            .num_columns(2)
            .spacing([14.0, 10.0])
            .show(ui, |ui| {
                campo_h(
                    ui,
                    "Nombre o razón social",
                    &mut self.nombre,
                    "Como figura en tu alta censal (modelo 036/037).",
                );
                campo_h(
                    ui,
                    "NIF",
                    &mut self.nif,
                    "DNI/NIE si eres autónomo, CIF si es sociedad. Se comprueba la letra de control.",
                );
            });
        if !self.nif.trim().is_empty() && !nif_valido(&self.nif) {
            ui.label(
                RichText::new("Ese NIF no cuadra con su letra de control. Revísalo o cámbialo luego en Ajustes.")
                    .size(11.0)
                    .color(tema::AMBAR),
            );
        }
        ui.add_space(14.0);
        tema::etiqueta_ayuda(
            ui,
            "¿Cómo tributas?",
            "Una sociedad presenta el Impuesto de Sociedades; un autónomo, IRPF con pagos a cuenta 130 o 131.",
        );
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.es_sociedad, false, "Autónomo (persona física)");
            ui.selectable_value(&mut self.es_sociedad, true, "Sociedad (SL, SA, cooperativa)");
        });
        ui.label(
            RichText::new(if self.es_sociedad {
                "Una sociedad tributa por el Impuesto de Sociedades (200/202): no presenta Renta ni 130/131."
            } else {
                "Un autónomo tributa por IRPF: presenta la Renta y adelanta el 20 % con el 130 (o el 131 en módulos)."
            })
            .size(10.5)
            .color(tema::TENUE),
        );
    }

    fn paso_actividad(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("¿A qué te dedicas?")
                .size(16.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new("La actividad decide el IVA que se propone y qué modelos informativos aparecen.")
                .size(12.0)
                .color(tema::SUAVE),
        );
        ui.add_space(10.0);
        ui.label(RichText::new("Actividad").size(12.0).color(tema::SUAVE));
        egui::ComboBox::from_id_salt("asis_actividad")
            .selected_text(self.actividad.etiqueta())
            .width(360.0)
            .show_ui(ui, |ui| {
                for a in Actividad::TODAS {
                    ui.selectable_value(&mut self.actividad, a, a.etiqueta());
                }
            });
        ui.add_space(12.0);
        tema::etiqueta_ayuda(
            ui,
            "Régimen del IRPF",
            "Simplificada es lo normal hasta 600.000 € de facturación. Módulos sólo para actividades y límites concretos. Normal si superas los límites de la simplificada.",
        );
        egui::ComboBox::from_id_salt("asis_irpf")
            .selected_text(self.regimen_irpf.etiqueta())
            .width(360.0)
            .show_ui(ui, |ui| {
                for r in RegimenIrpf::TODOS {
                    ui.selectable_value(&mut self.regimen_irpf, r, r.etiqueta());
                }
            });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(RichText::new("Facturación anual prevista").size(12.0).color(tema::SUAVE));
                tema::icono_ayuda(
                    ui,
                    "Facturación anual prevista",
                    "Una estimación basta: sirve para avisarte del 347, del SII y de la ventanilla única.",
                );
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.facturacion)
                    .desired_width(150.0)
                    .hint_text("25.000,00"),
            );
            ui.label(RichText::new("EUR").size(11.0).color(tema::TENUE));
        });
        ui.add_space(12.0);
        tema::checkbox_ayuda(
            ui,
            &mut self.sii,
            "Estoy obligado al SII (Suministro Inmediato de Información)",
            "Obligatorio para grandes empresas, grupos y quien tribute en régimen de consolidación. Con el SII el 303 es mensual.",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.empleados,
            "Tengo empleados o pago nóminas",
            "Añade el 111 trimestral y el 190 anual.",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.alquiler_local,
            "Pago alquiler de un local con retención",
            "Añade el 115 trimestral y el 180 anual.",
        );
    }

    fn paso_clientes(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("¿A quién facturas?")
                .size(16.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new(
                "Marca todo lo que uses: cada tipo de cliente lleva un IVA y una retención distintos y se puede fijar en su ficha.",
            )
            .size(12.0)
            .color(tema::SUAVE),
        );
        ui.add_space(8.0);
        tema::checkbox_ayuda(
            ui,
            &mut self.a_particulares,
            "Particulares",
            "Factura con IVA y sin retención. Si no dan NIF, sale simplificada.",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.a_empresas,
            "Empresas y sociedades españolas",
            "IVA del 21 % (o el reducido que corresponda) y sin retención si no eres profesional.",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.a_profesionales,
            "Autónomos profesionales que me retienen",
            "Te retienen el 15 % de IRPF en la factura (7 % los tres primeros años).",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.a_minoristas,
            "Comercios minoristas en recargo de equivalencia",
            "Les repercutes IVA + 5,2 % de recargo, que tú ingresas en el 303.",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.a_ue,
            "Empresas de la UE",
            "Factura sin IVA con su NIF-IVA validado en VIES; la operación va al 349.",
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.a_isp,
            "Clientes con inversión del sujeto pasivo",
            "Factura sin IVA con leyenda; el IVA lo declara el cliente.",
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new("Y de tu lado…")
                .size(12.0)
                .strong()
                .color(tema::SUAVE),
        );
        tema::checkbox_ayuda(
            ui,
            &mut self.recargo,
            "Soy comerciante minorista y compro a mayoristas con recargo de equivalencia",
            "Entonces no deduces el IVA de tus compras y no presentas 303 ni 390: sólo el 309 si compras a la UE o hay ISP.",
        );
        if self.recargo {
            ui.label(
                RichText::new(
                    "El recargo no te exime del IRPF: si eres persona física, el 130/131 sigue igual.",
                )
                .size(10.5)
                .color(tema::TENUE),
            );
        }
        tema::checkbox_ayuda(
            ui,
            &mut self.verifactu,
            "Quiero el registro Verifactu con QR en cada factura",
            "Registro local encadenado y QR tributario de cotejo (no verificable). No envía nada a la AEAT: la remisión exige un sistema certificado. Recomendado: deja las facturas preparadas para la normativa antifraude.",
        );
        if ui
            .small_button("¿Cómo funciona Verifactu?")
            .on_hover_text("Qué se encadena, qué significa el QR y qué no hace la app.")
            .clicked()
        {
            self.ayuda = true;
        }
    }

    fn paso_plan(&mut self, ui: &mut egui::Ui) {
        let plan = crate::asistente::plan(&self.perfil());
        ui.label(
            RichText::new("Tu plan")
                .size(16.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(6.0);
        for l in &plan.resumen {
            ui.label(RichText::new(format!("· {l}")).size(12.5).color(tema::TEXTO));
        }
        ui.add_space(14.0);
        ui.label(
            RichText::new("Modelos que tendrás que presentar")
                .size(13.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        for o in &plan.obligaciones {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(o.codigo).strong().color(tema::ACENTO));
                ui.label(RichText::new(o.nombre).strong().color(tema::TEXTO));
                ui.label(
                    RichText::new(format!("· {}", o.plazo))
                        .size(11.5)
                        .color(tema::SUAVE),
                );
            });
            ui.label(RichText::new(o.detalle).size(11.0).color(tema::TENUE));
            ui.add_space(5.0);
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("Consejos para no equivocarte")
                .size(13.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        for c in &plan.consejos {
            ui.label(RichText::new(format!("· {c}")).size(11.5).color(tema::SUAVE));
        }
        ui.add_space(12.0);
        ui.label(
            RichText::new("Tu rutina, en orden")
                .size(13.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        for r in crate::asistente::rutina(&self.perfil()) {
            ui.label(RichText::new(format!("· {r}")).size(11.5).color(tema::SUAVE));
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(
                "Puedes cambiar todo esto en Ajustes y repetir el asistente cuando quieras. No sustituye a un asesor, pero te deja el terreno preparado.",
            )
            .size(11.0)
            .color(tema::TENUE),
        );
    }
}

pub struct App {
    pub db: Db,
    pub vista: Vista,
    /// Equipo detectado al arrancar; se muestra en Ajustes.
    pub perfil: Perfil,
    pub aviso: Option<String>,
    pub toast: Option<(String, bool)>,
    asistente: Option<Asistente>,
    /// Lectura del tutorial con la voz del sistema.
    voz: crate::voz::Voz,
    /// Último paso leído, para no repetirlo en cada fotograma.
    paso_voz: usize,
    pub seleccion: Option<u64>,
    editor: Option<Editor>,
    ficha_cliente: Option<Cliente>,
    ficha_empleado: Option<EmpleadoBuf>,
    ficha_articulo: Option<ArticuloBuf>,
    ficha_gasto: Option<GastoBuf>,
    /// 0 = fichas de empleados, 1 = nóminas.
    pestana_empleados: u8,
    nomina_anyo: i32,
    nomina_mes: u32,
    /// Año y trimestre que se miran en Contabilidad (0 = todo el año).
    anyo: i32,
    trimestre: u32,
    /// El zoom guardado se aplica una sola vez, al primer fotograma.
    zoom_aplicado: bool,
    busqueda: String,
    /// Cobro en curso: (documento, fecha, importe) en texto.
    cobro: Option<(u64, String, String)>,
    /// Pestaña de Contabilidad: 0 resumen, 1 IVA, 2 IRPF, 3 347, 4 calendario, 5 clientes.
    pestana_contab: u8,
    /// Copia de seguridad pendiente de confirmar.
    restaurar: Option<PathBuf>,
    anular: Option<(u64, String)>,
    rectificar: Option<RectificarBuf>,
    ver_xml: Option<(String, String)>,
    /// Ventana «Cómo funciona Verifactu» abierta.
    ayuda_verifactu: bool,
    confirmar: Option<String>,
    /// Si hay cambios sin guardar: a dónde se quería ir al aceptar el aviso.
    descartar: Option<Vista>,
    /// Sólo con `AUTONOMOFACIL_PERF=1`: mide y se cierra solo.
    bench: Option<Benchmark>,
}

impl App {
    pub fn new(
        db: Db,
        aviso: Option<String>,
        perfil: Perfil,
        bench: Option<Benchmark>,
    ) -> Self {
        let mut toast = None;
        let asistente = if db.asistente_completado {
            None
        } else {
            Some(Asistente::desde(&db.empresa))
        };
        let vista = Vista::Inicio;
        if asistente.is_none() && !db.empresa.completa() {
            toast = Some((
                "Configura tu empresa en Ajustes antes de emitir la primera factura.".to_string(),
                false,
            ));
        }
        if let Some(a) = &aviso {
            toast = Some((a.clone(), true));
        }
        Self {
            db,
            vista,
            perfil,
            aviso,
            toast,
            asistente,
            voz: crate::voz::Voz::default(),
            paso_voz: usize::MAX,
            seleccion: None,
            editor: None,
            ficha_cliente: None,
            ficha_empleado: None,
            ficha_articulo: None,
            ficha_gasto: None,
            pestana_empleados: 0,
            nomina_anyo: Local::now().year(),
            nomina_mes: Local::now().month(),
            anyo: Local::now().year(),
            trimestre: 0,
            zoom_aplicado: false,
            busqueda: String::new(),
            cobro: None,
            pestana_contab: 0,
            restaurar: None,
            anular: None,
            rectificar: None,
            ver_xml: None,
            ayuda_verifactu: false,
            confirmar: None,
            descartar: None,
            bench,
        }
    }

    fn avisar(&mut self, msg: impl Into<String>, error: bool) {
        self.toast = Some((msg.into(), error));
    }

    fn guardar(&mut self) {
        if let Err(e) = self.db.guardar() {
            self.avisar(format!("No se pudo guardar: {e}"), true);
        }
    }

    fn ultima_huella(&self) -> String {
        self.db
            .registros()
            .last()
            .map(|r| r.huella.clone())
            .unwrap_or_default()
    }

    /// Asigna número, congela el cliente y genera el registro encadenado.
    fn emitir(&mut self, id: u64) -> Result<(), String> {
        if !self.db.empresa.completa() {
            return Err("Completa los datos de tu empresa en Ajustes antes de emitir.".into());
        }
        if self.db.empresa.verifactu_activo && !nif_valido(&self.db.empresa.nif) {
            return Err(
                "El NIF de tu empresa no es válido: la huella y el QR quedarían mal. Corrígelo en Ajustes."
                    .into(),
            );
        }
        let (tipo, serie, ya_numerado, cliente_id, lineas) = {
            let d = self.db.documento(id).ok_or("El documento no existe.")?;
            (
                d.tipo,
                d.serie.clone(),
                d.numero != 0,
                d.cliente_id,
                d.lineas.len(),
            )
        };
        if ya_numerado {
            return Err("Este documento ya está emitido.".into());
        }
        if lineas == 0 {
            return Err("Una factura sin líneas no se puede emitir.".into());
        }
        let fecha_doc = self
            .db
            .documento(id)
            .map(|d| d.fecha)
            .unwrap_or_else(|| Local::now().date_naive());
        if self.db.esta_cerrado(fecha_doc.year()) {
            return Err(format!(
                "El ejercicio {} está cerrado. Reábrelo en Contabilidad para emitir con esa fecha.",
                fecha_doc.year()
            ));
        }
        let numero = self.db.siguiente_numero(tipo, &serie);
        let huella_previa = self.ultima_huella();
        let nif_emisor = self.db.empresa.nif.clone();
        let cliente_snap = self
            .db
            .clientes
            .iter()
            .find(|c| c.id == cliente_id)
            .cloned();

        if let Some(d) = self.db.documento_mut(id) {
            d.numero = numero;
            d.estado = Estado::Emitido;
            if let Some(c) = cliente_snap {
                d.cliente = c;
            }
        }

        // Sólo las facturas entran en la cadena: una proforma no es fiscal.
        if tipo.es_fiscal() && self.db.empresa.verifactu_activo {
            let registro = {
                let d = self.db.documento(id).ok_or("El documento no existe.")?;
                verifactu::registro_alta(d, &nif_emisor, &huella_previa)
            };
            if let Some(d) = self.db.documento_mut(id) {
                d.verifactu = Some(registro);
            }
        }
        self.guardar();
        Ok(())
    }

    fn anular(&mut self, id: u64, motivo: String) -> Result<(), String> {
        let lleva_registro = self
            .db
            .documento(id)
            .map(|d| d.tipo.es_fiscal() && d.verifactu.is_some())
            .unwrap_or(false);
        if lleva_registro && !nif_valido(&self.db.empresa.nif) {
            return Err(
                "El NIF de tu empresa no es válido: la huella de la anulación quedaría mal. Corrígelo en Ajustes."
                    .into(),
            );
        }
        let nif = self.db.empresa.nif.clone();
        let huella_previa = self.ultima_huella();
        let registro = {
            let d = self.db.documento(id).ok_or("El documento no existe.")?;
            if d.estado == Estado::Anulado {
                return Err("Ya está anulada.".into());
            }
            if d.numero == 0 {
                return Err("Un borrador no se anula: se borra.".into());
            }
            if d.rectificada_por.is_some() {
                return Err(
                    "Esta factura está corregida por una R1. Anula primero la rectificativa y después la original."
                        .into(),
                );
            }
            // Presupuestos y proformas no están en la cadena de la AEAT. Una
            // factura emitida antes de activar Verifactu tampoco tiene alta:
            // no se le inventa una anulación suelta.
            if d.tipo.es_fiscal() && d.verifactu.is_some() {
                Some(verifactu::registro_anulacion(d, &nif, &huella_previa))
            } else {
                None
            }
        };
        if let Some(d) = self.db.documento_mut(id) {
            d.estado = Estado::Anulado;
            d.motivo_anulacion = motivo;
            if registro.is_some() {
                // Se conserva el registro de alta y se añade el de anulación.
                d.verifactu_anulacion = registro;
            }
        }
        // Si se anula una R1, la original deja de estar corregida por ella.
        // Si había otra R1 activa (por ejemplo, una por cada tipo de IVA),
        // el puntero pasa a esa otra en vez de borrarse sin más.
        if let Some(rid) = self
            .db
            .documento(id)
            .and_then(|d| d.rectifica.as_ref())
            .map(|r| r.documento_id)
        {
            let otra = self
                .db
                .documentos
                .iter()
                .filter(|x| {
                    x.id != id
                        && x.estado != Estado::Anulado
                        && x.rectifica
                            .as_ref()
                            .map(|r| r.documento_id == rid)
                            .unwrap_or(false)
                })
                .map(|x| x.id)
                .last();
            if let Some(o) = self.db.documento_mut(rid) {
                if o.rectificada_por == Some(id) {
                    o.rectificada_por = otra;
                }
            }
        }
        self.guardar();
        Ok(())
    }

    fn marcar_cobrado(&mut self, id: u64) {
        if let Some(d) = self.db.documento_mut(id) {
            d.estado = Estado::Pagado;
        }
        self.guardar();
    }

    fn convertir_en_factura(&mut self, id: u64) -> Result<(), String> {
        let original = self
            .db
            .documento(id)
            .cloned()
            .ok_or("El presupuesto no existe.")?;
        if original.tipo == TipoDoc::Factura {
            return Err("Una factura no se convierte.".into());
        }
        let mut doc = Documento::nuevo(
            TipoDoc::Factura,
            original.cliente.clone(),
            original.irpf_pb,
            self.db.empresa.serie.clone(),
        );
        doc.cliente_id = original.cliente_id;
        doc.lineas = original.lineas.clone();
        doc.notas = format!(
            "Según {} {}.",
            original.tipo.nombre().to_lowercase(),
            original.etiqueta_numero()
        );
        doc.forma_pago = original.forma_pago.clone();
        doc.id = self.db.id();
        let nuevo_id = doc.id;
        self.db.documentos.push(doc);
        self.guardar();
        self.seleccion = Some(nuevo_id);
        Ok(())
    }

    /// Escribe el PDF sin tocar la base: así sirve para la vista previa de un
    /// documento que todavía no se ha guardado.
    fn escribir_pdf(&self, doc: &Documento) -> Result<PathBuf, String> {
        let bytes = pdf::documento(doc, &self.db.empresa);
        let dir = export::dir_documento(doc, &self.db.empresa);
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(pdf::nombre_fichero(doc));
        std::fs::write(&ruta, bytes).map_err(|e| format!("escribir PDF: {e}"))?;
        Ok(ruta)
    }

    fn exportar_pdf(&mut self, id: u64) -> Result<PathBuf, String> {
        let doc = self
            .db
            .documento(id)
            .cloned()
            .ok_or("El documento no existe.")?;
        let ruta = self.escribir_pdf(&doc)?;
        if let Some(d) = self.db.documento_mut(id) {
            d.pdf = ruta.display().to_string();
        }
        self.guardar();
        // Ver el PDF es abrirlo: documento y carpeta, sin pasos extra.
        abrir_pdf_y_carpeta(&ruta);
        Ok(ruta)
    }

    fn exportar_nomina_pdf(&mut self, id: u64) -> Result<PathBuf, String> {
        let n = self
            .db
            .nominas
            .iter()
            .find(|n| n.id == id)
            .cloned()
            .ok_or("La nómina no existe.")?;
        let e = self
            .db
            .empleado(n.empleado_id)
            .cloned()
            .ok_or("El empleado de esta nómina ya no existe.")?;
        let bytes = pdf::nomina(&n, &e, &self.db.empresa);
        let dir = export::dir_nomina(&self.db.empresa, &e);
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(export::nombre_nomina(&n, &e));
        std::fs::write(&ruta, bytes).map_err(|e| format!("escribir PDF: {e}"))?;
        abrir_pdf_y_carpeta(&ruta);
        Ok(ruta)
    }

    fn exportar_xml(&mut self, id: u64) -> Result<PathBuf, String> {
        let empresa = self.db.empresa.clone();
        let doc = self
            .db
            .documento(id)
            .cloned()
            .ok_or("El documento no existe.")?;
        if doc.numero == 0 {
            return Err("Emite la factura antes de exportar el FacturaE.".into());
        }
        let xml = facturae::generar(&doc, &empresa);
        let nombre = facturae::nombre_fichero(&doc);
        let dir = export::dir_documento(&doc, &empresa);
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(&nombre);
        std::fs::write(&ruta, xml.clone()).map_err(|e| format!("escribir XML: {e}"))?;
        self.ver_xml = Some((nombre, xml));
        Ok(ruta)
    }
}

// ---------------------------------------------------------------------------
// Interfaz
// ---------------------------------------------------------------------------

/// Campo con explicación: la etiqueta lleva un icono «?» y tanto el icono
/// como el campo muestran la misma tarjeta de ayuda al pasar el ratón.
fn campo_h(ui: &mut egui::Ui, etiqueta: &str, valor: &mut String, ayuda: &str) {
    if ayuda.is_empty() {
        ui.label(RichText::new(etiqueta).size(12.0).color(tema::SUAVE));
    } else {
        tema::etiqueta_ayuda(ui, etiqueta, ayuda);
    }
    // add_sized fija el tamaño real: dentro de una Grid, desired_width se
    // recortaba y el campo quedaba minúsculo.
    let ancho = (ui.available_width() - 12.0).clamp(220.0, 460.0);
    let mut resp = ui.add_sized(
        [ancho, 30.0],
        egui::TextEdit::singleline(valor).margin(egui::Margin::symmetric(10.0, 7.0)),
    );
    if !ayuda.is_empty() {
        resp = resp.on_hover_ui(|ui| tema::ayuda_emergente(ui, etiqueta, ayuda));
    }
    let _ = resp;
    ui.end_row();
}

/// Tabla de bases y cuotas por tipo de IVA.
fn tabla_tipos(ui: &mut egui::Ui, filas: &[crate::contab::TipoIva]) {
    if filas.is_empty() {
        ui.label(RichText::new("Sin operaciones.").color(tema::TENUE));
        return;
    }
    egui::Grid::new(ui.next_auto_id())
        .num_columns(3)
        .spacing([18.0, 4.0])
        .show(ui, |ui| {
            for t in ["Tipo", "Base", "Cuota"] {
                ui.label(RichText::new(t).size(11.0).strong().color(tema::TENUE));
            }
            ui.end_row();
            for f in filas {
                ui.label(format!("{} %", fmt_pb(f.pb)));
                ui.label(RichText::new(fmt_cents(f.base)).monospace());
                ui.label(RichText::new(fmt_cents(f.cuota)).monospace());
                ui.end_row();
            }
        });
}

fn fila_total(ui: &mut egui::Ui, etiqueta: &str, importe: i64, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(etiqueta).strong().color(tema::SUAVE));
        ui.label(RichText::new(fmt_cents_eur(importe)).strong().color(color));
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(bench) = &mut self.bench {
            if bench.tick(ctx) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        self.sincronizar_zoom(ctx);

        // La barra lateral se adapta al ancho de la ventana y se puede
        // arrastrar: en pantallas pequeñas ocupa menos y deja sitio al
        // contenido, y en las grandes no desperdicia espacio.
        let ancho_nav = (ctx.screen_rect().width() * 0.24).clamp(158.0, 232.0);
        let mut zoom_nuevo: Option<f32> = None;
        egui::SidePanel::left("navegacion")
            .default_width(ancho_nav)
            .width_range(150.0..=280.0)
            .resizable(true)
            .frame(
                egui::Frame::none()
                    .fill(tema::SUPERFICIE)
                    .inner_margin(egui::Margin::symmetric(12.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("AutónomoFACIL")
                        .size(19.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                ui.label(
                    RichText::new(format!("versión {}", crate::version::VERSION))
                        .size(11.0)
                        .color(tema::TENUE),
                );

                // Zoom: además de Ctrl + rueda y Ctrl + / −, que ya trae
                // egui, aquí hay botones para ratón sin rueda.
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Zoom").size(10.5).color(tema::TENUE));
                    if ui.small_button("−").on_hover_text("Alejar (Ctrl + −)").clicked() {
                        zoom_nuevo = Some(ctx.zoom_factor() - 0.1);
                    }
                    ui.label(
                        RichText::new(format!("{:.0} %", ctx.zoom_factor() * 100.0))
                            .size(10.5)
                            .color(tema::SUAVE),
                    );
                    if ui.small_button("+").on_hover_text("Acercar (Ctrl + +)").clicked() {
                        zoom_nuevo = Some(ctx.zoom_factor() + 0.1);
                    }
                    if ui.small_button("100 %").on_hover_text("Tamaño normal").clicked() {
                        zoom_nuevo = Some(1.0);
                    }
                });
                ui.add_space(18.0);

                let mut cambiar: Option<Vista> = None;
                tema::grupo_nav(ui, "GESTIÓN");
                for (v, t) in [
                    (Vista::Inicio, "Inicio"),
                    (Vista::Presupuestos, "Presupuestos"),
                    (Vista::Proformas, "Proformas"),
                    (Vista::Facturas, "Facturas"),
                ] {
                    if tema::nav(ui, self.vista == v, t).clicked() {
                        cambiar = Some(v);
                    }
                }
                ui.add_space(12.0);
                tema::grupo_nav(ui, "DATOS");
                for (v, t) in [
                    (Vista::Clientes, "Clientes"),
                    (Vista::Empleados, "Empleados"),
                    (Vista::Articulos, "Artículos"),
                    (Vista::Contabilidad, "Contabilidad"),
                    (Vista::Modelos, "Modelos e impuestos"),
                ] {
                    if tema::nav(ui, self.vista == v, t).clicked() {
                        cambiar = Some(v);
                    }
                }
                ui.add_space(12.0);
                tema::grupo_nav(ui, "SISTEMA");
                if tema::nav(ui, self.vista == Vista::Guia, "Guía rápida").clicked() {
                    cambiar = Some(Vista::Guia);
                }
                if tema::nav(ui, self.vista == Vista::Ajustes, "Ajustes").clicked() {
                    cambiar = Some(Vista::Ajustes);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(2.0);
                    if self.db.empresa.completa() {
                        ui.label(RichText::new(&self.db.empresa.nif).size(11.0).color(tema::TENUE));
                        ui.label(
                            RichText::new(&self.db.empresa.nombre)
                                .size(12.0)
                                .strong()
                                .color(tema::TEXTO),
                        );
                    } else if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Configura tu empresa").size(12.0),
                            )
                            .fill(tema::AMBAR_CLARO)
                            .rounding(egui::Rounding::same(8.0)),
                        )
                        .clicked()
                    {
                        cambiar = Some(Vista::Ajustes);
                    }
                });

                if let Some(v) = cambiar {
                    // Si hay un documento abierto con cambios, se pregunta antes
                    // de perderlos: primero se avisa, no se cierra a ciegas.
                    if self
                        .editor
                        .as_ref()
                        .map(|e| e.modificado())
                        .unwrap_or(false)
                    {
                        self.descartar = Some(v);
                    } else {
                        self.vista = v;
                        self.editor = None;
                        self.seleccion = None;
                        self.voz.parar();
                    }
                }
            });

        if let Some(z) = zoom_nuevo {
            let z = z.clamp(0.6, 2.0);
            ctx.set_zoom_factor(z);
            self.db.zoom = z;
            self.guardar();
        }

        if let Some((msg, error)) = self.toast.clone() {
            let (fondo, color) = if error {
                (tema::ROJO_CLARO, tema::ROJO)
            } else {
                (tema::VERDE_CLARO, tema::VERDE)
            };
            egui::TopBottomPanel::bottom("toast")
                .frame(
                    egui::Frame::none()
                        .fill(fondo)
                        .inner_margin(egui::Margin::symmetric(18.0, 10.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&msg).size(12.5).strong().color(color));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("cerrar").clicked() {
                                self.toast = None;
                            }
                        });
                    });
                });
        }

        // Panel de detalle del documento seleccionado.
        if matches!(
            self.vista,
            Vista::Facturas | Vista::Presupuestos | Vista::Proformas
        ) && self.editor.is_none()
        {
            if let Some(id) = self.seleccion {
                if self.db.documento(id).is_some() {
                    let mut acciones: Vec<(&str, &str)> = Vec::new();
                    if let Some(d) = self.db.documento(id) {
                        if d.editable() {
                            acciones.push(("editar", "Editar"));
                            acciones.push(("emitir", "Emitir"));
                        }
                        acciones.push(("pdf", "PDF"));
                        acciones.push(("duplicar", "Duplicar"));
                        if d.numero > 0 && d.tipo.es_fiscal() && d.rectifica.is_none() {
                            acciones.push(("xml", "FacturaE"));
                        }
                        if matches!(d.estado, Estado::Emitido | Estado::Pagado)
                            && d.tipo.es_fiscal()
                            && d.rectifica.is_none()
                        {
                            acciones.push(("rectificar", "Rectificar R1"));
                        }
                        if d.tipo != TipoDoc::Factura {
                            acciones.push(("convertir", "Crear factura"));
                        }
                        if matches!(d.estado, Estado::Emitido) {
                            if d.pendiente_cents() > 0 {
                                acciones.push(("cobro", "Registrar cobro"));
                            }
                            acciones.push(("cobrado", "Marcar cobrada"));
                            acciones.push(("anular", "Anular"));
                        }
                    }
                    egui::SidePanel::right("detalle")
                        .resizable(true)
                        .width_range(240.0..=520.0)
                        .default_width(if ctx.screen_rect().width() < 1180.0 {
                            270.0
                        } else {
                            330.0
                        })
                        .show(ctx, |ui| {
                            self.detalle(ctx, ui, id, acciones);
                        });
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.vista {
                // Inicio y Guía no traían desplazamiento propio: se envuelven
                // aquí, con barra horizontal incluida, para que en pantallas
                // pequeñas no se corte nada.
                Vista::Inicio => {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.vista_inicio(ui));
                }
                Vista::Modelos => {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.vista_modelos(ui));
                }
                Vista::Guia => {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.vista_guia(ui));
                }
                Vista::Presupuestos => self.vista_lista(ui, TipoDoc::Presupuesto),
                Vista::Proformas => self.vista_lista(ui, TipoDoc::Proforma),
                Vista::Facturas => self.vista_lista(ui, TipoDoc::Factura),
                Vista::Clientes => self.vista_clientes(ui),
                Vista::Empleados => self.vista_empleados(ui),
                Vista::Articulos => self.vista_articulos(ui),
                Vista::Contabilidad => self.vista_contabilidad(ui),
                Vista::Ajustes => self.vista_ajustes(ui),
            }
        });

        if self.anular.is_some() {
            self.ventana_anular(ctx);
        }
        if self.rectificar.is_some() {
            self.ventana_rectificar(ctx);
        }
        if self.cobro.is_some() {
            self.ventana_cobro(ctx);
        }
        if self.restaurar.is_some() {
            self.ventana_restaurar(ctx);
        }
        if self.ver_xml.is_some() {
            self.ventana_xml(ctx);
        }
        if self.asistente.is_some() {
            self.ventana_asistente(ctx);
        }
        if self.descartar.is_some() {
            self.ventana_descartar(ctx);
        }
        if self.ayuda_verifactu {
            self.ventana_ayuda_verifactu(ctx);
        }
    }
}

impl App {
    /// Aplica al arrancar el zoom guardado y, si el usuario lo cambia con
    /// Ctrl + rueda o Ctrl + / −, lo guarda para la próxima vez.
    fn sincronizar_zoom(&mut self, ctx: &egui::Context) {
        if !self.zoom_aplicado {
            let z = if self.db.zoom > 0.4 { self.db.zoom } else { 1.0 };
            let z = z.clamp(0.6, 2.0);
            ctx.set_zoom_factor(z);
            self.db.zoom = z;
            self.zoom_aplicado = true;
            return;
        }
        let actual = ctx.zoom_factor();
        if (actual - self.db.zoom).abs() > 0.005 {
            self.db.zoom = actual;
            self.guardar();
        }
    }

    fn detalle(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui, id: u64, acciones: Vec<(&str, &str)>) {
        let doc = match self.db.documento(id) {
            Some(d) => d.clone(),
            None => return,
        };
        let t = doc.totales();
        ui.add_space(2.0);
        ui.label(RichText::new(doc.etiqueta_tipo()).size(11.5).color(tema::SUAVE));
        ui.label(
            RichText::new(doc.etiqueta_numero())
                .size(19.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if doc.rectificada_por.is_some() && doc.estado != Estado::Anulado {
                tema::insignia(ui, "Rectificada", tema::AMBAR_CLARO, tema::AMBAR);
            } else {
                tema::insignia_estado(ui, doc.estado);
            }
            ui.label(
                RichText::new(doc.fecha.format("%d/%m/%Y").to_string())
                    .size(11.5)
                    .color(tema::SUAVE),
            );
        });
        if let Some(r) = &doc.rectifica {
            ui.label(
                RichText::new(format!(
                    "Rectifica la factura {} del {}.",
                    if r.etiqueta.is_empty() { &r.num_serie } else { &r.etiqueta },
                    r.fecha.format("%d/%m/%Y")
                ))
                .size(11.0)
                .color(tema::SUAVE),
            );
            if !doc.motivo_rectificacion.trim().is_empty() {
                ui.label(
                    RichText::new(format!("Motivo: {}", doc.motivo_rectificacion))
                        .size(10.5)
                        .color(tema::TENUE),
                );
            }
        }
        if let Some(rid) = doc.rectificada_por {
            if let Some(r1) = self.db.documento(rid) {
                ui.label(
                    RichText::new(format!(
                        "Corregida por la rectificativa {}.",
                        r1.etiqueta_numero()
                    ))
                    .size(11.0)
                    .color(tema::AMBAR),
                );
            }
        }
        ui.add_space(10.0);
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("TOTAL").size(10.5).strong().color(tema::TENUE));
            ui.label(
                RichText::new(fmt_cents_eur(t.total))
                    .size(24.0)
                    .strong()
                    .color(tema::ACENTO),
            );
        });
        ui.add_space(8.0);

        if doc.tipo.es_fiscal() && doc.numero > 0 && doc.estado != Estado::Anulado {
            let pendiente = doc.pendiente_cents();
            egui::Grid::new("cobros_doc")
                .num_columns(2)
                .min_col_width(120.0)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Cobrado").size(11.5).color(tema::SUAVE));
                    ui.label(
                        RichText::new(fmt_cents_eur(doc.cobrado_cents())).monospace(),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Pendiente").size(11.5).color(tema::SUAVE));
                    ui.label(
                        RichText::new(fmt_cents_eur(pendiente))
                            .monospace()
                            .color(if pendiente > 0 { tema::AMBAR } else { tema::VERDE }),
                    );
                    ui.end_row();
                });
            ui.add_space(8.0);
        }
        ui.label(RichText::new(doc.cliente.nombre_completo()).strong());
        if !doc.cliente.nif.is_empty() {
            ui.label(RichText::new(&doc.cliente.nif).small());
        }
        ui.add_space(6.0);

        egui::Grid::new("resumen_detalle").num_columns(2).show(ui, |ui| {
            ui.label("Base");
            ui.label(fmt_cents(t.base_total));
            ui.end_row();
            for (tipo, _, cuota) in &t.bases {
                ui.label(format!("IVA {}", fmt_pb(*tipo)));
                ui.label(fmt_cents(*cuota));
                ui.end_row();
            }
            if doc.irpf_pb > 0 {
                ui.label(format!("IRPF {}", fmt_pb(doc.irpf_pb)));
                ui.label(format!("-{}", fmt_cents(t.retencion)));
                ui.end_row();
            }
        });

        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            for (clave, texto) in acciones {
                let pulsado = if matches!(clave, "emitir" | "convertir") {
                    tema::boton_primario(ui, texto).clicked()
                } else {
                    tema::boton_suave(ui, texto).clicked()
                };
                if pulsado {
                    match clave {
                        "editar" => {
                            if let Some(d) = self.db.documento(id).cloned() {
                                self.editor = Some(Editor::desde(d, false));
                            }
                        }
                        "duplicar" => self.duplicar(id),
                        "emitir" => match self.emitir(id) {
                            Ok(()) => self.avisar("Factura emitida y encadenada.", false),
                            Err(e) => self.avisar(e, true),
                        },
                        "pdf" => match self.exportar_pdf(id) {
                            Ok(p) => self.avisar(
                                format!(
                                    "PDF abierto: {}",
                                    p.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("documento.pdf")
                                ),
                                false,
                            ),
                            Err(e) => self.avisar(e, true),
                        },
                        "xml" => match self.exportar_xml(id) {
                            Ok(p) => self.avisar(format!("FacturaE: {}", p.display()), false),
                            Err(e) => self.avisar(e, true),
                        },
                        "convertir" => match self.convertir_en_factura(id) {
                            Ok(()) => self.avisar("Factura creada a partir del presupuesto.", false),
                            Err(e) => self.avisar(e, true),
                        },
                        "cobro" => {
                            let pendiente = self
                                .db
                                .documento(id)
                                .map(|d| d.pendiente_cents())
                                .unwrap_or(0);
                            self.cobro = Some((
                                id,
                                Local::now().format("%d/%m/%Y").to_string(),
                                fmt_cents(pendiente),
                            ));
                        }
                        "cobrado" => self.marcar_cobrado(id),
                        "anular" => self.anular = Some((id, String::new())),
                        "rectificar" => {
                            if let Some(d) = self.db.documento(id) {
                                let iva = d
                                    .totales()
                                    .bases
                                    .first()
                                    .map(|(t, _, _)| *t)
                                    .unwrap_or(self.db.empresa.iva_defecto);
                                self.rectificar = Some(RectificarBuf {
                                    original_id: id,
                                    base: String::new(),
                                    iva: fmt_pb(iva),
                                    motivo: String::new(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        });

        if let Some(reg) = &doc.verifactu {
            ui.add_space(12.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Registro Verifactu").small().strong());
                tema::icono_ayuda(
                    ui,
                    "Registro Verifactu",
                    "El alta queda encadenada con la huella anterior. El QR es de cotejo y no remite nada a la AEAT: la app no está conectada a la Sede.",
                );
            });
            egui::Grid::new("reg_verifactu").num_columns(2).show(ui, |ui| {
                ui.label("Registro");
                ui.label(&reg.tipo_registro);
                ui.end_row();
                ui.label("Factura");
                ui.label(doc.tipo_factura_aeat());
                ui.end_row();
                if doc.es_rectificativa() {
                    ui.label("Rectifica");
                    ui.label(reg.facturas_rectificadas.join(", "));
                    ui.end_row();
                }
                ui.label("Nº serie");
                ui.label(&reg.num_serie);
                ui.end_row();
                ui.label("Encadenamiento");
                ui.label(&reg.encadenamiento);
                ui.end_row();
                ui.label("Huella");
                let mut h = reg.huella.clone();
                h.truncate(16);
                ui.label(RichText::new(format!("{}…", h)).monospace().small());
                ui.end_row();
            });
            ui.label(
                RichText::new("QR tributario:")
                    .small()
                    .strong(),
            );
            if let Ok(code) = qrcode::QrCode::with_error_correction_level(
                reg.qr_url.as_bytes(),
                qrcode::EcLevel::M,
            ) {
                let n = code.width();
                let colores = code.to_colors();
                let lado = 128.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(lado, lado), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                // Fondo blanco y zona de silencio: en tema oscuro un QR negro
                // sobre el fondo no se podría leer.
                painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
                let silencio = 8.0;
                let m = (lado - 2.0 * silencio) / n as f32;
                for fy in 0..n {
                    for fx in 0..n {
                        if colores[fy * n + fx] == qrcode::Color::Dark {
                            painter.rect_filled(
                                egui::Rect::from_min_size(
                                    rect.min
                                        + egui::vec2(
                                            silencio + fx as f32 * m,
                                            silencio + fy as f32 * m,
                                        ),
                                    egui::vec2(m + 0.4, m + 0.4),
                                ),
                                0.0,
                                egui::Color32::BLACK,
                            );
                        }
                    }
                }
            }
            ui.label(
                RichText::new("Registro local encadenado · no remitido a la AEAT")
                    .small()
                    .color(egui::Color32::from_gray(110)),
            );
            ui.label(
                RichText::new(
                    "El QR es de cotejo (modo no VERI*FACTU): no envía nada y no acredita por sí solo que la factura esté validada en la Sede. Para remitir haría falta un certificado cualificado y la declaración responsable del productor.",
                )
                .size(10.0)
                .color(tema::TENUE),
            );
            if ui
                .small_button("¿Cómo funciona Verifactu?")
                .on_hover_text("Qué se encadena, qué significa el QR y qué no hace la app.")
                .clicked()
            {
                self.ayuda_verifactu = true;
            }
        }
        if let Some(reg) = &doc.verifactu_anulacion {
            ui.add_space(10.0);
            ui.label(
                RichText::new("Registro de anulación Verifactu")
                    .small()
                    .strong(),
            );
            egui::Grid::new("reg_verifactu_anulacion")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Tipo");
                    ui.label(&reg.tipo_registro);
                    ui.end_row();
                    ui.label("Nº serie");
                    ui.label(&reg.num_serie);
                    ui.end_row();
                    ui.label("Encadenamiento");
                    ui.label(&reg.encadenamiento);
                    ui.end_row();
                    ui.label("Huella");
                    let mut h = reg.huella.clone();
                    h.truncate(16);
                    ui.label(RichText::new(format!("{}…", h)).monospace().small());
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "El alta se conserva; la anulación va encadenada aparte con importes a cero.",
                )
                .size(10.0)
                .color(tema::TENUE),
            );
        }
    }

    /// Ayuda extensa de Verifactu: qué hace la app, qué no hace y cómo usarlo
    /// sin romper la cadena. Se abre desde Guía rápida, Ajustes, el asistente
    /// y la ficha de una factura.
    fn ventana_ayuda_verifactu(&mut self, ctx: &egui::Context) {
        let mut abierto = true;
        let mut cerrar = false;
        egui::Window::new("Cómo funciona Verifactu")
            .open(&mut abierto)
            .collapsible(false)
            .resizable(true)
            .order(egui::Order::Foreground)
            .default_width(720.0)
            .default_height(720.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Registro encadenado y QR, explicados sin jerga")
                        .size(12.0)
                        .color(tema::SUAVE),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(620.0)
                    .show(ui, |ui| {
                        for (titulo, texto) in crate::verifactu::AYUDA {
                            ui.label(
                                RichText::new(*titulo)
                                    .size(13.5)
                                    .strong()
                                    .color(tema::TEXTO),
                            );
                            ui.label(RichText::new(*texto).size(11.5).color(tema::SUAVE));
                            ui.add_space(10.0);
                        }
                        ui.separator();
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Preguntas rápidas")
                                .size(13.5)
                                .strong()
                                .color(tema::TEXTO),
                        );
                        ui.add_space(4.0);
                        for (pregunta, respuesta) in crate::verifactu::PREGUNTAS {
                            ui.label(
                                RichText::new(format!("¿{pregunta}?"))
                                    .size(11.5)
                                    .strong()
                                    .color(tema::TEXTO),
                            );
                            ui.label(
                                RichText::new(*respuesta).size(11.0).color(tema::SUAVE),
                            );
                            ui.add_space(6.0);
                        }
                    });
                ui.add_space(6.0);
                if ui.button("Entendido").clicked() {
                    cerrar = true;
                }
            });
        if cerrar || !abierto {
            self.ayuda_verifactu = false;
        }
    }

    /// Aviso al salir con cambios sin guardar: guardar, descartar o seguir.
    fn ventana_descartar(&mut self, ctx: &egui::Context) {
        let Some(destino) = self.descartar else {
            return;
        };
        let mut seguir = false;
        let mut descartar = false;
        let mut guardar = false;
        egui::Window::new("Cambios sin guardar")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.set_min_width(430.0);
                ui.label(
                    RichText::new("Este documento tiene cambios sin guardar.")
                        .strong()
                        .color(tema::TEXTO),
                );
                ui.label(
                    RichText::new(
                        "Si sales ahora se pierden. Puedes guardarlo como borrador y seguir donde lo dejaste.",
                    )
                    .size(11.5)
                    .color(tema::SUAVE),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if tema::boton_primario(ui, "Guardar borrador y salir").clicked() {
                        guardar = true;
                    }
                    if ui.button("Descartar cambios").clicked() {
                        descartar = true;
                    }
                    if ui.button("Seguir editando").clicked() {
                        seguir = true;
                    }
                });
            });
        if guardar {
            if self.guardar_borrador_actual() {
                self.descartar = None;
                self.vista = destino;
            }
        }
        if descartar {
            self.descartar = None;
            self.editor = None;
            self.seleccion = None;
            self.vista = destino;
            self.avisar("Cambios descartados.", false);
        }
        if seguir {
            self.descartar = None;
        }
    }

    /// Guarda lo que hay en el editor como borrador, sin emitir. Devuelve si
    /// se pudo: si las fechas o los números están mal, no se guarda nada.
    fn guardar_borrador_actual(&mut self) -> bool {
        let Some(mut editor) = self.editor.take() else {
            return false;
        };
        editor.volcar();
        if !editor.errores().is_empty() {
            self.avisar("Revisa fechas y números antes de guardar.", true);
            self.editor = Some(editor);
            return false;
        }
        let mut doc = editor.doc.clone();
        doc.serie = editor.serie.trim().to_string();
        doc.fecha = parse_fecha(&editor.fecha).unwrap_or(doc.fecha);
        doc.vencimiento = parse_fecha(&editor.vencimiento).unwrap_or(doc.vencimiento);
        doc.irpf_pb = parse_pb(&editor.irpf).unwrap_or(0);
        doc.cliente_id = editor.cliente_id;
        doc.lineas = editor.lineas.iter().map(LineaBuf::a_linea).collect();
        if let Some(d) = self.db.documento_mut(doc.id) {
            *d = doc;
        } else {
            self.db.documentos.push(doc);
        }
        self.seleccion = None;
        self.guardar();
        self.avisar("Borrador guardado.", false);
        true
    }

    /// Copia un documento como borrador nuevo con la fecha de hoy: la forma
    /// rápida de repetir una factura o un presupuesto.
    fn duplicar(&mut self, id: u64) {
        let Some(original) = self.db.documento(id).cloned() else {
            return;
        };
        let mut nuevo = original;
        nuevo.id = self.db.id();
        nuevo.numero = 0;
        nuevo.estado = Estado::Borrador;
        nuevo.verifactu = None;
        nuevo.verifactu_anulacion = None;
        nuevo.rectifica = None;
        nuevo.motivo_rectificacion.clear();
        nuevo.rectificada_por = None;
        nuevo.pagos.clear();
        nuevo.fecha = Local::now().date_naive();
        nuevo.vencimiento = nuevo.fecha
            + chrono::Duration::days(self.db.empresa.dias_vencimiento.clamp(0, 365) as i64);
        nuevo.aplicar_cliente();
        self.seleccion = Some(nuevo.id);
        self.editor = Some(Editor::desde(nuevo.clone(), true));
        self.db.documentos.push(nuevo);
        self.guardar();
        self.avisar(
            "Copia creada como borrador, con la fecha de hoy y el mismo cliente.",
            false,
        );
    }

    /// Diálogo de puesta en marcha: velo oscuro y ventana centrada, con cada
    /// paso y sus campos dentro. Es lo primero que se ve al abrir la app.
    fn ventana_asistente(&mut self, ctx: &egui::Context) {
        if self.asistente.as_ref().map(|a| a.ayuda).unwrap_or(false) {
            self.ayuda_verifactu = true;
            if let Some(a) = self.asistente.as_mut() {
                a.ayuda = false;
            }
        }
        let pantalla = ctx.screen_rect();
        egui::Area::new(egui::Id::new("velo_asistente"))
            // El velo va en Middle: tapa los paneles y campos del fondo, pero
            // queda SIEMPRE por debajo del diálogo (Foreground). Antes estaba
            // en Foreground y un clic derecho lo subía a la primera capa y
            // tapaba el tutorial: se veía un velo y no dejaba avanzar.
            .order(egui::Order::Middle)
            .fixed_pos(pantalla.min)
            .interactable(true)
            .show(ctx, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(pantalla.size(), egui::Sense::click());
                ui.painter().rect_filled(
                    rect,
                    egui::Rounding::ZERO,
                    egui::Color32::from_black_alpha(110),
                );
            });
        if self.asistente.is_none() {
            self.asistente = Some(Asistente::desde(&self.db.empresa));
            self.paso_voz = usize::MAX;
        }
        let paso = self.asistente.as_ref().map(|a| a.paso).unwrap_or(0);
        let guion = self
            .asistente
            .as_ref()
            .map(|a| crate::asistente::guion(a.paso, &a.perfil()))
            .unwrap_or_default();
        if self.db.voz {
            if self.paso_voz != paso {
                self.voz.decir(&guion);
                self.paso_voz = paso;
            }
        }
        let voz_antes = self.db.voz;
        let mut terminar = false;
        let mut luego = false;
        let mut repetir = false;
        // Un clic derecho (o el menú del sistema de la ventana) no debe dejar el
        // diálogo sin salida: Escape y el botón «Salir» están siempre activos.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            luego = true;
        }
        egui::Window::new("Puesta en marcha")
            .collapsible(false)
            .resizable(false)
            // Sin arrastre ni redimensionado: es un diálogo, no una ventana de
            // trabajo, y así no puede quedarse a medias ni fuera de pantalla.
            .movable(false)
            .constrain(true)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .default_width(660.0)
            .show(ctx, |ui| {
            ui.set_min_width(620.0);
            let a = self.asistente.as_mut().expect("asistente");
            ui.label(
                RichText::new("Vamos a conocernos")
                    .size(17.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            ui.label(
                RichText::new(
                    "Cuatro preguntas para saber cómo facturar y qué presentar. Puedes cambiar todo después en Ajustes.",
                )
                .size(11.5)
                .color(tema::SUAVE),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for (i, t) in Asistente::TITULOS.iter().enumerate() {
                    let color = if i == a.paso {
                        tema::ACENTO
                    } else if i < a.paso {
                        tema::VERDE
                    } else {
                        tema::TENUE
                    };
                    ui.label(
                        RichText::new(format!("{}. {}", i + 1, t))
                            .size(11.5)
                            .strong()
                            .color(color),
                    );
                    if i + 1 < Asistente::TITULOS.len() {
                        ui.label(RichText::new("·").size(11.5).color(tema::TENUE));
                    }
                }
            });
            ui.add_space(12.0);
            egui::Frame::none()
                .fill(tema::SUPERFICIE)
                .rounding(egui::Rounding::same(12.0))
                .inner_margin(egui::Margin::same(18.0))
                .show(ui, |ui| {
                    ui.set_max_width(720.0);
                    match a.paso {
                        0 => a.paso_quien(ui),
                        1 => a.paso_actividad(ui),
                        2 => a.paso_clientes(ui),
                        _ => a.paso_plan(ui),
                    }
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if a.paso > 0 && ui.button("← Anterior").clicked() {
                    a.paso -= 1;
                }
                ui.checkbox(&mut self.db.voz, "Voz")
                    .on_hover_text(
                        "Lee cada paso en voz alta con una voz femenina de Windows.",
                    );
                if ui
                    .button("Escuchar este paso")
                    .on_hover_text("Vuelve a leer la explicación de este paso.")
                    .clicked()
                {
                    repetir = true;
                }
                ui.separator();
                if a.paso < Asistente::TITULOS.len() - 1 {
                    if ui
                        .add(
                            egui::Button::new("Siguiente →")
                                .fill(tema::ACENTO)
                                .rounding(egui::Rounding::same(8.0)),
                        )
                        .clicked()
                    {
                        a.paso += 1;
                    }
                } else if ui
                    .add(
                        egui::Button::new("Guardar y empezar")
                            .fill(tema::VERDE)
                            .rounding(egui::Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    terminar = true;
                }
                // Siempre visible, en todos los pasos: si algo se atasca, se
                // sale sin guardar y el asistente vuelve a salir al abrir.
                if ui
                    .small_button("Salir")
                    .on_hover_text(
                        "Cierra el tutorial sin guardar (o pulsa Esc). Volverá a salir al abrir el programa.",
                    )
                    .clicked()
                {
                    luego = true;
                }
            });
            if self.voz.hablando() {
                ui.label(
                    RichText::new("Leyendo en voz alta…")
                        .size(11.0)
                        .color(tema::TENUE),
                );
            }
            ui.label(
                RichText::new("Esc o «Salir» cierran el tutorial sin guardar nada.")
                    .size(10.0)
                    .color(tema::TENUE),
            );
            });
        if repetir || paso != self.paso_voz {
            if self.db.voz {
                self.voz.decir(&guion);
                self.paso_voz = paso;
            }
        }
        if !self.db.voz {
            self.voz.parar();
        }
        if self.db.voz != voz_antes {
            self.guardar();
        }
        if terminar {
            self.aplicar_asistente();
        } else if luego {
            self.voz.parar();
            self.asistente = None;
        }
    }

    /// Vuelca las respuestas en la empresa y cierra el asistente.
    fn aplicar_asistente(&mut self) {
        self.voz.parar();
        let Some(a) = self.asistente.take() else { return };
        let p = a.perfil();
        let plan = crate::asistente::plan(&p);
        {
            let e = &mut self.db.empresa;
            if !p.nombre.trim().is_empty() {
                e.nombre = p.nombre.trim().to_string();
            }
            if !p.nif.trim().is_empty() {
                e.nif = p.nif.trim().to_uppercase();
            }
            e.persona_fisica = !p.es_sociedad;
            e.actividad = p.actividad;
            e.regimen_irpf = p.regimen_irpf;
            e.recargo_equivalencia = p.recargo_equivalencia;
            e.sii = p.sii;
            e.iva_defecto = plan.iva_pb;
            e.irpf_defecto = plan.irpf_pb;
            e.verifactu_activo = p.verifactu;
            if e.serie.trim().is_empty() {
                e.serie = chrono::Local::now().format("%Y").to_string();
            }
        }
        self.db.asistente_completado = true;
        self.guardar();
        self.vista = Vista::Inicio;
        self.avisar(
            "Listo: datos, IVA, retención y modelos aplicados. Puedes cambiarlos en Ajustes.",
            false,
        );
    }

    /// Datos de la empresa que faltan y por qué importa cada uno.
    fn perfil_pendiente(&self) -> Vec<(&'static str, &'static str)> {
        let e = &self.db.empresa;
        let mut v = Vec::new();
        if e.nombre.trim().is_empty() {
            v.push((
                "Nombre o razón social",
                "Sale en la cabecera de cada factura y tiene que coincidir con Hacienda.",
            ));
        }
        if e.nif.trim().is_empty() {
            v.push((
                "NIF",
                "Sin NIF la factura no vale como justificante y el 347 la rechaza.",
            ));
        }
        if e.direccion.trim().is_empty() || e.cp.trim().is_empty() || e.ciudad.trim().is_empty() {
            v.push((
                "Dirección fiscal",
                "La piden el FacturaE, el PDF y el 347: calle, código postal y ciudad.",
            ));
        }
        if e.email.trim().is_empty() {
            v.push((
                "Email",
                "Para enviar la factura al cliente y recibir avisos de la AEAT.",
            ));
        }
        if e.iban.trim().is_empty() {
            v.push((
                "IBAN",
                "Para que te paguen por transferencia y para imprimirlo en el PDF.",
            ));
        }
        if !self.db.asistente_completado {
            v.push((
                "Régimen y actividad",
                "Deciden el IVA, la retención y los modelos que te tocan. Son cuatro preguntas.",
            ));
        }
        v
    }

    fn vista_inicio(&mut self, ui: &mut egui::Ui) {
        let facturas: Vec<&Documento> = self.db.facturas();
        let emitidas = facturas
            .iter()
            .filter(|d| matches!(d.estado, Estado::Emitido | Estado::Pagado))
            .count();
        let pendiente = self.db.pendientes_cobro();
        let presupuestos = self.db.presupuestos().len();
        let vencidas = facturas.iter().filter(|d| d.vencida()).count();
        let problemas = verifactu::verificar_cadena(&self.db.documentos);

        ui.add_space(4.0);
        tema::titulo_ayuda(
            ui,
            "Inicio",
            "Tu negocio de un vistazo",
            "Aquí se resume lo que tienes emitido, pendiente de cobro, vencido y lo que la cadena Verifactu detecta. Pasa el ratón por las tarjetas para ver cómo se calcula cada cifra.",
        );
        ui.add_space(12.0);

        ui.horizontal_wrapped(|ui| {
            tema::kpi(ui, "Facturas emitidas", &emitidas.to_string(), tema::TEXTO);
            tema::kpi(ui, "Pendiente de cobro", &fmt_cents_eur(pendiente), tema::AMBAR);
            tema::kpi(ui, "Presupuestos", &presupuestos.to_string(), tema::TEXTO);
            tema::kpi_ayuda(
                ui,
                "Cadena Verifactu",
                if problemas.is_empty() {
                    "Íntegra"
                } else {
                    "Revisar"
                },
                if problemas.is_empty() {
                    tema::VERDE
                } else {
                    tema::ROJO
                },
                "Comprueba que cada registro enlaza con el anterior, que las huellas coinciden y que no falta ninguna factura posterior. Es una revisión local: no envía nada a la AEAT.",
            );
            tema::kpi_ayuda(
                ui,
                "Vencidas",
                &vencidas.to_string(),
                if vencidas > 0 {
                    tema::ROJO
                } else {
                    tema::VERDE
                },
                "Facturas emitidas o pendientes de cobro cuya fecha de vencimiento ya pasó. No cambia el estado fiscal: sólo te avisa para reclamar el cobro.",
            );
        });

        // Perfil fiscal: qué falta, por qué importa y cómo arreglarlo de un
        // clic. Seis datos que evitan el 90 % de los problemas al emitir.
        let pendientes = self.perfil_pendiente();
        if !pendientes.is_empty() {
            ui.add_space(16.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Tu perfil, listo para facturar")
                            .strong()
                            .color(tema::ACENTO),
                    );
                    ui.label(
                        RichText::new(format!("{}/6 completado", 6 - pendientes.len()))
                            .size(11.5)
                            .color(tema::SUAVE),
                    );
                });
                ui.label(
                    RichText::new("Cada dato tiene un motivo. Pasa el ratón por encima para verlo.")
                        .size(11.5)
                        .color(tema::SUAVE),
                );
                for (titulo, motivo) in &pendientes {
                    ui.label(
                        RichText::new(format!("· Falta: {titulo}"))
                            .size(11.5)
                            .color(egui::Color32::from_rgb(150, 30, 30)),
                    )
                    .on_hover_text(*motivo);
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if tema::boton_primario(ui, "Completar en Ajustes").clicked() {
                        self.vista = Vista::Ajustes;
                    }
                    if ui.button("Repetir el asistente").clicked() {
                        self.asistente = Some(Asistente::desde(&self.db.empresa));
                        self.paso_voz = usize::MAX;
                    }
                });
            });
        } else {
            ui.add_space(10.0);
            ui.label(
                RichText::new("✓ Perfil completo: nombre, NIF, dirección, email, IBAN y régimen configurados.")
                    .size(11.5)
                    .color(tema::VERDE),
            );
        }

        if !problemas.is_empty() {
            ui.add_space(16.0);
            ui.group(|ui| {
                ui.label(
                    RichText::new("La cadena de registros no cuadra")
                        .strong()
                        .color(egui::Color32::from_rgb(150, 30, 30)),
                );
                for p in problemas.iter().take(5) {
                    ui.label(RichText::new(p).small());
                }
            });
        }

        let anyo = Local::now().year();
        let r = crate::contab::resumen(&self.db, anyo, 0);
        ui.add_space(20.0);
        ui.label(
            RichText::new(format!("Año {}", anyo))
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            tema::kpi(ui, "Facturado", &fmt_cents_eur(r.ingresos), tema::TEXTO);
            tema::kpi(ui, "Gastos", &fmt_cents_eur(r.gastos), tema::TEXTO);
            tema::kpi(
                ui,
                "Resultado",
                &fmt_cents_eur(r.resultado()),
                if r.resultado() >= 0 {
                    tema::VERDE
                } else {
                    tema::ROJO
                },
            );
            tema::kpi(ui, "Pendiente de cobro", &fmt_cents_eur(pendiente), tema::AMBAR);
            tema::kpi_ayuda(
                ui,
                "IVA a ingresar",
                &fmt_cents_eur(r.iva_a_ingresar()),
                tema::ACENTO,
                "IVA repercutido menos IVA soportado del periodo. No incluye compensaciones de trimestres anteriores ni regímenes especiales: contrástalo antes de presentar el 303.",
            );
            tema::kpi_ayuda(
                ui,
                "IRPF 130 estimado",
                &fmt_cents_eur(r.irpf_130()),
                tema::ACENTO,
                "Estimación del 20 % del rendimiento neto menos retenciones. El 130 real resta además los pagos fraccionados de trimestres anteriores.",
            );
        });

        ui.add_space(20.0);
        ui.label(RichText::new("Últimos documentos").strong());
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("recientes").num_columns(5).striped(true).show(ui, |ui| {
                ui.label(RichText::new("Número").strong());
                ui.label(RichText::new("Fecha").strong());
                ui.label(RichText::new("Cliente").strong());
                ui.label(RichText::new("Estado").strong());
                ui.label(RichText::new("Total").strong());
                ui.end_row();
                let mut docs: Vec<&Documento> = self.db.documentos.iter().collect();
                docs.sort_by(|a, b| b.fecha.cmp(&a.fecha).then(b.id.cmp(&a.id)));
                for d in docs.into_iter().take(12) {
                    ui.label(d.etiqueta_numero());
                    ui.label(d.fecha.format("%d/%m/%Y").to_string());
                    ui.label(d.cliente.nombre_completo());
                    if d.vencida() {
                        tema::insignia(ui, "Vencida", tema::ROJO_CLARO, tema::ROJO);
                    } else {
                        tema::insignia_estado(ui, d.estado);
                    }
                    ui.label(RichText::new(fmt_cents_eur(d.totales().total)).monospace());
                    ui.end_row();
                }
            });
        });
    }

    fn vista_lista(&mut self, ui: &mut egui::Ui, tipo: TipoDoc) {
        if self.editor.is_some() {
            // Se saca del Option para poder prestar `&mut self` dentro del editor
            // sin tener el campo prestado a la vez.
            let mut ed = self.editor.take().unwrap();
            let mantener = self.editor_ui(ui, &mut ed);
            if mantener {
                self.editor = Some(ed);
            }
            return;
        }

        ui.horizontal(|ui| {
            let (nombre, sub, ayuda) = match tipo {
                TipoDoc::Factura => (
                    "Facturas",
                    "Documentos fiscales con registro Verifactu",
                    "Sólo las facturas emitidas llevan número, registro encadenado y QR. Una factura emitida no se edita: se anula o se rectifica con una R1.",
                ),
                TipoDoc::Presupuesto => (
                    "Presupuestos",
                    "Ofertas para el cliente, sin validez fiscal",
                    "Un presupuesto no lleva IVA devengado ni número fiscal. Cuando el cliente acepta, se convierte en factura y entonces se numera.",
                ),
                TipoDoc::Proforma => (
                    "Proformas",
                    "Cobro anticipado, sin validez fiscal",
                    "Una proforma sirve para cobrar antes de emitir la factura. No cuenta para el 303 ni para Verifactu hasta que se convierte en factura.",
                ),
            };
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new(nombre)
                            .size(24.0)
                            .strong()
                            .color(tema::TEXTO),
                    );
                    tema::icono_ayuda(ui, nombre, ayuda);
                });
                ui.label(RichText::new(sub).size(12.0).color(tema::SUAVE));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tema::boton_primario(ui, "+ Nuevo").clicked() {
                    if self.db.clientes.is_empty() {
                        self.avisar("Crea primero un cliente.", true);
                    } else if !self.db.empresa.completa() {
                        self.avisar("Configura tu empresa antes de crear documentos.", true);
                        self.vista = Vista::Ajustes;
                    } else {
                        // Cliente habitual: el último usado, si sigue existiendo.
                        let cliente = self
                            .db
                            .cliente(self.db.ultimo_cliente_id)
                            .cloned()
                            .unwrap_or_else(|| self.db.clientes[0].clone());
                        let serie = if tipo == TipoDoc::Factura {
                            self.db.empresa.serie.clone()
                        } else {
                            tipo.serie_defecto()
                        };
                        let mut doc =
                            Documento::nuevo(tipo, cliente, self.db.empresa.irpf_defecto, serie);
                        doc.aplicar_defectos(&self.db.empresa);
                doc.aplicar_cliente();
                        doc.id = self.db.id();
                        self.seleccion = Some(doc.id);
                        self.editor = Some(Editor::desde(doc, true));
                    }
                }
                ui.add_space(10.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.busqueda)
                        .hint_text("Buscar cliente, NIF o número")
                        .desired_width(230.0)
                        .margin(egui::Margin::symmetric(10.0, 6.0)),
                );
            });
        });
        ui.add_space(10.0);

        let consulta = self.busqueda.trim().to_lowercase();
        let mut docs: Vec<Documento> = self
            .db
            .documentos
            .iter()
            .filter(|d| d.tipo == tipo)
            .filter(|d| {
                consulta.is_empty()
                    || d.cliente.nombre_completo().to_lowercase().contains(&consulta)
                    || d.cliente.nif.to_lowercase().contains(&consulta)
                    || d.etiqueta_numero().to_lowercase().contains(&consulta)
            })
            .cloned()
            .collect();
        docs.sort_by(|a, b| b.fecha.cmp(&a.fecha).then(b.numero.cmp(&a.numero)));

        if docs.is_empty() {
            ui.add_space(32.0);
            tema::vacio(
                ui,
                if consulta.is_empty() {
                    "Todavía no hay nada aquí"
                } else {
                    "Sin resultados"
                },
                if consulta.is_empty() {
                    "Pulsa «+ Nuevo» para crear el primer documento."
                } else {
                    "Prueba con otro nombre, NIF o número."
                },
            );
            return;
        }

        ui.label(
            RichText::new(format!(
                "{} documentos · {}",
                docs.len(),
                fmt_cents_eur(docs.iter().map(|d| d.totales().total).sum())
            ))
            .size(11.5)
            .color(tema::TENUE),
        );
        ui.add_space(4.0);

        // Columnas de ancho fijo: la cabecera y las filas se pintan en grids
        // distintos, así que tienen que medir lo mismo. Con miles de facturas
        // se dibujan sólo las filas visibles (`show_rows`): antes se pintaban
        // las 4.000 de golpe y la ventana se quedaba congelada varios segundos.
        let ancho_num = 95.0;
        let ancho_fecha = 78.0;
        let ancho_cliente = 230.0;
        let ancho_estado = 86.0;
        let ancho_total = 108.0;
        let ancho_accion = 54.0;
        let alto_fila = 24.0;

        egui::Grid::new("lista_docs_cabecera")
            .num_columns(6)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for (titulo, ancho) in [
                    ("Número", ancho_num),
                    ("Fecha", ancho_fecha),
                    ("Cliente", ancho_cliente),
                    ("Estado", ancho_estado),
                    ("Total", ancho_total),
                    ("", ancho_accion),
                ] {
                    ui.add_sized(
                        [ancho, 20.0],
                        egui::Label::new(RichText::new(titulo).strong()),
                    );
                }
                ui.end_row();
            });
        ui.add_space(2.0);

        egui::ScrollArea::vertical().show_rows(ui, alto_fila, docs.len(), |ui, rango| {
            egui::Grid::new("lista_docs")
                .num_columns(6)
                .striped(true)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for d in &docs[rango] {
                        let sel = self.seleccion == Some(d.id);
                        let resp = ui.add_sized(
                            [ancho_num, 20.0],
                            egui::SelectableLabel::new(sel, d.etiqueta_numero()),
                        );
                        if resp.clicked() {
                            self.seleccion = Some(d.id);
                        }
                        ui.add_sized(
                            [ancho_fecha, 20.0],
                            egui::Label::new(d.fecha.format("%d/%m/%Y").to_string()),
                        );
                        ui.add_sized(
                            [ancho_cliente, 20.0],
                            egui::Label::new(d.cliente.nombre_completo()).truncate(),
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(ancho_estado, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if d.rectificada_por.is_some()
                                    && d.estado != Estado::Anulado
                                {
                                    tema::insignia(
                                        ui,
                                        "Rectificada",
                                        tema::AMBAR_CLARO,
                                        tema::AMBAR,
                                    );
                                } else if d.rectifica.is_some() {
                                    tema::insignia(
                                        ui,
                                        "R1",
                                        tema::AMBAR_CLARO,
                                        tema::AMBAR,
                                    );
                                } else if d.vencida() {
                                    tema::insignia(ui, "Vencida", tema::ROJO_CLARO, tema::ROJO);
                                } else {
                                    tema::insignia_estado(ui, d.estado);
                                }
                            },
                        );
                        ui.add_sized(
                            [ancho_total, 20.0],
                            egui::Label::new(
                                RichText::new(fmt_cents_eur(d.totales().total)).monospace(),
                            ),
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(ancho_accion, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if d.editable() {
                                    if ui.small_button("editar").clicked() {
                                        self.seleccion = Some(d.id);
                                        self.editor = Some(Editor::desde(d.clone(), false));
                                    }
                                } else if ui.small_button("ver").clicked() {
                                    self.seleccion = Some(d.id);
                                }
                            },
                        );
                        ui.end_row();
                    }
                });
        });
    }

    /// Devuelve `true` si el editor debe seguir abierto.
    fn editor_ui(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> bool {
        let clientes: Vec<(u64, String)> = self
            .db
            .clientes
            .iter()
            .map(|c| (c.id, c.etiqueta()))
            .collect();
        let articulos: Vec<Articulo> = self.db.articulos.clone();
        let mut accion = Accion::Nada;
        let mut quitar: Option<usize> = None;
        let mut anadir = false;
        let mut desde_articulo: Option<usize> = None;

        ui.label(
            RichText::new(if editor.nuevo {
                format!("Nuevo {}", editor.doc.tipo.nombre().to_lowercase())
            } else {
                format!("{} {}", editor.doc.tipo.nombre(), editor.doc.etiqueta_numero())
            })
            .size(24.0)
            .strong()
            .color(tema::TEXTO),
        );
        ui.label(
            RichText::new(if editor.nuevo {
                "Se guardará como borrador hasta que lo emitas."
            } else {
                "Editable mientras siga en borrador."
            })
            .size(12.0)
            .color(tema::SUAVE),
        );
        ui.add_space(12.0);

        let operacion_antes = editor.doc.operacion;
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let margen = egui::Margin::symmetric(10.0, 7.0);
            egui::Grid::new("cab_doc")
                .num_columns(4)
                .min_col_width(104.0)
                .spacing([14.0, 10.0])
                .show(ui, |ui| {
                    tema::etiqueta_ayuda(
                        ui,
                        "Serie",
                        "Prefijo de la numeración. Cada serie lleva su propia cuenta y debe ser única por ejercicio.",
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.serie)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    tema::etiqueta_ayuda(
                        ui,
                        "Cliente",
                        "El cliente se congela al emitir: sus datos quedan guardados como estaban en ese momento.",
                    );
                    egui::ComboBox::from_id_salt("combo_cliente")
                        .selected_text(
                            clientes
                                .iter()
                                .find(|(id, _)| *id == editor.cliente_id)
                                .map(|(_, t)| t.clone())
                                .unwrap_or_else(|| "(sin cliente)".to_string()),
                        )
                        .width(320.0)
                        .show_ui(ui, |ui| {
                            for (id, etiqueta) in &clientes {
                                ui.selectable_value(&mut editor.cliente_id, *id, etiqueta.clone());
                            }
                        });
                    ui.end_row();

                    tema::etiqueta_ayuda(
                        ui,
                        "Fecha",
                        "Fecha de expedición: decide el trimestre del 303 y del 130. Usa dd/mm/aaaa.",
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.fecha)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    tema::etiqueta_ayuda(
                        ui,
                        "Vencimiento",
                        "Cuándo se espera el cobro; no cambia los impuestos ni la fecha de expedición.",
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.vencimiento)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    ui.end_row();

                    tema::etiqueta_ayuda(
                        ui,
                        "IRPF %",
                        "Retención que te practica el cliente: 15 % (7 % los tres primeros años) si es empresa y eres profesional. 0 % para particulares y clientes de fuera.",
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.irpf)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    tema::etiqueta_ayuda(
                        ui,
                        "Forma de pago",
                        "Se imprime en la factura y sirve de referencia para el cobro. No cambia los impuestos.",
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.doc.forma_pago)
                            .desired_width(320.0)
                            .margin(margen),
                    );
                    ui.end_row();

                    tema::etiqueta_ayuda(
                        ui,
                        "Operación",
                        "Nacional lleva IVA. UE (intracomunitaria), exportación y exenta van sin IVA, con leyenda, y la UE suma en el 349.",
                    );
                    egui::ComboBox::from_id_salt("combo_operacion")
                        .selected_text(editor.doc.operacion.etiqueta())
                        .width(320.0)
                        .show_ui(ui, |ui| {
                            for op in TipoOperacion::TODAS {
                                ui.selectable_value(&mut editor.doc.operacion, op, op.etiqueta());
                            }
                        });
                    ui.label(
                        RichText::new("Sin IVA en el extranjero")
                            .size(11.0)
                            .color(tema::TENUE),
                    );
                    ui.label("");
                    ui.end_row();
                });
        });

        // Al cambiar de cliente se proponen su IVA, retención y recargo.
        if editor.doc.cliente_id != editor.cliente_id {
            if let Some(c) = self
                .db
                .clientes
                .iter()
                .find(|c| c.id == editor.cliente_id)
            {
                editor.doc.cliente_id = c.id;
                editor.doc.cliente = c.clone();
                editor.doc.aplicar_cliente();
                editor.irpf = fmt_pb(editor.doc.irpf_pb);
            }
        }

        // Cambiar la operación ajusta el IVA de las líneas: sin IVA en la UE,
        // exportación o exenta, y con el IVA por defecto al volver a nacional.
        if editor.doc.operacion != operacion_antes {
            let iva = if editor.doc.operacion.lleva_iva() {
                self.db.empresa.iva_defecto
            } else {
                0
            };
            for l in &mut editor.lineas {
                l.iva = fmt_pb(iva);
            }
        }

        if editor.doc.operacion != TipoOperacion::Nacional {
            ui.add_space(8.0);
            ui.label(
                RichText::new(editor.doc.operacion.leyenda())
                    .size(11.0)
                    .color(tema::SUAVE),
            );
            ui.label(
                RichText::new("La factura no lleva IVA ni retención y el PDF incluirá esa leyenda.")
                    .size(11.0)
                    .color(tema::TENUE),
            );
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Líneas").size(15.0).strong().color(tema::TEXTO));
            tema::icono_ayuda(
                ui,
                "Líneas de la factura",
                "Cantidad × precio, menos el descuento de la línea, más su IVA. La descripción puede autocompletar precio e IVA si empieza como un artículo guardado.",
            );
        });
        ui.add_space(4.0);

        // Reparto de anchos: la descripción se lleva lo que sobra, y las
        // columnas de importes tienen espacio para cifras de cuatro dígitos
        // con decimales sin cortar el texto.
        // Anchos explícitos con add_sized: en una Grid, desired_width se
        // recortaba y la descripción acababa en 50 px. En ventanas estrechas
        // se compactan las columnas numéricas para que quepa lo importante.
        let disponible = ui.available_width();
        let compacto = disponible < 780.0;
        let ancho_cant = if compacto { 68.0 } else { 86.0 };
        let ancho_precio = if compacto { 96.0 } else { 116.0 };
        let ancho_dto = if compacto { 64.0 } else { 80.0 };
        let ancho_iva = if compacto { 64.0 } else { 80.0 };
        let ancho_quitar = 30.0;
        let ancho_desc = (disponible
            - (ancho_cant + ancho_precio + ancho_dto + ancho_iva + ancho_quitar)
            - 40.0)
            .max(150.0);
        let alto_tabla = (ui.available_height() - 260.0).clamp(190.0, 560.0);
        let cabecera = [
            ("Descripción", ancho_desc, "Concepto que se imprime en la factura."),
            ("Cantidad", ancho_cant, "Admite decimales: 1,5 horas, 0,25 ud."),
            ("Precio", ancho_precio, "Precio unitario sin IVA."),
            ("Dto %", ancho_dto, "Descuento en % sobre el bruto de la línea."),
            ("IVA %", ancho_iva, "21 general, 10 reducido, 4 superreducido, 0 exento."),
            ("", ancho_quitar, "Quitar la línea."),
        ];

        egui::ScrollArea::vertical()
            .max_height(alto_tabla)
            .id_salt("lineas")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (t, ancho, ayuda) in cabecera {
                        ui.add_sized(
                            [ancho, 16.0],
                            egui::Label::new(
                                RichText::new(t).size(11.5).strong().color(tema::SUAVE),
                            )
                            .truncate(),
                        )
                        .on_hover_text(ayuda);
                    }
                });
                ui.add_space(4.0);

                for (i, l) in editor.lineas.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let resp = ui.add_sized(
                            [ancho_desc, 32.0],
                            egui::TextEdit::singleline(&mut l.descripcion).hint_text("Concepto"),
                        );
                        // Si empieza como un artículo guardado, se rellena solo:
                        // precio, IVA y cantidad, sin tener que buscarlo.
                        if resp.changed() && parse_cents(&l.precio).unwrap_or(0) == 0 {
                            let escrito = l.descripcion.trim().to_lowercase();
                            if escrito.chars().count() >= 3 {
                                if let Some(a) = articulos
                                    .iter()
                                    .find(|a| a.descripcion.to_lowercase().starts_with(&escrito))
                                {
                                    l.precio = fmt_cents(a.precio_cents);
                                    l.iva = fmt_pb(a.iva_pb);
                                    if parse_qty(&l.cantidad).unwrap_or(0) == 0 {
                                        l.cantidad = "1".to_string();
                                    }
                                }
                            }
                        }
                        ui.add_sized(
                            [ancho_cant, 32.0],
                            egui::TextEdit::singleline(&mut l.cantidad),
                        );
                        ui.add_sized(
                            [ancho_precio, 32.0],
                            egui::TextEdit::singleline(&mut l.precio),
                        );
                        ui.add_sized([ancho_dto, 32.0], egui::TextEdit::singleline(&mut l.dto));
                        ui.add_sized([ancho_iva, 32.0], egui::TextEdit::singleline(&mut l.iva));
                        if ui
                            .add_sized(
                                [ancho_quitar, 32.0],
                                egui::Button::new(
                                    RichText::new("×").size(17.0).color(tema::SUAVE),
                                )
                                .fill(egui::Color32::TRANSPARENT),
                            )
                            .on_hover_text("Quitar línea")
                            .clicked()
                        {
                            quitar = Some(i);
                        }
                    });
                    ui.add_space(6.0);
                }
            });

        ui.horizontal(|ui| {
            if ui.button("+ Añadir línea").clicked() {
                anadir = true;
            }
            if !articulos.is_empty() {
                egui::ComboBox::from_id_salt("combo_articulo")
                    .selected_text("Insertar artículo…")
                    .show_ui(ui, |ui| {
                        for (i, a) in articulos.iter().enumerate() {
                            if ui
                                .selectable_label(false, format!("{} · {}", a.descripcion, fmt_cents(a.precio_cents)))
                                .clicked()
                            {
                                desde_articulo = Some(i);
                            }
                        }
                    });
            }
        });

        if let Some(i) = quitar {
            editor.lineas.remove(i);
        }
        if anadir {
            editor.lineas.push(LineaBuf::desde(&Linea {
                iva_pb: self.db.empresa.iva_defecto,
                ..Default::default()
            }));
        }
        if let Some(i) = desde_articulo {
            let a = articulos[i].clone();
            editor.lineas.push(LineaBuf::desde(&Linea {
                descripcion: a.descripcion,
                cantidad_milli: 1000,
                precio_cents: a.precio_cents,
                descuento_pb: 0,
                iva_pb: a.iva_pb,
            }));
        }

        // Totales en vivo: se calculan sobre lo escrito, aunque aún no se guarde.
        editor.volcar();
        let totales = editor.doc.totales();
        ui.add_space(12.0);
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(340.0);
            egui::Grid::new("tot_editor")
                .num_columns(2)
                .min_col_width(160.0)
                .spacing([20.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Base imponible").color(tema::SUAVE));
                    ui.label(RichText::new(fmt_cents_eur(totales.base_total)).monospace());
                    ui.end_row();
                    for (tipo, _, cuota) in &totales.bases {
                        ui.label(
                            RichText::new(format!("IVA {} %", fmt_pb(*tipo))).color(tema::SUAVE),
                        );
                        ui.label(RichText::new(fmt_cents(*cuota)).monospace());
                        ui.end_row();
                    }
                    if totales.cuota_recargo != 0 {
                        ui.label(
                            RichText::new(format!(
                                "Recargo equivalencia {} %",
                                fmt_pb(editor.doc.recargo_pb)
                            ))
                            .color(tema::SUAVE),
                        )
                        .on_hover_text(
                            "Lo paga el comerciante minorista y lo ingresas tú en el 303.",
                        );
                        ui.label(RichText::new(fmt_cents(totales.cuota_recargo)).monospace());
                        ui.end_row();
                    }
                    if totales.retencion != 0 {
                        ui.label(RichText::new("Retención IRPF").color(tema::SUAVE));
                        ui.label(
                            RichText::new(format!("-{}", fmt_cents(totales.retencion)))
                                .monospace(),
                        );
                        ui.end_row();
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        ui.label(RichText::new("TOTAL").size(11.5).strong().color(tema::TENUE));
                        tema::icono_ayuda(
                            ui,
                            "TOTAL de la factura",
                            "Base + IVA + recargo − retención de IRPF. Es lo que paga el cliente. En el registro Verifactu, el importe oficial no resta el IRPF.",
                        );
                    });
                    ui.label(
                        RichText::new(fmt_cents_eur(totales.total))
                            .size(20.0)
                            .strong()
                            .color(tema::ACENTO),
                    );
                    ui.end_row();
                });
        });

        let problemas = editor.problemas();
        let errores = editor.errores();
        ui.add_space(6.0);
        if !problemas.is_empty() {
            ui.label(
                RichText::new(format!("Falta poco ({}) · pulsa en cada campo para arreglarlo", problemas.len()))
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_rgb(160, 90, 20)),
            );
            for p in problemas.iter().take(5) {
                ui.label(
                    RichText::new(format!("· {p}"))
                        .small()
                        .color(egui::Color32::from_rgb(160, 90, 20)),
                );
            }
            if !errores.is_empty() {
                ui.label(
                    RichText::new(
                        "Corrige fechas y números antes de guardar: si no, se guardarían a cero.",
                    )
                    .size(10.5)
                    .color(tema::ROJO),
                );
            }
        } else {
            ui.label(
                RichText::new("Todo listo: puedes guardar el borrador o emitir la factura.")
                    .size(11.0)
                    .color(tema::VERDE),
            );
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let valido = problemas.is_empty();
            let puede_emitir = valido && !editor.lineas.is_empty();
            let alto_boton = egui::vec2(0.0, 34.0);
            let relleno = if puede_emitir {
                tema::ACENTO
            } else {
                egui::Color32::from_rgb(0xC7, 0xD2, 0xE8)
            };
            if ui
                .add_enabled(
                    puede_emitir,
                    egui::Button::new(
                        RichText::new("Guardar y emitir")
                            .size(13.5)
                            .strong()
                            .color(egui::Color32::WHITE),
                    )
                    .fill(relleno)
                    .rounding(egui::Rounding::same(8.0))
                    .min_size(egui::vec2(172.0, alto_boton.y)),
                )
                .clicked()
            {
                accion = Accion::EmitirYGuardar;
            }
            // Un borrador puede quedarse a medias: basta con que fechas y
            // números estén bien escritos para no guardar ceros sin querer.
            if ui
                .add_enabled(
                    errores.is_empty(),
                    egui::Button::new("Guardar borrador").min_size(egui::vec2(150.0, alto_boton.y)),
                )
                .on_hover_text("Puedes guardarlo sin serie o sin líneas y terminarlo luego.")
                .clicked()
            {
                accion = Accion::GuardarBorrador;
            }
            if ui
                .add(egui::Button::new("Cancelar").min_size(egui::vec2(104.0, alto_boton.y)))
                .clicked()
            {
                accion = Accion::Cancelar;
            }
            ui.add_space(10.0);
            if ui
                .add(egui::Button::new("Vista previa PDF").min_size(egui::vec2(150.0, alto_boton.y)))
                .clicked()
            {
                editor.volcar();
                match self.escribir_pdf(&editor.doc) {
                    Ok(p) => {
                        abrir_pdf_y_carpeta(&p);
                        self.avisar(format!("PDF: {}", p.display()), false);
                    }
                    Err(e) => self.avisar(e, true),
                }
            }
        });

        // Atajos de teclado, como en cualquier programa de facturación.
        let (ctrl_s, ctrl_enter) = ui.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::S),
                i.modifiers.command && i.key_pressed(egui::Key::Enter),
            )
        });
        if ctrl_s && errores.is_empty() {
            accion = Accion::GuardarBorrador;
        }
        if ctrl_enter && problemas.is_empty() && !editor.lineas.is_empty() {
            accion = Accion::EmitirYGuardar;
        }
        ui.add_space(2.0);
        ui.label(
            RichText::new("Ctrl+S guarda el borrador · Ctrl+Enter emite · al salir se avisa si hay cambios")
                .size(10.0)
                .color(tema::TENUE),
        );

        // Aplicar la acción fuera del préstamo mutable del editor.
        match &accion {
            Accion::Nada => true,
            Accion::Bloqueada(m) => {
                self.avisar(m.clone(), true);
                true
            }
            Accion::Cancelar => {
                if editor.modificado() {
                    self.descartar = Some(self.vista);
                    true
                } else {
                    self.seleccion = None;
                    false
                }
            }
            Accion::GuardarBorrador | Accion::EmitirYGuardar => {
                let emitir = matches!(&accion, Accion::EmitirYGuardar);
                let mut doc = editor.doc.clone();
                doc.serie = editor.serie.trim().to_string();
                doc.fecha = parse_fecha(&editor.fecha).unwrap_or(doc.fecha);
                doc.vencimiento = parse_fecha(&editor.vencimiento).unwrap_or(doc.vencimiento);
                doc.irpf_pb = parse_pb(&editor.irpf).unwrap_or(0);
                doc.lineas = editor.lineas.iter().map(LineaBuf::a_linea).collect();
                if let Some(c) = self.db.clientes.iter().find(|c| c.id == editor.cliente_id) {
                    doc.cliente = c.clone();
                    doc.cliente_id = c.id;
                }
                let id = doc.id;
                let cliente_habitual = doc.cliente_id;
                let es_nuevo = editor.nuevo;
                if es_nuevo {
                    self.db.documentos.push(doc);
                } else if let Some(d) = self.db.documento_mut(id) {
                    *d = doc;
                }
                self.db.ultimo_cliente_id = cliente_habitual;
                self.guardar();
                self.seleccion = Some(id);
                if emitir {
                    match self.emitir(id) {
                        Ok(()) => self.avisar("Documento emitido y numerado.", false),
                        Err(e) => self.avisar(e, true),
                    }
                } else {
                    self.avisar("Borrador guardado.", false);
                }
                false
            }
        }
    }

    /// Rellena un modelo con los datos de la app, lo guarda en PDF y JSON y
    /// abre el PDF y su carpeta.
    fn generar_modelo(&mut self, codigo: &'static str) {
        let anyo = self.anyo;
        let tri = if !crate::modelos::trimestral(codigo) {
            // Los anuales (390, 347) no llevan trimestre.
            0
        } else if (1..=4).contains(&self.trimestre) {
            self.trimestre
        } else {
            crate::contab::trimestre(chrono::Local::now().date_naive())
        };
        let Some(mut m) = crate::modelos::generar(&self.db, codigo, anyo, tri) else {
            self.avisar(
                format!("Del modelo {codigo} todavía no se calculan las casillas."),
                true,
            );
            return;
        };

        // Lo escrito a mano manda sobre lo calculado en la app.
        if let Some(aj) = self
            .db
            .ajustes_modelos
            .get(&crate::modelos::clave(codigo, anyo, tri))
        {
            crate::modelos::aplicar(&mut m, aj);
        }

        let empresa = self.db.empresa.clone();
        let dir = export::dir_modelos(&empresa);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.avisar(format!("Crear {}: {e}", dir.display()), true);
            return;
        }

        let ruta_pdf = dir.join(export::nombre_modelo(codigo, anyo, tri, "pdf", &empresa));
        let ruta_json = dir.join(export::nombre_modelo(codigo, anyo, tri, "json", &empresa));

        if let Err(e) = std::fs::write(&ruta_pdf, crate::pdf::modelo(&m, &empresa)) {
            self.avisar(format!("Escribir el PDF: {e}"), true);
            return;
        }
        if let Err(e) = std::fs::write(&ruta_json, m.json(&empresa)) {
            self.avisar(format!("Escribir el JSON: {e}"), true);
            return;
        }

        abrir_pdf_y_carpeta(&ruta_pdf);
        self.avisar(
            format!(
                "Listo: modelo {codigo} del {} en {}. El PDF es para revisarlo y el JSON para el gestor.",
                m.periodo,
                ruta_pdf.display()
            ),
            false,
        );
    }

    /// Vista de modelos: los datos de la empresa, lo que falta para poder
    /// presentar y la generación de cada PDF, todo en un sitio.
    fn vista_modelos(&mut self, ui: &mut egui::Ui) {
        let hoy = Local::now().date_naive();
        if !(1..=4).contains(&self.trimestre) {
            self.trimestre = crate::contab::trimestre(hoy);
        }
        let e = self.db.empresa.clone();

        tema::titulo_ayuda(
            ui,
            "Modelos e impuestos",
            "Qué tienes, qué te falta y el PDF listo para la Sede",
            "La app calcula una propuesta de cada modelo con tus facturas, gastos y nóminas. Revisa las casillas y los avisos antes de presentarlo: no se envía nada a la AEAT.",
        );
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            tema::etiqueta_ayuda(
                ui,
                "Ejercicio",
                "Año fiscal de los modelos. Cámbialo para revisar ejercicios anteriores o preparar el siguiente.",
            );
            if ui.button("◀").on_hover_text("Año anterior").clicked() {
                self.anyo = (self.anyo - 1).max(2000);
            }
            ui.label(
                RichText::new(self.anyo.to_string())
                    .size(17.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            if ui.button("▶").on_hover_text("Año siguiente").clicked() {
                self.anyo = (self.anyo + 1).min(2100);
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("Trimestre activo: {}T", self.trimestre))
                    .size(11.0)
                    .color(tema::TENUE),
            );
        });
        ui.add_space(10.0);

        // Datos con los que se rellena el declarante de cada modelo.
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Tus datos fiscales")
                        .size(13.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                tema::icono_ayuda(
                    ui,
                    "Tus datos fiscales",
                    "Estos datos se copian como declarante en cada modelo. Si falta alguno, el PDF sale incompleto y la Sede puede rechazarlo.",
                );
            });
            let linea = |ui: &mut egui::Ui, etiqueta: &str, valor: &str| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(etiqueta).size(11.0).color(tema::TENUE));
                    let v = if valor.trim().is_empty() {
                        "— sin rellenar —"
                    } else {
                        valor
                    };
                    ui.label(RichText::new(v).size(11.5).color(tema::TEXTO));
                });
            };
            linea(ui, "Nombre", &e.nombre);
            linea(ui, "NIF", &e.nif);
            linea(
                ui,
                "Dirección",
                &format!("{} · {} {}", e.direccion, e.cp, e.ciudad),
            );
            linea(ui, "Provincia", &e.provincia);
            linea(ui, "Email", &e.email);
            linea(ui, "IBAN", &e.iban);
            linea(ui, "Actividad", e.actividad.etiqueta());
            ui.add_space(4.0);
            if ui.button("Editar mis datos").clicked() {
                self.vista = Vista::Ajustes;
            }
        });
        ui.add_space(8.0);

        // Lo que falta, en lenguaje llano.
        let mut faltan: Vec<String> = Vec::new();
        if e.nombre.trim().is_empty() {
            faltan.push("Tu nombre o razón social: sin él el PDF sale sin declarante.".into());
        }
        let nif = e.nif.trim();
        if nif.is_empty() {
            faltan.push("Tu NIF: la AEAT no admite un modelo sin NIF.".into());
        } else if !crate::model::nif_valido(nif) {
            faltan.push(format!(
                "El NIF «{nif}» no parece válido (la letra de control no cuadra): revísalo en Ajustes."
            ));
        }
        if e.direccion.trim().is_empty() || e.cp.trim().is_empty() || e.ciudad.trim().is_empty() {
            faltan.push(
                "Dirección completa (calle, código postal y ciudad): la Sede la pide al presentar."
                    .into(),
            );
        }
        if e.iban.trim().is_empty() {
            faltan.push("IBAN: hace falta para domiciliar el pago o la devolución.".into());
        }
        if e.email.trim().is_empty() {
            faltan.push("Correo electrónico: para avisos y justificantes de la AEAT.".into());
        }
        let facturas_ano = self
            .db
            .documentos
            .iter()
            .filter(|d| {
                d.tipo.es_fiscal()
                    && d.numero > 0
                    && d.estado != Estado::Anulado
                    && d.fecha.year() == self.anyo
            })
            .count();
        if facturas_ano == 0 {
            faltan.push(format!(
                "No hay facturas emitidas en {}: los modelos de IVA saldrán a cero (el 111 y el 190 salen de las nóminas).",
                self.anyo
            ));
        }
        let gastos_ano = self
            .db
            .gastos
            .iter()
            .filter(|g| g.fecha.year() == self.anyo)
            .count();
        if gastos_ano == 0 {
            faltan.push(format!(
                "No hay gastos anotados en {}: el IVA deducible saldrá 0.",
                self.anyo
            ));
        }
        if e.actividad == Actividad::Otra {
            faltan.push(
                "Actividad marcada como «Otra»: cámbiala en Ajustes para que la lista de modelos sea la que te toca."
                    .into(),
            );
        }

        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Lo que falta")
                        .size(13.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                tema::icono_ayuda(
                    ui,
                    "Lo que falta",
                    "Son avisos, no errores: el modelo se puede generar igualmente, pero conviene completar estos datos antes de presentarlo.",
                );
            });
            if faltan.is_empty() {
                ui.label(
                    RichText::new(
                        "✓ Tienes lo necesario para generar los modelos. Sólo queda presentarlos en la Sede.",
                    )
                    .size(11.5)
                    .color(tema::VERDE),
                );
            } else {
                for f in &faltan {
                    ui.label(
                        RichText::new(format!("• {f}"))
                            .size(11.5)
                            .color(tema::SUAVE),
                    );
                }
                ui.add_space(4.0);
                if ui.button("Rellenar lo que falta en Ajustes").clicked() {
                    self.vista = Vista::Ajustes;
                }
            }
        });
        ui.add_space(12.0);

        ui.label(
            RichText::new("Modelos de este ejercicio")
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new(
                "Marca lo presentado y prepara el siguiente: el PDF sale con las casillas ya calculadas y el resumen en una frase.",
            )
            .size(11.0)
            .color(tema::SUAVE),
        );
        ui.add_space(6.0);

        let mut cambios: Vec<String> = Vec::new();
        let mut generar: Option<&'static str> = None;
        let mut hay_ajuste = false;
        for ob in crate::fiscal::obligaciones_con(
            self.db.empresa.actividad,
            &self.db.empresa,
        ) {
            tema::tarjeta(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(format!("Modelo {} · {}", ob.codigo, ob.nombre))
                                .size(13.0)
                                .strong()
                                .color(tema::TEXTO),
                        );
                        ui.label(RichText::new(ob.plazo).size(11.0).color(tema::SUAVE));
                        ui.label(
                            RichText::new(format!(
                                "Por qué: {}",
                                crate::fiscal::motivo(&self.db.empresa, ob.codigo)
                            ))
                            .size(10.5)
                            .italics()
                            .color(tema::ACENTO),
                        );
                        if let Some((texto, color)) = plazo_texto(&self.db, &ob, self.anyo, hoy)
                        {
                            ui.label(RichText::new(texto).size(10.5).color(color));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::fiscal::es_mensual(&self.db, &ob) {
                            // Con SII el 303 se marca mes a mes, no por trimestres.
                            for m in (1..=12).rev() {
                                let clave =
                                    format!("{}-{}-{:02}M", self.anyo, ob.codigo, m);
                                let mut hecho =
                                    crate::fiscal::esta_presentado(&self.db, &clave);
                                if ui.checkbox(&mut hecho, format!("{m:02}")).changed() {
                                    cambios.push(clave);
                                }
                            }
                        } else if ob.trimestral {
                            for q in (1..=4).rev() {
                                let clave = format!("{}-{}-{}T", self.anyo, ob.codigo, q);
                                let mut hecho =
                                    crate::fiscal::esta_presentado(&self.db, &clave);
                                if ui.checkbox(&mut hecho, format!("{q}T")).changed() {
                                    cambios.push(clave);
                                }
                            }
                        } else {
                            let clave = format!("{}-{}", self.anyo, ob.codigo);
                            let mut hecho = crate::fiscal::esta_presentado(&self.db, &clave);
                            if ui.checkbox(&mut hecho, "hecho").changed() {
                                cambios.push(clave);
                            }
                        }
                    });
                });
                if crate::modelos::previsible(ob.codigo) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if crate::modelos::trimestral(ob.codigo) {
                            tema::etiqueta_ayuda(
                                ui,
                                "Periodo",
                                "Trimestre que se va a calcular y previsualizar. El modelo se genera con las facturas, gastos y nóminas de ese periodo.",
                            );
                            for q in 1..=4u32 {
                                if ui
                                    .selectable_label(self.trimestre == q, format!("{q}T"))
                                    .clicked()
                                {
                                    self.trimestre = q;
                                }
                            }
                        } else {
                            ui.label(
                                RichText::new(format!("Año completo {}", self.anyo))
                                    .size(11.0)
                                    .color(tema::SUAVE),
                            );
                        }
                        ui.add_space(8.0);
                        if ui
                            .button("Preparar el modelo")
                            .on_hover_text(
                                "Genera el PDF para revisar y el JSON para el gestor, y abre la carpeta.",
                            )
                            .clicked()
                        {
                            generar = Some(ob.codigo);
                        }
                    });
                    let tri = if crate::modelos::trimestral(ob.codigo) {
                        self.trimestre
                    } else {
                        0
                    };
                    let clave_ajustes = crate::modelos::clave(ob.codigo, self.anyo, tri);
                    let guardados = self.db.ajustes_modelos.get(&clave_ajustes).cloned();
                    if let Some(mut m) =
                        crate::modelos::generar(&self.db, ob.codigo, self.anyo, tri)
                    {
                        if let Some(aj) = &guardados {
                            crate::modelos::aplicar(&mut m, aj);
                        }
                        ui.label(
                            RichText::new(m.resumen())
                                .size(11.5)
                                .strong()
                                .color(tema::TEXTO),
                        );
                        // Erratas que la Sede rechazaría o que descuadran.
                        for aviso in crate::modelos::avisos(&m, &self.db, self.anyo, tri) {
                            ui.label(
                                RichText::new(format!("⚠ {aviso}"))
                                    .size(10.5)
                                    .color(tema::AMBAR),
                            );
                        }

                        // Justificante del periodo que se está mirando.
                        let clave_pres = if ob.trimestral {
                            format!("{}-{}-{}T", self.anyo, ob.codigo, self.trimestre)
                        } else {
                            format!("{}-{}", self.anyo, ob.codigo)
                        };
                        if crate::fiscal::esta_presentado(&self.db, &clave_pres) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Justificante")
                                        .size(11.0)
                                        .color(tema::TENUE),
                                );
                                let campo = self
                                    .db
                                    .justificantes
                                    .entry(clave_pres.clone())
                                    .or_default();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(campo)
                                            .desired_width(240.0)
                                            .hint_text("Nº de registro o CSV que dio la Sede"),
                                    )
                                    .changed()
                                {
                                    hay_ajuste = true;
                                }
                            });
                        }
                        // Casillas a mano: lo que la app no puede saber.
                        if matches!(ob.codigo, "303" | "130" | "390") {
                            ui.add_space(2.0);
                            egui::CollapsingHeader::new("Editar casillas a mano").show(ui, |ui| {
                                ui.label(
                                    RichText::new(
                                        "Escribe solo lo que quieras cambiar (AIB, prorrata, compensaciones, casilla 13…). Los totales se recalculan solos y el valor se guarda para este modelo y periodo.",
                                    )
                                    .size(10.5)
                                    .color(tema::TENUE),
                                );
                                ui.add_space(4.0);
                                egui::Grid::new(format!(
                                    "casillas-{}-{}",
                                    ob.codigo, clave_ajustes
                                ))
                                .num_columns(3)
                                .striped(true)
                                .show(ui, |ui| {
                                    for seccion in &m.secciones {
                                        for c in &seccion.casillas {
                                            let actual = match c.valor {
                                                crate::modelos::Valor::Dinero(v) => v,
                                                _ => continue,
                                            };
                                            ui.label(
                                                RichText::new(&c.numero)
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(tema::SUAVE),
                                            );
                                            ui.label(
                                                RichText::new(&c.etiqueta)
                                                    .size(11.0)
                                                    .color(tema::TEXTO),
                                            );
                                            let mapa = self
                                                .db
                                                .ajustes_modelos
                                                .entry(clave_ajustes.clone())
                                                .or_default();
                                            let campo =
                                                mapa.entry(c.numero.clone()).or_default();
                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(campo)
                                                        .desired_width(110.0)
                                                        .hint_text(fmt_cents(actual)),
                                                )
                                                .changed()
                                            {
                                                hay_ajuste = true;
                                            }
                                            ui.end_row();
                                        }
                                    }
                                });
                                if ui
                                    .button("Quitar los ajustes de este modelo")
                                    .clicked()
                                {
                                    self.db.ajustes_modelos.remove(&clave_ajustes);
                                    self.guardar();
                                }
                            });
                        }
                    }
                    if let Some(g) = crate::presentacion::guia(ob.codigo) {
                        ui.horizontal(|ui| {
                            if ui.button("Abrir la Sede de la AEAT").clicked() {
                                abrir_url(g.sede);
                            }
                            if ui.button("Ver el paso a paso").clicked() {
                                self.vista = Vista::Guia;
                            }
                        });
                    }
                } else {
                    ui.label(
                        RichText::new(
                            "Este modelo todavía no se rellena con tus datos: la Guía rápida explica qué es y cuándo se presenta.",
                        )
                        .size(10.5)
                        .color(tema::TENUE),
                    );
                }
            });
            ui.add_space(4.0);
        }
        if let Some(codigo) = generar {
            self.generar_modelo(codigo);
        }
        if !cambios.is_empty() {
            for clave in cambios {
                crate::fiscal::alternar(&mut self.db, &clave);
            }
            self.guardar();
        }
        if hay_ajuste {
            self.guardar();
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Abrir la carpeta de exportación").clicked() {
                let dir = export::raiz_empresa(&self.db.empresa);
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
            if ui.button("Ir a la Guía rápida").clicked() {
                self.vista = Vista::Guia;
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Los ficheros se guardan en %APPDATA%\\AutonomoFACIL\\export\\<empresa>\\Modelos, con el modelo, el año, el trimestre y la empresa en el nombre.",
            )
            .size(10.5)
            .color(tema::TENUE),
        );
    }

    fn vista_guia(&mut self, ui: &mut egui::Ui) {
        let empresa = self.db.empresa.completa();
        let clientes = !self.db.clientes.is_empty();
        let articulos = !self.db.articulos.is_empty();
        let facturas = !self.db.facturas().is_empty();
        let gastos = !self.db.gastos.is_empty();
        let cobros = self
            .db
            .facturas()
            .iter()
            .any(|d| d.estado == Estado::Pagado || !d.pagos.is_empty());
        let copias = !self.copias_disponibles().is_empty();

        // Los modelos se rellenan de un trimestre concreto: si aún no hay
        // ninguno elegido, se propone el actual.
        if !(1..=4).contains(&self.trimestre) {
            self.trimestre = crate::contab::trimestre(chrono::Local::now().date_naive());
        }

        tema::titulo_ayuda(
            ui,
            "Guía rápida",
            "De cero a presentar los impuestos, paso a paso",
            "Un recorrido recomendado para dejar la app lista y presentar sin sustos. Cada paso se puede abrir por separado; no es una obligación ni sustituye a tu asesor.",
        );
        ui.add_space(12.0);
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Verifactu, sin sorpresas")
                            .size(14.0)
                            .strong()
                            .color(tema::TEXTO),
                    );
                    ui.label(
                        RichText::new(
                            "Qué se encadena, qué significa el QR y qué no hace la app. Léelo una vez antes de emitir tu primera factura.",
                        )
                        .size(11.0)
                        .color(tema::SUAVE),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cómo funciona").clicked() {
                        self.ayuda_verifactu = true;
                    }
                });
            });
        });
        ui.add_space(12.0);

        let mut ir_empleados = false;
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("¿Tienes empleados?")
                            .size(14.0)
                            .strong()
                            .color(tema::TEXTO),
                    );
                    ui.label(
                        RichText::new(
                            "Fichas, nóminas estimadas, coste de empresa y retenciones para el 111 y el 190. Si trabajas solo, puedes ignorarlo.",
                        )
                        .size(11.0)
                        .color(tema::SUAVE),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Abrir Empleados").clicked() {
                        ir_empleados = true;
                    }
                });
            });
        });
        if ir_empleados {
            self.vista = Vista::Empleados;
            self.ficha_empleado = None;
        }
        ui.add_space(12.0);

        let pasos: [(bool, &str, &str, Vista); 7] = [
            (
                empresa,
                "1. Tus datos fiscales",
                "Sin nombre y NIF no se puede emitir. Pon también IBAN, IVA e IRPF habituales y los días de vencimiento: el resto se rellena solo.",
                Vista::Ajustes,
            ),
            (
                clientes,
                "2. Tus clientes",
                "Con el nombre y el NIF basta. Al facturar se propone el último cliente usado.",
                Vista::Clientes,
            ),
            (
                articulos,
                "3. Tu catálogo (opcional)",
                "Guarda servicios o productos con su precio; en una factura se insertan de un clic.",
                Vista::Articulos,
            ),
            (
                facturas,
                "4. Emitir una factura",
                "Elige cliente, escribe el concepto y pulsa «Guardar y emitir». Número, fecha, vencimiento, IVA y registro Verifactu se generan solos.",
                Vista::Facturas,
            ),
            (
                gastos,
                "5. Registrar gastos",
                "Apunta cada factura de gasto con su IVA; con «Duplicar» repites los fijos y con «Usados…» no vuelves a teclear el proveedor.",
                Vista::Contabilidad,
            ),
            (
                cobros,
                "6. Cobrar",
                "En la factura, «Registrar cobro» con fecha e importe. Lo pendiente y lo vencido se controla desde Inicio.",
                Vista::Facturas,
            ),
            (
                copias,
                "7. Copia de seguridad",
                "Crea una copia antes de cada cierre. Restaurar no borra: guarda el estado actual antes de sustituirlo.",
                Vista::Ajustes,
            ),
        ];

        let hechos = pasos.iter().filter(|p| p.0).count();
        ui.label(
            RichText::new(format!("{} de {} pasos completados", hechos, pasos.len()))
                .size(11.5)
                .color(tema::SUAVE),
        );
        ui.add_space(4.0);
        ui.add(
            egui::ProgressBar::new(hechos as f32 / pasos.len() as f32).desired_width(300.0),
        );
        ui.add_space(12.0);

        let mut ir: Option<Vista> = None;
        for (hecho, titulo, texto, vista) in pasos {
            tema::tarjeta(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    tema::insignia(
                        ui,
                        if hecho { "hecho" } else { "pendiente" },
                        if hecho {
                            tema::VERDE_CLARO
                        } else {
                            egui::Color32::from_rgb(0xF1, 0xF3, 0xF7)
                        },
                        if hecho { tema::VERDE } else { tema::TENUE },
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(titulo)
                                .size(14.0)
                                .strong()
                                .color(tema::TEXTO),
                        );
                        ui.label(RichText::new(texto).size(11.5).color(tema::SUAVE));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Ir").clicked() {
                            ir = Some(vista);
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        if let Some(v) = ir {
            self.vista = v;
            self.editor = None;
            self.seleccion = None;
        }

        ui.add_space(14.0);
        ui.label(
            RichText::new("Tus obligaciones")
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new(
                "Según tu actividad. Marca lo que ya hayas presentado: se guarda con su año y trimestre.",
            )
            .size(11.0)
            .color(tema::SUAVE),
        );
        ui.add_space(6.0);

        let mut cambios: Vec<String> = Vec::new();
        let mut generar: Option<&'static str> = None;
        let hoy = Local::now().date_naive();
        for ob in crate::fiscal::obligaciones_con(
            self.db.empresa.actividad,
            &self.db.empresa,
        ) {
            tema::tarjeta(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(format!("Modelo {} · {}", ob.codigo, ob.nombre))
                                .size(13.0)
                                .strong()
                                .color(tema::TEXTO),
                        );
                        ui.label(RichText::new(ob.plazo).size(11.5).color(tema::SUAVE));
                        ui.label(RichText::new(ob.detalle).size(11.0).color(tema::TENUE));
                        ui.label(
                            RichText::new(format!(
                                "Por qué: {}",
                                crate::fiscal::motivo(&self.db.empresa, ob.codigo)
                            ))
                            .size(10.5)
                            .italics()
                            .color(tema::ACENTO),
                        );
                        // Plazo automático: el primer periodo del año que
                        // siga sin marcar y su fecha límite.
                        if let Some((texto, color)) =
                            plazo_texto(&self.db, &ob, self.anyo, hoy)
                        {
                            ui.label(RichText::new(texto).size(10.5).color(color));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::fiscal::es_mensual(&self.db, &ob) {
                            // Con SII el 303 se marca mes a mes, no por trimestres.
                            for m in (1..=12).rev() {
                                let clave =
                                    format!("{}-{}-{:02}M", self.anyo, ob.codigo, m);
                                let mut hecho =
                                    crate::fiscal::esta_presentado(&self.db, &clave);
                                if ui.checkbox(&mut hecho, format!("{m:02}")).changed() {
                                    cambios.push(clave);
                                }
                            }
                        } else if ob.trimestral {
                            for q in (1..=4).rev() {
                                let clave = format!("{}-{}-{}T", self.anyo, ob.codigo, q);
                                let mut hecho =
                                    crate::fiscal::esta_presentado(&self.db, &clave);
                                if ui.checkbox(&mut hecho, format!("{q}T")).changed() {
                                    cambios.push(clave);
                                }
                            }
                        } else {
                            let clave = format!("{}-{}", self.anyo, ob.codigo);
                            let mut hecho = crate::fiscal::esta_presentado(&self.db, &clave);
                            if ui.checkbox(&mut hecho, "hecho").changed() {
                                cambios.push(clave);
                            }
                        }
                    });
                });
                if crate::modelos::previsible(ob.codigo) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if crate::modelos::trimestral(ob.codigo) {
                            tema::etiqueta_ayuda(
                                ui,
                                "Periodo",
                                "Trimestre que se va a calcular y previsualizar. El modelo se genera con las facturas, gastos y nóminas de ese periodo.",
                            );
                            for q in 1..=4u32 {
                                if ui
                                    .selectable_label(self.trimestre == q, format!("{q}T"))
                                    .clicked()
                                {
                                    self.trimestre = q;
                                }
                            }
                        } else {
                            ui.label(
                                RichText::new(format!("Año completo {}", self.anyo))
                                    .size(11.0)
                                    .color(tema::SUAVE),
                            );
                        }
                        ui.add_space(8.0);
                        if ui
                            .button("Preparar el modelo")
                            .on_hover_text(
                                "Genera el PDF para revisar y el JSON para el gestor, y abre la carpeta.",
                            )
                            .clicked()
                        {
                            generar = Some(ob.codigo);
                        }
                    });
                    // Lo importante, en una frase y sin casillas de por medio.
                    let tri_resumen = if crate::modelos::trimestral(ob.codigo) {
                        self.trimestre
                    } else {
                        0
                    };
                    if let Some(m) =
                        crate::modelos::generar(&self.db, ob.codigo, self.anyo, tri_resumen)
                    {
                        ui.label(
                            RichText::new(m.resumen())
                                .size(11.5)
                                .strong()
                                .color(tema::TEXTO),
                        );
                    }

                    // Guía de entrega: página oficial, pasos y qué subir.
                    if let Some(g) = crate::presentacion::guia(ob.codigo) {
                        ui.add_space(2.0);
                        egui::CollapsingHeader::new(format!(
                            "Cómo presentarlo en la Sede · {}",
                            ob.codigo
                        ))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Cómo te identificas")
                                    .size(11.0)
                                    .strong()
                                    .color(tema::TEXTO),
                            );
                            for id in g.identificacion {
                                ui.label(
                                    RichText::new(format!("· {id}"))
                                        .size(11.0)
                                        .color(tema::SUAVE),
                                );
                            }
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Paso a paso")
                                    .size(11.0)
                                    .strong()
                                    .color(tema::TEXTO),
                            );
                            for (i, paso) in g.pasos.iter().enumerate() {
                                ui.label(
                                    RichText::new(format!("{}. {paso}", i + 1))
                                        .size(11.0)
                                        .color(tema::SUAVE),
                                );
                            }
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("Documentos: {}", g.fichero))
                                    .size(10.5)
                                    .color(tema::TENUE),
                            );
                            ui.label(
                                RichText::new(g.aviso).size(10.5).color(tema::TENUE),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui.button("Abrir la Sede de la AEAT").clicked() {
                                    abrir_url(g.sede);
                                }
                                if ui
                                    .button("Copiar casillas (JSON)")
                                    .on_hover_text(
                                        "Pégalo en otro programa o pásaselo a tu gestor.",
                                    )
                                    .clicked()
                                {
                                    let tri = if crate::modelos::trimestral(ob.codigo) {
                                        self.trimestre
                                    } else {
                                        0
                                    };
                                    if let Some(m) = crate::modelos::generar(
                                        &self.db,
                                        ob.codigo,
                                        self.anyo,
                                        tri,
                                    ) {
                                        let json = m.json(&self.db.empresa);
                                        ui.output_mut(|o| o.copied_text = json);
                                        self.avisar("Casillas copiadas al portapapeles.", false);
                                    }
                                }
                                if let Some(url) = crate::presentacion::diseno_registro(ob.codigo) {
                                    if ui.button("Diseño de registro oficial").clicked() {
                                        abrir_url(url);
                                    }
                                }
                            });
                        });
                    }
                }
            });
            ui.add_space(4.0);
        }
        if let Some(codigo) = generar {
            self.generar_modelo(codigo);
        }
        if !cambios.is_empty() {
            for clave in cambios {
                crate::fiscal::alternar(&mut self.db, &clave);
            }
            self.guardar();
        }

        // Presentación automática: qué se puede y qué no.
        ui.add_space(14.0);
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new("Presentarlo automáticamente, estilo TaxDown")
                    .size(13.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            ui.label(
                RichText::new(
                    "La AEAT no tiene API pública de presentación. Estas son las vías reales, de la más sencilla a la más automática:",
                )
                .size(11.0)
                .color(tema::SUAVE),
            );
            ui.add_space(6.0);
            for (titulo, detalle) in crate::presentacion::AUTOMATICO {
                ui.label(
                    RichText::new(*titulo)
                        .size(11.5)
                        .strong()
                        .color(tema::TEXTO),
                );
                ui.label(RichText::new(*detalle).size(11.0).color(tema::SUAVE));
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                if ui.button("Abrir la Sede electrónica").clicked() {
                    abrir_url(crate::presentacion::SEDE);
                }
                if ui.button("Abrir la carpeta de exportación").clicked() {
                    let dir = export::raiz_empresa(&self.db.empresa);
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });
        });

        ui.add_space(14.0);
        ui.label(
            RichText::new(format!(
                "Según tu actividad: {}",
                self.db.empresa.actividad.etiqueta()
            ))
            .size(15.0)
            .strong()
            .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        for (titulo, texto) in crate::fiscal::guia(self.db.empresa.actividad) {
            ui.label(RichText::new(titulo).size(12.5).strong().color(tema::TEXTO));
            ui.label(RichText::new(texto).size(11.5).color(tema::SUAVE));
            ui.add_space(5.0);
        }

        // Comercio: lo que más se atasca al vender online en España.
        if matches!(
            self.db.empresa.actividad,
            Actividad::Comercio | Actividad::Digital
        ) {
            ui.add_space(8.0);
            tema::tarjeta(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    RichText::new("Comercio electrónico sin sustos")
                        .size(13.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                ui.label(
                    RichText::new(
                        "Vendas por tu web, por una tienda o por un marketplace, esto es lo que mira Hacienda.",
                    )
                    .size(11.0)
                    .color(tema::SUAVE),
                );
                ui.add_space(6.0);
                for (titulo, texto) in crate::fiscal::guia_comercio() {
                    ui.label(RichText::new(titulo).size(11.5).strong().color(tema::TEXTO));
                    ui.label(RichText::new(texto).size(11.0).color(tema::SUAVE));
                    ui.add_space(4.0);
                }
            });
        }
        if ui.button("Cambiar mi actividad").clicked() {
            self.vista = Vista::Ajustes;
        }

        ui.add_space(12.0);
        ui.label(
            RichText::new("Qué hay que presentar y cuándo")
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        for (fecha, modelo, detalle) in crate::contab::vencimientos(Local::now().year()) {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(fecha.format("%d/%m/%Y").to_string())
                        .monospace()
                        .color(tema::SUAVE),
                );
                ui.add_space(8.0);
                ui.label(RichText::new(modelo).strong().color(tema::TEXTO));
                ui.label(RichText::new(detalle).size(11.5).color(tema::SUAVE));
            });
        }

        ui.add_space(12.0);
        if ui
            .add(egui::Button::new("Descargar resumen del año para el gestor").min_size(egui::vec2(0.0, 32.0)))
            .clicked()
        {
            match self.exportar_informe() {
                Ok(p) => {
                    abrir_pdf_y_carpeta(&p);
                    self.avisar(format!("Resumen guardado: {}", p.display()), false);
                }
                Err(e) => self.avisar(e, true),
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Incluye cifras anuales, por trimestre y el 347: sirve de borrador para el 303 y el 130.",
            )
            .size(11.0)
            .color(tema::TENUE),
        );
    }

    fn vista_empleados(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Empleados")
                    .size(24.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            tema::icono_ayuda(
                ui,
                "Empleados",
                "Fichas laborales y nóminas mensuales. Los cálculos son orientativos: revisa cada recibo y el boletín de cotización con tu asesor.",
            );
            ui.add_space(10.0);
            if ui.button("+ Nuevo empleado").clicked() {
                let mut e = Empleado::default();
                e.id = self.db.id();
                self.ficha_empleado = Some(EmpleadoBuf::nuevo(e));
                self.pestana_empleados = 0;
            }
        });
        ui.label(
            RichText::new(
                "Fichas laborales y nóminas mensuales. Los cálculos son orientativos: revisa el recibo y el boletín de cotización con tu asesor.",
            )
            .size(11.5)
            .color(tema::SUAVE),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.pestana_empleados == 0, "Fichas")
                .clicked()
            {
                self.pestana_empleados = 0;
            }
            if ui
                .selectable_label(self.pestana_empleados == 1, "Nóminas")
                .clicked()
            {
                self.pestana_empleados = 1;
            }
            tema::icono_ayuda(
                ui,
                "Fichas y nóminas",
                "«Fichas» guarda los datos laborales; «Nóminas» genera los recibos del mes y los resúmenes para el 111 y el 190. Los importes son orientativos.",
            );
        });
        ui.separator();
        if self.pestana_empleados == 0 {
            self.vista_empleados_fichas(ui);
        } else {
            self.vista_empleados_nominas(ui);
        }
    }

    fn vista_empleados_fichas(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("lista_empleados")
                .show(&mut cols[0], |ui| {
                    if self.db.empleados.is_empty() {
                        tema::vacio(
                            ui,
                            "Sin empleados todavía",
                            "Pulsa «+ Nuevo empleado» para guardar la primera ficha.",
                        );
                    }
                    let mut abrir: Option<usize> = None;
                    for (i, e) in self.db.empleados.iter().enumerate() {
                        let actual = self
                            .ficha_empleado
                            .as_ref()
                            .map(|f| f.empleado.id == e.id)
                            .unwrap_or(false);
                        let estado = if e.activo() { "" } else { " · baja" };
                        let etiqueta = format!(
                            "{}{} · {}",
                            e.etiqueta(),
                            estado,
                            fmt_cents_eur(e.salario_bruto_anual_cents)
                        );
                        if ui.selectable_label(actual, etiqueta).clicked() {
                            abrir = Some(i);
                        }
                    }
                    if let Some(i) = abrir {
                        self.ficha_empleado =
                            Some(EmpleadoBuf::desde(self.db.empleados[i].clone()));
                    }
                });

            let mut guardar_datos: Option<Empleado> = None;
            let mut borrar_id: Option<u64> = None;
            let mut error: Option<String> = None;
            if let Some(f) = self.ficha_empleado.as_mut() {
                let mut guardar = false;
                let mut borrar = false;
                egui::ScrollArea::vertical()
                    .id_salt("ficha_empleado")
                    .show(&mut cols[1], |ui| {
                        ui.heading(if f.empleado.nombre.is_empty() {
                            "Nuevo empleado"
                        } else {
                            "Ficha de empleado"
                        });
                        egui::Grid::new("form_empleado")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                campo_h(
                                    ui,
                                    "Nombre",
                                    &mut f.empleado.nombre,
                                    "Nombre de la persona que aparece en la nómina.",
                                );
                                campo_h(
                                    ui,
                                    "Apellidos",
                                    &mut f.empleado.apellidos,
                                    "Apellidos de la persona que aparecen en la nómina y en el 190.",
                                );
                                campo_h(
                                    ui,
                                    "NIF / DNI / NIE",
                                    &mut f.empleado.nif,
                                    "Se usa para el 111 y el 190. Se comprueba la letra de control.",
                                );
                                if !f.empleado.nif.trim().is_empty()
                                    && !nif_valido(&f.empleado.nif)
                                {
                                    ui.label("");
                                    ui.label(
                                        RichText::new(
                                            "Ese NIF no cuadra con su letra de control. Revísalo antes de presentar el 190.",
                                        )
                                        .size(10.5)
                                        .color(tema::AMBAR),
                                    );
                                    ui.end_row();
                                }
                                campo_h(
                                    ui,
                                    "Nº afiliación SS",
                                    &mut f.empleado.naf,
                                    "Número de afiliación (NAF) del trabajador, si lo tienes a mano.",
                                );
                                campo_h(
                                    ui,
                                    "Puesto",
                                    &mut f.empleado.puesto,
                                    "Puesto o categoría profesional. Es informativo: no cambia las cotizaciones.",
                                );
                                campo_h(
                                    ui,
                                    "Email",
                                    &mut f.empleado.email,
                                    "Para enviar la nómina o avisos. No se imprime en el recibo.",
                                );
                                campo_h(
                                    ui,
                                    "Teléfono",
                                    &mut f.empleado.telefono,
                                    "Teléfono de contacto del empleado; opcional.",
                                );
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Tipo de contrato",
                                    "Se usa para la ficha laboral. Los tipos de cotización son orientativos y no sustituyen el criterio de tu asesor.",
                                );
                                egui::ComboBox::from_id_salt("combo_contrato")
                                    .selected_text(f.empleado.tipo_contrato.etiqueta())
                                    .width(220.0)
                                    .show_ui(ui, |ui| {
                                        for t in TipoContrato::TODOS {
                                            ui.selectable_value(
                                                &mut f.empleado.tipo_contrato,
                                                t,
                                                t.etiqueta(),
                                            );
                                        }
                                    });
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Fecha de alta",
                                    "Primer día con alta en la Seguridad Social. Decide en qué nóminas aparece el empleado.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.alta)
                                        .desired_width(120.0)
                                        .hint_text("dd/mm/aaaa"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Fecha de baja (opcional)",
                                    "Último día de alta. Si la dejas vacía, el empleado sigue activo y se incluye en las próximas nóminas.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.baja)
                                        .desired_width(120.0)
                                        .hint_text("dd/mm/aaaa"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Salario bruto anual",
                                    "Salario bruto de 12 pagas antes de cotizaciones y retenciones. La nómina mensual se calcula dividiéndolo entre 12.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.salario)
                                        .desired_width(120.0)
                                        .hint_text("24.000,00"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "IRPF %",
                                    "Tipo de retención de IRPF que aplicarás en la nómina. Es orientativo: confirma el porcentaje con tu asesor o con la AEAT.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.irpf)
                                        .desired_width(90.0)
                                        .hint_text("15"),
                                );
                                ui.end_row();
                                campo_h(
                                    ui,
                                    "Notas",
                                    &mut f.empleado.notas,
                                    "Notas internas de la ficha laboral. No salen en la nómina.",
                                );
                            });
                        ui.label(
                            RichText::new(
                                "Estimación orientativa: 12 pagas y tipos de 2026. No aplica bases mínimas/máximas ni sustituye el recibo oficial.",
                            )
                            .size(10.5)
                            .color(tema::TENUE),
                        );
                        ui.add_space(8.0);
                        let alta_ok = parse_fecha(&f.alta).is_some();
                        let baja_ok = f.baja.trim().is_empty() || parse_fecha(&f.baja).is_some();
                        let salario_ok = parse_cents(&f.salario)
                            .map(|s| s > 0)
                            .unwrap_or(false);
                        let irpf_ok = parse_pb(&f.irpf).is_some();
                        let valido = !f.empleado.nombre.trim().is_empty()
                            && alta_ok
                            && baja_ok
                            && salario_ok
                            && irpf_ok;
                        ui.horizontal(|ui| {
                            if ui.add_enabled(valido, egui::Button::new("Guardar")).clicked() {
                                guardar = true;
                            }
                            if ui.button("Borrar").clicked() {
                                borrar = true;
                            }
                        });
                    });
                if guardar {
                    let alta = parse_fecha(&f.alta);
                    let baja = if f.baja.trim().is_empty() {
                        None
                    } else {
                        parse_fecha(&f.baja)
                    };
                    let salario = parse_cents(&f.salario);
                    let irpf = parse_pb(&f.irpf);
                    if let (Some(alta), Some(salario), Some(irpf)) = (alta, salario, irpf) {
                        let baja_valida = f.baja.trim().is_empty()
                            || baja.map(|b| b >= alta).unwrap_or(false);
                        if salario > 0 && baja_valida {
                            f.empleado.fecha_alta = alta;
                            f.empleado.fecha_baja = baja;
                            f.empleado.salario_bruto_anual_cents = salario;
                            f.empleado.irpf_pb = irpf;
                            guardar_datos = Some(f.empleado.clone());
                        } else if salario <= 0 {
                            error = Some("El salario bruto anual debe ser mayor que cero.".into());
                        } else {
                            error = Some("La fecha de baja no puede ser anterior a la de alta.".into());
                        }
                    } else {
                        error = Some("Revisa las fechas y el salario: usa dd/mm/aaaa y un importe válido.".into());
                    }
                }
                if borrar {
                    borrar_id = Some(f.empleado.id);
                }
            }
            if let Some(msg) = error {
                self.avisar(msg, true);
            }
            if let Some(e) = guardar_datos {
                if let Some(existente) = self.db.empleado_mut(e.id) {
                    *existente = e;
                } else {
                    self.db.empleados.push(e);
                }
                self.guardar();
                self.avisar("Empleado guardado.", false);
                self.ficha_empleado = None;
            }
            if let Some(id) = borrar_id {
                if self.db.nominas.iter().any(|n| n.empleado_id == id) {
                    self.avisar(
                        "No se puede borrar: tiene nóminas generadas. Borra antes sus nóminas.",
                        true,
                    );
                } else {
                    self.db.empleados.retain(|e| e.id != id);
                    self.guardar();
                    self.avisar("Empleado borrado.", false);
                    self.ficha_empleado = None;
                }
            }
        });
    }

    fn vista_empleados_nominas(&mut self, ui: &mut egui::Ui) {
        let mut generar_retencion: Option<(&'static str, i32, u32)> = None;
        ui.horizontal(|ui| {
            ui.label("Año");
            let anyo_actual = Local::now().year();
            egui::ComboBox::from_id_salt("anyo_nomina")
                .selected_text(self.nomina_anyo.to_string())
                .width(85.0)
                .show_ui(ui, |ui| {
                    for y in (anyo_actual - 5)..=(anyo_actual + 1) {
                        ui.selectable_value(&mut self.nomina_anyo, y, y.to_string());
                    }
                });
            ui.label("Mes");
            egui::ComboBox::from_id_salt("mes_nomina")
                .selected_text(MESES_NOMINA[(self.nomina_mes.saturating_sub(1) as usize).min(11)])
                .width(120.0)
                .show_ui(ui, |ui| {
                    for (i, nombre) in MESES_NOMINA.iter().enumerate() {
                        ui.selectable_value(&mut self.nomina_mes, i as u32 + 1, *nombre);
                    }
                });
            if ui.button("Generar nóminas del mes").clicked() {
                match self.generar_nominas_mes() {
                    Ok((creadas, saltadas)) => {
                        let mut mensaje = format!("{} nómina(s) generada(s).", creadas);
                        if saltadas > 0 {
                            mensaje.push_str(&format!(" {} ya existían.", saltadas));
                        }
                        self.avisar(mensaje, false);
                    }
                    Err(e) => self.avisar(e, true),
                }
            }
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Estimación de nómina mensual con 12 pagas y los tipos generales de 2026. Revisa cada caso con tu asesor y usa el recibo oficial para presentar.",
            )
            .size(11.0)
            .color(tema::SUAVE),
        );
        ui.add_space(6.0);
        egui::Frame::none()
            .fill(tema::AMBAR_CLARO)
            .stroke(egui::Stroke::new(1.0, tema::AMBAR))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(12.0, 9.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("⚠ Estimación, no nómina oficial")
                        .size(12.0)
                        .strong()
                        .color(tema::AMBAR),
                );
                ui.label(
                    RichText::new(
                        "No se aplican las bases mínimas y máximas de cotización. En 2026 el tope máximo es 5.101,20 €/mes y la base mínima depende del grupo de cotización. En salarios altos, la Seguridad Social calculada puede ser superior a la real; en salarios bajos, inferior. Revisa el recibo y el boletín con tu asesor.",
                    )
                    .size(11.0)
                    .color(tema::AMBAR),
                );
            });
        ui.add_space(8.0);

        let trimestre = ((self.nomina_mes.saturating_sub(1)) / 3) + 1;
        let resumen_trim = crate::empleados::resumen_retenciones(
            &self.db.nominas,
            &self.db.empleados,
            self.nomina_anyo,
            trimestre,
        );
        let resumen_anual = crate::empleados::resumen_retenciones(
            &self.db.nominas,
            &self.db.empleados,
            self.nomina_anyo,
            0,
        );
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                RichText::new(format!(
                    "Modelos 111 y 190 · año {} · trimestre {}T",
                    self.nomina_anyo, trimestre
                ))
                .size(13.0)
                .strong()
                .color(tema::TEXTO),
            );
            ui.label(
                RichText::new(format!(
                    "111: {} nóminas, {} perceptores, base {} y retenciones {}. · 190: {} nóminas, {} perceptores, base {} y retenciones {}.",
                    resumen_trim.num_nominas,
                    resumen_trim.perceptores.len(),
                    fmt_cents_eur(resumen_trim.total_bruto_cents),
                    fmt_cents_eur(resumen_trim.total_irpf_cents),
                    resumen_anual.num_nominas,
                    resumen_anual.perceptores.len(),
                    fmt_cents_eur(resumen_anual.total_bruto_cents),
                    fmt_cents_eur(resumen_anual.total_irpf_cents)
                ))
                .size(11.0)
                .color(tema::SUAVE),
            );
            ui.label(
                RichText::new(
                    "Sólo se calculan con las nóminas generadas: no incluyen facturas de profesionales, rentas en especie, regularizaciones ni otros pagos. Revisa los NIF antes de presentar.",
                )
                .size(10.5)
                .color(tema::TENUE),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        resumen_trim.num_nominas > 0,
                        egui::Button::new("Preparar 111 del trimestre"),
                    )
                    .on_hover_text("Genera el PDF y el JSON del trimestre. El año y el trimestre salen del selector de arriba.")
                    .clicked()
                {
                    generar_retencion = Some(("111", self.nomina_anyo, trimestre));
                }
                if ui
                    .add_enabled(
                        resumen_anual.num_nominas > 0,
                        egui::Button::new("Preparar 190 del año"),
                    )
                    .on_hover_text("Genera el PDF y el JSON del resumen anual a partir de estas nóminas.")
                    .clicked()
                {
                    generar_retencion = Some(("190", self.nomina_anyo, 0));
                }
            });
        });
        ui.add_space(6.0);

        let filas: Vec<Nomina> = self
            .db
            .nominas
            .iter()
            .filter(|n| n.anyo == self.nomina_anyo && n.mes == self.nomina_mes)
            .cloned()
            .collect();
        if filas.is_empty() {
            tema::vacio(
                ui,
                "Sin nóminas este mes",
                "Guarda empleados activos y pulsa «Generar nóminas del mes».",
            );
        } else {
            let bruto: i64 = filas.iter().map(|n| n.bruto_cents).sum();
            let ss_empleado: i64 = filas.iter().map(|n| n.ss_empleado_cents).sum();
            let irpf: i64 = filas.iter().map(|n| n.irpf_cents).sum();
            let neto: i64 = filas.iter().map(|n| n.neto_cents).sum();
            let coste: i64 = filas.iter().map(|n| n.coste_empresa_cents).sum();
            tema::tarjeta(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Bruto {} · SS empleado {} · IRPF {} · Neto {}", fmt_cents_eur(bruto), fmt_cents_eur(ss_empleado), fmt_cents_eur(irpf), fmt_cents_eur(neto))).size(11.5).color(tema::SUAVE));
                    ui.label(RichText::new(format!("Coste empresa {}", fmt_cents_eur(coste))).size(12.0).strong().color(tema::ACENTO));
                });
            });
            ui.add_space(6.0);

            let mut borrar: Option<u64> = None;
            let mut pdf_id: Option<u64> = None;
            let mut pagada: Option<(u64, bool)> = None;
            // La tabla puede tener muchas nóminas: se limita al alto que queda
            // libre y se desplaza en vertical. Las acciones (PDF y Borrar)
            // siguen dentro de cada fila, así que se mueven con ella y no se
            // quedan fuera de la ventana.
            let alto_tabla = (ui.available_height() - 6.0).max(180.0);
            egui::ScrollArea::vertical()
                .id_salt("tabla_nominas_scroll")
                .max_height(alto_tabla)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("tabla_nominas")
                        .num_columns(8)
                        .spacing([12.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for cabecera in [
                                "Empleado", "Bruto", "SS empleado", "IRPF", "Neto", "Coste empresa", "Pagada", "Acciones",
                            ] {
                                ui.label(RichText::new(cabecera).size(10.5).strong().color(tema::TENUE));
                            }
                            ui.end_row();
                            for n in &filas {
                                ui.label(&n.empleado_nombre);
                                ui.label(fmt_cents_eur(n.bruto_cents));
                                ui.label(fmt_cents_eur(n.ss_empleado_cents));
                                ui.label(fmt_cents_eur(n.irpf_cents));
                                ui.label(fmt_cents_eur(n.neto_cents));
                                ui.label(fmt_cents_eur(n.coste_empresa_cents));
                                let mut p = n.pagada;
                                if ui.checkbox(&mut p, "").changed() {
                                    pagada = Some((n.id, p));
                                }
                                ui.horizontal(|ui| {
                                    if ui.small_button("PDF").clicked() {
                                        pdf_id = Some(n.id);
                                    }
                                    if ui.small_button("Borrar").clicked() {
                                        borrar = Some(n.id);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
            if let Some((id, p)) = pagada {
                if let Some(n) = self.db.nominas.iter_mut().find(|n| n.id == id) {
                    n.pagada = p;
                }
                self.guardar();
            }
            if let Some(id) = pdf_id {
                match self.exportar_nomina_pdf(id) {
                    Ok(p) => self.avisar(format!("Resumen de nómina PDF: {}", p.display()), false),
                    Err(e) => self.avisar(e, true),
                }
            }
            if let Some(id) = borrar {
                self.db.nominas.retain(|n| n.id != id);
                self.guardar();
                self.avisar("Nómina borrada.", false);
            }
        }
        if let Some((codigo, anyo, tri)) = generar_retencion {
            self.anyo = anyo;
            self.trimestre = tri;
            self.generar_modelo(codigo);
        }
    }

    fn generar_nominas_mes(&mut self) -> Result<(usize, usize), String> {
        let resultado = crate::empleados::generar_nominas_mes(
            &mut self.db.nominas,
            &self.db.empleados,
            &mut self.db.next_id,
            self.nomina_anyo,
            self.nomina_mes,
        );
        if let Ok((creadas, _)) = &resultado {
            if *creadas > 0 {
                self.guardar();
            }
        }
        resultado
    }

    fn vista_clientes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Clientes");
            tema::icono_ayuda(
                ui,
                "Clientes",
                "La ficha del cliente decide el IVA, la retención y el recargo que se proponen al facturar. Sin NIF se emite factura simplificada.",
            );
            ui.add_space(10.0);
            if ui.button("+ Nuevo cliente").clicked() {
                let mut c = Cliente::default();
                c.id = self.db.id();
                self.ficha_cliente = Some(c);
            }
        });
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("lista_clientes")
                .show(&mut cols[0], |ui| {
                    if self.db.clientes.is_empty() {
                        tema::vacio(
                            ui,
                            "Sin clientes todavía",
                            "Pulsa «+ Nuevo cliente» para crear el primero.",
                        );
                    }
                    let mut abrir: Option<usize> = None;
                    for (i, c) in self.db.clientes.iter().enumerate() {
                        let actual = self
                            .ficha_cliente
                            .as_ref()
                            .map(|f| f.id == c.id)
                            .unwrap_or(false);
                        if ui.selectable_label(actual, c.etiqueta()).clicked() {
                            abrir = Some(i);
                        }
                    }
                    if let Some(i) = abrir {
                        self.ficha_cliente = Some(self.db.clientes[i].clone());
                    }
                });

            let mut guardar_datos: Option<Cliente> = None;
            let mut borrar_id: Option<u64> = None;
            if let Some(f) = self.ficha_cliente.as_mut() {
                let mut guardar = false;
                let mut borrar = false;
                egui::ScrollArea::vertical()
                    .id_salt("ficha_cliente")
                    .show(&mut cols[1], |ui| {
                        ui.heading(if f.nombre.is_empty() {
                            "Nuevo cliente"
                        } else {
                            "Ficha de cliente"
                        });
                        egui::Grid::new("form_cliente")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                campo_h(
                                    ui,
                                    "Nombre / razón social",
                                    &mut f.nombre,
                                    "Como debe salir impreso en la factura.",
                                );
                                campo_h(
                                    ui,
                                    "NIF / DNI / NIE",
                                    &mut f.nif,
                                    "Vacío = particular sin NIF: la factura sale simplificada (F2/FA).",
                                );
                                if !f.nif.trim().is_empty() && !nif_valido(&f.nif) {
                                    ui.label("");
                                    ui.label(
                                        RichText::new(
                                            "Ese NIF no cuadra con su letra de control. Revísalo: una factura con NIF erróneo no vale como justificante del gasto del cliente.",
                                        )
                                        .size(10.5)
                                        .color(tema::AMBAR),
                                    );
                                    ui.end_row();
                                }
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Tipo",
                                    "Marca autónomo o particular. En FacturaE cambia Individual/LegalEntity y también decide si se piden apellidos.",
                                );
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.persona_fisica, "Persona física");
                                });
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Régimen fiscal",
                                    "Decide el IVA, la retención de IRPF y el recargo que se proponen en cada factura.",
                                );
                                egui::ComboBox::from_id_salt("combo_regimen_cliente")
                                    .selected_text(f.regimen.etiqueta())
                                    .width(320.0)
                                    .show_ui(ui, |ui| {
                                        for r in RegimenCliente::TODOS {
                                            ui.selectable_value(&mut f.regimen, r, r.etiqueta());
                                        }
                                    });
                                ui.end_row();
                                ui.label("");
                                ui.label(
                                    RichText::new("En facturas: 21 % general, 15 % de retención si es profesional, 5,2 % de recargo si es minorista y 0 % con leyenda si es de la UE o ISP.")
                                        .size(10.5)
                                        .color(tema::TENUE),
                                );
                                ui.end_row();
                                if f.persona_fisica {
                                    campo_h(
                                        ui,
                                        "Apellidos",
                                        &mut f.apellidos,
                                        "Apellidos de la persona. Se imprimen después del nombre y FacturaE los pide separados.",
                                    );
                                }
                                campo_h(
                                    ui,
                                    "Dirección",
                                    &mut f.direccion,
                                    "Calle, número, piso y puerta. Es la dirección que sale impresa en la factura.",
                                );
                                campo_h(
                                    ui,
                                    "Código postal",
                                    &mut f.cp,
                                    "Cinco dígitos. Ayuda a que la factura y el 347 identifiquen bien al cliente.",
                                );
                                campo_h(
                                    ui,
                                    "Ciudad",
                                    &mut f.ciudad,
                                    "Municipio de la dirección de facturación.",
                                );
                                campo_h(
                                    ui,
                                    "Provincia",
                                    &mut f.provincia,
                                    "Provincia de la dirección de facturación.",
                                );
                                campo_h(
                                    ui,
                                    "Email",
                                    &mut f.email,
                                    "Para enviar la factura o el presupuesto. No se imprime si lo dejas vacío.",
                                );
                                campo_h(
                                    ui,
                                    "Teléfono",
                                    &mut f.telefono,
                                    "Teléfono de contacto del cliente; opcional.",
                                );
                                campo_h(
                                    ui,
                                    "Notas",
                                    &mut f.notas,
                                    "Notas internas del cliente. No se imprimen en la factura; para eso usa las notas del documento.",
                                );
                            });
                        ui.label(
                            RichText::new("Sin NIF se emite factura simplificada (F2/FA).")
                                .small()
                                .color(egui::Color32::from_gray(120)),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !f.nombre.trim().is_empty(),
                                    egui::Button::new("Guardar"),
                                )
                                .clicked()
                            {
                                guardar = true;
                            }
                            if ui.button("Borrar").clicked() {
                                borrar = true;
                            }
                        });
                    });
                if guardar {
                    let mut c = f.clone();
                    // Un NIF que empieza por dígito es DNI/NIE: persona física.
                    if c
                        .nif
                        .trim()
                        .chars()
                        .next()
                        .map(|x| x.is_ascii_digit())
                        .unwrap_or(false)
                    {
                        c.persona_fisica = true;
                    }
                    if matches!(
                        c.regimen,
                        RegimenCliente::Profesional | RegimenCliente::Minorista
                    ) {
                        c.persona_fisica = true;
                    }
                    guardar_datos = Some(c);
                }
                if borrar {
                    borrar_id = Some(f.id);
                }
            }
            if let Some(c) = guardar_datos {
                if let Some(existente) = self.db.cliente_mut(c.id) {
                    *existente = c;
                } else {
                    self.db.clientes.push(c);
                }
                self.guardar();
                self.avisar("Cliente guardado.", false);
                self.ficha_cliente = None;
            }
            if let Some(id) = borrar_id {
                let usado = self.db.documentos.iter().any(|d| d.cliente_id == id);
                if usado {
                    self.avisar("No se puede borrar: tiene documentos asociados.", true);
                } else {
                    self.db.clientes.retain(|c| c.id != id);
                    self.guardar();
                    self.ficha_cliente = None;
                }
            }
        });
    }

    fn vista_articulos(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Artículos")
                    .size(24.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            tema::icono_ayuda(
                ui,
                "Artículos",
                "Un catálogo para no repetir precios. Al escribir una descripción en una factura, se propone el artículo que empiece igual.",
            );
            ui.add_space(10.0);
            if ui.button("+ Nuevo artículo").clicked() {
                let mut a = Articulo {
                    iva_pb: self.db.empresa.iva_defecto,
                    ..Default::default()
                };
                a.id = self.db.id();
                self.ficha_articulo = Some(ArticuloBuf::nuevo(a));
            }
        });
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            egui::ScrollArea::vertical()
                .id_salt("lista_articulos")
                .show(&mut cols[0], |ui| {
                    if self.db.articulos.is_empty() {
                        ui.label("Sin artículos todavía.");
                    }
                    let mut abrir: Option<usize> = None;
                    for (i, a) in self.db.articulos.iter().enumerate() {
                        let actual = self
                            .ficha_articulo
                            .as_ref()
                            .map(|f| f.art.id == a.id)
                            .unwrap_or(false);
                        let etiqueta = format!("{}  ·  {}", a.descripcion, fmt_cents(a.precio_cents));
                        if ui.selectable_label(actual, etiqueta).clicked() {
                            abrir = Some(i);
                        }
                    }
                    if let Some(i) = abrir {
                        self.ficha_articulo =
                            Some(ArticuloBuf::desde(self.db.articulos[i].clone()));
                    }
                });

            let mut guardar_datos: Option<Articulo> = None;
            let mut borrar_id: Option<u64> = None;
            let mut error: Option<String> = None;
            if let Some(f) = self.ficha_articulo.as_mut() {
                let mut guardar = false;
                let mut borrar = false;
                egui::ScrollArea::vertical()
                    .id_salt("ficha_articulo")
                    .show(&mut cols[1], |ui| {
                        ui.heading(if f.art.descripcion.is_empty() {
                            "Nuevo artículo"
                        } else {
                            "Ficha de artículo"
                        });
                        egui::Grid::new("form_articulo")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                campo_h(
                                    ui,
                                    "Descripción",
                                    &mut f.art.descripcion,
                                    "Nombre del producto o servicio. Al escribir las primeras letras en una factura, se propone este artículo.",
                                );
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Precio unitario",
                                    "Precio sin IVA. En la factura se multiplica por la cantidad y se le aplica el descuento de la línea.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.precio)
                                        .desired_width(120.0)
                                        .hint_text("0,00"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "IVA %",
                                    "21 general, 10 reducido, 4 superreducido o 0 exento. Se propone al insertar el artículo, pero puedes cambiarlo en cada línea.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.iva)
                                        .desired_width(120.0)
                                        .hint_text("21"),
                                );
                                ui.end_row();
                                campo_h(
                                    ui,
                                    "Unidad",
                                    &mut f.art.unidad,
                                    "Unidad de medida (ud., hora, kg…). Es informativa; la cantidad de la factura manda.",
                                );
                            });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let valido = !f.art.descripcion.trim().is_empty()
                                && parse_cents(&f.precio).is_some()
                                && parse_pb(&f.iva).is_some();
                            if ui.add_enabled(valido, egui::Button::new("Guardar")).clicked() {
                                guardar = true;
                            }
                            if ui.button("Borrar").clicked() {
                                borrar = true;
                            }
                        });
                    });
                if guardar {
                    match (parse_cents(&f.precio), parse_pb(&f.iva)) {
                        (Some(p), Some(i)) => {
                            f.art.precio_cents = p;
                            f.art.iva_pb = i;
                            guardar_datos = Some(f.art.clone());
                        }
                        _ => {
                            error = Some(
                                "Precio o IVA no válido: escribe números, por ejemplo 1.250,50"
                                    .to_string(),
                            );
                        }
                    }
                }
                if borrar {
                    borrar_id = Some(f.art.id);
                }
            }
            if let Some(msg) = error {
                self.avisar(msg, true);
            }
            if let Some(a) = guardar_datos {
                if let Some(existente) = self.db.articulos.iter_mut().find(|x| x.id == a.id) {
                    *existente = a;
                } else {
                    self.db.articulos.push(a);
                }
                self.guardar();
                self.avisar("Artículo guardado.", false);
                self.ficha_articulo = None;
            }
            if let Some(id) = borrar_id {
                self.db.articulos.retain(|a| a.id != id);
                self.guardar();
                self.ficha_articulo = None;
            }
        });
    }

    /// Copia con fecha del fichero de datos completo.
    fn crear_copia(&mut self) -> Result<PathBuf, String> {
        self.guardar();
        let dir = self.db.dir_copias();
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let sello = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let destino = dir.join(export::nombre_copia(&self.db.empresa, &sello));
        std::fs::copy(&self.db.ruta, &destino)
            .map_err(|e| format!("copiar a {}: {e}", destino.display()))?;
        Ok(destino)
    }

    /// Copias existentes, de la más reciente a la más antigua.
    fn copias_disponibles(&self) -> Vec<(String, PathBuf)> {
        let dir = self.db.dir_copias();
        let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
            .map(|it| {
                it.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
                    .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.0.cmp(&a.0));
        v
    }

    fn restaurar_copia(&mut self, ruta: PathBuf) -> Result<(), String> {
        // Red de seguridad: lo actual queda copiado antes de sustituirlo.
        let _ = self.crear_copia();
        std::fs::copy(&ruta, &self.db.ruta)
            .map_err(|e| format!("restaurar {}: {e}", ruta.display()))?;
        let (nuevo, aviso) = Db::cargar();
        self.db = nuevo;
        self.editor = None;
        self.seleccion = None;
        self.ficha_cliente = None;
        self.ficha_empleado = None;
        self.ficha_articulo = None;
        self.ficha_gasto = None;
        self.cobro = None;
        let mensaje = aviso.unwrap_or_else(|| {
            format!(
                "Copia restaurada: {}",
                ruta.file_name().and_then(|n| n.to_str()).unwrap_or("datos")
            )
        });
        self.avisar(mensaje, false);
        Ok(())
    }

    fn ventana_restaurar(&mut self, ctx: &egui::Context) {
        let Some(ruta) = self.restaurar.clone() else {
            return;
        };
        let mut cerrar = false;
        egui::Window::new("Restaurar copia")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(440.0);
                ui.label("Se sustituirán los datos actuales por los de esta copia:");
                ui.label(
                    RichText::new(
                        ruta.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("datos.json"),
                    )
                    .monospace()
                    .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Antes se guarda una copia automática del estado actual.")
                        .size(11.5)
                        .color(tema::SUAVE),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if tema::boton_primario(ui, "Restaurar").clicked() {
                        match self.restaurar_copia(ruta.clone()) {
                            Ok(()) => cerrar = true,
                            Err(e) => self.avisar(e, true),
                        }
                    }
                    if ui.button("Cancelar").clicked() {
                        cerrar = true;
                    }
                });
            });
        if !cerrar {
            self.restaurar = Some(ruta);
        }
    }

    fn ventana_cobro(&mut self, ctx: &egui::Context) {
        let Some((id, mut fecha, mut importe)) = self.cobro.take() else {
            return;
        };
        let mut cerrar = false;
        egui::Window::new("Registrar cobro")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                let pendiente = self
                    .db
                    .documento(id)
                    .map(|d| d.pendiente_cents())
                    .unwrap_or(0);
                ui.label(RichText::new("Pendiente").size(11.5).color(tema::SUAVE));
                ui.label(
                    RichText::new(fmt_cents_eur(pendiente))
                        .size(20.0)
                        .strong()
                        .color(tema::ACENTO),
                );
                ui.add_space(8.0);
                egui::Grid::new("form_cobro")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        tema::etiqueta_ayuda(
                            ui,
                            "Fecha",
                            "Fecha en la que recibes el cobro. Se guarda en el historial de pagos y decide si la factura pasa a cobrada.",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut fecha)
                                .desired_width(150.0)
                                .margin(egui::Margin::symmetric(10.0, 7.0)),
                        );
                        ui.end_row();
                        tema::etiqueta_ayuda(
                            ui,
                            "Importe",
                            "Cantidad cobrada en este pago. Puede ser parcial: la factura seguirá pendiente hasta cubrir el total.",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut importe)
                                .desired_width(150.0)
                                .margin(egui::Margin::symmetric(10.0, 7.0)),
                        );
                        ui.end_row();
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if tema::boton_primario(ui, "Registrar cobro").clicked() {
                        match (parse_fecha(&fecha), parse_cents(&importe)) {
                            (Some(f), Some(c)) if c > 0 => {
                                if let Some(d) = self.db.documento_mut(id) {
                                    d.pagos.push(Pago {
                                        fecha: f,
                                        importe_cents: c,
                                    });
                                    let total = d.totales().total;
                                    let cobrado: i64 =
                                        d.pagos.iter().map(|p| p.importe_cents).sum();
                                    if cobrado >= total {
                                        d.estado = Estado::Pagado;
                                    }
                                }
                                self.guardar();
                                self.avisar("Cobro registrado.", false);
                                cerrar = true;
                            }
                            _ => self.avisar("Revisa la fecha y el importe.", true),
                        }
                    }
                    if ui.button("Cancelar").clicked() {
                        cerrar = true;
                    }
                });
            });
        if !cerrar {
            self.cobro = Some((id, fecha, importe));
        }
    }

    fn exportar_informe(&mut self) -> Result<PathBuf, String> {
        let texto = crate::contab::informe_anual(&self.db, self.anyo);
        let dir = export::dir_contabilidad(&self.db.empresa);
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(export::nombre_informe(self.anyo, &self.db.empresa));
        std::fs::write(&ruta, texto).map_err(|e| format!("escribir resumen: {e}"))?;
        Ok(ruta)
    }

    fn exportar_libro(&mut self) -> Result<PathBuf, String> {
        let csv = crate::contab::csv_libro(&self.db, self.anyo);
        let dir = export::dir_contabilidad(&self.db.empresa);
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(export::nombre_libro(self.anyo, &self.db.empresa));
        std::fs::write(&ruta, csv).map_err(|e| format!("escribir CSV: {e}"))?;
        Ok(ruta)
    }

    fn contab_iva(&mut self, ui: &mut egui::Ui) {
        let (rep, sop) = crate::contab::desglose_iva(&self.db, self.anyo, self.trimestre);
        let r = crate::contab::resumen(&self.db, self.anyo, self.trimestre);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("IVA repercutido")
                        .size(15.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                ui.label(RichText::new("De las facturas emitidas").size(11.5).color(tema::SUAVE));
                ui.add_space(6.0);
                tabla_tipos(ui, &rep);
                ui.add_space(6.0);
                fila_total(ui, "Total repercutido", r.iva_repercutido, tema::TEXTO);
            });
            ui.add_space(36.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("IVA soportado")
                        .size(15.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                ui.label(RichText::new("Deducible de los gastos").size(11.5).color(tema::SUAVE));
                ui.add_space(6.0);
                tabla_tipos(ui, &sop);
                ui.add_space(6.0);
                fila_total(ui, "Total soportado", r.iva_soportado, tema::TEXTO);
            });
        });
        ui.add_space(16.0);
        let serie = crate::graficas::serie_iva_trimestral(&self.db, self.anyo);
        crate::graficas::tarjeta(ui, |ui| {
            crate::graficas::cabecera(
                ui,
                &format!("IVA trimestre a trimestre · {}", self.anyo),
                "Azul es lo repercutido; verde, lo deducible de tus gastos.",
            );
            crate::graficas::barras_iva(ui, &serie, 150.0);
        });

        ui.add_space(18.0);
        tema::tarjeta(ui, |ui| {
            ui.set_min_width(380.0);
            ui.label(
                RichText::new("Resultado del 303")
                    .size(11.5)
                    .strong()
                    .color(tema::TENUE),
            );
            ui.label(
                RichText::new(fmt_cents_eur(r.iva_a_ingresar()))
                    .size(24.0)
                    .strong()
                    .color(if r.iva_a_ingresar() >= 0 {
                        tema::ACENTO
                    } else {
                        tema::VERDE
                    }),
            );
            ui.label(
                RichText::new(if r.iva_a_ingresar() >= 0 {
                    "A ingresar"
                } else {
                    "A compensar o devolver"
                })
                .size(12.0)
                .color(tema::SUAVE),
            );
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "No incluye compensaciones de trimestres anteriores ni regímenes especiales.",
            )
            .size(11.0)
            .color(tema::TENUE),
        );
    }

    fn contab_irpf(&mut self, ui: &mut egui::Ui) {
        let a = crate::contab::acumulado_130(&self.db, self.anyo, self.trimestre);
        ui.label(
            RichText::new("Modelo 130 · pago fraccionado")
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new("Acumulado del año hasta el trimestre elegido (Año = año completo).")
                .size(11.5)
                .color(tema::SUAVE),
        );
        ui.add_space(10.0);
        egui::Grid::new("irpf130")
            .num_columns(2)
            .min_col_width(230.0)
            .spacing([24.0, 6.0])
            .show(ui, |ui| {
                for (t, v) in [
                    ("Ingresos acumulados", a.ingresos),
                    ("Gastos acumulados", a.gastos),
                    ("Rendimiento neto", a.ingresos - a.gastos),
                    ("Retenciones soportadas", a.retenciones),
                ] {
                    ui.label(RichText::new(t).color(tema::SUAVE));
                    ui.label(RichText::new(fmt_cents_eur(v)).monospace());
                    ui.end_row();
                }
                ui.label(RichText::new("A ingresar (20 % − retenciones)").strong());
                ui.label(
                    RichText::new(fmt_cents_eur(a.cuota))
                        .size(19.0)
                        .strong()
                        .color(tema::ACENTO),
                );
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Estimación: el 130 real resta además los pagos de los trimestres anteriores.",
            )
            .size(11.0)
            .color(tema::TENUE),
        );
    }

    fn contab_347(&mut self, ui: &mut egui::Ui) {
        let ops = crate::contab::modelo_347(&self.db, self.anyo);
        ui.label(
            RichText::new("Modelo 347 · operaciones con terceros")
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new("Clientes y proveedores con más de 3.005,06 EUR en el año (base + IVA).")
                .size(11.5)
                .color(tema::SUAVE),
        );
        ui.add_space(10.0);
        if ops.is_empty() {
            tema::vacio(
                ui,
                "Sin operaciones que declarar",
                "Ningún cliente ni proveedor supera el límite este año.",
            );
            return;
        }
        egui::Grid::new("m347")
            .num_columns(4)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                for t in ["Tipo", "NIF", "Nombre", "Importe"] {
                    ui.label(RichText::new(t).size(11.5).strong().color(tema::SUAVE));
                }
                ui.end_row();
                for o in &ops {
                    ui.label(o.tipo);
                    ui.label(RichText::new(&o.nif).monospace());
                    ui.label(&o.nombre);
                    ui.label(RichText::new(fmt_cents_eur(o.importe)).monospace());
                    ui.end_row();
                }
            });
    }

    fn contab_calendario(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!("Calendario fiscal {}", self.anyo))
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.label(
            RichText::new("Fechas orientativas: confirma siempre en la sede de la AEAT.")
                .size(11.5)
                .color(tema::SUAVE),
        );
        ui.add_space(10.0);
        let hoy = Local::now().date_naive();
        for (fecha, modelo, detalle) in crate::contab::vencimientos(self.anyo) {
            let pasado = fecha < hoy;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(fecha.format("%d/%m/%Y").to_string())
                        .monospace()
                        .color(tema::SUAVE),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(modelo)
                        .strong()
                        .color(if pasado { tema::TENUE } else { tema::ACENTO }),
                );
                ui.label(RichText::new(detalle).size(11.5).color(tema::SUAVE));
                if pasado {
                    tema::insignia(ui, "pasado", egui::Color32::from_rgb(0xF1, 0xF3, 0xF7), tema::TENUE);
                } else {
                    tema::insignia(ui, "pendiente", tema::AMBAR_CLARO, tema::AMBAR);
                }
            });
            ui.add_space(4.0);
        }
    }

    fn contab_clientes(&mut self, ui: &mut egui::Ui) {
        let filas = crate::contab::rentabilidad_clientes(&self.db, self.anyo, self.trimestre);
        ui.label(
            RichText::new("Rentabilidad por cliente")
                .size(15.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(10.0);
        if filas.is_empty() {
            tema::vacio(
                ui,
                "Sin facturas en el periodo",
                "Cambia de año o de trimestre para ver otros datos.",
            );
            return;
        }
        egui::Grid::new("rent_clientes")
            .num_columns(5)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                for t in ["Cliente", "Facturas", "Facturado", "Cobrado", "Pendiente"] {
                    ui.label(RichText::new(t).size(11.5).strong().color(tema::SUAVE));
                }
                ui.end_row();
                for r in &filas {
                    ui.label(&r.cliente);
                    ui.label(r.n.to_string());
                    ui.label(RichText::new(fmt_cents(r.facturado)).monospace());
                    ui.label(RichText::new(fmt_cents(r.cobrado)).monospace());
                    ui.label(
                        RichText::new(fmt_cents(r.pendiente))
                            .monospace()
                            .color(if r.pendiente > 0 { tema::AMBAR } else { tema::VERDE }),
                    );
                    ui.end_row();
                }
            });
    }

    fn vista_contabilidad(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Contabilidad")
                    .size(24.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            tema::icono_ayuda(
                ui,
                "Contabilidad",
                "Resumen de ingresos, gastos, IVA, IRPF y rentabilidad. Todo se calcula con las fechas de los documentos; no sustituye la contabilidad oficial.",
            );
            ui.add_space(12.0);
            egui::ComboBox::from_id_salt("anyo_contab")
                .selected_text(self.anyo.to_string())
                .show_ui(ui, |ui| {
                    for a in (self.anyo - 4)..=(self.anyo + 1) {
                        ui.selectable_value(&mut self.anyo, a, a.to_string());
                    }
                });
            ui.add_space(10.0);
            for (t, texto) in [(0u32, "Año"), (1, "T1"), (2, "T2"), (3, "T3"), (4, "T4")] {
                if ui.selectable_label(self.trimestre == t, texto).clicked() {
                    self.trimestre = t;
                }
            }
            ui.add_space(10.0);
            if ui.button("Exportar libro (CSV)").clicked() {
                match self.exportar_libro() {
                    Ok(p) => {
                        // Se abre el CSV con su programa y la carpeta que lo
                        // contiene, igual que los PDF.
                        abrir_pdf_y_carpeta(&p);
                        self.avisar(format!("Libro exportado: {}", p.display()), false);
                    }
                    Err(e) => self.avisar(e, true),
                }
            }
            if ui.button("Informe anual").clicked() {
                match self.exportar_informe() {
                    Ok(p) => {
                        abrir_pdf_y_carpeta(&p);
                        self.avisar(format!("Informe guardado: {}", p.display()), false);
                    }
                    Err(e) => self.avisar(e, true),
                }
            }
            ui.add_space(10.0);
            let cerrado = self.db.esta_cerrado(self.anyo);
            let texto = if cerrado {
                format!("Reabrir {}", self.anyo)
            } else {
                format!("Cerrar {}", self.anyo)
            };
            if ui.button(texto).clicked() {
                if cerrado {
                    self.db.cerrados.retain(|a| *a != self.anyo);
                    self.guardar();
                    self.avisar(format!("Ejercicio {} reabierto.", self.anyo), false);
                } else {
                    self.db.cerrados.push(self.anyo);
                    self.db.cerrados.sort_unstable();
                    self.guardar();
                    self.avisar(
                        format!(
                            "Ejercicio {} cerrado: no se emitirán documentos con esa fecha.",
                            self.anyo
                        ),
                        false,
                    );
                }
            }
            if cerrado {
                tema::insignia(ui, "cerrado", tema::AMBAR_CLARO, tema::AMBAR);
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (i, t) in [
                "Resumen",
                "IVA (303)",
                "IRPF (130)",
                "347",
                "Calendario",
                "Clientes",
            ]
            .iter()
            .enumerate()
            {
                if ui.selectable_label(self.pestana_contab == i as u8, *t).clicked() {
                    self.pestana_contab = i as u8;
                }
            }
            tema::icono_ayuda(
                ui,
                "Pestañas de contabilidad",
                "Resumen general, IVA del 303, pago fraccionado del 130, operaciones del 347, calendario y rentabilidad por cliente. Todo es informativo.",
            );
        });
        ui.add_space(10.0);

        match self.pestana_contab {
            1 => {
                self.contab_iva(ui);
                return;
            }
            2 => {
                self.contab_irpf(ui);
                return;
            }
            3 => {
                self.contab_347(ui);
                return;
            }
            4 => {
                self.contab_calendario(ui);
                return;
            }
            5 => {
                self.contab_clientes(ui);
                return;
            }
            _ => {}
        }

        let r = crate::contab::resumen(&self.db, self.anyo, self.trimestre);
        let pendiente = self.db.pendientes_cobro();
        egui::Grid::new("kpi_contab")
            .num_columns(4)
            .spacing([22.0, 4.0])
            .show(ui, |ui| {
                for t in ["Ingresos", "Gastos", "Resultado", "Pendiente de cobro"] {
                    ui.label(RichText::new(t).small().color(egui::Color32::from_gray(110)));
                }
                ui.end_row();
                for v in [r.ingresos, r.gastos, r.resultado(), pendiente] {
                    ui.label(RichText::new(fmt_cents_eur(v)).strong());
                }
                ui.end_row();
                for t in [
                    "IVA repercutido",
                    "IVA soportado",
                    "IVA a ingresar (303)",
                    "IRPF 130 estimado",
                ] {
                    ui.label(RichText::new(t).small().color(egui::Color32::from_gray(110)));
                }
                ui.end_row();
                for v in [
                    r.iva_repercutido,
                    r.iva_soportado,
                    r.iva_a_ingresar(),
                    r.irpf_130(),
                ] {
                    ui.label(RichText::new(fmt_cents_eur(v)).strong());
                }
                ui.end_row();
            });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Estimación informativa. El 303 y el 130 definitivos dependen de tu situación \
                 fiscal: revisa también las casillas de compensación y las retenciones.",
            )
            .small()
            .color(egui::Color32::from_gray(120)),
        );

        let otras = crate::fiscal::bases_por_operacion(&self.db, self.anyo, self.trimestre);
        if !otras.is_empty() {
            ui.add_space(10.0);
            ui.label(
                RichText::new("Operaciones sin IVA")
                    .size(13.5)
                    .strong()
                    .color(tema::TEXTO),
            );
            ui.add_space(2.0);
            egui::Grid::new("otras_operaciones")
                .num_columns(2)
                .striped(true)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for (op, base) in &otras {
                        ui.label(op.etiqueta());
                        ui.label(RichText::new(fmt_cents(*base)).monospace());
                        ui.end_row();
                    }
                });
            ui.label(
                RichText::new(
                    "Van a las casillas de exportaciones y operaciones no sujetas del 303; no generan cuota.",
                )
                .size(11.0)
                .color(tema::TENUE),
            );
        }

        // --- Evolución del año -------------------------------------------------
        let serie = crate::graficas::serie_mensual(&self.db, self.anyo);
        let anual = crate::contab::resumen(&self.db, self.anyo, 0);
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Evolución de {}", self.anyo))
                    .size(16.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!(
                    "Resultado del año: {}",
                    fmt_cents_eur(anual.resultado())
                ))
                .size(11.5)
                .strong()
                .color(if anual.resultado() >= 0 {
                    tema::VERDE
                } else {
                    tema::ROJO
                }),
            );
        });
        ui.label(
            RichText::new(
                "Base imponible sin IVA. Se cuenta por la fecha de la factura y del gasto, igual que el 303 y el 130.",
            )
            .size(10.5)
            .color(tema::TENUE),
        );
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            crate::graficas::tarjeta(&mut cols[0], |ui| {
                ui.set_min_width(280.0);
                crate::graficas::cabecera(
                    ui,
                    "Ingresos y gastos por mes",
                    "Cada barra es un mes del año",
                );
                crate::graficas::barras_mensuales(ui, &serie, 180.0);
            });
            crate::graficas::tarjeta(&mut cols[1], |ui| {
                ui.set_min_width(280.0);
                crate::graficas::cabecera(
                    ui,
                    "Resultado acumulado",
                    "Lo que llevas ganado o perdido a cierre de cada mes",
                );
                crate::graficas::area_acumulada(ui, &serie, 180.0);
            });
        });

        let iva = crate::graficas::serie_iva_trimestral(&self.db, self.anyo);
        let clientes = crate::graficas::top_clientes(&self.db, self.anyo, 0, 5);
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            crate::graficas::tarjeta(&mut cols[0], |ui| {
                ui.set_min_width(280.0);
                crate::graficas::cabecera(
                    ui,
                    "IVA por trimestre",
                    "Repercutido en facturas y soportado en gastos",
                );
                crate::graficas::barras_iva(ui, &iva, 160.0);
            });
            crate::graficas::tarjeta(&mut cols[1], |ui| {
                ui.set_min_width(280.0);
                crate::graficas::cabecera(ui, "Clientes que más facturan", "Año completo");
                if clientes.is_empty() {
                    ui.label(
                        RichText::new("Aún no hay facturas emitidas este año.")
                            .size(11.5)
                            .color(tema::TENUE),
                    );
                } else {
                    crate::graficas::barras_horizontales(ui, &clientes, tema::ACENTO);
                }
            });
        });

        let categorias = crate::contab::gastos_por_categoria(&self.db, self.anyo, self.trimestre);
        if !categorias.is_empty() {
            ui.add_space(10.0);
            crate::graficas::tarjeta(ui, |ui| {
                crate::graficas::cabecera(
                    ui,
                    "Gastos por categoría",
                    "Del periodo elegido arriba (Año o trimestre)",
                );
                let filas: Vec<(String, i64)> = categorias
                    .iter()
                    .take(8)
                    .map(|(c, base, _)| (c.clone(), *base))
                    .collect();
                crate::graficas::barras_horizontales(ui, &filas, tema::AMBAR);
            });
        }

        ui.add_space(14.0);

        ui.columns(2, |cols| {
            cols[0].horizontal(|ui| {
                ui.label(RichText::new("Gastos del periodo").strong());
                tema::icono_ayuda(
                    ui,
                    "Gastos del periodo",
                    "Facturas de proveedores y gastos afectos a la actividad. El IVA soportado se deduce en el 303 y la base resta en el 130.",
                );
                if ui.button("+ Nuevo gasto").clicked() {
                    let mut g = Gasto::default();
                    g.id = self.db.id();
                    self.ficha_gasto = Some(GastoBuf::nuevo(g));
                }
            });
            let mut gastos: Vec<Gasto> = self
                .db
                .gastos
                .iter()
                .filter(|g| crate::contab::en_rango(g.fecha, self.anyo, self.trimestre))
                .cloned()
                .collect();
            gastos.sort_by(|a, b| b.fecha.cmp(&a.fecha));
            egui::ScrollArea::vertical()
                .id_salt("lista_gastos")
                .show(&mut cols[0], |ui| {
                    if gastos.is_empty() {
                        ui.label(
                            RichText::new("Sin gastos en este periodo.")
                                .color(egui::Color32::from_gray(120)),
                        );
                    }
                    for g in &gastos {
                        let actual = self
                            .ficha_gasto
                            .as_ref()
                            .map(|f| f.gasto.id == g.id)
                            .unwrap_or(false);
                        let etiqueta = format!(
                            "{} · {} · {}",
                            g.fecha.format("%d/%m/%Y"),
                            if g.proveedor.is_empty() { &g.concepto } else { &g.proveedor },
                            fmt_cents(g.total())
                        );
                        if ui.selectable_label(actual, etiqueta).clicked() {
                            self.ficha_gasto = Some(GastoBuf::desde(g.clone()));
                        }
                    }
                });

            let mut guardar_gasto: Option<Gasto> = None;
            let mut borrar_gasto: Option<u64> = None;
            let mut duplicado: Option<Gasto> = None;
            let mut error: Option<String> = None;

            // Proveedores ya usados y categorías habituales: se ofrecen para no
            // volver a teclearlos.
            let mut proveedores: Vec<(String, String, String, i32)> = Vec::new();
            for g in &self.db.gastos {
                if g.proveedor.trim().is_empty() {
                    continue;
                }
                if proveedores.iter().any(|p| p.0 == g.proveedor) {
                    continue;
                }
                proveedores.push((
                    g.proveedor.clone(),
                    g.nif.clone(),
                    g.categoria.clone(),
                    g.iva_pb,
                ));
            }
            proveedores.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            let mut categorias_gasto: Vec<String> = vec![
                "Alquiler".into(),
                "Suministros".into(),
                "Teléfono e internet".into(),
                "Material".into(),
                "Servicios profesionales".into(),
                "Seguros".into(),
                "Transporte".into(),
                "Comidas".into(),
                "Cuota de autónomos".into(),
                "Gestoría".into(),
                "Formación".into(),
                "Otros".into(),
            ];
            for (_, _, categoria, _) in &proveedores {
                let c = categoria.trim();
                if !c.is_empty() && !categorias_gasto.iter().any(|x| x == c) {
                    categorias_gasto.push(c.to_string());
                }
            }
            if let Some(f) = self.ficha_gasto.as_mut() {
                let mut guardar = false;
                let mut borrar = false;
                let mut duplicar = false;
                egui::ScrollArea::vertical()
                    .id_salt("ficha_gasto")
                    .show(&mut cols[1], |ui| {
                        ui.heading(if f.gasto.concepto.is_empty() {
                            "Nuevo gasto"
                        } else {
                            "Ficha de gasto"
                        });
                        egui::Grid::new("form_gasto")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                campo_h(
                                    ui,
                                    "Fecha (dd/mm/aaaa)",
                                    &mut f.fecha,
                                    "Fecha de la factura del proveedor. Decide en qué trimestre se deduce el IVA y en qué año cuenta el gasto.",
                                );
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Proveedor",
                                    "Nombre o razón social de quien te emite la factura. Puedes reutilizar uno de los ya guardados.",
                                );
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [250.0, 30.0],
                                        egui::TextEdit::singleline(&mut f.gasto.proveedor),
                                    );
                                    if !proveedores.is_empty() {
                                        egui::ComboBox::from_id_salt("prov_usados")
                                            .selected_text("Usados…")
                                            .width(130.0)
                                            .show_ui(ui, |ui| {
                                                for (nombre, nif, cat, iva) in &proveedores {
                                                    if ui
                                                        .selectable_label(false, nombre.clone())
                                                        .clicked()
                                                    {
                                                        f.gasto.proveedor = nombre.clone();
                                                        f.gasto.nif = nif.clone();
                                                        if !cat.is_empty() {
                                                            f.gasto.categoria = cat.clone();
                                                        }
                                                        f.iva = fmt_pb(*iva);
                                                    }
                                                }
                                            });
                                    }
                                });
                                ui.end_row();
                                campo_h(
                                    ui,
                                    "NIF",
                                    &mut f.gasto.nif,
                                    "NIF del proveedor. Se comprueba la letra de control: sin un NIF válido el gasto no se puede deducir ni entra en el 347.",
                                );
                                if !f.gasto.nif.trim().is_empty()
                                    && !crate::model::nif_valido(&f.gasto.nif)
                                {
                                    ui.label("");
                                    ui.label(
                                        RichText::new(
                                            "El NIF del proveedor no cuadra con su letra de control: sin NIF válido no entra en el 347 ni como gasto deducible.",
                                        )
                                        .size(10.5)
                                        .color(tema::AMBAR),
                                    );
                                    ui.end_row();
                                }
                                campo_h(
                                    ui,
                                    "Concepto",
                                    &mut f.gasto.concepto,
                                    "Qué compraste o qué servicio recibiste. Este texto es el que verás en el listado de gastos.",
                                );
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Categoría",
                                    "Sirve para agrupar los gastos en el resumen y el informe anual. No cambia la deducción.",
                                );
                                ui.horizontal(|ui| {
                                    ui.add_sized(
                                        [250.0, 30.0],
                                        egui::TextEdit::singleline(&mut f.gasto.categoria),
                                    );
                                    egui::ComboBox::from_id_salt("cat_usadas")
                                        .selected_text("Elegir…")
                                        .width(130.0)
                                        .show_ui(ui, |ui| {
                                            for c in &categorias_gasto {
                                                if ui.selectable_label(false, c.clone()).clicked() {
                                                    f.gasto.categoria = c.clone();
                                                }
                                            }
                                        });
                                });
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Base imponible",
                                    "Importe sin IVA ni retención. Es la base que resta en el 130 y sobre la que se calcula la cuota de IVA.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.base)
                                        .desired_width(120.0)
                                        .hint_text("0,00"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "IVA %",
                                    "IVA soportado de la factura. Se deduce en el 303 si el gasto está afecto a la actividad.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.iva)
                                        .desired_width(120.0)
                                        .hint_text("21"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Retención IRPF %",
                                    "Retención que te practica el proveedor (por ejemplo, un profesional). Se descuenta del total del gasto.",
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.irpf)
                                        .desired_width(120.0)
                                        .hint_text("0"),
                                );
                                ui.end_row();
                                tema::etiqueta_ayuda(
                                    ui,
                                    "Pagado",
                                    "Márcalo cuando ya hayas pagado esta factura. No cambia el IVA ni el gasto: sólo el estado de tesorería.",
                                );
                                ui.checkbox(&mut f.gasto.pagado, "Ya está pagado");
                                ui.end_row();
                            });

                        // Vista previa en vivo: cuota, retención y total sin guardar.
                        let mut previo = f.gasto.clone();
                        previo.base_cents = parse_cents(&f.base).unwrap_or(0);
                        previo.iva_pb = parse_pb(&f.iva).unwrap_or(0);
                        previo.irpf_pb = parse_pb(&f.irpf).unwrap_or(0);
                        ui.add_space(8.0);
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgb(0xF7, 0xF9, 0xFC))
                            .stroke(egui::Stroke::new(1.0, tema::BORDE))
                            .rounding(egui::Rounding::same(8.0))
                            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_min_width(300.0);
                                egui::Grid::new("previo_gasto")
                                    .num_columns(2)
                                    .spacing([18.0, 5.0])
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new("Cuota de IVA")
                                                .size(11.5)
                                                .color(tema::SUAVE),
                                        );
                                        ui.label(
                                            RichText::new(fmt_cents_eur(previo.cuota_iva()))
                                                .monospace(),
                                        );
                                        ui.end_row();
                                        if previo.irpf_pb > 0 {
                                            ui.label(
                                                RichText::new("Retención IRPF")
                                                    .size(11.5)
                                                    .color(tema::SUAVE),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "-{}",
                                                    fmt_cents_eur(previo.retencion())
                                                ))
                                                .monospace(),
                                            );
                                            ui.end_row();
                                        }
                                        ui.label(
                                            RichText::new("Total del gasto")
                                                .size(12.0)
                                                .strong()
                                                .color(tema::TEXTO),
                                        );
                                        ui.label(
                                            RichText::new(fmt_cents_eur(previo.total()))
                                                .size(14.0)
                                                .strong()
                                                .color(tema::ACENTO),
                                        );
                                        ui.end_row();
                                    });
                                ui.label(
                                    RichText::new(
                                        "La cuota de IVA se deduce en el 303; la base resta en el 130.",
                                    )
                                    .size(10.0)
                                    .color(tema::TENUE),
                                );
                            });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let valido = parse_fecha(&f.fecha).is_some()
                                && parse_cents(&f.base).is_some()
                                && parse_pb(&f.iva).is_some()
                                && parse_pb(&f.irpf).is_some();
                            if ui
                                .add_enabled(valido, egui::Button::new("Guardar"))
                                .clicked()
                            {
                                guardar = true;
                            }
                            if ui.button("Duplicar").clicked() {
                                duplicar = true;
                            }
                            if ui.button("Borrar").clicked() {
                                borrar = true;
                            }
                        });
                    });
                if guardar {
                    match (
                        parse_fecha(&f.fecha),
                        parse_cents(&f.base),
                        parse_pb(&f.iva),
                        parse_pb(&f.irpf),
                    ) {
                        (Some(fe), Some(b), Some(i), Some(r)) => {
                            f.gasto.fecha = fe;
                            f.gasto.base_cents = b;
                            f.gasto.iva_pb = i;
                            f.gasto.irpf_pb = r;
                            guardar_gasto = Some(f.gasto.clone());
                        }
                        _ => error = Some("Revisa la fecha y los importes del gasto.".to_string()),
                    }
                }
                if borrar {
                    borrar_gasto = Some(f.gasto.id);
                }
                if duplicar {
                    duplicado = Some(f.gasto.clone());
                }
            }
            if let Some(mut g) = duplicado {
                g.id = self.db.id();
                self.ficha_gasto = Some(GastoBuf::desde(g));
            }
            if let Some(g) = guardar_gasto {
                if let Some(e) = self.db.gastos.iter_mut().find(|x| x.id == g.id) {
                    *e = g;
                } else {
                    self.db.gastos.push(g);
                }
                self.guardar();
                self.avisar("Gasto guardado.", false);
                self.ficha_gasto = None;
            }
            if let Some(id) = borrar_gasto {
                self.db.gastos.retain(|g| g.id != id);
                self.guardar();
                self.ficha_gasto = None;
            }
            if let Some(msg) = error {
                self.avisar(msg, true);
            }
        });
    }

    /// Tarjeta del logo: elegir imagen, convertirla a JPEG y quitarla.
    fn ajustes_logo(&mut self, ui: &mut egui::Ui) {
        let mut elegir = false;
        let mut quitar = false;
        let mut copiado = false;
        let nombre = if self.db.empresa.logo.trim().is_empty() {
            "Sin logo".to_string()
        } else {
            std::path::Path::new(&self.db.empresa.logo)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.db.empresa.logo.clone())
        };
        let hay_logo = !self.db.empresa.logo.trim().is_empty();
        let mut voz = self.db.voz;
        egui::Frame::none()
            .fill(tema::SUPERFICIE)
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Logo de la empresa").strong().color(tema::TEXTO));
                tema::icono_ayuda(
                    ui,
                    "Logo de la empresa",
                    "Se imprime arriba a la izquierda en facturas, presupuestos y proformas. Se convierte a JPEG sobre fondo blanco para que el PDF sea ligero.",
                );
                    ui.label(RichText::new(nombre).size(11.5).color(tema::SUAVE));
                    if ui.button("Elegir imagen…").clicked() {
                        elegir = true;
                    }
                    if hay_logo && ui.button("Quitar").clicked() {
                        quitar = true;
                    }
                    ui.separator();
                    tema::checkbox_ayuda(
                        ui,
                        &mut voz,
                        "Voz en el tutorial",
                        "El asistente lee cada paso en voz alta con una voz femenina del sistema. Puedes desactivarlo cuando quieras.",
                    );
                });
                ui.label(
                    RichText::new(
                        "Se imprime arriba a la izquierda en facturas, presupuestos y proformas. Cualquier imagen vale: se convierte a JPEG sobre fondo blanco.",
                    )
                    .size(10.5)
                    .color(tema::TENUE),
                );
                ui.add_space(4.0);
                ui.collapsing(
                    "¿No tienes logo? Créalo con IA o gratis en dos minutos",
                    |ui| {
                        ui.label(
                            RichText::new(
                                "Pide un logo con este prompt y cambia sólo lo que va entre corchetes:",
                            )
                            .size(10.5)
                            .color(tema::SUAVE),
                        );
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgb(0xF7, 0xF9, 0xFC))
                            .stroke(egui::Stroke::new(1.0, tema::BORDE))
                            .rounding(egui::Rounding::same(8.0))
                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(crate::recursos::PROMPT_LOGO)
                                        .size(10.5)
                                        .color(tema::TEXTO),
                                );
                                ui.add_space(6.0);
                                if ui
                                    .button("Copiar el prompt")
                                    .on_hover_text("Lo copia al portapapeles para pegarlo en la IA.")
                                    .clicked()
                                {
                                    copiado = true;
                                }
                            });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Dónde generarlo")
                                .size(10.5)
                                .strong()
                                .color(tema::TENUE),
                        );
                        ui.horizontal_wrapped(|ui| {
                            for r in &crate::recursos::LOGO {
                                if ui
                                    .button(r.nombre)
                                    .on_hover_text(r.nota)
                                    .clicked()
                                {
                                    abrir_url(r.url);
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "Consejos: cuadrado, fondo blanco o transparente, 800–1000 px, sin texto diminuto. Luego pulsa «Elegir imagen…»: la app lo convierte a JPEG y lo imprime arriba a la izquierda.",
                            )
                            .size(10.0)
                            .color(tema::TENUE),
                        );
                    },
                );
            });
        if copiado {
            ui.output_mut(|o| o.copied_text = crate::recursos::PROMPT_LOGO.to_string());
            self.avisar("Prompt copiado: pégalo en Nano Banana, ChatGPT o Canva.", false);
        }
        if elegir {
            if let Some(ruta) = crate::logo::elegir() {
                match crate::logo::normalizar(&ruta) {
                    Ok(destino) => {
                        self.db.empresa.logo = destino;
                        self.guardar();
                        self.avisar("Logo guardado: saldrá en los próximos PDF.", false);
                    }
                    Err(e) => self.avisar(format!("No se pudo usar esa imagen: {e}"), true),
                }
            }
        }
        if quitar {
            self.db.empresa.logo.clear();
            self.guardar();
            self.avisar("Logo quitado de los documentos.", false);
        }
        if voz != self.db.voz {
            self.db.voz = voz;
            if !voz {
                self.voz.parar();
            }
            self.guardar();
        }
    }

    fn vista_ajustes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                RichText::new("Ajustes")
                    .size(24.0)
                    .strong()
                    .color(tema::TEXTO),
            );
            tema::icono_ayuda(
                ui,
                "Ajustes",
                "Estos datos son la base de la facturación y de los modelos. Se guardan sólo en tu equipo: la app no los envía a Hacienda.",
            );
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Estos datos deciden qué modelos te tocan y salen impresos en cada factura. Si cambias de autónomo a sociedad, o al contrario, cámbialo aquí: la lista de modelos, el IVA y los plazos se recalculan solos. No se envía nada a Hacienda; es tu copia local.",
            )
            .size(11.0)
            .color(tema::SUAVE),
        );
        if ui
            .button("Cómo funciona Verifactu")
            .on_hover_text("Qué se encadena, qué hace el QR y qué no hace la app.")
            .clicked()
        {
            self.ayuda_verifactu = true;
        }
        ui.add_space(6.0);
        self.ajustes_logo(ui);
        ui.add_space(6.0);
        if ui
            .button("Repetir el asistente de puesta en marcha")
            .on_hover_text("Vuelve a abrir las preguntas para revisar régimen, IVA y modelos.")
            .clicked()
        {
            self.asistente = Some(Asistente::desde(&self.db.empresa));
            self.paso_voz = usize::MAX;
        }
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let e = &mut self.db.empresa;
            egui::Grid::new("form_empresa").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                campo_h(
                    ui,
                    "Nombre o razón social",
                    &mut e.nombre,
                    "Como figura en tu alta censal: es el declarante de todos los modelos.",
                );
                campo_h(
                    ui,
                    "NIF",
                    &mut e.nif,
                    "Con la letra de control correcta. La vista de modelos lo valida y avisa si no cuadra.",
                );
                tema::etiqueta_ayuda(
                    ui,
                    "Tipo de titular",
                    "Autónomo o persona física si el NIF es DNI/NIE; sociedad si es CIF. Cambia los modelos y cómo se rellena FacturaE.",
                );
                tema::checkbox_ayuda(
                    ui,
                    &mut e.persona_fisica,
                    "Autónomo / persona física",
                    "Márcalo si tu NIF es DNI o NIE. Si es una sociedad con CIF, déjalo desmarcado para que FacturaE use LegalEntity.",
                );
                ui.end_row();
                if e.persona_fisica {
                    campo_h(
                        ui,
                        "Apellidos",
                        &mut e.apellidos,
                        "Sólo para persona física: FacturaE los exige separados del nombre.",
                    );
                }
                campo_h(
                    ui,
                    "Dirección",
                    &mut e.direccion,
                    "Calle, número, piso y puerta, como en el censo. La Sede la pide al presentar.",
                );
                campo_h(ui, "Código postal", &mut e.cp, "Cinco dígitos.");
                campo_h(ui, "Ciudad", &mut e.ciudad, "Municipio del domicilio fiscal.");
                campo_h(ui, "Provincia", &mut e.provincia, "Provincia del domicilio fiscal.");
                campo_h(
                    ui,
                    "Email",
                    &mut e.email,
                    "Para avisos de la AEAT y para enviar justificantes y facturas.",
                );
                campo_h(
                    ui,
                    "Teléfono",
                    &mut e.telefono,
                    "Se imprime en la factura; opcional pero recomendable.",
                );
                campo_h(
                    ui,
                    "IBAN",
                    &mut e.iban,
                    "Para domiciliar el pago o la devolución y para el recibo SEPA.",
                );
                campo_h(
                    ui,
                    "Serie de factura",
                    &mut e.serie,
                    "Prefijo de la numeración. Debe ser única por ejercicio.",
                );

                let mut dias_venc = e.dias_vencimiento.to_string();
                tema::etiqueta_ayuda(
                    ui,
                    "Días de vencimiento",
                    "Días naturales que se suman a la fecha de la factura para proponer el vencimiento. Puedes cambiarlo en cada documento.",
                );
                if ui
                    .add(egui::TextEdit::singleline(&mut dias_venc).desired_width(90.0))
                    .changed()
                {
                    if let Ok(v) = dias_venc.trim().parse::<i32>() {
                        e.dias_vencimiento = v.clamp(0, 365);
                    }
                }
                ui.end_row();
                campo_h(
                    ui,
                    "Forma de pago por defecto",
                    &mut e.forma_pago_defecto,
                    "Se propone en cada factura nueva; luego se puede cambiar en cada una.",
                );

                tema::etiqueta_ayuda(
                    ui,
                    "Actividad",
                    "Decide el IVA y la retención que se proponen por defecto y qué modelos aparecen en la lista. Si no encaja, elige «Otra» y ajústalo a mano.",
                );
                let antes = e.actividad;
                egui::ComboBox::from_id_salt("combo_actividad")
                    .selected_text(e.actividad.etiqueta())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for a in Actividad::TODAS {
                            ui.selectable_value(&mut e.actividad, a, a.etiqueta());
                        }
                    });
                ui.end_row();
                if e.actividad != antes {
                    // Al cambiar de actividad se proponen sus tipos típicos.
                    e.iva_defecto = e.actividad.iva_pct() * 100;
                    e.irpf_defecto = e.actividad.retencion_pct() * 100;
                }

                tema::etiqueta_ayuda(
                    ui,
                    "Régimen del IRPF",
                    "Simplificada es lo habitual hasta 600.000 € de facturación. Módulos y normal tienen requisitos y límites propios.",
                );
                egui::ComboBox::from_id_salt("combo_regimen")
                    .selected_text(e.regimen_irpf.etiqueta())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for r in RegimenIrpf::TODOS {
                            ui.selectable_value(&mut e.regimen_irpf, r, r.etiqueta());
                        }
                    });
                ui.end_row();
                tema::etiqueta_ayuda(
                    ui,
                    "Regímenes especiales de IVA",
                    "Recargo de equivalencia y SII cambian qué modelos presentas y con qué periodicidad. Márcalos sólo si te corresponden.",
                );
                ui.vertical(|ui| {
                    tema::checkbox_ayuda(
                        ui,
                        &mut e.recargo_equivalencia,
                        "Recargo de equivalencia (comercio minorista)",
                        "Si eres minorista, tus proveedores te repercuten IVA + recargo y tú no deduces el IVA de esas compras. Tampoco presentas 303 ni 390.",
                    );
                    tema::checkbox_ayuda(
                        ui,
                        &mut e.sii,
                        "SII: libros en tiempo real, 303 mensual y sin 390",
                        "El Suministro Inmediato de Información obliga a llevar los libros en tiempo real y cambia el 303 a mensual. Sólo si te corresponde.",
                    );
                    ui.label(
                        RichText::new(
                            "Con recargo de equivalencia no se presentan 303 ni 390; tus proveedores ingresan el IVA y el recargo.",
                        )
                        .size(10.5)
                        .color(tema::TENUE),
                    );
                });
                ui.end_row();

                let mut iva = fmt_pb(e.iva_defecto);
                let mut irpf = fmt_pb(e.irpf_defecto);
                tema::etiqueta_ayuda(
                    ui,
                    "IVA por defecto %",
                    "Tipo que se propone en las líneas nuevas. Puedes cambiarlo en cada factura sin tocar este ajuste.",
                );
                if ui
                    .add(egui::TextEdit::singleline(&mut iva).desired_width(90.0))
                    .changed()
                {
                    if let Some(v) = parse_pb(&iva) {
                        e.iva_defecto = v;
                    }
                }
                ui.end_row();
                tema::etiqueta_ayuda(
                    ui,
                    "IRPF por defecto %",
                    "Retención que se propone al crear una factura. Para particulares suele ser 0 %; para empresas profesionales, 15 % o 7 % los primeros años.",
                );
                if ui
                    .add(egui::TextEdit::singleline(&mut irpf).desired_width(90.0))
                    .changed()
                {
                    if let Some(v) = parse_pb(&irpf) {
                        e.irpf_defecto = v;
                    }
                }
                ui.end_row();
                campo_h(
                    ui,
                    "Condiciones de pago",
                    &mut e.condiciones,
                    "Texto legal que se imprime al pie de la factura.",
                );
                tema::etiqueta_ayuda(
                    ui,
                    "Verifactu",
                    "Genera un registro local encadenado y un QR de cotejo. No remite nada a la AEAT: eso exige un sistema certificado y un certificado cualificado.",
                );
                ui.vertical(|ui| {
                    tema::checkbox_ayuda(
                        ui,
                        &mut e.verifactu_activo,
                        "Generar registro encadenado y QR",
                        "Activa la huella, el encadenamiento y el QR de cotejo en las facturas nuevas. No remite nada a la AEAT ni convierte la app en un SIF certificado.",
                    );
                    ui.label(
                        RichText::new(
                            "Registro local y QR; no envía nada a la AEAT. Para remitir haría falta un sistema certificado.",
                        )
                        .size(10.0)
                        .color(tema::TENUE),
                    );
                });
                ui.end_row();
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Guardar ajustes").clicked() {
                    self.guardar();
                    self.avisar("Ajustes guardados.", false);
                }
                if ui.button("Abrir carpeta de datos").clicked() {
                    if let Some(dir) = self.db.ruta.parent() {
                        let _ = std::process::Command::new("explorer").arg(dir).spawn();
                    }
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Copias de seguridad")
                        .size(15.0)
                        .strong()
                        .color(tema::TEXTO),
                );
                tema::icono_ayuda(
                    ui,
                    "Copias de seguridad",
                    "Un fichero JSON con todos tus datos. Restaurar sustituye los datos actuales, pero antes se guarda una copia automática del estado que había.",
                );
            });
            ui.label(
                RichText::new("Un fichero completo de tus datos, con fecha, en la carpeta «copias».")
                    .size(11.5)
                    .color(tema::SUAVE),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Crear copia ahora").clicked() {
                    match self.crear_copia() {
                        Ok(p) => self.avisar(format!("Copia creada: {}", p.display()), false),
                        Err(e) => self.avisar(e, true),
                    }
                }
                if ui.button("Abrir carpeta de copias").clicked() {
                    let dir = self.db.dir_copias();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            });

            let copias = self.copias_disponibles();
            ui.add_space(6.0);
            if copias.is_empty() {
                ui.label(
                    RichText::new("Todavía no hay copias.")
                        .size(11.5)
                        .color(tema::TENUE),
                );
            } else {
                egui::Grid::new("lista_copias")
                    .num_columns(2)
                    .striped(true)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for (nombre, ruta) in copias.iter().take(8) {
                            ui.label(RichText::new(nombre).monospace().size(11.5));
                            if ui.small_button("Restaurar").clicked() {
                                self.restaurar = Some(ruta.clone());
                            }
                            ui.end_row();
                        }
                    });
            }
            ui.label(
                RichText::new(
                    "Restaurar sustituye los datos actuales; antes se guarda una copia automática.",
                )
                .size(11.0)
                .color(tema::TENUE),
            );

            ui.add_space(16.0);
            ui.separator();
            ui.label(RichText::new("Rendimiento").strong());
            ui.label(
                RichText::new(self.perfil.resumen())
                    .small()
                    .color(egui::Color32::from_gray(95)),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Perfil:").small());
                for nivel in [
                    crate::perf::Nivel::Bajo,
                    crate::perf::Nivel::Medio,
                    crate::perf::Nivel::Alto,
                ] {
                    if ui
                        .selectable_label(self.perfil.nivel == nivel, nivel.etiqueta())
                        .clicked()
                    {
                        self.perfil.nivel = nivel;
                        self.perfil.motivo = "elegido a mano en Ajustes".to_string();
                        let ctx = ui.ctx().clone();
                        let mut estilo = (*ctx.style()).clone();
                        crate::perf::aplicar(&mut estilo, &ctx, &self.perfil);
                        ctx.set_style(estilo);
                    }
                }
            });
            ui.label(
                RichText::new(
                    "Se elige solo al arrancar según la tarjeta gráfica y los núcleos. \
                     En equipos modestos apaga animaciones y sombras para que la ventana vaya fluida.",
                )
                .small()
                .color(egui::Color32::from_gray(120)),
            );

            ui.add_space(16.0);
            ui.separator();
            ui.label(RichText::new("Cumplimiento").strong());
            let problemas = verifactu::verificar_cadena(&self.db.documentos);
            if problemas.is_empty() {
                ui.label(
                    RichText::new("Cadena de registros íntegra.").color(egui::Color32::from_rgb(
                        20, 90, 60,
                    )),
                );
            } else {
                for p in &problemas {
                    ui.label(
                        RichText::new(p).small().color(egui::Color32::from_rgb(150, 30, 30)),
                    );
                }
            }
            ui.label(
                RichText::new(format!(
                    "Fichero: {}\nEste programa no remite registros a la AEAT: genera el registro, \
                     la huella encadenada, el QR y el FacturaE. La remisión exige certificado \
                     electrónico y la declaración responsable del productor.",
                    self.db.ruta.display()
                ))
                .small()
                .color(egui::Color32::from_gray(110)),
            );
        });
    }

    /// Crea y emite una R1 por diferencias. La original no se anula: se marca
    /// como rectificada y se enlaza con la nueva factura.
    /// Crea una R1 por diferencias a partir de una factura emitida. La original
    /// no se toca salvo para dejar la marca `rectificada_por`; la R1 se emite
    /// con su propio número, registro y fecha.
    fn crear_rectificativa(&mut self, buf: &RectificarBuf) -> Result<u64, String> {
        let original = self
            .db
            .documento(buf.original_id)
            .cloned()
            .ok_or("La factura original ya no existe.")?;
        if original.estado == Estado::Anulado {
            return Err("Una factura anulada no se rectifica: emite una nueva.".into());
        }
        if original.rectifica.is_some() {
            return Err("Una rectificativa no se rectifica: crea otra corrección sobre la original.".into());
        }
        let base = parse_cents(buf.base.trim())
            .ok_or("La diferencia de base no es un importe válido.")?;
        if base == 0 {
            return Err("La diferencia de base no puede ser cero.".into());
        }
        let iva_tecleado = parse_pb(buf.iva.trim()).ok_or("El IVA debe ser un número (0–100).")?;
        // En exportación, intracomunitaria o exenta el IVA no se devenga: la
        // línea debe quedar a 0 aunque el campo tenga un valor viejo.
        let iva = if original.operacion.lleva_iva() {
            iva_tecleado
        } else {
            0
        };
        let motivo = buf.motivo.trim().to_string();
        if motivo.is_empty() {
            return Err("Escribe el motivo de la rectificación.".into());
        }

        let hoy = chrono::Local::now().date_naive();
        let serie = format!("R{}", hoy.format("%Y"));
        let mut d = Documento::nuevo(
            TipoDoc::Factura,
            original.cliente.clone(),
            original.irpf_pb,
            serie,
        );
        d.id = self.db.id();
        d.fecha = hoy;
        // Conserva el plazo de pago de la original (30 días por defecto, pero
        // puede ser 60 o el que se haya pactado).
        let plazo = (original.vencimiento - original.fecha)
            .num_days()
            .clamp(0, 365);
        d.vencimiento = hoy + chrono::Duration::days(plazo);
        d.forma_pago = original.forma_pago.clone();
        d.operacion = original.operacion;
        d.recargo_pb = original.recargo_pb;
        d.rectifica = Some(crate::model::Rectificacion {
            documento_id: original.id,
            num_serie: original.num_serie_aeat(),
            etiqueta: original.etiqueta_numero(),
            fecha: original.fecha,
            tipo: "I".to_string(),
        });
        d.motivo_rectificacion = motivo.clone();
        d.lineas = vec![Linea {
            descripcion: format!(
                "Rectificación de la factura {}: {}",
                original.etiqueta_numero(),
                motivo
            ),
            cantidad_milli: 1000,
            precio_cents: base,
            descuento_pb: 0,
            iva_pb: iva,
        }];
        d.notas = format!(
            "Rectificativa por diferencias de la factura {} del {}.",
            original.etiqueta_numero(),
            original.fecha.format("%d/%m/%Y")
        );
        let id = d.id;
        self.db.documentos.push(d);

        match self.emitir(id) {
            Ok(()) => {
                if let Some(o) = self.db.documento_mut(original.id) {
                    o.rectificada_por = Some(id);
                }
                self.guardar();
                Ok(id)
            }
            Err(e) => {
                // Si no llegó a numerarse, no dejamos un borrador huérfano.
                let sin_numero = self
                    .db
                    .documento(id)
                    .map(|d| d.numero == 0)
                    .unwrap_or(false);
                if sin_numero {
                    self.db.documentos.retain(|d| d.id != id);
                    self.guardar();
                }
                Err(e)
            }
        }
    }

    /// Ventana de corrección: pide la diferencia, valida y previsualiza la R1
    /// antes de emitirla. La ayuda contextual explica cuándo usar R1 o anular.
    fn ventana_rectificar(&mut self, ctx: &egui::Context) {
        let Some(buf) = self.rectificar.clone() else {
            return;
        };
        let Some(original) = self.db.documento(buf.original_id).cloned() else {
            self.rectificar = None;
            return;
        };
        let mut base = buf.base.clone();
        let mut iva = buf.iva.clone();
        let mut motivo = buf.motivo.clone();
        let mut cerrar = false;
        let mut confirmar = false;
        let mut abrir_ayuda = false;

        egui::Window::new("Rectificar factura (R1)")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Original: {} · {}",
                        original.etiqueta_numero(),
                        original.cliente.nombre_completo()
                    ))
                    .strong(),
                );
                ui.label(
                    RichText::new(
                        "Se emite una factura rectificativa R1 por la diferencia. La original no se anula ni se borra: queda marcada como rectificada.",
                    )
                    .size(11.0)
                    .color(tema::SUAVE),
                );
                if ui
                    .link("¿Cuándo usar una R1 y cuándo anular?")
                    .clicked()
                {
                    abrir_ayuda = true;
                }
                ui.add_space(8.0);
                egui::Grid::new("rectificar_campos")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        tema::etiqueta_ayuda(
                            ui,
                            "Diferencia de base (EUR)",
                            "Diferencia con signo: negativa para bajar la base de la original y positiva para subirla. No escribas el total de la nueva factura.",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut base)
                                .hint_text("Ej.: -100,00 para bajar 100 €"),
                        );
                        ui.end_row();
                        tema::etiqueta_ayuda(
                            ui,
                            "IVA de la diferencia %",
                            "Tipo de IVA que se aplica a la diferencia. Si la original mezcla varios tipos, crea una R1 por cada uno.",
                        );
                        ui.add(egui::TextEdit::singleline(&mut iva).desired_width(90.0));
                        ui.end_row();
                        tema::etiqueta_ayuda(
                            ui,
                            "Motivo",
                            "Explica brevemente por qué rectificas (descuento no aplicado, error de precio, devolución…). Se imprime y forma parte del registro.",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut motivo)
                                .hint_text("Ej.: descuento no aplicado"),
                        );
                        ui.end_row();
                    });
                ui.add_space(6.0);
                // Se valida después de dibujar los campos: así el botón se
                // activa en el mismo fotograma en que se termina de escribir.
                let base_c = parse_cents(base.trim());
                let iva_pb = parse_pb(iva.trim());
                let (cuota, recargo, retencion) = match (base_c, iva_pb) {
                    (Some(b), Some(i)) => {
                        let cuota = if original.operacion.lleva_iva() {
                            crate::model::div_round(b * i as i64, 10_000)
                        } else {
                            0
                        };
                        let recargo = if original.operacion.lleva_iva() {
                            crate::model::div_round(b * original.recargo_pb as i64, 10_000)
                        } else {
                            0
                        };
                        (cuota, recargo, crate::model::div_round(b * original.irpf_pb as i64, 10_000))
                    }
                    _ => (0, 0, 0),
                };
                let total = base_c.unwrap_or(0) + cuota + recargo - retencion;
                let valido = base_c.map(|b| b != 0).unwrap_or(false)
                    && iva_pb.is_some()
                    && !motivo.trim().is_empty();
                let tipos_iva: Vec<String> = original
                    .totales()
                    .bases
                    .iter()
                    .map(|(tipo, _, _)| fmt_pb(*tipo))
                    .collect();
                if tipos_iva.len() > 1 {
                    ui.label(
                        RichText::new(format!(
                            "La original mezcla varios tipos de IVA ({} %). Esta R1 aplica un solo tipo: si la diferencia afecta a varios, crea una R1 por cada tipo.",
                            tipos_iva.join(", ")
                        ))
                        .size(10.0)
                        .color(tema::AMBAR),
                    );
                }
                ui.label(
                    RichText::new("FacturaE no está disponible para R1; el PDF sí.")
                        .size(10.0)
                        .color(tema::TENUE),
                );
                ui.label(
                    RichText::new(format!(
                        "IVA: {} · Recargo: {} · IRPF: {} · Total R1: {}",
                        fmt_cents_eur(cuota),
                        fmt_cents_eur(recargo),
                        fmt_cents_eur(retencion),
                        fmt_cents_eur(total)
                    ))
                    .monospace()
                    .size(11.0),
                );
                if original.operacion != TipoOperacion::Nacional {
                    ui.label(
                        RichText::new(
                            "La operación original no lleva IVA: la rectificativa tampoco.",
                        )
                        .size(10.0)
                        .color(tema::TENUE),
                    );
                }
                if base_c == Some(0) {
                    ui.label(
                        RichText::new("La diferencia no puede ser cero.")
                            .size(10.0)
                            .color(tema::ROJO),
                    );
                }
                ui.add_space(8.0);
                let intro = ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(valido, egui::Button::new("Crear y emitir R1"))
                        .clicked()
                        || (intro && valido)
                    {
                        confirmar = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cerrar = true;
                    }
                });
                ui.label(
                    RichText::new("También puedes pulsar Intro para crear.")
                        .size(10.0)
                        .color(tema::TENUE),
                );
            });

        if abrir_ayuda {
            self.ayuda_verifactu = true;
        }

        if confirmar {
            let nuevo = RectificarBuf {
                original_id: buf.original_id,
                base: base.clone(),
                iva: iva.clone(),
                motivo: motivo.clone(),
            };
            match self.crear_rectificativa(&nuevo) {
                Ok(id) => {
                    self.seleccion = Some(id);
                    self.vista = Vista::Facturas;
                    self.rectificar = None;
                    self.avisar(
                        "Rectificativa R1 emitida y encadenada. La original sigue intacta.",
                        false,
                    );
                }
                Err(e) => self.avisar(e, true),
            }
        } else if cerrar {
            self.rectificar = None;
        } else {
            self.rectificar = Some(RectificarBuf {
                original_id: buf.original_id,
                base,
                iva,
                motivo,
            });
        }
    }

    fn ventana_anular(&mut self, ctx: &egui::Context) {
        let Some((id, motivo)) = self.anular.clone() else {
            return;
        };
        let mut motivo = motivo;
        let mut cerrar = false;
        let mut confirmar = false;
        egui::Window::new("Anular factura")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Anular genera un registro de anulación encadenado. La factura no se borra.");
                    tema::icono_ayuda(
                        ui,
                        "Anulación",
                        "La anulación conserva el alta original y añade un registro nuevo con importes a cero. No se puede deshacer: si sólo cambia un importe, valora una rectificativa R1.",
                    );
                });
                ui.add(egui::TextEdit::singleline(&mut motivo).hint_text("Motivo de la anulación"));
                ui.horizontal(|ui| {
                    if ui.button("Anular").clicked() {
                        confirmar = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cerrar = true;
                    }
                });
            });
        if confirmar {
            match self.anular(id, motivo) {
                Ok(()) => self.avisar("Factura anulada y registrada.", false),
                Err(e) => self.avisar(e, true),
            }
            self.anular = None;
        } else if cerrar {
            self.anular = None;
        } else {
            self.anular = Some((id, motivo));
        }
    }

    fn ventana_xml(&mut self, ctx: &egui::Context) {
        let Some((nombre, xml)) = self.ver_xml.clone() else {
            return;
        };
        let mut abierto = true;
        egui::Window::new(format!("FacturaE · {}", nombre))
            .open(&mut abierto)
            .default_size([720.0, 520.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Validar contra Facturaev3_2_2.xsd antes de usarlo con FACe.")
                        .small()
                        .color(egui::Color32::from_gray(110)),
                );
                egui::ScrollArea::both().show(ui, |ui| {
                    let mut texto = xml.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut texto)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
            });
        if !abierto {
            self.ver_xml = None;
        }
    }
}

fn tarjeta(ui: &mut egui::Ui, titulo: &str, valor: &str) {
    egui::Frame::none()
        .fill(egui::Color32::WHITE)
        .inner_margin(16.0)
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(224)))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(titulo)
                        .small()
                        .color(egui::Color32::from_gray(110)),
                );
                ui.label(RichText::new(valor).size(20.0).strong());
            });
        });
}
