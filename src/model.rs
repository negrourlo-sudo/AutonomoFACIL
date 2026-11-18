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

/// Deja el NIF como lo espera la AEAT: mayúsculas y sin espacios, guiones ni
/// puntos. Así la huella y el QR no cambian por cómo se haya tecleado.
pub fn nif_normalizado(nif: &str) -> String {
    nif.trim().to_uppercase().replace([' ', '-', '.'], "")
}

/// Comprueba el NIF español: DNI (8 dígitos + letra), NIE (X/Y/Z + 7 dígitos
/// + letra) y CIF (letra + 7 dígitos + control). Se aceptan K, L y M, que
/// siguen la regla del DNI. Devuelve `false` si la letra de control no cuadra,
/// que es el error que de verdad rechaza la AEAT.
pub fn nif_valido(nif: &str) -> bool {
    let n = nif_normalizado(nif);
    let b = n.as_bytes();
    if b.len() != 9 {
        return false;
    }
    const LETRAS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";
    const LETRAS_CIF: &[u8] = b"JABCDEFGHI";
    let digito = |c: u8| c.is_ascii_digit();

    let suma_cif = |b: &[u8]| -> u32 {
        let mut suma = 0u32;
        for (i, c) in b[1..8].iter().enumerate() {
            let d = (c - b'0') as u32;
            if i % 2 == 0 {
                let doble = d * 2;
                suma += doble / 10 + doble % 10;
            } else {
                suma += d;
            }
        }
        (10 - (suma % 10)) % 10
    };

    match b[0] {
        b'0'..=b'9' => {
            b[..8].iter().all(|c| digito(*c))
                && b[8].is_ascii_alphabetic()
                && LETRAS[(n[..8].parse::<u32>().unwrap_or(0) % 23) as usize] == b[8]
        }
        b'X' | b'Y' | b'Z' | b'K' | b'L' | b'M' => {
            let pref = match b[0] {
                b'X' => 0,
                b'Y' => 1,
                b'Z' => 2,
                _ => 0,
            };
            if !b[1..8].iter().all(|c| digito(*c)) || !b[8].is_ascii_alphabetic() {
                return false;
            }
            let mut numero = pref.to_string();
            numero.push_str(&n[1..8]);
            LETRAS[(numero.parse::<u32>().unwrap_or(0) % 23) as usize] == b[8]
        }
        _ => {
            if !b[0].is_ascii_alphabetic() || !b[1..8].iter().all(|c| digito(*c)) {
                return false;
            }
            let control = suma_cif(b);
            if b[8].is_ascii_digit() {
                control == (b[8] - b'0') as u32
            } else {
                b[8].is_ascii_alphabetic() && LETRAS_CIF[control as usize] == b[8]
            }
        }
    }
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

/// `"21,5"` o `"21.5"` o `"21"` -> puntos base. Acepta de 0 a 100 %,
/// que es el rango de IVA, IRPF, recargo y descuento que se teclea en la app.
/// Tolerante con coma y con el signo `%` porque lo escribe una persona.
pub fn parse_pb(s: &str) -> Option<i32> {
    let limpio = s.trim().replace(',', ".").replace('%', "");
    let limpio = limpio.trim();
    if limpio.is_empty() {
        return Some(0);
    }
    let v: f64 = limpio.parse().ok()?;
    if !v.is_finite() || v < 0.0 || v > 100.0 {
        return None;
    }
    Some((v * 100.0).round() as i32)
}

/// Convierte un decimal escrito por una persona a la escala indicada.
/// Acepta separadores españoles e ingleses; si hay coma Y punto, la coma es
/// decimal.
fn parse_decimal(s: &str, escala: f64) -> Option<i64> {
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
    Some((v * escala).round() as i64)
}

/// `"1200"`, `"1.200,50"`, `"1200.5"` -> céntimos.
pub fn parse_cents(s: &str) -> Option<i64> {
    parse_decimal(s, 100.0)
}

/// `"1,5"` -> milésimas (1,5 -> 1.500). Conserva hasta tres decimales:
/// `"1,255"` -> 1.255, sin redondear antes a céntimos.
pub fn parse_qty(s: &str) -> Option<i64> {
    parse_decimal(s, 1000.0)
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

/// Tipo de cliente a efectos de IVA e IRPF. Al elegirlo en una factura se
/// proponen el IVA, la retención y el recargo que le corresponden.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RegimenCliente {
    #[default]
    General,
    Profesional,
    Minorista,
    Intracomunitario,
    InversionSujetoPasivo,
}

impl RegimenCliente {
    pub const TODOS: [RegimenCliente; 5] = [
        RegimenCliente::General,
        RegimenCliente::Profesional,
        RegimenCliente::Minorista,
        RegimenCliente::Intracomunitario,
        RegimenCliente::InversionSujetoPasivo,
    ];

    pub fn etiqueta(self) -> &'static str {
        match self {
            RegimenCliente::General => "Empresa o particular · régimen general",
            RegimenCliente::Profesional => "Autónomo profesional · con retención",
            RegimenCliente::Minorista => "Comercio minorista · recargo de equivalencia",
            RegimenCliente::Intracomunitario => "Empresa de la UE · intracomunitaria (0 %)",
            RegimenCliente::InversionSujetoPasivo => "Inversión del sujeto pasivo (ISP)",
        }
    }

    /// IVA que se propone al elegir este régimen.
    pub fn iva_pb(self) -> i32 {
        match self {
            RegimenCliente::Intracomunitario | RegimenCliente::InversionSujetoPasivo => 0,
            _ => 2100,
        }
    }

    /// Retención de IRPF que aplica el cliente (sólo profesionales).
    pub fn irpf_pb(self) -> i32 {
        match self {
            RegimenCliente::Profesional => 1500,
            _ => 0,
        }
    }

    /// Recargo de equivalencia que hay que repercutir al minorista.
    pub fn recargo_pb(self) -> i32 {
        match self {
            RegimenCliente::Minorista => 520,
            _ => 0,
        }
    }

    pub fn operacion(self) -> TipoOperacion {
        match self {
            RegimenCliente::Intracomunitario => TipoOperacion::Intracomunitaria,
            _ => TipoOperacion::Nacional,
        }
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
    /// Régimen a efectos de IVA/IRPF: decide retención, recargo y operación.
    #[serde(default)]
    pub regimen: RegimenCliente,
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

/// Cómo tributa el IRPF de la actividad. Cambia el pago fraccionado: 130 en
/// estimación directa y 131 en estimación objetiva (módulos).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RegimenIrpf {
    #[default]
    DirectaNormal,
    DirectaSimplificada,
    Objetiva,
}

impl RegimenIrpf {
    pub const TODOS: [RegimenIrpf; 3] = [
        RegimenIrpf::DirectaNormal,
        RegimenIrpf::DirectaSimplificada,
        RegimenIrpf::Objetiva,
    ];

    pub fn etiqueta(self) -> &'static str {
        match self {
            RegimenIrpf::DirectaNormal => "Estimación directa (normal)",
            RegimenIrpf::DirectaSimplificada => "Estimación directa simplificada",
            RegimenIrpf::Objetiva => "Estimación objetiva (módulos)",
        }
    }

    pub fn es_objetiva(self) -> bool {
        matches!(self, RegimenIrpf::Objetiva)
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
    /// Ruta al logo JPEG que se imprime en facturas, presupuestos y proformas.
    #[serde(default)]
    pub logo: String,
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
    /// Cómo tributa el IRPF: decide si toca el 130 o el 131.
    #[serde(default)]
    pub regimen_irpf: RegimenIrpf,
    /// Comerciante minorista en recargo de equivalencia: sin 303 ni 390.
    #[serde(default)]
    pub recargo_equivalencia: bool,
    /// Obligado al Suministro Inmediato de Información: 303 mensual y sin 390.
    #[serde(default)]
    pub sii: bool,
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
            // Lo habitual es un autónomo; el asistente lo cambia si es sociedad.
            persona_fisica: true,
            apellidos: String::new(),
            direccion: String::new(),
            cp: String::new(),
            ciudad: String::new(),
            provincia: String::new(),
            email: String::new(),
            telefono: String::new(),
            iban: String::new(),
            logo: String::new(),
            serie: y,
            iva_defecto: 2100,
            irpf_defecto: 0,
            condiciones: "Transferencia a 30 días desde la fecha de factura.".to_string(),
            verifactu_activo: true,
            dias_vencimiento: 30,
            forma_pago_defecto: "Transferencia".to_string(),
            regimen_irpf: RegimenIrpf::DirectaNormal,
            recargo_equivalencia: false,
            sii: false,
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

/// Categoría que identifica un pago bancario de nóminas ya incluido en el
/// coste de personal.
pub const CATEGORIA_PAGO_PERSONAL: &str = "Nóminas";

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
    /// Pago bancario de una nómina ya incluida en el coste de personal. Se
    /// excluye de los gastos para no contarlo dos veces en el 130.
    #[serde(default)]
    pub pago_personal: bool,
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
            pago_personal: false,
        }
    }
}

