//! Generador de PDF sin dependencias.
//!
//! Escribir un PDF a mano es menos exótico de lo que parece: catálogo, árbol de
//! páginas, dos fuentes base-14 y un flujo de contenido por página. A cambio no
//! hay binario nativo, ni fuentes que embeber, ni crates que se rompan.
//!
//! El texto va en WinAnsi, que cubre el español completo (á é í ó ú ñ ü ¿ ¡ º ª)
//! más el euro (0x80 en cp1252). Cualquier otro carácter sale como `?`.

use crate::empleados::{Empleado, Nomina};
use crate::model::{
    fmt_cents, fmt_cents_eur, fmt_pb, fmt_qty, Documento, Empresa, Linea, RegistroVerifactu,
};

const ANCHO: f32 = 595.28;
const ALTO: f32 = 841.89;
const MARGEN: f32 = 45.0;
const DERECHA: f32 = ANCHO - MARGEN;
const MM_PT: f32 = 72.0 / 25.4;
/// El reglamento exige un QR de 30 a 40 mm; 34 mm deja sitio a la leyenda.
const QR_LADO: f32 = 34.0 * MM_PT;
/// Zona de silencio mínima de 2 mm alrededor del código.
const QR_SILENCIO: f32 = 2.0 * MM_PT;

/// Anchos Helvetica (unidades/1000) para ASCII 32..126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Anchos Helvetica-Bold.
const HELV_B: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

fn ancho_texto(s: &str, size: f32, negrita: bool) -> f32 {
    let tabla = if negrita { &HELV_B } else { &HELV };
    let mut w = 0f32;
    for b in s.bytes() {
        let idx = if (32..=126).contains(&b) {
            (b - 32) as usize
        } else {
            // Aproximación: acentos y símbolos anchos se miden como 'n'.
            77usize
        };
        w += tabla[idx] as f32;
    }
    w * size / 1000.0
}

/// Cadena literal PDF, siempre en ASCII.
///
/// Los caracteres de WinAnsi se escriben con escapes octales (`\351` = é), que
/// es lo único que el lector interpreta como un byte: si se dejara el char tal
/// cual, Rust lo codificaría en UTF-8 (dos bytes) y el PDF mostraría basura.
fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push(' '),
            '€' => out.push_str("\\200"),
            c if (c as u32) < 128 => out.push(c),
            c if (c as u32) < 256 => out.push_str(&format!("\\{:03o}", c as u32)),
            _ => out.push('?'),
        }
    }
    out
}

/// PDF trabaja con el origen abajo a la izquierda; la maquetación se piensa
/// de arriba abajo, así que se convierte aquí.
fn y(y_top: f32) -> f32 {
    ALTO - y_top
}

struct Pagina {
    ops: String,
}

impl Pagina {
    fn nueva() -> Self {
        Self { ops: String::new() }
    }

    fn texto(&mut self, x: f32, y_top: f32, size: f32, negrita: bool, gris: f32, s: &str) {
        let fuente = if negrita { "/F2" } else { "/F1" };
        self.ops.push_str(&format!(
            "BT {} {} Tf {} g 1 0 0 1 {:.2} {:.2} Tm ({}) Tj ET\n",
            fuente,
            size,
            gris,
            x,
            y(y_top),
            literal(s)
        ));
    }

    fn texto_der(&mut self, x_der: f32, y_top: f32, size: f32, negrita: bool, gris: f32, s: &str) {
        let w = ancho_texto(s, size, negrita);
        self.texto(x_der - w, y_top, size, negrita, gris, s);
    }

    fn linea(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, grosor: f32, gris: f32) {
        self.ops.push_str(&format!(
            "{:.2} w {:.2} G {:.2} {:.2} m {:.2} {:.2} l S\n",
            grosor,
            gris,
            x1,
            y(y1),
            x2,
            y(y2)
        ));
    }

    fn rect(&mut self, x: f32, y_top: f32, w: f32, h: f32, gris: f32) {
        self.ops.push_str(&format!(
            "{:.3} g {:.2} {:.2} {:.2} {:.2} re f\n",
            gris,
            x,
            y(y_top) - h,
            w,
            h
        ));
    }

    /// Caja con borde.
    fn caja(&mut self, x: f32, y_top: f32, w: f32, h: f32, grosor: f32, gris: f32) {
        self.ops.push_str(&format!(
            "{:.2} w {:.2} G {:.2} {:.2} {:.2} {:.2} re S\n",
            grosor,
            gris,
            x,
            y(y_top) - h,
            w,
            h
        ));
    }

    /// QR de cotejo. Nivel M y zona de silencio blanca, como pide el
    /// reglamento; el tamaño se pasa en puntos (34 mm en la factura).
    fn qr(&mut self, x0: f32, y_top: f32, lado: f32, datos: &str) {
        let Ok(code) = qrcode::QrCode::with_error_correction_level(
            datos.as_bytes(),
            qrcode::EcLevel::M,
        ) else {
            return;
        };
        // Marco blanco: sin él, el lector puede perder los módulos del borde.
        self.rect(x0, y_top, lado, lado, 1.0);
        let n = code.width();
        let colores = code.to_colors();
        let interior = (lado - 2.0 * QR_SILENCIO).max(1.0);
        let m = interior / n as f32;
        let x = x0 + QR_SILENCIO;
        let y = y_top + QR_SILENCIO;
        for fy in 0..n {
            for fx in 0..n {
                if colores[fy * n + fx] == qrcode::Color::Dark {
                    self.rect(x + fx as f32 * m, y + fy as f32 * m, m + 0.05, m + 0.05, 0.0);
                }
            }
        }
    }
    /// Dibuja el logo (`/Im1`) en el rectángulo indicado.
    fn imagen(&mut self, x: f32, y_top: f32, w: f32, h: f32) {
        self.ops.push_str(&format!(
            "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im1 Do Q\n",
            w,
            h,
            x,
            y(y_top + h)
        ));
    }
}

/// Escala (ancho, alto) para que quepa en la caja sin deformarse.
fn ajustar(ancho: f32, alto: f32, max_w: f32, max_h: f32) -> (f32, f32) {
    if ancho <= 0.0 || alto <= 0.0 {
        return (0.0, 0.0);
    }
    let escala = (max_w / ancho).min(max_h / alto);
    (ancho * escala, alto * escala)
}

