//! Sistema visual de la aplicación.
//!
//! Una sola paleta y unos pocos ayudantes para que todas las pantallas se vean
//! de la misma familia: fondo gris muy claro, tarjetas blancas con borde fino,
//! un azul de acento para lo accionable y colores de estado consistentes.

use crate::model::Estado;
use eframe::egui::{self, Color32, Response, RichText, Stroke};

// ---------------------------------------------------------------------------
// Paleta
// ---------------------------------------------------------------------------

pub const FONDO: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF9);
pub const SUPERFICIE: Color32 = Color32::WHITE;
pub const BORDE: Color32 = Color32::from_rgb(0xE2, 0xE6, 0xEE);
pub const TEXTO: Color32 = Color32::from_rgb(0x18, 0x20, 0x2E);
pub const SUAVE: Color32 = Color32::from_rgb(0x66, 0x70, 0x82);
pub const TENUE: Color32 = Color32::from_rgb(0x92, 0x9C, 0xAC);

pub const ACENTO: Color32 = Color32::from_rgb(0x2B, 0x59, 0xC8);
pub const ACENTO_CLARO: Color32 = Color32::from_rgb(0xE9, 0xEF, 0xFC);

pub const VERDE: Color32 = Color32::from_rgb(0x0E, 0x7A, 0x5D);
pub const VERDE_CLARO: Color32 = Color32::from_rgb(0xE6, 0xF5, 0xEF);

pub const AMBAR: Color32 = Color32::from_rgb(0x9A, 0x58, 0x00);
pub const AMBAR_CLARO: Color32 = Color32::from_rgb(0xFD, 0xF3, 0xE3);

pub const ROJO: Color32 = Color32::from_rgb(0xB4, 0x23, 0x18);
pub const ROJO_CLARO: Color32 = Color32::from_rgb(0xFD, 0xEC, 0xEA);

// ---------------------------------------------------------------------------
// Estilo global
// ---------------------------------------------------------------------------

pub fn aplicar_estilo(estilo: &mut egui::Style) {
    let mut v = egui::Visuals::light();

    v.panel_fill = FONDO;
    v.window_fill = SUPERFICIE;
    v.faint_bg_color = Color32::from_rgb(0xF8, 0xFA, 0xFC);
    v.extreme_bg_color = Color32::WHITE;
    v.window_stroke = Stroke::new(1.0_f32, BORDE);
    v.window_rounding = egui::Rounding::same(12.0);
    v.menu_rounding = egui::Rounding::same(10.0);
    v.selection.bg_fill = ACENTO_CLARO;
    v.selection.stroke = Stroke::new(1.0_f32, ACENTO);

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDE);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, SUAVE);

    w.inactive.bg_fill = SUPERFICIE;
    w.inactive.weak_bg_fill = Color32::from_rgb(0xF7, 0xF9, 0xFC);
    w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDE);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXTO);
    w.inactive.rounding = egui::Rounding::same(8.0);

    w.hovered.bg_fill = Color32::from_rgb(0xF1, 0xF5, 0xFE);
    w.hovered.weak_bg_fill = Color32::from_rgb(0xF1, 0xF5, 0xFE);
    w.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0xB8, 0xC8, 0xF0));
    w.hovered.fg_stroke = Stroke::new(1.0_f32, TEXTO);
    w.hovered.rounding = egui::Rounding::same(8.0);

    w.active.bg_fill = ACENTO_CLARO;
    w.active.weak_bg_fill = ACENTO_CLARO;
    w.active.bg_stroke = Stroke::new(1.0_f32, ACENTO);
    w.active.fg_stroke = Stroke::new(1.0_f32, ACENTO);
    w.active.rounding = egui::Rounding::same(8.0);

    w.open.bg_fill = SUPERFICIE;
    w.open.weak_bg_fill = SUPERFICIE;
    w.open.bg_stroke = Stroke::new(1.0_f32, BORDE);
    w.open.fg_stroke = Stroke::new(1.0_f32, TEXTO);
    w.open.rounding = egui::Rounding::same(8.0);

    estilo.visuals = v;
    // Los tooltips de egui se desvanecen con esta duración. El perfil de
    // rendimiento puede acortarla o desactivarla en equipos muy modestos.
    estilo.animation_time = 0.16;
    estilo.spacing.item_spacing = egui::vec2(8.0, 7.0);
    estilo.spacing.button_padding = egui::vec2(12.0, 6.0);
    estilo.spacing.interact_size = egui::vec2(40.0, 28.0);
    estilo
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
    estilo
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.5));
    estilo
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(13.5));
    estilo
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
}

// ---------------------------------------------------------------------------
// Piezas
// ---------------------------------------------------------------------------

