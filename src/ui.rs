//! Interfaz de escritorio (egui/eframe).
//!
//! Sin framework web ni WebView: una ventana nativa, un lienzo inmediato y
//! estado en memoria. La regla de la casa es que **una factura emitida no se
//! edita**: se anula, porque el registro encadenado ya está generado.

use crate::model::*;
use crate::perf::{Benchmark, Perfil};
use crate::store::Db;
use crate::{facturae, pdf, tema, verifactu};
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
    Articulos,
    Contabilidad,
    Guia,
    Ajustes,
}

/// Acciones que se difieren hasta que termina el préstamo mutable del editor.
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

struct Editor {
    doc: Documento,
    serie: String,
    fecha: String,
    vencimiento: String,
    irpf: String,
    cliente_id: u64,
    lineas: Vec<LineaBuf>,
    nuevo: bool,
}

impl Editor {
    fn desde(doc: Documento, nuevo: bool) -> Self {
        Self {
            serie: doc.serie.clone(),
            fecha: doc.fecha.format("%d/%m/%Y").to_string(),
            vencimiento: doc.vencimiento.format("%d/%m/%Y").to_string(),
            irpf: fmt_pb(doc.irpf_pb),
            cliente_id: doc.cliente_id,
            lineas: doc.lineas.iter().map(LineaBuf::desde).collect(),
            doc,
            nuevo,
        }
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
}

fn parse_fecha(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    NaiveDate::parse_from_str(t, "%d/%m/%Y")
        .ok()
        .or_else(|| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok())
}

pub fn dir_export() -> PathBuf {
    crate::store::dir_base().join("export")
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

pub struct App {
    pub db: Db,
    pub vista: Vista,
    /// Equipo detectado al arrancar; se muestra en Ajustes.
    pub perfil: Perfil,
    pub aviso: Option<String>,
    pub toast: Option<(String, bool)>,
    pub seleccion: Option<u64>,
    editor: Option<Editor>,
    ficha_cliente: Option<Cliente>,
    ficha_articulo: Option<ArticuloBuf>,
    ficha_gasto: Option<GastoBuf>,
    /// Año y trimestre que se miran en Contabilidad (0 = todo el año).
    anyo: i32,
    trimestre: u32,
    busqueda: String,
    /// Cobro en curso: (documento, fecha, importe) en texto.
    cobro: Option<(u64, String, String)>,
    /// Pestaña de Contabilidad: 0 resumen, 1 IVA, 2 IRPF, 3 347, 4 calendario, 5 clientes.
    pestana_contab: u8,
    /// Copia de seguridad pendiente de confirmar.
    restaurar: Option<PathBuf>,
    anular: Option<(u64, String)>,
    ver_xml: Option<(String, String)>,
    confirmar: Option<String>,
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
        if !db.empresa.completa() {
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
            vista: Vista::Inicio,
            perfil,
            aviso,
            toast,
            seleccion: None,
            editor: None,
            ficha_cliente: None,
            ficha_articulo: None,
            ficha_gasto: None,
            anyo: Local::now().year(),
            trimestre: 0,
            busqueda: String::new(),
            cobro: None,
            pestana_contab: 0,
            restaurar: None,
            anular: None,
            ver_xml: None,
            confirmar: None,
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
            .documentos
            .iter()
            .filter_map(|d| d.verifactu.as_ref())
            .max_by(|a, b| a.fecha_hora_gen.cmp(&b.fecha_hora_gen))
            .map(|r| r.huella.clone())
            .unwrap_or_default()
    }

    /// Asigna número, congela el cliente y genera el registro encadenado.
    fn emitir(&mut self, id: u64) -> Result<(), String> {
        if !self.db.empresa.completa() {
            return Err("Completa los datos de tu empresa en Ajustes antes de emitir.".into());
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
            // Presupuestos y proformas no están en la cadena de la AEAT: se
            // anulan sin generar registro.
            if d.tipo.es_fiscal() {
                Some(verifactu::registro_anulacion(d, &nif, &huella_previa))
            } else {
                None
            }
        };
        if let Some(d) = self.db.documento_mut(id) {
            d.estado = Estado::Anulado;
            d.motivo_anulacion = motivo;
            if registro.is_some() {
                d.verifactu = registro;
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
        let dir = dir_export();
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
        let dir = dir_export();
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(facturae::nombre_fichero(&doc));
        std::fs::write(&ruta, xml.clone()).map_err(|e| format!("escribir XML: {e}"))?;
        self.ver_xml = Some((facturae::nombre_fichero(&doc), xml));
        Ok(ruta)
    }
}

// ---------------------------------------------------------------------------
// Interfaz
// ---------------------------------------------------------------------------

fn campo(ui: &mut egui::Ui, etiqueta: &str, valor: &mut String) {
    ui.label(RichText::new(etiqueta).size(12.0).color(tema::SUAVE));
    // add_sized fija el tamaño real: dentro de una Grid, desired_width se
    // recortaba y el campo quedaba minúsculo.
    let ancho = (ui.available_width() - 12.0).clamp(220.0, 460.0);
    ui.add_sized(
        [ancho, 30.0],
        egui::TextEdit::singleline(valor).margin(egui::Margin::symmetric(10.0, 7.0)),
    );
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

        egui::SidePanel::left("navegacion")
            .exact_width(212.0)
            .resizable(false)
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
                ui.add_space(22.0);

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
                    (Vista::Articulos, "Artículos"),
                    (Vista::Contabilidad, "Contabilidad"),
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
                    self.vista = v;
                    self.editor = None;
                    self.seleccion = None;
                }
            });

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
                        if d.numero > 0 && d.tipo.es_fiscal() {
                            acciones.push(("xml", "FacturaE"));
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
                        .default_width(330.0)
                        .show(ctx, |ui| {
                            self.detalle(ctx, ui, id, acciones);
                        });
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.vista {
                Vista::Inicio => self.vista_inicio(ui),
                Vista::Presupuestos => self.vista_lista(ui, TipoDoc::Presupuesto),
                Vista::Proformas => self.vista_lista(ui, TipoDoc::Proforma),
                Vista::Facturas => self.vista_lista(ui, TipoDoc::Factura),
                Vista::Guia => self.vista_guia(ui),
                Vista::Clientes => self.vista_clientes(ui),
                Vista::Articulos => self.vista_articulos(ui),
                Vista::Contabilidad => self.vista_contabilidad(ui),
                Vista::Ajustes => self.vista_ajustes(ui),
            }
        });