fn ensamblar(paginas: Vec<String>, logo: Option<&crate::logo::Jpeg>) -> Vec<u8> {
    let n = paginas.len();
    let mut out: Vec<u8> = Vec::with_capacity(8192 * n + 1024);
    out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

    // Con logo, la imagen ocupa el objeto 5 y las páginas empiezan en el 6.
    let primero = if logo.is_some() { 6 } else { 5 };
    let total_obj = primero - 1 + n * 2;
    let mut offsets = vec![0usize; total_obj + 1];

    fn obj(out: &mut Vec<u8>, offsets: &mut [usize], id: usize, cuerpo: &str) {
        offsets[id] = out.len();
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", id, cuerpo).as_bytes());
    }

    obj(&mut out, &mut offsets, 1, "<</Type/Catalog/Pages 2 0 R>>");
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", primero + i * 2)).collect();
    obj(
        &mut out,
        &mut offsets,
        2,
        &format!("<</Type/Pages/Kids[{}]/Count {}>>", kids.join(" "), n),
    );
    obj(
        &mut out,
        &mut offsets,
        3,
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica/Encoding/WinAnsiEncoding>>",
    );
    obj(
        &mut out,
        &mut offsets,
        4,
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica-Bold/Encoding/WinAnsiEncoding>>",
    );

    // El JPEG se copia tal cual: es lo único que hay que embeber.
    if let Some(l) = logo {
        offsets[5] = out.len();
        out.extend_from_slice(
            format!(
                "5 0 obj\n<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/DeviceRGB/BitsPerComponent 8/Filter/DCTDecode/Length {}>>\nstream\n",
                l.ancho,
                l.alto,
                l.datos.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&l.datos);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }

    for (i, contenido) in paginas.iter().enumerate() {
        let pid = primero + i * 2;
        let cid = pid + 1;
        obj(
            &mut out,
            &mut offsets,
            pid,
            &format!(
                "<</Type/Page/Parent 2 0 R/MediaBox[0 0 {:.2} {:.2}]/Resources<</Font<</F1 3 0 R/F2 4 0 R>>{}>>/Contents {} 0 R>>",
                ANCHO,
                ALTO,
                if logo.is_some() { "/XObject<</Im1 5 0 R>>" } else { "" },
                cid
            ),
        );
        // El contenido de `p.ops` termina en salto de línea. No se añade otro:
        // /Length debe coincidir con los bytes reales del stream.
        let stream = format!(
            "<</Length {}>>\nstream\n{}endstream",
            contenido.len(),
            contenido
        );
        obj(&mut out, &mut offsets, cid, &stream);
    }

    let inicio_xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", total_obj + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for i in 1..=total_obj {
        out.extend_from_slice(format!("{:010} 00000 n \n", offsets[i]).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{}\n%%EOF\n",
            total_obj + 1,
            inicio_xref
        )
        .as_bytes(),
    );
    out
}

/// Cabecera de página: emisor a la izquierda, identificación a la derecha.
fn cabecera(
    p: &mut Pagina,
    doc: &Documento,
    empresa: &Empresa,
    titulo: &str,
    logo: Option<&crate::logo::Jpeg>,
    desplazamiento: f32,
) {
    p.rect(0.0, 0.0, ANCHO, 6.0, 0.10);

    // El logo va arriba a la izquierda y el bloque de la empresa se aparta a su
    // derecha, sin mover nada en vertical.
    let mut x = MARGEN;
    if let Some(l) = logo {
        let (w, h) = ajustar(l.ancho as f32, l.alto as f32, 150.0, 44.0);
        p.imagen(MARGEN, MARGEN + 4.0 + (44.0 - h) / 2.0, w, h);
        x = MARGEN + w + 12.0;
    }

    let mut y = MARGEN + 14.0 + desplazamiento;
    let nombre = if empresa.nombre.trim().is_empty() {
        "Tu empresa"
    } else {
        &empresa.nombre
    };
    p.texto(x, y, 15.0, true, 0.10, &nombre.to_uppercase());
    y += 14.0;
    if !empresa.nif.trim().is_empty() {
        p.texto(x, y, 9.0, false, 0.35, &format!("NIF {}", empresa.nif));
        y += 11.0;
    }
    let mut linea_dir = empresa.direccion.clone();
    if !empresa.cp.is_empty() || !empresa.ciudad.is_empty() {
        linea_dir.push_str(&format!(" · {} {}", empresa.cp, empresa.ciudad));
    }
    if !linea_dir.trim().is_empty() {
        p.texto(x, y, 9.0, false, 0.35, &linea_dir);
        y += 11.0;
    }
    if !empresa.provincia.trim().is_empty() {
        p.texto(x, y, 9.0, false, 0.35, &empresa.provincia);
        y += 11.0;
    }
    if !empresa.email.trim().is_empty() {
        p.texto(x, y, 9.0, false, 0.35, &empresa.email);
        y += 11.0;
    }
    if !empresa.telefono.trim().is_empty() {
        p.texto(x, y, 9.0, false, 0.35, &empresa.telefono);
    }

    p.texto_der(
        DERECHA,
        MARGEN + 16.0 + desplazamiento,
        22.0,
        true,
        0.10,
        titulo,
    );
    let mut y2 = MARGEN + 34.0 + desplazamiento;
    p.texto_der(DERECHA, y2, 11.0, true, 0.10, &doc.etiqueta_numero());
    y2 += 13.0;
    let estado = match doc.estado {
        crate::model::Estado::Anulado => "ANULADA",
        _ => "",
    };
    if !estado.is_empty() {
        p.texto_der(DERECHA, y2, 9.0, true, 0.55, estado);
        y2 += 11.0;
    }
    p.texto_der(
        DERECHA,
        y2,
        9.0,
        false,
        0.35,
        &format!("Fecha: {}", doc.fecha.format("%d/%m/%Y")),
    );
    y2 += 11.0;
    p.texto_der(
        DERECHA,
        y2,
        9.0,
        false,
        0.35,
        &format!("Vence: {}", doc.vencimiento.format("%d/%m/%Y")),
    );
}

fn bloque_cliente(p: &mut Pagina, doc: &Documento, y: f32) -> f32 {
    p.caja(MARGEN, y, DERECHA - MARGEN, 54.0, 0.4, 0.75);
    p.texto(MARGEN + 8.0, y + 13.0, 8.0, true, 0.45, "CLIENTE");
    p.texto(MARGEN + 8.0, y + 26.0, 11.0, true, 0.10, &doc.cliente.nombre_completo());
    let mut detalle = String::new();
    if !doc.cliente.nif.is_empty() {
        detalle.push_str(&format!("NIF {}   ", doc.cliente.nif));
    }
    if !doc.cliente.direccion.is_empty() {
        detalle.push_str(&doc.cliente.direccion);
    }
    if !doc.cliente.cp.is_empty() || !doc.cliente.ciudad.is_empty() {
        detalle.push_str(&format!("   {} {}", doc.cliente.cp, doc.cliente.ciudad));
    }
    p.texto(MARGEN + 8.0, y + 39.0, 9.0, false, 0.30, &detalle);
    y + 54.0
}

const COL_X: [f32; 6] = [MARGEN, 330.0, 390.0, 440.0, 490.0, DERECHA];

