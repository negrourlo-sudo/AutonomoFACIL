//! Modelo de dominio.
//!
//! Todo el dinero se maneja en **céntimos como `i64`** y los porcentajes en
//! **puntos base** (`i32`, 2100 = 21,00 %). Nunca `f64`: los totales de una
//! factura tienen que cuadrar al céntimo con el papel.

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Aritmética monetaria
// ---------------------------------------------------------------------------

/// División entera con redondeo al más cercano, alejándose de cero en el .5.
/// Es el criterio habitual en facturación: nunca se pierde ni se gana un
/// céntimo por truncar.
pub fn div_round(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }
    let negativo = (a < 0) != (b < 0);
    let a = a.abs();
    let b = b.abs();
    let q = (a + b / 2) / b;
    if negativo {
        -q
    } else {
        q
    }
}

/// `123456` -> `"1.234,56"`
pub fn fmt_cents(c: i64) -> String {
    let negativo = c < 0;
    let v = c.abs();
    let euros = v / 100;
    let cents = v % 100;
    let digitos = euros.to_string();
    let mut miles = String::with_capacity(digitos.len() + 4);
    for (i, ch) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i) % 3 == 0 {
            miles.push('.');
        }
        miles.push(ch);
    }
    format!("{}{},{:02}", if negativo { "-" } else { "" }, miles, cents)
}

/// `123456` -> `"1.234,56 EUR"` para el PDF.
pub fn fmt_cents_eur(c: i64) -> String {
    format!("{} EUR", fmt_cents(c))
}

/// `1500` -> `"1,5"` (cantidad en milésimas, sin ceros de relleno).
pub fn fmt_qty(m: i64) -> String {
    let negativo = m < 0;
    let v = m.abs();
    let ent = v / 1000;
    let frac = v % 1000;
    let signo = if negativo { "-" } else { "" };
    if frac == 0 {
        return format!("{}{}", signo, ent);
    }
    let mut f = format!("{:03}", frac);
    while f.ends_with('0') {
        f.pop();
    }
    format!("{}{},{}", signo, ent, f)
}

/// `2100` -> `"21"`, `2150` -> `"21,5"`
pub fn fmt_pb(pb: i32) -> String {
    let ent = pb / 100;
    let frac = (pb % 100).abs();
    if frac == 0 {
        format!("{}", ent)
    } else {
        let mut f = format!("{:02}", frac);
        while f.ends_with('0') {
            f.pop();
        }
        format!("{},{}", ent, f)
    }
}

/// `"21,5"` o `"21.5"` o `"21"` -> puntos base. Tolerante porque lo teclea una
/// persona.
pub fn parse_pb(s: &str) -> Option<i32> {
    let limpio = s.trim().replace(',', ".").replace('%', "");
    let limpio = limpio.trim();
    if limpio.is_empty() {
        return Some(0);
    }
    let v: f64 = limpio.parse().ok()?;
    if !v.is_finite() || v < 0.0 || v > 1000.0 {
        return None;
    }
    Some((v * 100.0).round() as i32)
}

/// `"1200"`, `"1.200,50"`, `"1200.5"` -> céntimos. Acepta separadores
/// españoles e ingleses; si hay coma Y punto, la coma es decimal.
pub fn parse_cents(s: &str) -> Option<i64> {
    let bruto = s.trim().replace(' ', "").replace('€', "");
    if bruto.is_empty() {
        return Some(0);
    }
    let normalizado = if bruto.contains(',') {
        bruto.replace('.', "").replace(',', ".")
    } else {
        bruto
    };
    let v: f64 = normalizado.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some((v * 100.0).round() as i64)
}

/// `"1,5"` -> milésimas (1,5 -> 1.500).
pub fn parse_qty(s: &str) -> Option<i64> {
    parse_cents(s).map(|c| c * 10)
}

// ---------------------------------------------------------------------------
// Tipos
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoDoc {
    Presupuesto,
    /// Presupuesto con datos fiscales y aspecto de factura, pero sin validez
    /// fiscal: no lleva registro Verifactu ni se exporta a FacturaE.
    Proforma,
    Factura,
}