impl Gasto {
    /// ¿Es el pago bancario de una nómina, ya contabilizado como coste de
    /// personal? Se reconoce por la marca explícita o por una categoría
    /// habitual, para que los datos ya guardados no se dupliquen.
    pub fn es_pago_personal(&self) -> bool {
        let categoria = self.categoria.trim().to_lowercase();
        self.pago_personal
            || matches!(
                categoria.as_str(),
                "nóminas" | "nominas" | "sueldos" | "salarios"
            )
    }

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
    /// Recargo de equivalencia repercutido (minoristas).
    pub cuota_recargo: i64,
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
    /// En una R1: serie+número de la factura rectificada.
    #[serde(default)]
    pub facturas_rectificadas: Vec<String>,
    /// "I" por diferencias o "S" sustitutiva.
    #[serde(default)]
    pub tipo_rectificativa: String,
    /// Versión del formato de huella. Los datos anteriores a la corrección de
    /// la anulación no tienen este campo y valen 0; los nuevos valen 2.
    #[serde(default)]
    pub version: u8,
    /// Desempate local para dos registros generados en el mismo segundo. No
    /// forma parte de la huella ni del XML oficial; sólo ordena la cadena.
    #[serde(default)]
    pub orden: u64,
}

/// Cobro recibido de una factura. Puede haber varios y parciales: el saldo
/// pendiente es el total menos la suma de pagos.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pago {
    pub fecha: NaiveDate,
    pub importe_cents: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rectificacion {
    /// Factura original que se corrige. No se anula: se referencia aquí.
    pub documento_id: u64,
    /// Serie + número de la original tal y como lo espera la AEAT.
    pub num_serie: String,
    /// La misma referencia pero legible: «2026/0014».
    #[serde(default)]
    pub etiqueta: String,
    pub fecha: NaiveDate,
    /// "I" por diferencias; "S" sustitutiva (reservado).
    pub tipo: String,
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
    /// Registro de anulación (tipo N). Va aparte para no borrar el alta: la
    /// cadena necesita conservar los dos, como en la norma.
    #[serde(default)]
    pub verifactu_anulacion: Option<RegistroVerifactu>,
    /// Si es una factura rectificativa R1, a qué factura corrige y por qué.
    #[serde(default)]
    pub rectifica: Option<Rectificacion>,
    #[serde(default)]
    pub motivo_rectificacion: String,
    /// En la factura original: id de la última R1 que la corrige.
    #[serde(default)]
    pub rectificada_por: Option<u64>,
    /// Ruta del último PDF generado (informativo).
    #[serde(default)]
    pub pdf: String,
    /// Cobros recibidos. `serde(default)` para datos anteriores.
    #[serde(default)]
    pub pagos: Vec<Pago>,
    /// Localización a efectos de IVA. Los datos antiguos son nacionales.
    #[serde(default)]
    pub operacion: TipoOperacion,
    /// Recargo de equivalencia en puntos base (520 = 5,2 %). 0 en el resto.
    #[serde(default)]
    pub recargo_pb: i32,
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
            verifactu_anulacion: None,
            rectifica: None,
            motivo_rectificacion: String::new(),
            rectificada_por: None,
            pdf: String::new(),
            pagos: Vec::new(),
            operacion: TipoOperacion::Nacional,
            recargo_pb: 0,
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

    /// Ajusta IVA, retención, recargo y tipo de operación al régimen del
    /// cliente. Se llama al elegirlo en el editor y al crear el borrador.
    pub fn aplicar_cliente(&mut self) {
        if !self.tipo.es_fiscal() {
            return;
        }
        self.irpf_pb = self.cliente.regimen.irpf_pb();
        self.recargo_pb = self.cliente.regimen.recargo_pb();
        self.operacion = self.cliente.regimen.operacion();
        let iva = self.cliente.regimen.iva_pb();
        if iva == 0 {
            for l in &mut self.lineas {
                l.iva_pb = 0;
            }
        } else if self.lineas.iter().all(|l| l.iva_pb == 0) {
            for l in &mut self.lineas {
                l.iva_pb = iva;
            }
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
        let cuota_recargo = if self.operacion.lleva_iva() {
            div_round(base_total * self.recargo_pb as i64, 10_000)
        } else {
            0
        };
        Totales {
            bases,
            base_total,
            cuota_iva,
            cuota_recargo,
            retencion,
            total: base_total + cuota_iva + cuota_recargo - retencion,
        }
    }

    pub fn es_simplificada(&self) -> bool {
        self.cliente.nif.trim().is_empty()
    }

    pub fn es_rectificativa(&self) -> bool {
        self.rectifica.is_some()
    }

    /// Etiqueta para la ficha y el PDF: una R1 no es una factura normal.
    pub fn etiqueta_tipo(&self) -> &'static str {
        if self.es_rectificativa() {
            "Factura rectificativa R1"
        } else {
            self.tipo.nombre()
        }
    }

    /// Código de tipo de factura para Verifactu.
    pub fn tipo_factura_aeat(&self) -> &'static str {
        if self.es_rectificativa() {
            "R1"
        } else if self.es_simplificada() {
            "F2"
        } else {
            "F1"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_con(regimen: RegimenCliente, iva_pb: i32, base: i64) -> Documento {
        let mut c = Cliente::default();
        c.regimen = regimen;
        let mut d = Documento::nuevo(TipoDoc::Factura, c, 0, "A".to_string());
        d.numero = 1;
        d.estado = Estado::Emitido;
        d.lineas.push(Linea {
            cantidad_milli: 1000,
            precio_cents: base,
            iva_pb,
            ..Default::default()
        });
        d.aplicar_cliente();
        d
    }

    #[test]
    fn la_rectificativa_es_r1_y_admite_diferencias_negativas() {
        let mut d = doc_con(RegimenCliente::General, 2100, 100_000);
        d.rectifica = Some(Rectificacion {
            documento_id: 7,
            num_serie: "20260007".to_string(),
            etiqueta: "2026/0007".to_string(),
            fecha: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
            tipo: "I".to_string(),
        });
        assert_eq!(d.tipo_factura_aeat(), "R1");
        assert_eq!(d.etiqueta_tipo(), "Factura rectificativa R1");
        d.lineas[0].precio_cents = -10_000;
        let t = d.totales();
        assert_eq!(t.base_total, -10_000);
        assert_eq!(t.cuota_iva, -2_100);
        assert_eq!(t.total, -12_100);
    }

    #[test]
    fn el_regimen_del_cliente_fija_iva_retencion_y_recargo() {
        let pro = doc_con(RegimenCliente::Profesional, 2100, 1000_00);
        assert_eq!(pro.irpf_pb, 1500);
        assert_eq!(pro.totales().retencion, 150_00);
        assert_eq!(pro.totales().total, 1060_00);

        let min = doc_con(RegimenCliente::Minorista, 2100, 1000_00);
        assert_eq!(min.recargo_pb, 520);
        assert_eq!(min.totales().cuota_recargo, 52_00);
        assert_eq!(min.totales().total, 1262_00);

        let ue = doc_con(RegimenCliente::Intracomunitario, 2100, 1000_00);
        assert_eq!(ue.operacion, TipoOperacion::Intracomunitaria);
        assert_eq!(ue.totales().cuota_iva, 0);
        assert_eq!(ue.totales().cuota_recargo, 0);
        assert_eq!(ue.totales().total, 1000_00);

        let general = doc_con(RegimenCliente::General, 2100, 1000_00);
        assert_eq!(general.irpf_pb, 0);
        assert_eq!(general.recargo_pb, 0);
        assert_eq!(general.totales().total, 1210_00);
    }

    #[test]
    fn el_gasto_calcula_cuota_retencion_y_total() {
        let g = Gasto {
            base_cents: 10_000,
            iva_pb: 2100,
            irpf_pb: 1500,
            ..Default::default()
        };
        assert_eq!(g.cuota_iva(), 2_100);
        assert_eq!(g.retencion(), 1_500);
        assert_eq!(g.total(), 10_600);

        // Redondeo al céntimo, no truncado.
        let r = Gasto {
            base_cents: 1_234,
            iva_pb: 2100,
            irpf_pb: 0,
            ..Default::default()
        };
        assert_eq!(r.cuota_iva(), 259);
        assert_eq!(r.total(), 1_493);
    }

    #[test]
    fn los_porcentajes_no_pasan_del_100() {
        assert_eq!(parse_pb("21,5"), Some(2150));
        assert_eq!(parse_pb("100"), Some(10_000));
        assert_eq!(parse_pb(""), Some(0));
        assert!(parse_pb("100,01").is_none());
        assert!(parse_pb("1000").is_none());
        assert!(parse_pb("-1").is_none());
    }

    #[test]
    fn las_cantidades_de_tres_decimales_sobreviven_a_ida_y_vuelta() {
        for (texto, milli) in [("1,255", 1_255), ("0,125", 125), ("1,999", 1_999)] {
            assert_eq!(parse_qty(texto), Some(milli), "{texto}");
            assert_eq!(fmt_qty(milli), texto);
            assert_eq!(parse_qty(&fmt_qty(milli)), Some(milli), "{texto}");
        }
    }

    #[test]
    fn el_importe_de_la_linea_usa_los_tres_decimales() {
        for (texto, bruto) in [("1,255", 1_255), ("0,125", 125), ("1,999", 1_999)] {
            let linea = Linea {
                cantidad_milli: parse_qty(texto).unwrap(),
                precio_cents: 1000,
                ..Default::default()
            };
            assert_eq!(Documento::calc_linea(&linea).bruto, bruto, "{texto}");
        }
    }

    #[test]
    fn el_nif_se_valida_con_su_letra_de_control() {
        // DNI
        assert!(nif_valido("12345678Z"));
        assert!(nif_valido("12345678-Z"));
        assert!(nif_valido(" 12345678z "));
        assert!(!nif_valido("12345678A"));
        // NIE
        assert!(nif_valido("X1234567L"));
        assert!(!nif_valido("X1234567A"));
        // CIF
        assert!(nif_valido("B12345674"));
        assert!(!nif_valido("B12345678"));
        // Basura
        assert!(!nif_valido(""));
        assert!(!nif_valido("1234567Z"));
    }
}