fn cabecera_tabla(p: &mut Pagina, y: f32) -> f32 {
    p.rect(MARGEN, y, DERECHA - MARGEN, 16.0, 0.93);
    p.texto(MARGEN + 6.0, y + 11.0, 8.0, true, 0.30, "DESCRIPCIÓN");
    p.texto_der(COL_X[2] - 6.0, y + 11.0, 8.0, true, 0.30, "CANT.");
    p.texto_der(COL_X[3] - 6.0, y + 11.0, 8.0, true, 0.30, "PRECIO");
    p.texto_der(COL_X[4] - 6.0, y + 11.0, 8.0, true, 0.30, "DTO.");
    p.texto_der(COL_X[5] - 6.0, y + 11.0, 8.0, true, 0.30, "IMPORTE");
    y + 16.0
}

fn fila(p: &mut Pagina, l: &Linea, y: f32) -> f32 {
    let lc = Documento::calc_linea(l);
    let alto = 18.0;
    // Descripción recortada para que no invada las columnas numéricas.
    let mut desc = l.descripcion.clone();
    while ancho_texto(&desc, 9.0, false) > COL_X[1] - MARGEN - 10.0 && desc.chars().count() > 4 {
        desc.pop();
    }
    if desc != l.descripcion {
        desc.push_str("..");
    }
    p.texto(MARGEN + 4.0, y + 12.0, 9.0, false, 0.10, &desc);
    p.texto_der(COL_X[2] - 6.0, y + 12.0, 9.0, false, 0.20, &fmt_qty(l.cantidad_milli));
    p.texto_der(COL_X[3] - 6.0, y + 12.0, 9.0, false, 0.20, &fmt_cents(l.precio_cents));
    p.texto_der(
        COL_X[4] - 6.0,
        y + 12.0,
        9.0,
        false,
        0.20,
        &if l.descuento_pb > 0 {
            format!("{}%", fmt_pb(l.descuento_pb))
        } else {
            "—".to_string()
        },
    );
    p.texto_der(COL_X[5] - 6.0, y + 12.0, 9.0, false, 0.10, &fmt_cents(lc.importe));
    p.linea(MARGEN, y + alto, DERECHA, y + alto, 0.3, 0.90);
    y + alto
}

fn bloque_totales(p: &mut Pagina, doc: &Documento, y: f32) -> f32 {
    let t = doc.totales();
    let x_etq = 330.0;
    let mut y = y;
    let fila = |p: &mut Pagina, y: f32, etq: &str, val: String, negrita: bool| {
        p.texto(x_etq, y, 9.0, negrita, if negrita { 0.10 } else { 0.35 }, etq);
        p.texto_der(DERECHA - 6.0, y, 9.0, negrita, 0.10, &val);
    };
    fila(p, y, "Base imponible", fmt_cents(t.base_total), false);
    y += 13.0;
    for (tipo, base, cuota) in &t.bases {
        if !doc.operacion.lleva_iva() {
            break;
        }
        fila(
            p,
            y,
            &format!("IVA {} % sobre {}", fmt_pb(*tipo), fmt_cents(*base)),
            fmt_cents(*cuota),
            false,
        );
        y += 13.0;
    }
    if doc.recargo_pb > 0 {
        fila(
            p,
            y,
            &format!("Recargo de equivalencia {} %", fmt_pb(doc.recargo_pb)),
            fmt_cents(t.cuota_recargo),
            false,
        );
        y += 13.0;
    }
    if doc.irpf_pb > 0 {
        fila(
            p,
            y,
            &format!("Retención IRPF {} %", fmt_pb(doc.irpf_pb)),
            format!("-{}", fmt_cents(t.retencion)),
            false,
        );
        y += 13.0;
    }
    y += 2.0;
    p.linea(x_etq, y, DERECHA - 6.0, y, 0.4, 0.70);
    y += 4.0;
    p.texto(x_etq, y + 10.0, 12.0, true, 0.10, "TOTAL");
    p.texto_der(DERECHA - 6.0, y + 10.0, 12.0, true, 0.10, &fmt_cents_eur(t.total));

    // Leyenda obligatoria: sin ella la factura no vale para una exportación o
    // una operación con inversión del sujeto pasivo.
    if !doc.operacion.leyenda().is_empty() {
        p.texto(MARGEN, y + 32.0, 7.0, false, 0.45, doc.operacion.leyenda());
    }
    y + 20.0
}

/// QR tributario de la primera página. La AEAT exige que sea el primer QR y
/// que vaya al principio, antes del contenido; no se repite en las demás
/// páginas. Va centrado y con la etiqueta «QR tributario:» encima.
fn qr_tributario(p: &mut Pagina, reg: &RegistroVerifactu) {
    let x = (ANCHO - QR_LADO) / 2.0;
    p.texto(x, 37.0, 7.0, true, 0.25, "QR tributario:");
    p.qr(x, 45.0, QR_LADO, &reg.qr_url);

    let tx = x + QR_LADO + 12.0;
    p.texto(tx, 57.0, 7.5, true, 0.25, "Registro local encadenado");
    p.texto(
        tx,
        69.0,
        6.6,
        false,
        0.35,
        "No remitido a la AEAT.",
    );
    p.texto(
        tx,
        79.0,
        6.6,
        false,
        0.35,
        "QR de cotejo, no verificable.",
    );
    let mut h = reg.huella.clone();
    h.truncate(24);
    p.texto(
        tx,
        95.0,
        6.2,
        false,
        0.55,
        &format!("Huella: {}...", h),
    );
}

fn pie(p: &mut Pagina, doc: &Documento, empresa: &Empresa, pagina: usize, total: usize) {
    let y_fin = ALTO - 34.0;
    p.linea(MARGEN, y_fin - 26.0, DERECHA, y_fin - 26.0, 0.3, 0.85);

    let y = y_fin - 16.0;

    if doc.tipo == crate::model::TipoDoc::Proforma {
        p.texto(
            MARGEN,
            y - 30.0,
            8.0,
            true,
            0.45,
            "PROFORMA — no constituye factura ni tiene validez fiscal.",
        );
    } else if doc.tipo == crate::model::TipoDoc::Presupuesto {
        p.texto(
            MARGEN,
            y - 30.0,
            8.0,
            true,
            0.45,
            "PRESUPUESTO — no constituye factura ni tiene validez fiscal.",
        );
    }

    if !empresa.iban.trim().is_empty() {
        p.texto_der(
            DERECHA,
            y - 8.0,
            7.5,
            false,
            0.35,
            &format!("IBAN {}", empresa.iban),
        );
    }
    if !doc.forma_pago.trim().is_empty() {
        p.texto_der(DERECHA, y - 19.0, 7.5, false, 0.35, &doc.forma_pago);
    }
    p.texto_der(
        DERECHA,
        y - 30.0,
        7.5,
        false,
        0.55,
        &format!("Página {} de {}", pagina, total),
    );
}

/// Formatea una deducción con su signo: en un recibo de diferencias una
/// deducción negativa devuelve dinero y se muestra con «+».
fn deduccion(cents: i64, diferencia: bool) -> String {
    let signo = if diferencia && cents < 0 { "+" } else { "-" };
    format!("{signo}{}", fmt_cents_eur(cents.saturating_abs()))
}