        if self.anular.is_some() {
            self.ventana_anular(ctx);
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
    }
}

impl App {
    fn detalle(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui, id: u64, acciones: Vec<(&str, &str)>) {
        let doc = match self.db.documento(id) {
            Some(d) => d.clone(),
            None => return,
        };
        let t = doc.totales();
        ui.add_space(2.0);
        ui.label(RichText::new(doc.tipo.nombre()).size(11.5).color(tema::SUAVE));
        ui.label(
            RichText::new(doc.etiqueta_numero())
                .size(19.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            tema::insignia_estado(ui, doc.estado);
            ui.label(
                RichText::new(doc.fecha.format("%d/%m/%Y").to_string())
                    .size(11.5)
                    .color(tema::SUAVE),
            );
        });
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
                        _ => {}
                    }
                }
            }
        });

        if let Some(reg) = &doc.verifactu {
            ui.add_space(12.0);
            ui.separator();
            ui.label(RichText::new("Registro Verifactu").small().strong());
            egui::Grid::new("reg_verifactu").num_columns(2).show(ui, |ui| {
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
            if let Ok(code) = qrcode::QrCode::new(reg.qr_url.as_bytes()) {
                let n = code.width();
                let colores = code.to_colors();
                let lado = 128.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(lado, lado), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                let m = lado / n as f32;
                for fy in 0..n {
                    for fx in 0..n {
                        if colores[fy * n + fx] == qrcode::Color::Dark {
                            painter.rect_filled(
                                egui::Rect::from_min_size(
                                    rect.min + egui::vec2(fx as f32 * m, fy as f32 * m),
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
                RichText::new("Factura verificable en la sede electrónica de la AEAT.")
                    .small()
                    .color(egui::Color32::from_gray(110)),
            );
        }
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
        tema::titulo(ui, "Inicio", "Tu negocio de un vistazo");
        ui.add_space(12.0);

        ui.horizontal_wrapped(|ui| {
            tema::kpi(ui, "Facturas emitidas", &emitidas.to_string(), tema::TEXTO);
            tema::kpi(ui, "Pendiente de cobro", &fmt_cents_eur(pendiente), tema::AMBAR);
            tema::kpi(ui, "Presupuestos", &presupuestos.to_string(), tema::TEXTO);
            tema::kpi(
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
            );
            tema::kpi(
                ui,
                "Vencidas",
                &vencidas.to_string(),
                if vencidas > 0 {
                    tema::ROJO
                } else {
                    tema::VERDE
                },
            );
        });

        if !self.db.empresa.completa() {
            ui.add_space(16.0);
            ui.group(|ui| {
                ui.label(
                    RichText::new("Falta configurar tu empresa")
                        .strong()
                        .color(egui::Color32::from_rgb(150, 30, 30)),
                );
                ui.label("Nombre y NIF son obligatorios para poder emitir.");
                if ui.button("Ir a Ajustes").clicked() {
                    self.vista = Vista::Ajustes;
                }
            });
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
            tema::kpi(
                ui,
                "IVA a ingresar",
                &fmt_cents_eur(r.iva_a_ingresar()),
                tema::ACENTO,
            );
            tema::kpi(
                ui,
                "IRPF 130 estimado",
                &fmt_cents_eur(r.irpf_130()),
                tema::ACENTO,
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
            let (nombre, sub) = match tipo {
                TipoDoc::Factura => ("Facturas", "Documentos fiscales con registro Verifactu"),
                TipoDoc::Presupuesto => {
                    ("Presupuestos", "Ofertas para el cliente, sin validez fiscal")
                }
                TipoDoc::Proforma => ("Proformas", "Cobro anticipado, sin validez fiscal"),
            };
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(nombre)
                        .size(24.0)
                        .strong()
                        .color(tema::TEXTO),
                );
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

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("lista_docs")
                .num_columns(6)
                .striped(true)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Número").strong());
                    ui.label(RichText::new("Fecha").strong());
                    ui.label(RichText::new("Cliente").strong());
                    ui.label(RichText::new("Estado").strong());
                    ui.label(RichText::new("Total").strong());
                    ui.label("");
                    ui.end_row();
                    for d in &docs {
                        let sel = self.seleccion == Some(d.id);
                        if ui.selectable_label(sel, d.etiqueta_numero()).clicked() {
                            self.seleccion = Some(d.id);
                        }
                        ui.label(d.fecha.format("%d/%m/%Y").to_string());
                        ui.label(d.cliente.nombre_completo());
                        if d.vencida() {
                            tema::insignia(ui, "Vencida", tema::ROJO_CLARO, tema::ROJO);
                        } else {
                            if d.vencida() {
                        tema::insignia(ui, "Vencida", tema::ROJO_CLARO, tema::ROJO);
                    } else {
                        tema::insignia_estado(ui, d.estado);
                    }
                        }
                        ui.label(RichText::new(fmt_cents_eur(d.totales().total)).monospace());
                        if d.editable() {
                            if ui.small_button("editar").clicked() {
                                self.seleccion = Some(d.id);
                                self.editor = Some(Editor::desde(d.clone(), false));
                            }
                        } else if ui.small_button("ver").clicked() {
                            self.seleccion = Some(d.id);
                        }
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

        tema::tarjeta(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let margen = egui::Margin::symmetric(10.0, 7.0);
            egui::Grid::new("cab_doc")
                .num_columns(4)
                .min_col_width(104.0)
                .spacing([14.0, 10.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Serie").size(12.0).color(tema::SUAVE));
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.serie)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    ui.label(RichText::new("Cliente").size(12.0).color(tema::SUAVE));
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

                    ui.label(RichText::new("Fecha").size(12.0).color(tema::SUAVE));
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.fecha)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    ui.label(RichText::new("Vencimiento").size(12.0).color(tema::SUAVE));
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.vencimiento)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    ui.end_row();

                    ui.label(RichText::new("IRPF %").size(12.0).color(tema::SUAVE));
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.irpf)
                            .desired_width(124.0)
                            .margin(margen),
                    );
                    ui.label(RichText::new("Forma de pago").size(12.0).color(tema::SUAVE));
                    ui.add(
                        egui::TextEdit::singleline(&mut editor.doc.forma_pago)
                            .desired_width(320.0)
                            .margin(margen),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Operación").size(12.0).color(tema::SUAVE));
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
        ui.label(RichText::new("Líneas").size(15.0).strong().color(tema::TEXTO));
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
            ("Descripción", ancho_desc),
            ("Cantidad", ancho_cant),
            ("Precio", ancho_precio),
            ("Dto %", ancho_dto),
            ("IVA %", ancho_iva),
            ("", ancho_quitar),
        ];

        egui::ScrollArea::vertical()
            .max_height(alto_tabla)
            .id_salt("lineas")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (t, ancho) in cabecera {
                        ui.add_sized(
                            [ancho, 16.0],
                            egui::Label::new(
                                RichText::new(t).size(11.5).strong().color(tema::SUAVE),
                            )
                            .truncate(),
                        );
                    }
                });
                ui.add_space(4.0);

                for (i, l) in editor.lineas.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [ancho_desc, 32.0],
                            egui::TextEdit::singleline(&mut l.descripcion).hint_text("Concepto"),
                        );
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
                    if totales.retencion != 0 {
                        ui.label(RichText::new("Retención IRPF").color(tema::SUAVE));
                        ui.label(
                            RichText::new(format!("-{}", fmt_cents(totales.retencion)))
                                .monospace(),
                        );
                        ui.end_row();
                    }
                    ui.label(RichText::new("TOTAL").size(11.5).strong().color(tema::TENUE));
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
        if !problemas.is_empty() {
            ui.add_space(6.0);
            for p in problemas.iter().take(4) {
                ui.label(
                    RichText::new(p)
                        .small()
                        .color(egui::Color32::from_rgb(160, 90, 20)),
                );
            }
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
            if ui
                .add_enabled(
                    valido,
                    egui::Button::new("Guardar borrador").min_size(egui::vec2(150.0, alto_boton.y)),
                )
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

        // Aplicar la acción fuera del préstamo mutable del editor.
        match &accion {
            Accion::Nada => true,
            Accion::Bloqueada(m) => {
                self.avisar(m.clone(), true);
                true
            }
            Accion::Cancelar => {
                self.seleccion = None;
                false
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

        tema::titulo(
            ui,
            "Guía rápida",
            "De cero a presentar los impuestos, paso a paso",
        );
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
        for ob in crate::fiscal::obligaciones(self.db.empresa.actividad) {
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
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ob.trimestral {
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
            });
            ui.add_space(4.0);
        }
        if !cambios.is_empty() {
            for clave in cambios {
                crate::fiscal::alternar(&mut self.db, &clave);
            }
            self.guardar();
        }

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

    fn vista_clientes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Clientes");
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
                                campo(ui, "Nombre / razón social", &mut f.nombre);
                                campo(ui, "NIF / DNI / NIE", &mut f.nif);
                                ui.label("Tipo");
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.persona_fisica, "Persona física");
                                });
                                ui.end_row();
                                if f.persona_fisica {
                                    campo(ui, "Apellidos", &mut f.apellidos);
                                }
                                campo(ui, "Dirección", &mut f.direccion);
                                campo(ui, "Código postal", &mut f.cp);
                                campo(ui, "Ciudad", &mut f.ciudad);
                                campo(ui, "Provincia", &mut f.provincia);
                                campo(ui, "Email", &mut f.email);
                                campo(ui, "Teléfono", &mut f.telefono);
                                campo(ui, "Notas", &mut f.notas);
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
                                campo(ui, "Descripción", &mut f.art.descripcion);
                                ui.label("Precio unitario");
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.precio)
                                        .desired_width(120.0)
                                        .hint_text("0,00"),
                                );
                                ui.end_row();
                                ui.label("IVA %");
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.iva)
                                        .desired_width(120.0)
                                        .hint_text("21"),
                                );
                                ui.end_row();
                                campo(ui, "Unidad", &mut f.art.unidad);
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
        let sello = Local::now().format("%Y%m%d-%H%M%S");
        let destino = dir.join(format!("datos-{}.json", sello));
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
                        ui.label(RichText::new("Fecha").size(12.0).color(tema::SUAVE));
                        ui.add(
                            egui::TextEdit::singleline(&mut fecha)
                                .desired_width(150.0)
                                .margin(egui::Margin::symmetric(10.0, 7.0)),
                        );
                        ui.end_row();
                        ui.label(RichText::new("Importe").size(12.0).color(tema::SUAVE));
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
        let dir = dir_export();
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(format!("resumen-{}.txt", self.anyo));
        std::fs::write(&ruta, texto).map_err(|e| format!("escribir resumen: {e}"))?;
        Ok(ruta)
    }

    fn exportar_libro(&mut self) -> Result<PathBuf, String> {
        let csv = crate::contab::csv_libro(&self.db, self.anyo);
        let dir = dir_export();
        std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;
        let ruta = dir.join(format!("libro-{}.csv", self.anyo));
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

        let categorias = crate::contab::gastos_por_categoria(&self.db, self.anyo, self.trimestre);
        if !categorias.is_empty() {
            ui.add_space(14.0);
            ui.label(
                RichText::new("Gastos por categoría")
                    .size(13.5)
                    .strong()
                    .color(tema::TEXTO),
            );
            ui.add_space(4.0);
            egui::Grid::new("gastos_categoria")
                .num_columns(3)
                .striped(true)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for t in ["Categoría", "Base", "Nº"] {
                        ui.label(RichText::new(t).size(11.0).strong().color(tema::TENUE));
                    }
                    ui.end_row();
                    for (categoria, base, n) in &categorias {
                        ui.label(categoria);
                        ui.label(RichText::new(fmt_cents(*base)).monospace());
                        ui.label(n.to_string());
                        ui.end_row();
                    }
                });
        }

        ui.add_space(14.0);

        ui.columns(2, |cols| {
            cols[0].horizontal(|ui| {
                ui.label(RichText::new("Gastos del periodo").strong());
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
                                campo(ui, "Fecha (dd/mm/aaaa)", &mut f.fecha);
                                ui.label(RichText::new("Proveedor").size(12.0).color(tema::SUAVE));
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
                                campo(ui, "NIF", &mut f.gasto.nif);
                                campo(ui, "Concepto", &mut f.gasto.concepto);
                                ui.label(RichText::new("Categoría").size(12.0).color(tema::SUAVE));
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
                                ui.label("Base imponible");
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.base)
                                        .desired_width(120.0)
                                        .hint_text("0,00"),
                                );
                                ui.end_row();
                                ui.label("IVA %");
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.iva)
                                        .desired_width(120.0)
                                        .hint_text("21"),
                                );
                                ui.end_row();
                                ui.label("Retención IRPF %");
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.irpf)
                                        .desired_width(120.0)
                                        .hint_text("0"),
                                );
                                ui.end_row();
                                ui.label("Pagado");
                                ui.checkbox(&mut f.gasto.pagado, "Ya está pagado");
                                ui.end_row();
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

    fn vista_ajustes(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Ajustes")
                .size(24.0)
                .strong()
                .color(tema::TEXTO),
        );
        ui.add_space(6.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let e = &mut self.db.empresa;
            egui::Grid::new("form_empresa").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                campo(ui, "Nombre o razón social", &mut e.nombre);
                campo(ui, "NIF", &mut e.nif);
                ui.label("Tipo de titular");
                ui.checkbox(&mut e.persona_fisica, "Autónomo / persona física");
                ui.end_row();
                if e.persona_fisica {
                    campo(ui, "Apellidos", &mut e.apellidos);
                }
                campo(ui, "Dirección", &mut e.direccion);
                campo(ui, "Código postal", &mut e.cp);
                campo(ui, "Ciudad", &mut e.ciudad);
                campo(ui, "Provincia", &mut e.provincia);
                campo(ui, "Email", &mut e.email);
                campo(ui, "Teléfono", &mut e.telefono);
                campo(ui, "IBAN", &mut e.iban);
                campo(ui, "Serie de factura", &mut e.serie);

                let mut dias_venc = e.dias_vencimiento.to_string();
                ui.label("Días de vencimiento");
                if ui
                    .add(egui::TextEdit::singleline(&mut dias_venc).desired_width(90.0))
                    .changed()
                {
                    if let Ok(v) = dias_venc.trim().parse::<i32>() {
                        e.dias_vencimiento = v.clamp(0, 365);
                    }
                }
                ui.end_row();
                campo(ui, "Forma de pago por defecto", &mut e.forma_pago_defecto);

                ui.label("Actividad");
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

                let mut iva = fmt_pb(e.iva_defecto);
                let mut irpf = fmt_pb(e.irpf_defecto);
                ui.label("IVA por defecto %");
                if ui
                    .add(egui::TextEdit::singleline(&mut iva).desired_width(90.0))
                    .changed()
                {
                    if let Some(v) = parse_pb(&iva) {
                        e.iva_defecto = v;
                    }
                }
                ui.end_row();
                ui.label("IRPF por defecto %");
                if ui
                    .add(egui::TextEdit::singleline(&mut irpf).desired_width(90.0))
                    .changed()
                {
                    if let Some(v) = parse_pb(&irpf) {
                        e.irpf_defecto = v;
                    }
                }
                ui.end_row();
                campo(ui, "Condiciones de pago", &mut e.condiciones);
                ui.label("Verifactu");
                ui.checkbox(&mut e.verifactu_activo, "Generar registro encadenado y QR");
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
            ui.label(
                RichText::new("Copias de seguridad")
                    .size(15.0)
                    .strong()
                    .color(tema::TEXTO),
            );
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
                ui.label("Anular genera un registro de anulación encadenado. La factura no se borra.");
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