/// Contenedor blanco con borde fino y esquinas suaves.
pub fn tarjeta<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(SUPERFICIE)
        .stroke(Stroke::new(1.0_f32, BORDE))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, add)
        .inner
}

/// Tarjeta de cifra: etiqueta pequeña arriba, número grande debajo.
pub fn kpi(ui: &mut egui::Ui, etiqueta: &str, valor: &str, tono: Color32) {
    tarjeta(ui, |ui| {
        ui.set_min_width(148.0);
        ui.vertical(|ui| {
            ui.set_min_height(45.0);
            ui.label(RichText::new(etiqueta).size(11.0).color(SUAVE));
            ui.add_space(3.0);
            ui.label(RichText::new(valor).size(20.0).strong().color(tono));
        });
    });
}

/// Igual que `kpi`, pero con un icono de ayuda junto a la etiqueta. Se usa en
/// las cifras que necesitan una explicación (IVA, IRPF, cadena Verifactu…).
pub fn kpi_ayuda(
    ui: &mut egui::Ui,
    etiqueta: &str,
    valor: &str,
    tono: Color32,
    texto: &str,
) {
    tarjeta(ui, |ui| {
        ui.set_min_width(148.0);
        ui.vertical(|ui| {
            ui.set_min_height(45.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(RichText::new(etiqueta).size(11.0).color(SUAVE))
                    .on_hover_ui(|ui| ayuda_emergente(ui, etiqueta, texto));
                icono_ayuda(ui, etiqueta, texto);
            });
            ui.add_space(3.0);
            ui.label(RichText::new(valor).size(20.0).strong().color(tono));
        });
    });
}

// ---------------------------------------------------------------------------
// Ayuda contextual
// ---------------------------------------------------------------------------

/// Icono «?» pequeño y consistente en toda la app. Al pasar el ratón muestra
/// una tarjeta de ayuda; egui la desvanece al salir, sin dejar ventanas
/// flotantes abiertas.
pub fn icono_ayuda(ui: &mut egui::Ui, titulo: &str, texto: &str) -> Response {
    let (rect, respuesta) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let centro = rect.center();
    let hovered = respuesta.hovered();
    {
        let pintor = ui.painter();
        if hovered {
            pintor.circle_filled(centro, 8.5, ACENTO);
            pintor.text(
                centro,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );
        } else {
            pintor.circle_filled(centro, 8.5, ACENTO_CLARO);
            pintor.circle_stroke(
                centro,
                8.5,
                Stroke::new(1.0_f32, Color32::from_rgb(0xC6, 0xD4, 0xF2)),
            );
            pintor.text(
                centro,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(11.0),
                ACENTO,
            );
        }
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Help);
    }
    // El icono no tiene texto visible: que AccessKit anuncie al menos qué
    // ayuda representa. El contenido completo sigue en el tooltip.
    respuesta.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Label,
            ui.is_enabled(),
            format!("Ayuda: {titulo}"),
        )
    });
    respuesta.on_hover_ui(|ui| ayuda_emergente(ui, titulo, texto))
}

/// Contenido de la tarjeta de ayuda. El marco redondeado y la sombra los pone
/// el tooltip de egui; aquí sólo se cuida la jerarquía y el ancho de lectura.
pub fn ayuda_emergente(ui: &mut egui::Ui, titulo: &str, texto: &str) {
    ui.set_max_width(310.0);
    ui.spacing_mut().item_spacing.y = 4.0;
    ui.label(RichText::new("AYUDA").size(9.0).strong().color(ACENTO));
    ui.label(
        RichText::new(titulo)
            .size(13.0)
            .strong()
            .color(TEXTO),
    );
    ui.add_space(1.0);
    ui.separator();
    ui.add_space(2.0);
    ui.label(RichText::new(texto).size(11.5).color(SUAVE));
}

/// Etiqueta de formulario con su icono de ayuda al lado. Mantiene el mismo
/// tamaño y color que el resto de etiquetas para no romper las rejillas.
pub fn etiqueta_ayuda(ui: &mut egui::Ui, etiqueta: &str, texto: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.label(RichText::new(etiqueta).size(12.0).color(SUAVE))
            .on_hover_ui(|ui| ayuda_emergente(ui, etiqueta, texto));
        icono_ayuda(ui, etiqueta, texto);
    });
}

/// Casilla con su icono de ayuda. La casilla sigue siendo clicable; el icono
/// sólo añade la explicación al pasar el ratón por encima.
pub fn checkbox_ayuda(
    ui: &mut egui::Ui,
    checked: &mut bool,
    texto: &str,
    ayuda: &str,
) -> Response {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let respuesta = ui
            .checkbox(checked, texto)
            .on_hover_ui(|ui| ayuda_emergente(ui, texto, ayuda));
        icono_ayuda(ui, texto, ayuda);
        respuesta
    })
    .inner
}