/// Resumen de nómina sencillo. Es un documento de trabajo, no el modelo
/// oficial de nómina ni el boletín de cotización.
pub fn nomina(n: &Nomina, e: &Empleado, empresa: &Empresa) -> Vec<u8> {
    let mut p = Pagina::nueva();
    p.rect(0.0, 0.0, ANCHO, 6.0, 0.10);

    // Marca de borrador en la esquina superior derecha. Va en el margen,
    // separada del título, para que se vea incluso al imprimir en blanco y
    // negro y no se confunda con un recibo oficial.
    p.rect(DERECHA - 150.0, 28.0, 150.0, 16.0, 0.18);
    let marca = if n.tipo.diferencia() {
        if n.cerrada() {
            "DIFERENCIAS · CERRADA"
        } else {
            "DIFERENCIAS · BORRADOR"
        }
    } else if n.cerrada() {
        "CERRADA · NO OFICIAL"
    } else {
        "BORRADOR · NO OFICIAL"
    };
    p.texto_der(DERECHA - 6.0, 39.0, 8.5, true, 1.0, marca);

    let mut y = MARGEN + 14.0;
    p.texto(
        MARGEN,
        y,
        15.0,
        true,
        0.10,
        &if empresa.nombre.trim().is_empty() {
            "TU EMPRESA".to_string()
        } else {
            empresa.nombre.to_uppercase()
        },
    );
    p.texto_der(
        DERECHA,
        y,
        16.0,
        true,
        0.10,
        if n.tipo.diferencia() {
            "RECIBO DE DIFERENCIAS"
        } else {
            "RESUMEN DE NÓMINA"
        },
    );
    y += 16.0;
    if !empresa.nif.trim().is_empty() {
        p.texto(MARGEN, y, 9.0, false, 0.35, &format!("NIF {}", empresa.nif));
    }
    let periodo = if n.dias_mes > 0 {
        format!(
            "Periodo {:02}/{} · {}/{} días",
            n.mes, n.anyo, n.dias_trabajados, n.dias_mes
        )
    } else {
        format!("Periodo {:02}/{}", n.mes, n.anyo)
    };
    p.texto_der(DERECHA, y, 9.0, false, 0.35, &periodo);
    if !n.estado.nombre().is_empty() {
        p.texto(
            DERECHA - 150.0,
            y + 12.0,
            7.5,
            false,
            0.45,
            &format!("Estado: {}", n.estado.nombre()),
        );
    }
    y += 18.0;
    p.linea(MARGEN, y, DERECHA, y, 0.5, 0.55);

    y += 18.0;
    p.texto(MARGEN, y, 10.0, true, 0.15, "TRABAJADOR");
    y += 14.0;
    let nombre = if n.empleado_nombre.trim().is_empty() {
        e.nombre_completo()
    } else {
        n.empleado_nombre.clone()
    };
    p.texto(MARGEN, y, 10.0, true, 0.10, &nombre);
    y += 13.0;
    let mut datos = Vec::new();
    if !n.empleado_nif.trim().is_empty() {
        datos.push(format!("NIF {}", n.empleado_nif));
    } else if !e.nif.trim().is_empty() {
        datos.push(format!("NIF {}", e.nif));
    }
    if !n.empleado_naf.trim().is_empty() {
        datos.push(format!("NAF {}", n.empleado_naf));
    } else if !e.naf.trim().is_empty() {
        datos.push(format!("NAF {}", e.naf));
    }
    if !n.puesto.trim().is_empty() {
        datos.push(n.puesto.clone());
    } else if !e.puesto.trim().is_empty() {
        datos.push(e.puesto.clone());
    }
    if !datos.is_empty() {
        p.texto(MARGEN, y, 8.5, false, 0.35, &datos.join(" · "));
        y += 12.0;
    }
    p.texto(
        MARGEN,
        y,
        8.5,
        false,
        0.35,
        &format!(
            "Contrato: {}",
            n.tipo_contrato.unwrap_or(e.tipo_contrato).etiqueta()
        ),
    );

    if n.tipo.diferencia() {
        y += 14.0;
        p.texto(
            MARGEN,
            y,
            8.5,
            false,
            0.35,
            &format!(
                "Diferencia de la nómina #{}. {}",
                n.nomina_origen
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                n.motivo_diferencia
            ),
        );
    }

    y += 24.0;
    p.texto(MARGEN, y, 10.0, true, 0.15, "DEVENGOS");
    p.texto_der(DERECHA, y, 9.0, true, 0.15, "IMPORTE");
    y += 12.0;
    p.linea(MARGEN, y, DERECHA, y, 0.3, 0.75);
    y += 13.0;
    let conceptos = &n.conceptos;
    let con_conceptos = n.tipo == crate::empleados::TipoNomina::Ordinaria && !conceptos.vacia();
    p.texto(
        MARGEN,
        y,
        9.0,
        false,
        0.25,
        if n.tipo.diferencia() {
            "Diferencia de bruto"
        } else if con_conceptos {
            "Salario base del periodo"
        } else {
            "Salario bruto del periodo"
        },
    );
    p.texto_der(
        DERECHA,
        y,
        9.0,
        false,
        0.25,
        &fmt_cents_eur(if n.tipo.diferencia() {
            n.bruto_cents
        } else {
            n.salario_base_efectivo()
        }),
    );
    if con_conceptos {
        let filas = [
            ("Pagas extras", conceptos.pagas_extras_cents),
            ("Horas extra", conceptos.horas_extra_cents),
            ("Variables y complementos", conceptos.variables_cents),
            (
                "Vacaciones / compensación",
                conceptos.vacaciones_cents,
            ),
            ("Descuento por baja", -conceptos.bajas_cents),
        ];
        for (etiqueta, importe) in filas {
            if importe == 0 {
                continue;
            }
            y += 12.0;
            p.texto(MARGEN, y, 9.0, false, 0.25, etiqueta);
            p.texto_der(DERECHA, y, 9.0, false, 0.25, &fmt_cents_eur(importe));
        }
        if conceptos.vacaciones_dias > 0 || conceptos.bajas_dias > 0 {
            y += 12.0;
            p.texto(
                MARGEN,
                y,
                8.0,
                false,
                0.55,
                &format!(
                    "Días informativos: vacaciones {} · baja {}",
                    conceptos.vacaciones_dias, conceptos.bajas_dias
                ),
            );
        }
        y += 12.0;
        p.texto(MARGEN, y, 9.0, true, 0.20, "Bruto total del periodo");
        p.texto_der(DERECHA, y, 9.0, true, 0.20, &fmt_cents_eur(n.bruto_cents));
    }
    if n.base_cotizacion_cents > 0 {
        y += 12.0;
        p.texto(MARGEN, y, 9.0, false, 0.25, "Base de cotización (SS)");
        p.texto_der(
            DERECHA,
            y,
            9.0,
            false,
            0.25,
            &fmt_cents_eur(n.base_cotizacion_cents),
        );
    }

    y += 24.0;
    p.texto(MARGEN, y, 10.0, true, 0.15, "DEDUCCIONES");
    p.texto_der(DERECHA, y, 9.0, true, 0.15, "IMPORTE");
    y += 12.0;
    p.linea(MARGEN, y, DERECHA, y, 0.3, 0.75);
    y += 13.0;
    p.texto(MARGEN, y, 9.0, false, 0.25, "Seguridad Social a cargo del trabajador");
    p.texto_der(
        DERECHA,
        y,
        9.0,
        false,
        0.25,
        &deduccion(n.ss_empleado_cents, n.tipo.diferencia()),
    );
    y += 12.0;
    let etiqueta_irpf = if n.tipo.diferencia() {
        "Retención IRPF (diferencia)".to_string()
    } else {
        format!(
            "Retención IRPF {} %",
            fmt_pb(n.irpf_pb.unwrap_or(e.irpf_pb))
        )
    };
    p.texto(MARGEN, y, 9.0, false, 0.25, &etiqueta_irpf);
    p.texto_der(
        DERECHA,
        y,
        9.0,
        false,
        0.25,
        &deduccion(n.irpf_cents, n.tipo.diferencia()),
    );
    if !n.tipo.diferencia() && conceptos.deducciones_cents != 0 {
        y += 12.0;
        p.texto(MARGEN, y, 9.0, false, 0.25, "Otras deducciones");
        p.texto_der(
            DERECHA,
            y,
            9.0,
            false,
            0.25,
            &fmt_cents_eur(-conceptos.deducciones_cents),
        );
    }

    y += 24.0;
    p.linea(MARGEN, y, DERECHA, y, 0.6, 0.35);
    y += 10.0;
    p.texto(MARGEN, y, 12.0, true, 0.10, "NETO A PERCIBIR");
    p.texto_der(DERECHA, y, 13.0, true, 0.10, &fmt_cents_eur(n.neto_cents));

    y += 28.0;
    p.texto(MARGEN, y, 9.0, true, 0.20, "COSTE PARA LA EMPRESA");
    y += 13.0;
    p.texto(MARGEN, y, 9.0, false, 0.30, "Seguridad Social a cargo de la empresa");
    p.texto_der(DERECHA, y, 9.0, false, 0.30, &fmt_cents_eur(n.ss_empresa_cents));
    y += 13.0;
    p.texto(MARGEN, y, 9.0, true, 0.20, "Coste total");
    p.texto_der(DERECHA, y, 9.0, true, 0.20, &fmt_cents_eur(n.coste_empresa_cents));

    p.texto(
        MARGEN,
        ALTO - 76.0,
        7.0,
        false,
        0.55,
        "Resumen orientativo generado por AutónomoFACIL. No sustituye el recibo oficial de salarios ni el boletín de cotización.",
    );
    if let Some(paquete) = &n.paquete {
        p.texto(
            MARGEN,
            ALTO - 88.0,
            7.0,
            false,
            0.55,
            &format!(
                "Parámetros: paquete anual {} · {} · AT/EP estimado: {}.",
                paquete.version,
                paquete.fuente,
                if paquete.at_ep_estimado { "sí" } else { "no" }
            ),
        );
    }
    p.texto(
        MARGEN,
        ALTO - 64.0,
        7.0,
        false,
        0.55,
        "SS sobre la base de cotización; IRPF sobre el bruto. Grupos 1-7: base mensual (divisor 30 si el mes es incompleto).",
    );
    p.texto(
        MARGEN,
        ALTO - 52.0,
        7.0,
        false,
        0.55,
        "Grupos 8-11: base diaria si el mes es incompleto; en mes completo, equivalencia de 30 días y tope mensual. No aplica bonificaciones.",
    );
    p.texto_der(
        DERECHA,
        ALTO - 34.0,
        7.0,
        false,
        0.55,
        "Página 1 de 1",
    );
    ensamblar(vec![p.ops], None)
}