impl TipoDoc {
    pub fn nombre(&self) -> &'static str {
        match self {
            TipoDoc::Presupuesto => "Presupuesto",
            TipoDoc::Proforma => "Factura proforma",
            TipoDoc::Factura => "Factura",
        }
    }

    /// Sólo la factura es documento fiscal. De esto dependen el registro
    /// encadenado, el QR y la exportación FacturaE.
    pub fn es_fiscal(&self) -> bool {
        matches!(self, TipoDoc::Factura)
    }

    /// Serie por defecto: facturas el año, presupuestos "P"+año, proformas
    /// "PRO"+año.
    pub fn serie_defecto(&self) -> String {
        let y = Local::now().format("%Y").to_string();
        match self {
            TipoDoc::Factura => y,
            TipoDoc::Presupuesto => format!("P{}", y),
            TipoDoc::Proforma => format!("PRO{}", y),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Estado {
    /// Editable. Sin número asignado todavía.
    #[default]
    Borrador,
    /// Emitido: número asignado, registro Verifactu generado, inmutable.
    Emitido,
    Pagado,
    /// Anulado: se conserva para no romper la cadena ni la numeración.
    Anulado,
}

impl Estado {
    pub fn nombre(&self) -> &'static str {
        match self {
            Estado::Borrador => "Borrador",
            Estado::Emitido => "Emitido",
            Estado::Pagado => "Pagado",
            Estado::Anulado => "Anulado",
        }
    }
    pub fn cobrado(&self) -> bool {
        matches!(self, Estado::Pagado)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Cliente {
    pub id: u64,
    pub nombre: String,
    /// NIF/DNI/NIE sin prefijo de país. Vacío = particular sin NIF
    /// (factura simplificada, tipo F2 en Verifactu / FA en FacturaE).
    pub nif: String,
    pub persona_fisica: bool,
    /// Apellidos, sólo para persona física (FacturaE los exige separados).
    pub apellidos: String,
    pub direccion: String,
    pub cp: String,
    pub ciudad: String,
    pub provincia: String,
    pub email: String,
    pub telefono: String,
    pub notas: String,
}

impl Cliente {
    pub fn etiqueta(&self) -> String {
        if self.nombre.is_empty() {
            "(sin nombre)".to_string()
        } else if self.nif.is_empty() {
            self.nombre.clone()
        } else {
            format!("{} ({})", self.nombre, self.nif)
        }
    }
    pub fn nombre_completo(&self) -> String {
        if self.persona_fisica && !self.apellidos.is_empty() {
            format!("{} {}", self.nombre, self.apellidos)
        } else {
            self.nombre.clone()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Empresa {
    pub nombre: String,
    pub nif: String,
    /// Titular autónomo: FacturaE usa `Individual` en vez de `LegalEntity`.
    pub persona_fisica: bool,
    pub apellidos: String,
    pub direccion: String,
    pub cp: String,
    pub ciudad: String,
    pub provincia: String,
    pub email: String,
    pub telefono: String,
    pub iban: String,
    pub serie: String,
    pub iva_defecto: i32,
    pub irpf_defecto: i32,
    pub condiciones: String,
    /// Si está activo, al emitir se genera el registro encadenado + QR.
    pub verifactu_activo: bool,
    /// Días de vencimiento que se proponen al crear una factura.
    #[serde(default = "dias_vencimiento_defecto")]
    pub dias_vencimiento: i32,
    /// Forma de pago que se propone por defecto.
    #[serde(default = "forma_pago_defecto")]
    pub forma_pago_defecto: String,
    /// A qué se dedica el negocio.
    #[serde(default)]
    pub actividad: Actividad,
}

fn dias_vencimiento_defecto() -> i32 {
    30
}

fn forma_pago_defecto() -> String {
    "Transferencia".to_string()
}

/// A qué se dedica el negocio. Decide qué modelos se listan, qué IVA se
/// propone y si los clientes retienen IRPF.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Actividad {
    #[default]
    Servicios,
    Comercio,
    Digital,
    Formacion,
    Alquiler,
    Otra,
}

impl Actividad {
    pub const TODAS: [Actividad; 6] = [
        Actividad::Servicios,
        Actividad::Comercio,
        Actividad::Digital,
        Actividad::Formacion,
        Actividad::Alquiler,
        Actividad::Otra,
    ];

    pub fn etiqueta(self) -> &'static str {
        match self {
            Actividad::Servicios => "Servicios profesionales / freelance",
            Actividad::Comercio => "Comercio o venta de productos",
            Actividad::Digital => "Venta digital (Steam, apps, suscripciones)",
            Actividad::Formacion => "Formación o enseñanza",
            Actividad::Alquiler => "Alquiler de inmuebles",
            Actividad::Otra => "Otra actividad",
        }
    }

    /// IVA que se propone al cambiar a esta actividad.
    pub fn iva_pct(self) -> i32 {
        match self {
            Actividad::Formacion | Actividad::Alquiler => 0,
            _ => 21,
        }
    }

    /// Retención de IRPF que aplican los clientes empresas.
    pub fn retencion_pct(self) -> i32 {
        match self {
            Actividad::Servicios => 15,
            _ => 0,
        }
    }
}

/// Dónde se localiza la operación a efectos de IVA. Decide si la factura
/// lleva IVA, la leyenda obligatoria y en qué modelo informativo entra.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TipoOperacion {
    #[default]
    Nacional,
    /// Empresa de otro país de la UE con NIF-IVA: inversión del sujeto pasivo.
    Intracomunitaria,
    /// Cliente de fuera de la UE: exportación de servicios.
    Exportacion,
    /// Exenta por la naturaleza de la actividad (formación reglada, vivienda…).
    Exenta,
}

impl TipoOperacion {
    pub const TODAS: [TipoOperacion; 4] = [
        TipoOperacion::Nacional,
        TipoOperacion::Intracomunitaria,
        TipoOperacion::Exportacion,
        TipoOperacion::Exenta,
    ];

    pub fn etiqueta(self) -> &'static str {
        match self {
            TipoOperacion::Nacional => "Nacional (con IVA)",
            TipoOperacion::Intracomunitaria => "Empresa de la UE (sin IVA)",
            TipoOperacion::Exportacion => "Fuera de la UE (exportación)",
            TipoOperacion::Exenta => "Exenta de IVA",
        }
    }

    /// Sólo la operación nacional repercute IVA español.
    pub fn lleva_iva(self) -> bool {
        matches!(self, TipoOperacion::Nacional)
    }

    /// Texto que debe figurar en la factura.
    pub fn leyenda(self) -> &'static str {
        match self {
            TipoOperacion::Nacional => "",
            TipoOperacion::Intracomunitaria => {
                "Operación no sujeta a IVA español (inversión del sujeto pasivo)."
            }
            TipoOperacion::Exportacion => {
                "Operación exenta de IVA: exportación de servicios (art. 21 LIVA)."
            }
            TipoOperacion::Exenta => "Operación exenta de IVA (art. 20 LIVA).",
        }
    }
}

impl Default for Empresa {
    fn default() -> Self {
        let y = Local::now().format("%Y").to_string();
        Self {
            nombre: String::new(),
            nif: String::new(),
            persona_fisica: false,
            apellidos: String::new(),
            direccion: String::new(),
            cp: String::new(),
            ciudad: String::new(),
            provincia: String::new(),
            email: String::new(),
            telefono: String::new(),
            iban: String::new(),
            serie: y,
            iva_defecto: 2100,
            irpf_defecto: 0,
            condiciones: "Transferencia a 30 días desde la fecha de factura.".to_string(),
            verifactu_activo: true,
            dias_vencimiento: 30,
            forma_pago_defecto: "Transferencia".to_string(),
            actividad: Actividad::Servicios,
        }
    }
}

impl Empresa {
    pub fn completa(&self) -> bool {
        !self.nombre.trim().is_empty() && !self.nif.trim().is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Articulo {
    pub id: u64,
    pub descripcion: String,
    pub precio_cents: i64,
    pub iva_pb: i32,
    pub unidad: String,
}

/// Gasto del negocio: factura recibida o compra con IVA deducible.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Gasto {
    pub id: u64,
    pub fecha: NaiveDate,
    pub proveedor: String,
    pub nif: String,
    pub concepto: String,
    /// Base imponible en céntimos (sin IVA).
    pub base_cents: i64,
    pub iva_pb: i32,
    /// Retención de IRPF que le practican al autónomo (0 o 15 normalmente).
    #[serde(default)]
    pub irpf_pb: i32,
    #[serde(default)]
    pub pagado: bool,
    pub categoria: String,
}

impl Default for Gasto {
    fn default() -> Self {
        Self {
            id: 0,
            fecha: Local::now().date_naive(),
            proveedor: String::new(),
            nif: String::new(),
            concepto: String::new(),
            base_cents: 0,
            iva_pb: 2100,
            irpf_pb: 0,
            pagado: false,
            categoria: String::new(),
        }
    }
}

impl Gasto {
    pub fn cuota_iva(&self) -> i64 {
        div_round(self.base_cents * self.iva_pb as i64, 10_000)
    }

    pub fn retencion(&self) -> i64 {
        div_round(self.base_cents * self.irpf_pb as i64, 10_000)
    }

    pub fn total(&self) -> i64 {
        self.base_cents + self.cuota_iva() - self.retencion()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Linea {
    pub descripcion: String,
    /// Cantidad en milésimas: permite 1,250 h y 0,5 ud sin perder precisión.
    pub cantidad_milli: i64,
    pub precio_cents: i64,
    pub descuento_pb: i32,
    pub iva_pb: i32,
}

impl Default for Linea {
    fn default() -> Self {
        Self {
            descripcion: String::new(),
            cantidad_milli: 1000,
            precio_cents: 0,
            descuento_pb: 0,
            iva_pb: 2100,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LineaCalc {
    pub bruto: i64,
    pub descuento: i64,
    pub importe: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Totales {
    /// (tipo %, base, cuota), ordenado por tipo.
    pub bases: Vec<(i32, i64, i64)>,
    pub base_total: i64,
    pub cuota_iva: i64,
    pub retencion: i64,
    pub total: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RegistroVerifactu {
    /// "A" alta, "N" anulación.
    pub tipo_registro: String,
    pub nif: String,
    /// Serie + número concatenados, sin separador (formato AEAT).
    pub num_serie: String,
    /// dd-mm-yyyy
    pub fecha_expedicion: String,
    /// F1 completa, F2 simplificada, R1 rectificativa.
    pub tipo_factura: String,
    pub cuota_total: i64,
    pub importe_total: i64,
    pub huella: String,
    /// Vacío en el primer registro de la cadena ("Primero").
    pub huella_anterior: String,
    /// ISO-8601 con huso: 2026-11-10T09:19:00+01:00
    pub fecha_hora_gen: String,
    pub qr_url: String,
    pub encadenamiento: String,
}

/// Cobro recibido de una factura. Puede haber varios y parciales: el saldo
/// pendiente es el total menos la suma de pagos.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pago {
    pub fecha: NaiveDate,
    pub importe_cents: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Documento {
    pub id: u64,
    pub tipo: TipoDoc,
    pub serie: String,
    /// 0 mientras es borrador.
    pub numero: u64,
    pub fecha: NaiveDate,
    pub vencimiento: NaiveDate,
    pub cliente_id: u64,
    /// Copia congelada del cliente en el momento de emitir: la factura no
    /// puede cambiar porque después se edite la ficha del cliente.
    pub cliente: Cliente,
    pub lineas: Vec<Linea>,
    pub irpf_pb: i32,
    pub estado: Estado,
    pub notas: String,
    #[serde(default)]
    pub forma_pago: String,
    #[serde(default)]
    pub motivo_anulacion: String,
    #[serde(default)]
    pub verifactu: Option<RegistroVerifactu>,
    /// Ruta del último PDF generado (informativo).
    #[serde(default)]
    pub pdf: String,
    /// Cobros recibidos. `serde(default)` para datos anteriores.
    #[serde(default)]
    pub pagos: Vec<Pago>,
    /// Localización a efectos de IVA. Los datos antiguos son nacionales.
    #[serde(default)]
    pub operacion: TipoOperacion,
}

impl Documento {
    pub fn nuevo(tipo: TipoDoc, cliente: Cliente, irpf_pb: i32, serie: String) -> Self {
        let hoy = Local::now().date_naive();
        Self {
            id: 0,
            tipo,
            serie,
            numero: 0,
            fecha: hoy,
            vencimiento: hoy + Duration::days(30),
            cliente_id: cliente.id,
            cliente,
            lineas: Vec::new(),
            irpf_pb,
            estado: Estado::Borrador,
            notas: String::new(),
            forma_pago: "Transferencia".to_string(),
            motivo_anulacion: String::new(),
            verifactu: None,
            pdf: String::new(),
            pagos: Vec::new(),
            operacion: TipoOperacion::Nacional,
        }
    }

    pub fn editable(&self) -> bool {
        matches!(self.estado, Estado::Borrador)
    }

    /// Rellena lo que se puede deducir sin preguntar: fecha de hoy, vencimiento
    /// según los días configurados, forma de pago, IRPF y una primera línea con
    /// el IVA habitual. El usuario sólo tiene que poner el concepto.
    pub fn aplicar_defectos(&mut self, empresa: &Empresa) {
        let hoy = Local::now().date_naive();
        self.fecha = hoy;
        self.vencimiento =
            hoy + Duration::days(empresa.dias_vencimiento.clamp(0, 365) as i64);
        if self.forma_pago.trim().is_empty() {
            self.forma_pago = empresa.forma_pago_defecto.clone();
        }
        // Presupuestos y proformas no llevan retención por defecto: sólo la
        // factura retiene IRPF cuando el negocio lo tiene configurado.
        self.irpf_pb = if self.tipo.es_fiscal() {
            empresa.irpf_defecto
        } else {
            0
        };
        if self.lineas.is_empty() {
            self.lineas.push(Linea {
                iva_pb: empresa.iva_defecto,
                ..Default::default()
            });
        }
    }

    /// Total cobrado hasta ahora.
    pub fn cobrado_cents(&self) -> i64 {
        self.pagos.iter().map(|p| p.importe_cents).sum()
    }

    /// Vencida: emitida, sin cobrar del todo y con el vencimiento pasado.
    pub fn vencida(&self) -> bool {
        self.pendiente_cents() > 0
            && matches!(self.estado, Estado::Emitido)
            && self.vencimiento < Local::now().date_naive()
    }

    /// Lo que falta por cobrar. Cero si no es una factura emitida, si está
    /// anulada o si ya figura como pagada.
    pub fn pendiente_cents(&self) -> i64 {
        if !self.tipo.es_fiscal() || self.numero == 0 {
            return 0;
        }
        if matches!(self.estado, Estado::Anulado | Estado::Pagado) {
            return 0;
        }
        (self.totales().total - self.cobrado_cents()).max(0)
    }

    pub fn etiqueta_numero(&self) -> String {
        if self.numero == 0 {
            "— borrador —".to_string()
        } else {
            format!("{}/{:04}", self.serie, self.numero)
        }
    }

    /// Serie+número sin separador, como lo espera la AEAT.
    pub fn num_serie_aeat(&self) -> String {
        format!("{}{:04}", self.serie, self.numero)
    }

    pub fn calc_linea(l: &Linea) -> LineaCalc {
        let bruto = div_round(l.precio_cents * l.cantidad_milli, 1000);
        let descuento = div_round(bruto * l.descuento_pb as i64, 10_000);
        LineaCalc {
            bruto,
            descuento,
            importe: bruto - descuento,
        }
    }

    /// Se agrupa por tipo de IVA y se redondea **una vez por grupo**, que es
    /// como el desglose de FacturaE tiene que cuadrar.
    pub fn totales(&self) -> Totales {
        let mut por_tipo: BTreeMap<i32, i64> = BTreeMap::new();
        let mut base_total = 0i64;
        for l in &self.lineas {
            let c = Self::calc_linea(l);
            base_total += c.importe;
            *por_tipo.entry(l.iva_pb).or_insert(0) += c.importe;
        }
        let mut bases = Vec::with_capacity(por_tipo.len());
        let mut cuota_iva = 0i64;
        for (tipo, base) in por_tipo {
            // Exportaciones, operaciones no sujetas por localización y exentas
            // no llevan IVA repercutido aunque la línea tenga un tipo escrito.
            let cuota = if self.operacion.lleva_iva() {
                div_round(base * tipo as i64, 10_000)
            } else {
                0
            };
            cuota_iva += cuota;
            bases.push((tipo, base, cuota));
        }
        let retencion = div_round(base_total * self.irpf_pb as i64, 10_000);
        Totales {
            bases,
            base_total,
            cuota_iva,
            retencion,
            total: base_total + cuota_iva - retencion,
        }
    }

    pub fn es_simplificada(&self) -> bool {
        self.cliente.nif.trim().is_empty()
    }

    /// Código de tipo de factura para Verifactu.
    pub fn tipo_factura_aeat(&self) -> &'static str {
        if self.es_simplificada() {
            "F2"
        } else {
            "F1"
        }
    }
}