/// Etiqueta de estado en forma de píldora.
pub fn insignia(ui: &mut egui::Ui, texto: &str, fondo: Color32, color: Color32) {
    egui::Frame::none()
        .fill(fondo)
        .rounding(egui::Rounding::same(20.0))
        .inner_margin(egui::Margin::symmetric(9.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(texto).size(10.5).strong().color(color));
        });
}

pub fn insignia_estado(ui: &mut egui::Ui, estado: Estado) {
    let (fondo, color) = match estado {
        Estado::Borrador => (Color32::from_rgb(0xF1, 0xF3, 0xF7), SUAVE),
        Estado::Emitido => (ACENTO_CLARO, ACENTO),
        Estado::Pagado => (VERDE_CLARO, VERDE),
        Estado::Anulado => (ROJO_CLARO, ROJO),
    };
    insignia(ui, estado.nombre(), fondo, color);
}

/// Botón de acción principal: relleno de acento y texto blanco.
pub fn boton_primario(ui: &mut egui::Ui, texto: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(texto).size(13.0).strong().color(Color32::WHITE))
            .fill(ACENTO)
            .rounding(egui::Rounding::same(8.0)),
    )
}

/// Botón para una acción destructiva: rojo y texto blanco.
pub fn boton_peligro(ui: &mut egui::Ui, texto: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(texto).size(13.0).strong().color(Color32::WHITE))
            .fill(ROJO)
            .rounding(egui::Rounding::same(8.0)),
    )
}

/// Botón secundario, discreto.
pub fn boton_suave(ui: &mut egui::Ui, texto: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(texto).size(13.0).color(TEXTO))
            .fill(Color32::from_rgb(0xF4, 0xF6, 0xFB))
            .rounding(egui::Rounding::same(8.0)),
    )
}

/// Título de pantalla con un icono de ayuda al lado. Sirve para explicar de un
/// vistazo qué hace cada pantalla sin añadir párrafos permanentes.
pub fn titulo_ayuda(ui: &mut egui::Ui, texto: &str, sub: &str, ayuda: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(texto).size(24.0).strong().color(TEXTO))
            .on_hover_ui(|ui| ayuda_emergente(ui, texto, ayuda));
        icono_ayuda(ui, texto, ayuda);
    });
    if !sub.is_empty() {
        ui.label(RichText::new(sub).size(12.0).color(SUAVE));
    }
}

/// Encabezado de un grupo de navegación.
pub fn grupo_nav(ui: &mut egui::Ui, texto: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(texto).size(9.5).strong().color(TENUE));
    ui.add_space(5.0);
}

/// Elemento de navegación lateral con barra de acento cuando está activo.
pub fn nav(ui: &mut egui::Ui, activo: bool, texto: &str) -> Response {
    let ancho = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ancho, 30.0), egui::Sense::click());
    let pintor = ui.painter();
    if activo {
        pintor.rect_filled(rect, egui::Rounding::same(8.0), ACENTO_CLARO);
        let barra = egui::Rect::from_min_size(
            rect.min + egui::vec2(0.0, 8.0),
            egui::vec2(3.0, rect.height() - 16.0),
        );
        pintor.rect_filled(barra, egui::Rounding::same(2.0), ACENTO);
    } else if resp.hovered() {
        pintor.rect_filled(rect, egui::Rounding::same(8.0), Color32::from_rgb(0xF5, 0xF7, 0xFB));
    }
    pintor.text(
        rect.min + egui::vec2(14.0, rect.height() / 2.0),
        egui::Align2::LEFT_CENTER,
        texto,
        egui::FontId::proportional(13.5),
        if activo { ACENTO } else { TEXTO },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // El dibujo es propio: sin esta información, un lector de pantalla vería
    // una zona vacía en vez de la sección de navegación.
    resp.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            activo,
            texto,
        )
    });
    resp
}

/// Estado vacío: explica qué hacer en vez de dejar la pantalla en blanco.
pub fn vacio(ui: &mut egui::Ui, titulo: &str, texto: &str) {
    egui::Frame::none()
        .fill(Color32::from_rgb(0xFA, 0xFB, 0xFD))
        .stroke(Stroke::new(1.0_f32, BORDE))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(26.0))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(titulo).size(15.0).strong().color(TEXTO));
                ui.add_space(3.0);
                ui.label(RichText::new(texto).size(12.0).color(SUAVE));
            });
        });
}