/// Genera el PDF completo de una factura o presupuesto.
pub fn documento(doc: &Documento, empresa: &Empresa) -> Vec<u8> {
    let titulo = if doc.es_rectificativa() {
        "FACTURA RECTIFICATIVA"
    } else {
        match doc.tipo {
            crate::model::TipoDoc::Factura => "FACTURA",
            crate::model::TipoDoc::Presupuesto => "PRESUPUESTO",
            crate::model::TipoDoc::Proforma => "FACTURA PROFORMA",
        }
    };

    let vacio: &[Linea] = &[];
    // La primera página reserva sitio para el QR de cabecera; las siguientes
    // mantienen el ritmo de 19 líneas.
    let primera = if doc.verifactu.is_some() { 14 } else { 19 };
    let grupos: Vec<&[Linea]> = if doc.lineas.is_empty() {
        vec![vacio]
    } else {
        let corte = doc.lineas.len().min(primera);
        let mut v = vec![&doc.lineas[..corte]];
        v.extend(doc.lineas[corte..].chunks(19));
        v
    };
    let total_paginas = grupos.len();

    let mut paginas: Vec<String> = Vec::with_capacity(total_paginas);

    let logo = crate::logo::cargar(&empresa.logo).ok();

    for (i, grupo) in grupos.iter().enumerate() {
        let mut p = Pagina::nueva();
        let con_qr = i == 0 && doc.verifactu.is_some();
        let desplazamiento = if con_qr { QR_LADO + 22.0 } else { 0.0 };
        if con_qr {
            if let Some(reg) = &doc.verifactu {
                qr_tributario(&mut p, reg);
            }
        }
        cabecera(
            &mut p,
            doc,
            empresa,
            titulo,
            logo.as_ref(),
            desplazamiento,
        );

        let mut y = MARGEN + 74.0 + desplazamiento;
        if i == 0 {
            y = bloque_cliente(&mut p, doc, y);
            if let Some(r) = &doc.rectifica {
                y += 12.0;
                p.texto(
                    MARGEN,
                    y,
                    8.0,
                    false,
                    0.4,
                    &format!(
                        "Rectifica la factura {} del {}.",
                        if r.etiqueta.is_empty() {
                            r.num_serie.as_str()
                        } else {
                            r.etiqueta.as_str()
                        },
                        r.fecha.format("%d/%m/%Y")
                    ),
                );
                y += 10.0;
                if !doc.motivo_rectificacion.trim().is_empty() {
                    p.texto(
                        MARGEN,
                        y,
                        8.0,
                        false,
                        0.4,
                        &format!("Motivo: {}", doc.motivo_rectificacion),
                    );
                    y += 10.0;
                }
            }
            y += 18.0;
        } else {
            p.texto(MARGEN, y, 9.0, true, 0.45, "CONTINUACIÓN");
            y += 18.0;
        }

        y = cabecera_tabla(&mut p, y);
        for l in grupo.iter() {
            y = fila(&mut p, l, y);
        }

        // Totales y notas sólo en la última página.
        if i + 1 == total_paginas {
            y += 10.0;
            if !doc.notas.trim().is_empty() {
                let mut ny = y;
                p.texto(MARGEN, ny, 8.0, true, 0.45, "NOTAS");
                ny += 12.0;
                for linea_nota in doc.notas.lines().take(8) {
                    p.texto(MARGEN, ny, 8.0, false, 0.30, linea_nota);
                    ny += 10.0;
                }
            }
            bloque_totales(&mut p, doc, y);
            if !empresa.condiciones.trim().is_empty() {
                p.texto(
                    MARGEN,
                    ALTO - 74.0,
                    7.0,
                    false,
                    0.55,
                    &empresa.condiciones.chars().take(120).collect::<String>(),
                );
            }
        }

        pie(&mut p, doc, empresa, i + 1, total_paginas);
        paginas.push(p.ops);
    }

    ensamblar(paginas, logo.as_ref())
}

/// Texto apto para un nombre de fichero en Windows: sin acentos, sin
/// caracteres prohibidos y con guiones en vez de espacios.
pub(crate) fn slug(texto: &str) -> String {
    let mut salida = String::with_capacity(texto.len());
    let mut guion = false;
    for c in texto.chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
            'ñ' | 'Ñ' => 'n',
            'ç' | 'Ç' => 'c',
            otro => otro,
        };
        if c.is_ascii_alphanumeric() {
            salida.push(c.to_ascii_lowercase());
            guion = false;
        } else if !guion && !salida.is_empty() {
            salida.push('-');
            guion = true;
        }
    }
    while salida.ends_with('-') {
        salida.pop();
    }
    salida.truncate(40);
    while salida.ends_with('-') {
        salida.pop();
    }
    salida
}

/// PDF de un modelo fiscal con las casillas ya calculadas. Es un borrador
/// para revisar y teclear en la Sede, no un documento oficial de la AEAT.
pub fn modelo(m: &crate::modelos::Modelo, empresa: &Empresa) -> Vec<u8> {
    let mut filas: Vec<(bool, String, String, String)> = Vec::new();
    for s in &m.secciones {
        filas.push((true, String::new(), s.titulo.to_string(), String::new()));
        for c in &s.casillas {
            filas.push((
                false,
                c.numero.to_string(),
                c.etiqueta.to_string(),
                c.valor.texto(),
            ));
        }
    }

    let por_pagina = 24usize;
    let paginas_casillas = if filas.is_empty() {
        1
    } else {
        (filas.len() + por_pagina - 1) / por_pagina
    };
    let por_pagina_detalle = 24usize;
    let paginas_detalle = if m.detalle.is_empty() {
        0
    } else {
        (m.detalle.len() + por_pagina_detalle - 1) / por_pagina_detalle
    };
    let total = paginas_casillas + paginas_detalle;
    let mut paginas = Vec::with_capacity(total);

    for i in 0..total {
        let mut p = Pagina::nueva();

        p.texto(
            MARGEN,
            MARGEN + 8.0,
            16.0,
            true,
            0.1,
            &format!("MODELO {}", m.codigo),
        );
        p.texto_der(DERECHA, MARGEN + 8.0, 11.0, true, 0.15, &m.periodo);
        p.texto(MARGEN, MARGEN + 26.0, 9.5, false, 0.30, m.titulo);
        p.linea(MARGEN, MARGEN + 34.0, DERECHA, MARGEN + 34.0, 1.0, 0.15);
        // El titular en una frase: se entiende sin mirar las casillas.
        for (n, linea) in envolver(&m.resumen(), 96).iter().take(2).enumerate() {
            p.texto(
                MARGEN,
                MARGEN + 44.0 + n as f32 * 11.0,
                11.0,
                true,
                0.10,
                linea,
            );
        }

        p.texto(MARGEN, MARGEN + 72.0, 7.5, true, 0.45, "DECLARANTE");
        p.texto(
            MARGEN,
            MARGEN + 84.0,
            9.5,
            false,
            0.15,
            &format!("{} · {}", empresa.nombre, empresa.nif),
        );
        p.texto_der(DERECHA, MARGEN + 84.0, 7.5, true, 0.45, "BORRADOR");

        if i < paginas_casillas {
            let mut y = MARGEN + 104.0;
            p.texto(MARGEN, y, 7.0, true, 0.50, "CASILLA");
            p.texto(MARGEN + 48.0, y, 7.0, true, 0.50, "CONCEPTO");
            p.texto_der(DERECHA, y, 7.0, true, 0.50, "IMPORTE");
            y += 5.0;
            p.linea(MARGEN, y, DERECHA, y, 0.6, 0.55);
            y += 16.0;

            for (es_seccion, num, etq, valor) in filas.iter().skip(i * por_pagina).take(por_pagina) {
                if *es_seccion {
                    p.rect(MARGEN, y - 10.0, DERECHA - MARGEN, 16.0, 0.95);
                    p.texto(MARGEN + 2.0, y, 8.5, true, 0.20, etq);
                    y += 20.0;
                } else {
                    p.caja(MARGEN, y - 10.0, 38.0, 13.0, 0.5, 0.60);
                    p.texto(MARGEN + 4.0, y, 8.0, true, 0.25, num);
                    let recorte: String = etq.chars().take(70).collect();
                    p.texto(MARGEN + 48.0, y, 8.5, false, 0.20, &recorte);
                    p.texto_der(DERECHA, y, 8.5, true, 0.10, valor);
                    y += 16.0;
                }
            }
        } else {
            let pagina_detalle = i - paginas_casillas;
            let total_nominas: usize = m.detalle.iter().map(|f| f.nominas).sum();
            let mut y = MARGEN + 104.0;
            p.texto(
                MARGEN,
                y,
                11.0,
                true,
                0.10,
                if m.codigo == "190" {
                    "DETALLE ANUAL POR PERCEPTOR"
                } else {
                    "DETALLE DE NÓMINAS DEL TRIMESTRE"
                },
            );
            y += 18.0;
            p.texto(MARGEN, y, 7.0, true, 0.50, "NIF");
            p.texto(MARGEN + 92.0, y, 7.0, true, 0.50, "PERCEPTOR");
            p.texto_der(MARGEN + 360.0, y, 7.0, true, 0.50, "NÓMINAS");
            p.texto_der(DERECHA - 60.0, y, 7.0, true, 0.50, "BASE");
            p.texto_der(DERECHA, y, 7.0, true, 0.50, "RETENCIÓN");
            y += 5.0;
            p.linea(MARGEN, y, DERECHA, y, 0.6, 0.55);
            y += 15.0;

            let desde = pagina_detalle * por_pagina_detalle;
            for (n, f) in m.detalle.iter().skip(desde).take(por_pagina_detalle).enumerate() {
                if n % 2 == 1 {
                    p.rect(MARGEN, y - 10.0, DERECHA - MARGEN, 15.0, 0.97);
                }
                p.texto(MARGEN, y, 8.5, false, 0.20, &f.nif.chars().take(16).collect::<String>());
                p.texto(MARGEN + 92.0, y, 8.5, false, 0.20, &f.nombre.chars().take(38).collect::<String>());
                p.texto_der(MARGEN + 360.0, y, 8.5, false, 0.20, &f.nominas.to_string());
                p.texto_der(DERECHA - 60.0, y, 8.5, false, 0.20, &fmt_cents_eur(f.bruto_cents));
                p.texto_der(DERECHA, y, 8.5, true, 0.10, &fmt_cents_eur(f.irpf_cents));
                y += 15.0;
            }
            if pagina_detalle + 1 == paginas_detalle {
                y += 6.0;
                p.linea(MARGEN, y, DERECHA, y, 0.6, 0.40);
                y += 12.0;
                p.texto(MARGEN, y, 8.5, true, 0.20, "TOTAL");
                p.texto_der(MARGEN + 360.0, y, 8.5, true, 0.20, &total_nominas.to_string());
                p.texto_der(DERECHA - 60.0, y, 8.5, true, 0.20, &fmt_cents_eur(m.importe("02")));
                p.texto_der(DERECHA, y, 8.5, true, 0.20, &fmt_cents_eur(m.importe("03")));
            }
            p.texto(
                MARGEN,
                ALTO - 110.0,
                7.0,
                false,
                0.55,
                "El detalle es informativo: no es una casilla oficial. Comprueba NIF y que cada perceptor coincida con sus nóminas.",
            );
        }

        let pie_y = ALTO - 82.0;
        p.linea(MARGEN, pie_y - 10.0, DERECHA, pie_y - 10.0, 0.4, 0.75);
        for (n, linea) in envolver(m.nota, 104).iter().take(3).enumerate() {
            p.texto(MARGEN, pie_y + n as f32 * 9.5, 7.0, false, 0.45, linea);
        }
        p.texto_der(
            DERECHA,
            ALTO - 40.0,
            7.0,
            false,
            0.55,
            &format!("AutónomoFACIL · página {} de {}", i + 1, total),
        );

        paginas.push(p.ops);
    }

    ensamblar(paginas, None)
}

/// Parte un texto largo en líneas de como mucho `ancho` caracteres.
fn envolver(texto: &str, ancho: usize) -> Vec<String> {
    let mut lineas = Vec::new();
    let mut actual = String::new();
    for palabra in texto.split_whitespace() {
        if !actual.is_empty() && actual.chars().count() + 1 + palabra.chars().count() > ancho {
            lineas.push(std::mem::take(&mut actual));
        }
        if !actual.is_empty() {
            actual.push(' ');
        }
        actual.push_str(palabra);
    }
    if !actual.is_empty() {
        lineas.push(actual);
    }
    lineas
}

/// Nombre que dice qué es el fichero sin abrirlo:
/// `Factura-2026-0042-agencia-creativa-sl-2026-11-10.pdf`.
pub fn nombre_fichero(doc: &Documento) -> String {
    let tipo = if doc.es_rectificativa() {
        "Rectificativa"
    } else {
        match doc.tipo {
            crate::model::TipoDoc::Factura => "Factura",
            crate::model::TipoDoc::Presupuesto => "Presupuesto",
            crate::model::TipoDoc::Proforma => "Proforma",
        }
    };
    let numero = if doc.numero == 0 {
        format!("borrador-{}", doc.id)
    } else {
        format!("{:04}", doc.numero)
    };
    let serie = slug(&doc.serie);
    let serie = if serie.is_empty() {
        "sin-serie".to_string()
    } else {
        serie
    };
    let cliente = slug(&doc.cliente.nombre_completo());
    let mut nombre = format!("{}-{}-{}", tipo, serie, numero);
    if !cliente.is_empty() {
        nombre.push('-');
        nombre.push_str(&cliente);
    }
    nombre.push('-');
    nombre.push_str(&doc.fecha.format("%Y-%m-%d").to_string());
    nombre.push_str(".pdf");
    nombre
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use crate::model::{Cliente, Linea, Rectificacion, TipoDoc};

    fn doc() -> (Documento, Empresa) {
        let empresa = Empresa {
            nombre: "Estudio Ñandú S.L.".into(),
            nif: "B12345678".into(),
            direccion: "Calle Mayor 1".into(),
            cp: "28013".into(),
            ciudad: "Madrid".into(),
            provincia: "Madrid".into(),
            iban: "ES9121000418450200051332".into(),
            ..Default::default()
        };
        let cliente = Cliente {
            id: 1,
            nombre: "Agencia Creativa S.L.".into(),
            nif: "B76281996".into(),
            ..Default::default()
        };
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente, 0, "2026".into());
        d.numero = 42;
        d.lineas = (1..=25)
            .map(|i| Linea {
                descripcion: format!("Servicio de diseño número {} con acentos: ñáéíóú", i),
                cantidad_milli: 1500,
                precio_cents: 12_345,
                descuento_pb: 500,
                iva_pb: 2100,
            })
            .collect();
        (d, empresa)
    }

    #[test]
    fn genera_pdf_con_estructura_valida() {
        let (d, e) = doc();
        let bytes = documento(&d, &e);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let texto = String::from_utf8_lossy(&bytes);
        // 25 líneas a 19 por página => 2 páginas.
        assert!(texto.contains("/Count 2"));
        // El texto va en WinAnsi octal: á = 0xE1 -> \341
        assert!(texto.contains(r"P\341gina 2 de 2"));
        // El xref debe apuntar a objetos reales.
        assert!(texto.contains("/Type/Catalog"));
        assert!(texto.contains("startxref"));
    }

    #[test]
    fn la_r1_se_titula_y_cita_la_factura_original() {
        let (mut d, e) = doc();
        d.rectifica = Some(Rectificacion {
            documento_id: 7,
            num_serie: d.num_serie_aeat(),
            etiqueta: d.etiqueta_numero(),
            fecha: d.fecha,
            tipo: "I".to_string(),
        });
        d.motivo_rectificacion = "Descuento no aplicado".to_string();
        let texto = String::from_utf8_lossy(&documento(&d, &e)).to_string();
        assert!(texto.contains("FACTURA RECTIFICATIVA"));
        assert!(texto.contains("Rectifica la factura 2026/0042 del"));
        assert!(texto.contains("Motivo: Descuento no aplicado"));
    }

    #[test]
    fn los_acentos_sobreviven_al_pdf() {
        let (d, e) = doc();
        let bytes = documento(&d, &e);
        let texto = String::from_utf8_lossy(&bytes);
        // WinAnsi en octal: é = 0xE9 -> \351, ñ = 0xF1 -> \361
        assert!(texto.contains("\\351"));
        assert!(texto.contains("\\361"));
        // El flujo de contenido debe ser ASCII puro para que /Length cuadre.
        assert!(bytes.iter().all(|b| *b == b'\n' || (32..127).contains(b) || *b == b'\r' || *b == 0xE2 || *b == 0xE3 || *b == 0xCF || *b == 0xD3));
    }

    #[test]
    fn el_pdf_de_un_modelo_lleva_las_casillas() {
        let db = crate::store::Db::default();
        let m = crate::modelos::iva_303(&db, 2026, 1);
        let bytes = modelo(&m, &Empresa::default());
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1500);
        let texto = String::from_utf8_lossy(&bytes);
        assert!(
            texto.contains("Sale a"),
            "el PDF tiene que llevar el resumen en una frase"
        );
        assert!(texto.contains("MODELO 303"));
        assert!(texto.contains("BORRADOR"));
        assert!(
            texto.contains("/Resources<</Font<</F1 3 0 R/F2 4 0 R>>>>/Contents"),
            "las fuentes deben cerrar Resources antes de Contents"
        );
    }

    #[test]
    fn el_logo_se_embebe_y_se_dibuja() {
        // JPEG mínimo: cabecera JFIF y SOF0 que declara 120x40.
        let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x07];
        jpg.extend_from_slice(b"JFIF\0");
        jpg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x28, 0x00, 0x78]);
        jpg.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        let ruta = std::env::temp_dir().join("autonomofacil-logo-prueba.jpg");
        std::fs::write(&ruta, &jpg).expect("escribir el jpeg de prueba");

        let (d, mut e) = doc();
        e.logo = ruta.display().to_string();
        let con = String::from_utf8_lossy(&documento(&d, &e)).to_string();
        assert!(con.contains("/Subtype/Image"), "el logo va como XObject");
        assert!(con.contains("/DCTDecode"), "el JPEG se copia sin recodificar");
        assert!(con.contains("/Im1 Do"), "y se dibuja en la cabecera");
        assert!(con.contains("/XObject<</Im1 5 0 R>>"));

        // Sin logo el PDF no lleva imagen y conserva la numeración de antes.
        e.logo.clear();
        let sin = String::from_utf8_lossy(&documento(&d, &e)).to_string();
        assert!(!sin.contains("/Subtype/Image"));
        assert!(sin.contains("5 0 obj"));
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn la_nomina_genera_un_resumen_con_neto_y_coste() {
        let e = Empleado {
            id: 1,
            nombre: "Ana".into(),
            apellidos: "García Ruiz".into(),
            nif: "12345678Z".into(),
            fecha_alta: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        };
        let n = Nomina::calcular(&e, 2, 2026, 1);
        let bytes = nomina(&n, &e, &Empresa::default());
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        assert!(bytes.len() > 1_500);
        let texto = String::from_utf8_lossy(&bytes);
        assert!(texto.contains("RESUMEN DE N"));
        assert!(texto.contains("31/31"));
        assert!(texto.contains("BORRADOR"));
        assert!(texto.contains("Base de cotizaci"));
        assert!(texto.contains("NETO A PERCIBIR"));
        assert!(texto.contains("COSTE PARA LA EMPRESA"));
        assert!(texto.contains("Grupos 1-7"));
        assert!(texto.contains("Grupos 8-11"));
        assert!(texto.contains("No aplica bonificaciones"));
        assert!(texto.contains("paquete anual 2026.1"));
    }

    #[test]
    fn el_pdf_de_nomina_incluye_los_conceptos_del_mes() {
        use crate::empleados::{ConceptosNomina, Empleado, Nomina};
        let e = Empleado {
            id: 3,
            nombre: "Ana".into(),
            apellidos: "García".into(),
            nif: "12345678Z".into(),
            fecha_alta: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        };
        let n = Nomina::calcular_con_conceptos(
            &e,
            3,
            2026,
            1,
            &ConceptosNomina {
                pagas_extras_cents: 10_000,
                deducciones_cents: 2_500,
                ..Default::default()
            },
        );
        let texto = String::from_utf8_lossy(&nomina(&n, &e, &Empresa::default())).to_string();
        assert!(texto.contains("Salario base del periodo"));
        assert!(texto.contains("Pagas extras"));
        assert!(texto.contains("Bruto total del periodo"));
        assert!(texto.contains("Otras deducciones"));
    }

    #[test]
    fn el_pdf_de_retenciones_incluye_el_detalle_por_perceptor() {
        use crate::empleados::{Empleado, Nomina};
        let e = Empleado {
            id: 3,
            nombre: "Ana".into(),
            apellidos: "García".into(),
            nif: "12345678Z".into(),
            fecha_alta: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        };
        let mut db = crate::store::Db::default();
        db.empleados.push(e.clone());
        db.nominas.push(Nomina::calcular(&e, 1, 2026, 1));
        db.nominas.push(Nomina::calcular(&e, 2, 2026, 2));

        let trimestre = crate::modelos::retenciones_111(&db, 2026, 1);
        let bytes = modelo(&trimestre, &Empresa::default());
        assert!(bytes.starts_with(b"%PDF"));
        let texto = String::from_utf8_lossy(&bytes);
        assert!(texto.contains("DETALLE"));
        assert!(texto.contains("12345678Z"));
        assert!(texto.contains("/Count 2"));

        let anual = crate::modelos::resumen_190(&db, 2026);
        let texto = String::from_utf8_lossy(&modelo(&anual, &Empresa::default())).to_string();
        assert!(texto.contains("DETALLE ANUAL POR PERCEPTOR"));
        assert!(texto.contains("12345678Z"));
    }

    #[test]
    fn el_qr_solo_aparece_en_la_primera_pagina() {
        let (mut d, e) = doc();
        d.verifactu = Some(crate::verifactu::registro_alta(&d, "B12345678", ""));
        let texto = String::from_utf8_lossy(&documento(&d, &e)).to_string();
        assert_eq!(
            texto.matches("QR tributario:").count(),
            1,
            "el QR tributario debe figurar una sola vez"
        );
        assert!(
            texto.contains("/Count 2"),
            "25 líneas deben seguir ocupando dos páginas"
        );
    }

    #[test]
    fn el_qr_aparece_cuando_hay_registro() {
        let (mut d, e) = doc();
        d.verifactu = Some(crate::verifactu::registro_alta(&d, "B12345678", ""));
        let con_bytes = documento(&d, &e);
        let con_texto = String::from_utf8_lossy(&con_bytes);
        assert!(
            con_texto.contains("QR tributario:"),
            "el QR debe llevar la etiqueta obligatoria"
        );
        assert!(
            !con_texto.contains("Factura verificable"),
            "un registro local no debe anunciarse como verificable"
        );
        let con = con_bytes.len();
        d.verifactu = None;
        let sin = documento(&d, &e).len();
        assert!(con > sin, "el QR debe añadir contenido al PDF");
    }
}
