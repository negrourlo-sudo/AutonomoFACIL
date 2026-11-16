//! Los modelos de la AEAT ya rellenados con los datos de la app.
//!
//! La AEAT no tiene una API abierta para presentar: todo pasa por su Sede con
//! certificado electrónico, DNIe o Cl@ve, bien tecleando el formulario, bien
//! subiendo un fichero con el diseño de registro oficial. Por eso lo que se
//! genera aquí son las **casillas calculadas**: el PDF para revisarlas y el
//! JSON para pasárselas a otro programa o a un robot de presentación.
//!
//! Criterio de devengo (fecha de la factura o del gasto), igual que en
//! `contab`. Son borradores: la responsabilidad de presentar es del autónomo.

use crate::contab;
use crate::empleados::{resumen_retenciones, ResumenRetenciones};
use crate::model::*;
use crate::store::Db;
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub enum Valor {
    /// Importe en céntimos.
    Dinero(i64),
    /// Tipo o porcentaje en puntos base (2100 = 21 %).
    Porcentaje(i32),
    /// Cantidad entera.
    Numero(i64),
}

impl Valor {
    pub fn texto(&self) -> String {
        match *self {
            Valor::Dinero(c) => fmt_cents_eur(c),
            Valor::Porcentaje(pb) => fmt_pb(pb),
            Valor::Numero(n) => n.to_string(),
        }
    }

    /// Para el JSON: céntimos, puntos base o entero, según el tipo.
    fn json(&self) -> (i64, &'static str) {
        match *self {
            Valor::Dinero(c) => (c, "cents"),
            Valor::Porcentaje(pb) => (pb as i64, "pb"),
            Valor::Numero(n) => (n, "numero"),
        }
    }
}

pub struct Casilla {
    pub numero: String,
    pub etiqueta: String,
    pub valor: Valor,
}

/// Atajo para las casillas de número fijo.
fn c(numero: &'static str, etiqueta: &str, valor: Valor) -> Casilla {
    Casilla {
        numero: numero.to_string(),
        etiqueta: etiqueta.to_string(),
        valor,
    }
}

pub struct Seccion {
    pub titulo: String,
    pub casillas: Vec<Casilla>,
}

fn seccion(titulo: &str, casillas: Vec<Casilla>) -> Seccion {
    Seccion {
        titulo: titulo.to_string(),
        casillas,
    }
}

#[derive(Clone, Debug, Default)]
pub struct FilaDetalle {
    pub nif: String,
    pub nombre: String,
    pub nominas: usize,
    pub bruto_cents: i64,
    pub irpf_cents: i64,
}

pub struct Modelo {
    pub codigo: &'static str,
    pub titulo: &'static str,
    /// «2026 · 1T» o «2026 · anual».
    pub periodo: String,
    pub secciones: Vec<Seccion>,
    /// Aviso impreso al pie del PDF.
    pub nota: &'static str,
    /// Detalle por perceptor para los modelos de retenciones (111 y 190).
    /// Se imprime en una tabla después de las casillas.
    pub detalle: Vec<FilaDetalle>,
}

impl Modelo {
    /// Importe de una casilla de dinero, para no repetir la suma.
    pub fn importe(&self, numero: &str) -> i64 {
        for s in &self.secciones {
            for c in &s.casillas {
                if c.numero == numero {
                    if let Valor::Dinero(v) = c.valor {
                        return v;
                    }
                }
            }
        }
        0
    }

    /// Número entero de una casilla, para contar perceptores o nóminas.
    pub fn numero(&self, numero: &str) -> i64 {
        for s in &self.secciones {
            for c in &s.casillas {
                if c.numero == numero {
                    if let Valor::Numero(v) = c.valor {
                        return v;
                    }
                }
            }
        }
        0
    }

    /// Porcentaje de una casilla de tipo, en puntos base.
    pub fn tipo(&self, numero: &str) -> i32 {
        for s in &self.secciones {
            for c in &s.casillas {
                if c.numero == numero {
                    if let Valor::Porcentaje(pb) = c.valor {
                        return pb;
                    }
                }
            }
        }
        0
    }

    /// Una frase en castellano llano con lo importante del modelo, para no
    /// tener que leer casillas: «Sale a ingresar 1.234,56 €…».
    pub fn resumen(&self) -> String {
        match self.codigo {
            "303" | "390" => {
                let (c27, c45) = if self.codigo == "303" {
                    (self.importe("27"), self.importe("45"))
                } else {
                    (self.importe("34"), self.importe("64"))
                };
                let resultado = c27 - c45;
                if resultado >= 0 {
                    format!(
                        "Sale a ingresar {}: {} de IVA repercutido menos {} deducible.",
                        fmt_cents_eur(resultado),
                        fmt_cents_eur(c27),
                        fmt_cents_eur(c45)
                    )
                } else {
                    format!(
                        "Sale a compensar o devolver {}: {} de IVA repercutido menos {} deducible.",
                        fmt_cents_eur(-resultado),
                        fmt_cents_eur(c27),
                        fmt_cents_eur(c45)
                    )
                }
            }
            "130" => {
                let beneficio = self.importe("01") - self.importe("02");
                format!(
                    "A pagar a Hacienda {} sobre un beneficio acumulado de {}.",
                    fmt_cents_eur(self.importe("19")),
                    fmt_cents_eur(beneficio)
                )
            }
            "347" => {
                let n: usize = self
                    .secciones
                    .iter()
                    .flat_map(|s| s.casillas.iter())
                    .filter(|c| c.numero != "T")
                    .count();
                format!(
                    "{} terceros pasan de 3.005,06 €, por {} en total.",
                    n,
                    fmt_cents_eur(self.importe("T"))
                )
            }
            "349" => {
                let n: usize = self
                    .secciones
                    .iter()
                    .flat_map(|s| s.casillas.iter())
                    .filter(|c| c.numero != "T")
                    .count();
                format!(
                    "{} clientes de la UE, por {} este trimestre.",
                    n,
                    fmt_cents_eur(self.importe("T"))
                )
            }
            "111" => format!(
                "{} perceptores y {} de retenciones de nóminas en {}.",
                self.numero("01"),
                fmt_cents_eur(self.importe("03")),
                self.periodo
            ),
            "190" => format!(
                "{} nóminas y {} de retenciones de nóminas en {}.",
                self.numero("01"),
                fmt_cents_eur(self.importe("03")),
                self.periodo
            ),
            _ => String::new(),
        }
    }

    /// JSON con las casillas, para integrarlo con otro programa o con un
    /// robot que rellene la Sede.
    pub fn json(&self, empresa: &Empresa) -> String {
        let mut s = String::with_capacity(512);
        s.push_str(&format!(
            "{{\"codigo\":\"{}\",\"titulo\":\"{}\",\"periodo\":\"{}\",\"nif\":\"{}\",\"nombre\":\"{}\",\"casillas\":[",
            esc(self.codigo),
            esc(self.titulo),
            esc(&self.periodo),
            esc(&empresa.nif),
            esc(&empresa.nombre)
        ));
        let mut primero = true;
        for seccion in &self.secciones {
            for c in &seccion.casillas {
                if !primero {
                    s.push(',');
                }
                primero = false;
                let (valor, unidad) = c.valor.json();
                s.push_str(&format!(
                    "{{\"numero\":\"{}\",\"seccion\":\"{}\",\"etiqueta\":\"{}\",\"valor\":{},\"unidad\":\"{}\",\"texto\":\"{}\"}}",
                    esc(&c.numero),
                    esc(&seccion.titulo),
                    esc(&c.etiqueta),
                    valor,
                    unidad,
                    esc(&c.valor.texto())
                ));
            }
        }
        s.push_str("],\"detalle\":[");
        let mut primera_fila = true;
        for f in &self.detalle {
            if !primera_fila {
                s.push(',');
            }
            primera_fila = false;
            s.push_str(&format!(
                "{{\"nif\":\"{}\",\"nombre\":\"{}\",\"nominas\":{},\"bruto\":{},\"irpf\":{}}}",
                esc(&f.nif),
                esc(&f.nombre),
                f.nominas,
                f.bruto_cents,
                f.irpf_cents
            ));
        }
        s.push_str("]}");
        s
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn inicio_trimestre(anyo: i32, tri: u32) -> NaiveDate {
    let mes = ((tri.clamp(1, 4) - 1) * 3) + 1;
    NaiveDate::from_ymd_opt(anyo, mes, 1).unwrap_or_default()
}

fn fin_trimestre(anyo: i32, tri: u32) -> NaiveDate {
    let (mes, dia) = match tri {
        1 => (3, 31),
        2 => (6, 30),
        3 => (9, 30),
        _ => (12, 31),
    };
    NaiveDate::from_ymd_opt(anyo, mes, dia).unwrap_or_default()
}

fn etiqueta_periodo(anyo: i32, tri: u32) -> String {
    if tri == 0 {
        format!("{anyo} · anual")
    } else {
        format!("{anyo} · {tri}T")
    }
}

/// Modelo 303 (IVA trimestral), régimen general.
///
/// Casillas: 01-03 al 4 %, 04-06 al 10 %, 07-09 al 21 %, 150-152 al 0 %,
/// 27 total devengado, 28-29 IVA soportado deducible, 45 total deducible,
/// 46 resultado del régimen general y 61 resultado de la autoliquidación.
///
/// No incluye adquisiciones intracomunitarias (AIB) ni las operaciones
/// exentas o de exportación: ésas van en otras casillas del modelo y se
/// avisa al pie.
pub fn iva_303(db: &Db, anyo: i32, tri: u32) -> Modelo {
    let (ini, fin) = (inicio_trimestre(anyo, tri), fin_trimestre(anyo, tri));

    let mut base4 = 0i64;
    let mut cuota4 = 0i64;
    let mut base10 = 0i64;
    let mut cuota10 = 0i64;
    let mut base21 = 0i64;
    let mut cuota21 = 0i64;
    let mut base0 = 0i64;
    let mut fuera = 0i64;

    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha < ini || d.fecha > fin {
            continue;
        }
        let t = d.totales();
        // Exportación, entrega intracomunitaria o exenta: no devenga IVA
        // español y no se mete en las casillas 01-09.
        if !d.operacion.lleva_iva() {
            fuera += t.base_total;
            continue;
        }
        for (pb, base, cuota) in t.bases {
            match pb {
                400 => {
                    base4 += base;
                    cuota4 += cuota;
                }
                1000 => {
                    base10 += base;
                    cuota10 += cuota;
                }
                2100 => {
                    base21 += base;
                    cuota21 += cuota;
                }
                _ => base0 += base,
            }
        }
    }

    let mut base_sop = 0i64;
    let mut cuota_sop = 0i64;
    for g in &db.gastos {
        if g.fecha < ini || g.fecha > fin || g.es_pago_personal() {
            continue;
        }
        base_sop += g.base_cents;
        cuota_sop += g.cuota_iva();
    }

    let devengado = cuota4 + cuota10 + cuota21;
    let resultado = devengado - cuota_sop;

    let nota = if fuera != 0 {
        "Borrador con criterio de devengo. Las operaciones exentas, de exportación o intracomunitarias no se incluyen en las casillas 01-09: van en sus casillas propias del 303. Revísalo antes de presentar."
    } else {
        "Borrador con criterio de devengo. No incluye adquisiciones intracomunitarias (AIB) ni compensaciones de trimestres anteriores. Revísalo antes de presentar."
    };

    Modelo {
        codigo: "303",
        titulo: "IVA · autoliquidación trimestral (régimen general)",
        periodo: etiqueta_periodo(anyo, tri),
        secciones: vec![
            seccion(
                "IVA devengado",
                vec![
                    c("01", "Base imponible al tipo superreducido (4 %)", Valor::Dinero(base4)),
                    c("02", "Tipo superreducido", Valor::Porcentaje(400)),
                    c("03", "Cuota al 4 %", Valor::Dinero(cuota4)),
                    c("04", "Base imponible al tipo reducido (10 %)", Valor::Dinero(base10)),
                    c("05", "Tipo reducido", Valor::Porcentaje(1000)),
                    c("06", "Cuota al 10 %", Valor::Dinero(cuota10)),
                    c("07", "Base imponible al tipo general (21 %)", Valor::Dinero(base21)),
                    c("08", "Tipo general", Valor::Porcentaje(2100)),
                    c("09", "Cuota al 21 %", Valor::Dinero(cuota21)),
                    c("150", "Base imponible al 0 %", Valor::Dinero(base0)),
                    c("151", "Tipo 0 %", Valor::Porcentaje(0)),
                    c("152", "Cuota al 0 %", Valor::Dinero(0)),
                    c("27", "Total cuota devengada", Valor::Dinero(devengado)),
                ],
            ),
            seccion(
                "IVA deducible",
                vec![
                    c(
                        "28",
                        "Base de las compras y gastos con IVA soportado",
                        Valor::Dinero(base_sop),
                    ),
                    c(
                        "29",
                        "Cuota de IVA soportado deducible",
                        Valor::Dinero(cuota_sop),
                    ),
                    c("45", "Total cuota deducible", Valor::Dinero(cuota_sop)),
                ],
            ),
            seccion(
                "Liquidación",
                vec![
                    c(
                        "46",
                        "Resultado del régimen general (27 − 45)",
                        Valor::Dinero(resultado),
                    ),
                    c(
                        "64",
                        "Suma de resultados (46 + 58 + 76)",
                        Valor::Dinero(resultado),
                    ),
                    c(
                        "69",
                        "Resultado (66 + 77 − 78 + 68 + 108)",
                        Valor::Dinero(resultado),
                    ),
                ],
            ),
        ],
        nota,
        detalle: Vec::new(),
    }
}

/// Modelo 390: resumen anual de IVA, suma de los cuatro 303 del año.
///
/// Se calcula llamando al 303 de cada trimestre, así que cuadra por
/// construcción con lo que se ha presentado. Conserva la numeración del 303
/// (01-09, 150-152, 27, 28-29, 45, 46, 61) para poder comprobarlo casilla a
/// casilla; en el formulario de la Sede el 390 agrupa las mismas magnitudes.
pub fn resumen_390(db: &Db, anyo: i32) -> Modelo {
    let trimestres: Vec<Modelo> = (1..=4).map(|t| iva_303(db, anyo, t)).collect();
    let suma = |num: &str| -> i64 { trimestres.iter().map(|m| m.importe(num)).sum() };

    let base4 = suma("01");
    let base10 = suma("04");
    let base21 = suma("07");
    let cuota4 = suma("03");
    let cuota10 = suma("06");
    let cuota21 = suma("09");
    let devengado = cuota4 + cuota10 + cuota21;
    let base_ded = suma("28");
    let cuota_ded = suma("29");
    let resultado = devengado - cuota_ded;

    Modelo {
        codigo: "390",
        titulo: "IVA · resumen anual",
        periodo: etiqueta_periodo(anyo, 0),
        secciones: vec![
            seccion(
                "IVA devengado del año",
                vec![
                    c("01", "Base imponible al 4 %", Valor::Dinero(base4)),
                    c("02", "Cuota al 4 %", Valor::Dinero(cuota4)),
                    c("03", "Base imponible al 10 %", Valor::Dinero(base10)),
                    c("04", "Cuota al 10 %", Valor::Dinero(cuota10)),
                    c("05", "Base imponible al 21 %", Valor::Dinero(base21)),
                    c("06", "Cuota al 21 %", Valor::Dinero(cuota21)),
                    c(
                        "33",
                        "Total bases imponibles",
                        Valor::Dinero(base4 + base10 + base21),
                    ),
                    c("34", "Total cuotas devengadas", Valor::Dinero(devengado)),
                ],
            ),
            seccion(
                "IVA deducible del año",
                vec![
                    c(
                        "48",
                        "Base de las cuotas soportadas deducibles",
                        Valor::Dinero(base_ded),
                    ),
                    c(
                        "49",
                        "Cuotas soportadas deducibles",
                        Valor::Dinero(cuota_ded),
                    ),
                    c("64", "Suma de deducciones", Valor::Dinero(cuota_ded)),
                ],
            ),
            seccion(
                "Resultado de la liquidación anual",
                vec![
                    c(
                        "65",
                        "Resultado del régimen general (34 − 64)",
                        Valor::Dinero(resultado),
                    ),
                    c("84", "Suma de resultados", Valor::Dinero(resultado)),
                    c(
                        "86",
                        "Resultado de la liquidación",
                        Valor::Dinero(resultado),
                    ),
                ],
            ),
        ],
        nota: "Resumen anual: suma los cuatro 303 del año. No incluye regímenes especiales, prorrata ni AIB, y las operaciones al 0 % no se desglosan aquí. Revísalo antes de presentar.",
        detalle: Vec::new(),
    }
}

/// Modelo 347: operaciones con terceros que superan 3.005,06 € con IVA.
///
/// Reutiliza el cálculo de `contab::modelo_347`, que ya separa clientes y
/// proveedores y excluye las operaciones no nacionales.
pub fn operaciones_347(db: &Db, anyo: i32) -> Modelo {
    let ops = contab::modelo_347(db, anyo);

    let fila = |i: usize, nif: &str, nombre: &str, importe: i64| -> Casilla {
        c(
            "",
            &format!("{nif} · {nombre}"),
            Valor::Dinero(importe),
        )
        .con_numero(format!("{:02}", i + 1))
    };

    let mut secciones = Vec::new();
    for tipo in ["Cliente", "Proveedor"] {
        let del_tipo: Vec<&contab::Operacion347> =
            ops.iter().filter(|o| o.tipo == tipo).collect();
        if del_tipo.is_empty() {
            continue;
        }
        let total: i64 = del_tipo.iter().map(|o| o.importe).sum();
        let mut casillas: Vec<Casilla> = del_tipo
            .iter()
            .enumerate()
            .map(|(i, o)| fila(i, &o.nif, &o.nombre, o.importe))
            .collect();
        casillas.push(
            c("T", "Total del año", Valor::Dinero(total)).con_numero("T".to_string()),
        );
        let titulo = if tipo == "Cliente" {
            "Clientes (importe con IVA)"
        } else {
            "Proveedores (importe con IVA)"
        };
        secciones.push(seccion(titulo, casillas));
    }

    if secciones.is_empty() {
        secciones.push(seccion(
            "Sin operaciones declarables",
            vec![c(
                "—",
                "Ningún cliente ni proveedor supera 3.005,06 € con IVA en el año",
                Valor::Dinero(0),
            )],
        ));
    }

    Modelo {
        codigo: "347",
        titulo: "Operaciones con terceros (anual)",
        periodo: etiqueta_periodo(anyo, 0),
        secciones,
        nota: "Sólo operaciones nacionales que superan 3.005,06 € con IVA en el año, sumando las de todo el año. No incluye alquileres ni operaciones con el extranjero.",
        detalle: Vec::new(),
    }
}

/// Modelo 349: operaciones intracomunitarias del trimestre, por NIF-IVA.
pub fn operaciones_349(db: &Db, anyo: i32, tri: u32) -> Modelo {
    use std::collections::BTreeMap;
    let (ini, fin) = (inicio_trimestre(anyo, tri), fin_trimestre(anyo, tri));

    let mut acc: BTreeMap<String, (String, i64)> = BTreeMap::new();
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha < ini || d.fecha > fin {
            continue;
        }
        if d.operacion != TipoOperacion::Intracomunitaria {
            continue;
        }
        let nif = d.cliente.nif.trim().to_uppercase();
        if nif.is_empty() {
            continue;
        }
        let e = acc
            .entry(nif)
            .or_insert((d.cliente.nombre_completo(), 0));
        e.1 += d.totales().base_total;
    }

    let total: i64 = acc.values().map(|(_, base)| *base).sum();
    let mut casillas: Vec<Casilla> = acc
        .iter()
        .enumerate()
        .map(|(i, (nif, (nombre, base)))| {
            c("", &format!("{nif} · {nombre}"), Valor::Dinero(*base))
                .con_numero(format!("{:02}", i + 1))
        })
        .collect();
    casillas.push(c("T", "Total de entregas intracomunitarias", Valor::Dinero(total)));

    Modelo {
        codigo: "349",
        titulo: "Operaciones intracomunitarias",
        periodo: etiqueta_periodo(anyo, tri),
        secciones: vec![seccion("Entregas de bienes y servicios a la UE", casillas)],
        nota: "Sólo facturas emitidas como intracomunitarias con NIF-IVA del cliente. Comprueba en VIES que el NIF es válido antes de presentar.",
        detalle: Vec::new(),
    }
}

fn detalle_retenciones(r: &ResumenRetenciones) -> Vec<FilaDetalle> {
    r.perceptores
        .iter()
        .map(|p| FilaDetalle {
            nif: if p.nif.trim().is_empty() {
                "(sin NIF)".to_string()
            } else {
                p.nif.clone()
            },
            nombre: p.nombre.clone(),
            nominas: p.nominas,
            bruto_cents: p.bruto_cents,
            irpf_cents: p.irpf_cents,
        })
        .collect()
}

/// Modelo 111 calculado sólo con las nóminas generadas en la app.
///
/// Las casillas 01–03 son las de rendimientos del trabajo dinerarios. El
/// detalle por trabajador es informativo: no es una casilla oficial.
pub fn retenciones_111(db: &Db, anyo: i32, tri: u32) -> Modelo {
    let tri = tri.clamp(1, 4);
    let r = resumen_retenciones(&db.nominas, &db.empleados, anyo, tri);
    let detalle = detalle_retenciones(&r);
    let mut casillas = vec![
        c(
            "01",
            "Número de perceptores con nómina en el trimestre",
            Valor::Numero(r.perceptores.len() as i64),
        ),
        c(
            "02",
            "Importe de las percepciones íntegras",
            Valor::Dinero(r.total_bruto_cents),
        ),
        c(
            "03",
            "Retenciones e ingresos a cuenta",
            Valor::Dinero(r.total_irpf_cents),
        ),
        c(
            "N",
            "Nóminas incluidas en el cálculo (informativo)",
            Valor::Numero(r.num_nominas as i64),
        ),
    ];
    if r.perceptores.is_empty() {
        casillas.push(c(
            "—",
            "No hay nóminas generadas en este trimestre",
            Valor::Dinero(0),
        ));
    }

    Modelo {
        codigo: "111",
        titulo: "Retenciones de nóminas · autoliquidación trimestral",
        periodo: etiqueta_periodo(anyo, tri),
        secciones: vec![seccion("Rendimientos del trabajo", casillas)],
        nota: "Borrador calculado SÓLO con las nóminas generadas. La base del 111 es el bruto de cada nómina, no la base de cotización. No incluye facturas de profesionales (casillas 07-09), retribuciones en especie ni otras rentas. Los recibos de diferencias se suman al periodo en que se registran. Revisa NIF, perceptores y retenciones antes de presentar.",
        detalle,
    }
}

/// Modelo 190 calculado sólo con las nóminas generadas en la app.
///
/// La casilla 01 cuenta nóminas (percepciones), no personas distintas; el
/// detalle anual por trabajador se imprime en una tabla aparte.
pub fn resumen_190(db: &Db, anyo: i32) -> Modelo {
    let r = resumen_retenciones(&db.nominas, &db.empleados, anyo, 0);
    let detalle = detalle_retenciones(&r);
    let mut casillas = vec![
        c(
            "01",
            "Número total de percepciones (nóminas)",
            Valor::Numero(r.num_nominas as i64),
        ),
        c(
            "02",
            "Total de las percepciones íntegras",
            Valor::Dinero(r.total_bruto_cents),
        ),
        c(
            "03",
            "Total retenciones e ingresos a cuenta",
            Valor::Dinero(r.total_irpf_cents),
        ),
        c(
            "P",
            "Personas distintas con nómina (informativo)",
            Valor::Numero(r.perceptores.len() as i64),
        ),
    ];
    if r.perceptores.is_empty() {
        casillas.push(c(
            "—",
            "No hay nóminas generadas en este año",
            Valor::Dinero(0),
        ));
    }

    Modelo {
        codigo: "190",
        titulo: "Retenciones de nóminas · resumen anual",
        periodo: etiqueta_periodo(anyo, 0),
        secciones: vec![seccion("Resumen de la declaración", casillas)],
        nota: "Borrador calculado SÓLO con las nóminas generadas. Usa percepciones íntegras (bruto) e IRPF, no la base de cotización. No incluye facturas de profesionales, retribuciones en especie ni otras rentas; los recibos de diferencias se suman al periodo en que se registran. Cada perceptor debe tener un NIF válido y los totales deben cuadrar con los cuatro 111 del año.",
        detalle,
    }
}

impl Casilla {
    fn con_numero(mut self, numero: String) -> Casilla {
        self.numero = numero;
        self
    }
}

/// Modelo del que sabemos rellenar las casillas, si lo hay.
pub fn previsible(codigo: &str) -> bool {
    matches!(codigo, "111" | "190" | "303" | "130" | "390" | "347" | "349")
}

/// Si el modelo se presenta por trimestres (y por tanto se elige periodo).
pub fn trimestral(codigo: &str) -> bool {
    matches!(codigo, "111" | "303" | "130" | "349")
}

/// El modelo ya rellenado para un periodo. En los anuales `tri` se ignora.
pub fn generar(db: &Db, codigo: &str, anyo: i32, tri: u32) -> Option<Modelo> {
    match codigo {
        "111" => Some(retenciones_111(db, anyo, tri)),
        "190" => Some(resumen_190(db, anyo)),
        "303" => Some(iva_303(db, anyo, tri)),
        "130" => Some(fraccionado_130(db, anyo, tri)),
        "390" => Some(resumen_390(db, anyo)),
        "347" => Some(operaciones_347(db, anyo)),
        "349" => Some(operaciones_349(db, anyo, tri)),
        _ => None,
    }
}

/// Clave con la que se guardan los valores escritos a mano:
/// `303-2026-1T` o `390-2026-anual`.
pub fn clave(codigo: &str, anyo: i32, tri: u32) -> String {
    if tri == 0 {
        format!("{codigo}-{anyo}-anual")
    } else {
        format!("{codigo}-{anyo}-{tri}T")
    }
}

fn poner(m: &mut Modelo, numero: &str, valor: i64) {
    for s in &mut m.secciones {
        for c in &mut s.casillas {
            if c.numero == numero {
                c.valor = Valor::Dinero(valor);
                return;
            }
        }
    }
}

/// Recalcula las casillas de total con lo que haya en ese momento, para que
/// editar una base o una cuota no deje los totales mintiendo.
pub fn recalcular(m: &mut Modelo) {
    match m.codigo {
        "303" => {
            let devengado =
                m.importe("03") + m.importe("06") + m.importe("09") + m.importe("152");
            poner(m, "27", devengado);
            let deducible = m.importe("29");
            poner(m, "45", deducible);
            let resultado = devengado - deducible;
            poner(m, "46", resultado);
            poner(m, "64", resultado);
            poner(m, "69", resultado);
        }
        "390" => {
            let base = m.importe("01") + m.importe("03") + m.importe("05");
            poner(m, "33", base);
            let devengado = m.importe("02") + m.importe("04") + m.importe("06");
            poner(m, "34", devengado);
            let deducible = m.importe("49");
            poner(m, "64", deducible);
            let resultado = devengado - deducible;
            poner(m, "65", resultado);
            poner(m, "84", resultado);
            poner(m, "86", resultado);
        }
        "130" => {
            let neto = m.importe("01") - m.importe("02");
            poner(m, "03", neto);
            let cuota = div_round(neto.max(0) * 2000, 10_000);
            poner(m, "04", cuota);
            let resultado = (cuota - m.importe("05") - m.importe("06")).max(0);
            poner(m, "07", resultado);
            poner(m, "19", resultado);
        }
        _ => {}
    }
}

/// Aplica los valores escritos a mano y recalcula los totales. Así se puede
/// afinar lo que la app no sabe (AIB, prorrata, compensaciones, casilla 13
/// del 130) sin salir de aquí.
pub fn aplicar(m: &mut Modelo, ajustes: &BTreeMap<String, String>) {
    let mut tocado = false;
    for s in &mut m.secciones {
        for c in &mut s.casillas {
            if !matches!(c.valor, Valor::Dinero(_)) {
                continue;
            }
            if let Some(txt) = ajustes.get(&c.numero) {
                if txt.trim().is_empty() {
                    continue;
                }
                if let Some(v) = parse_cents(txt) {
                    c.valor = Valor::Dinero(v);
                    tocado = true;
                }
            }
        }
    }
    if tocado {
        recalcular(m);
    }
}

/// Nombre del fichero, sin acentos: `Modelo-303-2026-1T.pdf` o
/// `Modelo-390-2026-anual.pdf`.
pub fn nombre(codigo: &str, anyo: i32, tri: u32, extension: &str) -> String {
    let periodo = if tri == 0 {
        "anual".to_string()
    } else {
        format!("{tri}T")
    };
    format!("Modelo-{codigo}-{anyo}-{periodo}.{extension}")
}

/// Resultado (casilla 07) del 130 de un trimestre, sin minoraciones.
fn resultado_130(db: &Db, anyo: i32, tri: u32) -> i64 {
    let a = contab::acumulado_130(db, anyo, tri);
    let cuota = div_round((a.ingresos - a.gastos).max(0) * 2000, 10_000);
    let previos: i64 = (1..tri).map(|q| resultado_130(db, anyo, q)).sum();
    (cuota - previos - a.retenciones).max(0)
}

/// Modelo 130 (pago a cuenta del IRPF en estimación directa).
///
/// Casillas 01 y 02 acumuladas del año, 03 rendimiento neto, 04 el 20 %,
/// 05 pagos de trimestres anteriores, 06 retenciones del trimestre,
/// 07 resultado y 12 resultado de la declaración.
pub fn fraccionado_130(db: &Db, anyo: i32, tri: u32) -> Modelo {
    let a = contab::acumulado_130(db, anyo, tri);
    let neto = a.ingresos - a.gastos;
    let cuota = div_round(neto.max(0) * 2000, 10_000);
    let previos: i64 = (1..tri).map(|q| resultado_130(db, anyo, q)).sum();

    // Retenciones del trimestre, no las acumuladas: las acumuladas ya están
    // descontadas en los pagos anteriores.
    let mut ret_trim = 0i64;
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha < inicio_trimestre(anyo, tri) || d.fecha > fin_trimestre(anyo, tri) {
            continue;
        }
        ret_trim += d.totales().retencion;
    }

    let resultado = (cuota - previos - ret_trim).max(0);

    Modelo {
        codigo: "130",
        titulo: "IRPF · pago fraccionado (estimación directa)",
        periodo: etiqueta_periodo(anyo, tri),
        secciones: vec![seccion(
            "Liquidación acumulada del ejercicio",
            vec![
                c(
                    "01",
                    "Ingresos computables acumulados (sin IVA)",
                    Valor::Dinero(a.ingresos),
                ),
                c(
                    "02",
                    "Gastos deducibles acumulados (incluye personal)",
                    Valor::Dinero(a.gastos),
                ),
                c(
                    "03",
                    "Rendimiento neto (01 − 02)",
                    Valor::Dinero(neto),
                ),
                c(
                    "04",
                    "20 % del rendimiento neto positivo",
                    Valor::Dinero(cuota),
                ),
                c(
                    "05",
                    "Pagos fraccionados de trimestres anteriores",
                    Valor::Dinero(previos),
                ),
                c(
                    "06",
                    "Retenciones e ingresos a cuenta del trimestre",
                    Valor::Dinero(ret_trim),
                ),
                c(
                    "07",
                    "Resultado (04 − 05 − 06)",
                    Valor::Dinero(resultado),
                ),
                c(
                    "19",
                    "Resultado final de la autoliquidación",
                    Valor::Dinero(resultado),
                ),
            ],
        )],
        nota: "Borrador con criterio de devengo. Sin casillas 13 a 18 la 19 coincide con la 07. No aplica la minoración por rendimientos bajos del año anterior (casilla 13) ni el 5 % de gastos de difícil justificación: si te corresponden, réstalas tú.",
        detalle: Vec::new(),
    }
}

/// Avisos para no presentar con erratas. No bloquean nada: el autónomo
/// decide, pero es mejor enterarse aquí que en la Sede.
pub fn avisos(m: &Modelo, db: &Db, anyo: i32, tri: u32) -> Vec<String> {
    let mut v = Vec::new();
    match m.codigo {
        "303" => {
            for (base, tipo, cuota) in
                [("01", "02", "03"), ("04", "05", "06"), ("07", "08", "09")]
            {
                let b = m.importe(base);
                if b == 0 {
                    continue;
                }
                let pb = m.tipo(tipo);
                let esperado = div_round(b * pb as i64, 10_000);
                let c = m.importe(cuota);
                if (c - esperado).abs() > 1 {
                    v.push(format!(
                        "La cuota {cuota} ({}) no cuadra con la base {base} al {} %: serían {}.",
                        fmt_cents_eur(c),
                        fmt_pb(pb),
                        fmt_cents_eur(esperado)
                    ));
                }
            }
            let resultado = m.importe("69");
            if resultado < 0 {
                v.push(format!(
                    "El resultado es negativo ({}): sale a compensar, no a pagar.",
                    fmt_cents_eur(resultado)
                ));
            }
            let sin_iva: i64 = crate::fiscal::bases_por_operacion(db, anyo, tri)
                .iter()
                .map(|(_, b)| *b)
                .sum();
            if sin_iva != 0 {
                v.push(format!(
                    "Hay {} en operaciones exentas, de exportación o intracomunitarias: no van en las casillas 01-09, van en las suyas.",
                    fmt_cents_eur(sin_iva)
                ));
            }
        }
        "390" => {
            let suma: i64 = (1..=4).map(|q| iva_303(db, anyo, q).importe("27")).sum();
            if (m.importe("34") - suma).abs() > 1 {
                v.push(format!(
                    "El total devengado ({}) no coincide con la suma de los cuatro 303 ({}).",
                    fmt_cents_eur(m.importe("34")),
                    fmt_cents_eur(suma)
                ));
            }
        }
        "130" => {
            let previos: i64 = (1..tri).map(|q| resultado_130(db, anyo, q)).sum();
            if (m.importe("05") - previos).abs() > 1 {
                v.push(format!(
                    "La casilla 05 ({}) no coincide con lo que salió en los trimestres anteriores ({}).",
                    fmt_cents_eur(m.importe("05")),
                    fmt_cents_eur(previos)
                ));
            }
            let hay_retencion = db.documentos.iter().any(|d| {
                d.tipo.es_fiscal()
                    && d.estado != Estado::Anulado
                    && d.fecha >= inicio_trimestre(anyo, tri)
                    && d.fecha <= fin_trimestre(anyo, tri)
                    && d.totales().retencion > 0
            });
            if hay_retencion && m.importe("06") == 0 && m.importe("19") > 0 {
                v.push(
                    "Hay facturas con retención este trimestre y la casilla 06 está a cero."
                        .to_string(),
                );
            }
        }
        "347" => {
            let sin_nif = db
                .gastos
                .iter()
                .filter(|g| {
                    g.fecha.year() == anyo
                        && !g.es_pago_personal()
                        && g.nif.trim().is_empty()
                })
                .count();
            if sin_nif > 0 {
                v.push(format!(
                    "{sin_nif} gastos sin NIF no entran en el 347: añade el proveedor para que cuenten."
                ));
            }
        }
        "349" => {
            let malas = db
                .documentos
                .iter()
                .filter(|d| {
                    d.operacion == TipoOperacion::Intracomunitaria
                        && d.estado != Estado::Anulado
                        && d.fecha >= inicio_trimestre(anyo, tri)
                        && d.fecha <= fin_trimestre(anyo, tri)
                        && {
                            let nif = d.cliente.nif.trim();
                            !(nif.chars().count() >= 3
                                && nif.chars().take(2).all(|c| c.is_ascii_alphabetic()))
                        }
                })
                .count();
            if malas > 0 {
                v.push(format!(
                    "{malas} facturas intracomunitarias no tienen NIF-IVA con prefijo de país: la Sede las rechazará."
                ));
            }
        }
        "111" | "190" => {
            let anual = m.codigo == "190";
            let (ini, fin) = if anual {
                (
                    NaiveDate::from_ymd_opt(anyo, 1, 1).unwrap_or_default(),
                    NaiveDate::from_ymd_opt(anyo, 12, 31).unwrap_or_default(),
                )
            } else {
                (inicio_trimestre(anyo, tri), fin_trimestre(anyo, tri))
            };
            let r = resumen_retenciones(&db.nominas, &db.empleados, anyo, if anual { 0 } else { tri });
            if r.num_nominas == 0 {
                v.push(format!(
                    "No hay nóminas generadas en {}: el modelo saldría a cero.",
                    if anual { format!("el año {anyo}") } else { format!("el {tri}T de {anyo}") }
                ));
            }
            if r.sin_nif() > 0 {
                v.push(format!(
                    "{} perceptor(es) sin NIF: el 190 los rechazaría y el 111 no puede identificar bien la retención.",
                    r.sin_nif()
                ));
            }
            let ret_facturas: i64 = db
                .documentos
                .iter()
                .filter(|d| {
                    d.tipo.es_fiscal()
                        && d.numero > 0
                        && d.estado != Estado::Anulado
                        && d.fecha >= ini
                        && d.fecha <= fin
                })
                .map(|d| d.totales().retencion)
                .sum();
            if ret_facturas > 0 {
                v.push(format!(
                    "Hay {} de retención en facturas de profesionales: este borrador sólo suma nóminas. Esas retenciones van en las casillas 07-09 del 111 y en otro apartado del 190.",
                    fmt_cents_eur(ret_facturas)
                ));
            }
            if anual {
                let suma_111: i64 = (1..=4)
                    .map(|q| retenciones_111(db, anyo, q).importe("03"))
                    .sum();
                if (m.importe("03") - suma_111).abs() > 1 {
                    v.push(format!(
                        "Las retenciones del 190 ({}) no coinciden con la suma de los cuatro 111 ({}) del año.",
                        fmt_cents_eur(m.importe("03")),
                        fmt_cents_eur(suma_111)
                    ));
                }
            }
        }
        _ => {}
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn cliente(nif: &str, nombre: &str) -> Cliente {
        Cliente {
            id: 1,
            nombre: nombre.to_string(),
            nif: nif.to_string(),
            ..Default::default()
        }
    }

    fn factura(
        nif: &str,
        nombre: &str,
        fecha: NaiveDate,
        op: TipoOperacion,
        iva_pb: i32,
        base: i64,
    ) -> Documento {
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente(nif, nombre), 0, "2026".to_string());
        d.numero = 1;
        d.estado = Estado::Emitido;
        d.fecha = fecha;
        d.operacion = op;
        d.lineas.push(Linea {
            cantidad_milli: 1000,
            precio_cents: base,
            iva_pb,
            ..Default::default()
        });
        d
    }

    fn dia(a: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(a, m, d).unwrap()
    }

    #[test]
    fn el_303_tiene_las_casillas_del_regimen_general() {
        let db = Db::default();
        let m = iva_303(&db, 2026, 1);
        for num in ["01", "03", "07", "09", "27", "28", "29", "45", "46", "64", "69"] {
            assert!(
                m.secciones.iter().any(|s| s.casillas.iter().any(|c| c.numero == num)),
                "falta la casilla {num}"
            );
        }
        assert_eq!(m.importe("27"), 0);
        assert_eq!(m.importe("69"), 0);
        assert!(!m.resumen().is_empty());
    }

    #[test]
    fn el_303_no_deduce_el_pago_bancario_de_nominas() {
        let mut db = Db::default();
        db.gastos.push(Gasto {
            id: 1,
            fecha: dia(2026, 2, 1),
            base_cents: 100_000,
            iva_pb: 2100,
            pago_personal: true,
            ..Default::default()
        });
        db.gastos.push(Gasto {
            id: 2,
            fecha: dia(2026, 2, 2),
            base_cents: 1_000,
            iva_pb: 2100,
            ..Default::default()
        });

        let m = iva_303(&db, 2026, 1);
        assert_eq!(m.importe("28"), 1_000);
        assert_eq!(m.importe("29"), 210);
    }

    #[test]
    fn el_130_incluye_el_coste_de_personal_sin_duplicar_el_pago() {
        use crate::empleados::{Empleado, Nomina};
        let mut db = Db::default();
        db.documentos.push(factura(
            "B1",
            "Cliente",
            dia(2026, 2, 10),
            TipoOperacion::Nacional,
            2100,
            500_000,
        ));
        let empleado = Empleado {
            id: 1,
            nombre: "Ana".into(),
            apellidos: "García".into(),
            nif: "12345678Z".into(),
            fecha_alta: dia(2026, 1, 1),
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        };
        let n = Nomina::calcular(&empleado, 10, 2026, 1);
        let coste = n.coste_empresa_cents;
        db.nominas.push(n);
        db.gastos.push(Gasto {
            id: 2,
            fecha: dia(2026, 1, 31),
            base_cents: 157_040,
            pago_personal: true,
            ..Default::default()
        });
        db.gastos.push(Gasto {
            id: 3,
            fecha: dia(2026, 2, 2),
            base_cents: 10_000,
            iva_pb: 2100,
            ..Default::default()
        });

        let m = fraccionado_130(&db, 2026, 1);
        assert_eq!(m.importe("02"), coste + 10_000);
        assert_eq!(m.importe("03"), 500_000 - coste - 10_000);
        assert_eq!(
            m.importe("04"),
            div_round((500_000 - coste - 10_000) * 2000, 10_000)
        );
    }

    #[test]
    fn el_json_lleva_las_casillas_y_el_nif() {
        let db = Db::default();
        let mut empresa = Empresa::default();
        empresa.nif = "12345678Z".into();
        let json = iva_303(&db, 2026, 2).json(&empresa);
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(json.contains("\"codigo\":\"303\""));
        assert!(json.contains("\"periodo\":\"2026 · 2T\""));
        assert!(json.contains("\"nif\":\"12345678Z\""));
        assert!(json.contains("\"numero\":\"69\""));
    }

    #[test]
    fn el_130_empieza_en_cero_sin_datos() {
        let db = Db::default();
        let m = fraccionado_130(&db, 2026, 3);
        assert_eq!(m.importe("01"), 0);
        assert_eq!(m.importe("04"), 0);
        assert_eq!(m.importe("07"), 0);
        assert!(nombre("130", 2026, 3, "pdf") == "Modelo-130-2026-3T.pdf");
    }

    #[test]
    fn el_390_suma_los_cuatro_trimestres() {
        let mut db = Db::default();
        db.documentos.push(factura(
            "B1",
            "Agencia",
            dia(2026, 2, 1),
            TipoOperacion::Nacional,
            2100,
            1000_00,
        ));
        db.documentos.push(factura(
            "B2",
            "Otra",
            dia(2026, 11, 1),
            TipoOperacion::Nacional,
            2100,
            500_00,
        ));
        let anual = resumen_390(&db, 2026);
        assert_eq!(anual.importe("05"), 1500_00);
        assert_eq!(anual.importe("06"), 315_00);
        assert_eq!(anual.importe("34"), 315_00);
        assert_eq!(anual.importe("86"), 315_00);
        assert!(anual.resumen().contains("315,00"));
        assert!(anual.periodo.contains("anual"));
        assert!(nombre("390", 2026, 0, "pdf") == "Modelo-390-2026-anual.pdf");
    }

    #[test]
    fn los_ajustes_a_mano_recalculan_los_totales() {
        let db = Db::default();
        let mut m = iva_303(&db, 2026, 1);
        let mut aj = BTreeMap::new();
        aj.insert("07".to_string(), "1.000,00".to_string());
        aj.insert("09".to_string(), "210,00".to_string());
        aj.insert("29".to_string(), "100,00".to_string());
        aplicar(&mut m, &aj);
        assert_eq!(m.importe("07"), 1000_00);
        assert_eq!(m.importe("27"), 210_00);
        assert_eq!(m.importe("45"), 100_00);
        assert_eq!(m.importe("69"), 110_00);
        assert!(m.resumen().contains("110,00"));
        assert_eq!(clave("303", 2026, 1), "303-2026-1T");
        assert_eq!(clave("390", 2026, 0), "390-2026-anual");
    }

    #[test]
    fn avisa_cuando_una_cuota_no_cuadra_con_su_base() {
        let db = Db::default();
        let mut m = iva_303(&db, 2026, 1);
        let mut aj = BTreeMap::new();
        aj.insert("07".to_string(), "1.000,00".to_string());
        aj.insert("09".to_string(), "100,00".to_string());
        aplicar(&mut m, &aj);
        let a = avisos(&m, &db, 2026, 1);
        assert!(a.iter().any(|t| t.contains("no cuadra")), "{a:?}");

        // Con la cuota bien, ese aviso desaparece.
        aj.insert("09".to_string(), "210,00".to_string());
        aplicar(&mut m, &aj);
        let a = avisos(&m, &db, 2026, 1);
        assert!(!a.iter().any(|t| t.contains("no cuadra")), "{a:?}");
    }

    #[test]
    fn una_r1_resta_en_el_303_sin_tocar_la_original() {
        let mut db = Db::default();
        db.documentos.push(factura(
            "B12345678",
            "Cliente con corrección",
            dia(2026, 1, 10),
            TipoOperacion::Nacional,
            2100,
            1_000_00,
        ));
        let mut r1 = factura(
            "B12345678",
            "Cliente con corrección",
            dia(2026, 2, 10),
            TipoOperacion::Nacional,
            2100,
            -100_00,
        );
        r1.rectifica = Some(crate::model::Rectificacion {
            documento_id: 1,
            num_serie: "20260001".to_string(),
            etiqueta: "2026/0001".to_string(),
            fecha: dia(2026, 1, 10),
            tipo: "I".to_string(),
        });
        db.documentos.push(r1);

        let m = iva_303(&db, 2026, 1);
        assert_eq!(m.importe("07"), 900_00);
        assert_eq!(m.importe("09"), 189_00);
    }

    #[test]
    fn el_resumen_se_entiende_sin_leer_casillas() {
        let mut db = Db::default();
        db.documentos.push(factura(
            "B1",
            "Agencia",
            dia(2026, 1, 1),
            TipoOperacion::Nacional,
            2100,
            1000_00,
        ));
        let r = iva_303(&db, 2026, 1).resumen();
        assert!(r.starts_with("Sale a ingresar"), "{r}");
        assert!(r.contains("210,00"), "{r}");
        assert_eq!(
            fraccionado_130(&db, 2026, 1).resumen(),
            "A pagar a Hacienda 200,00 EUR sobre un beneficio acumulado de 1.000,00 EUR."
        );
    }

    #[test]
    fn el_347_solo_lista_a_quien_supera_el_limite() {
        let mut db = Db::default();
        db.documentos.push(factura(
            "B12345678",
            "Grande",
            dia(2026, 5, 1),
            TipoOperacion::Nacional,
            2100,
            4000_00,
        ));
        db.documentos.push(factura(
            "B87654321",
            "Pequeño",
            dia(2026, 5, 1),
            TipoOperacion::Nacional,
            2100,
            100_00,
        ));
        let m = operaciones_347(&db, 2026);
        let texto: String = m
            .secciones
            .iter()
            .flat_map(|s| s.casillas.iter())
            .map(|c| c.etiqueta.clone())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(texto.contains("Grande"));
        assert!(!texto.contains("Pequeño"));
        // 4.000 + 21 % = 4.840 €: el importe del 347 va con IVA.
        let con_iva = m.secciones[0].casillas.iter().any(|c| match c.valor {
            Valor::Dinero(v) => v == 4840_00,
            _ => false,
        });
        assert!(con_iva, "falta el importe con IVA de 4.840,00 €");
    }

    #[test]
    fn el_349_agrupa_por_nif_y_solo_el_trimestre() {
        let mut db = Db::default();
        db.documentos.push(factura(
            "DE123",
            "Estudio",
            dia(2026, 2, 1),
            TipoOperacion::Intracomunitaria,
            0,
            1000_00,
        ));
        db.documentos.push(factura(
            "DE123",
            "Estudio",
            dia(2026, 3, 1),
            TipoOperacion::Intracomunitaria,
            0,
            500_00,
        ));
        db.documentos.push(factura(
            "FR999",
            "Editor",
            dia(2026, 8, 1),
            TipoOperacion::Intracomunitaria,
            0,
            700_00,
        ));
        let m = operaciones_349(&db, 2026, 1);
        assert_eq!(m.importe("T"), 1500_00);
        assert_eq!(m.secciones[0].casillas.len(), 2); // un cliente + total
        assert!(m.secciones[0].casillas[0].etiqueta.contains("DE123"));
    }

    #[test]
    fn el_111_y_el_190_salen_de_las_nominas_del_periodo() {
        use crate::empleados::{Empleado, Nomina, TipoContrato};
        let ana = Empleado {
            id: 1,
            nombre: "Ana".into(),
            apellidos: "García".into(),
            nif: "12345678Z".into(),
            fecha_alta: dia(2026, 1, 1),
            tipo_contrato: TipoContrato::Indefinido,
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        };
        let luis = Empleado {
            id: 2,
            nombre: "Luis".into(),
            apellidos: "Pérez".into(),
            nif: "87654321X".into(),
            fecha_alta: dia(2026, 1, 1),
            tipo_contrato: TipoContrato::Temporal,
            salario_bruto_anual_cents: 1_800_000,
            irpf_pb: 1000,
            ..Default::default()
        };
        let mut db = Db::default();
        db.empleados.push(ana.clone());
        db.empleados.push(luis.clone());
        db.nominas.push(Nomina::calcular(&ana, 10, 2026, 1));
        db.nominas.push(Nomina::calcular(&luis, 11, 2026, 1));
        db.nominas.push(Nomina::calcular(&ana, 12, 2026, 2));
        db.nominas.push(Nomina::calcular(&luis, 13, 2026, 2));
        db.nominas.push(Nomina::calcular(&ana, 14, 2026, 4));

        let q1 = retenciones_111(&db, 2026, 1);
        assert_eq!(q1.numero("01"), 2);
        assert_eq!(q1.importe("02"), 700_000);
        assert_eq!(q1.importe("03"), 90_000);
        assert_eq!(q1.numero("N"), 4);
        assert_eq!(q1.detalle.len(), 2);
        assert!(q1.resumen().contains("2 perceptores"));
        assert!(q1.json(&Empresa::default()).contains("\"detalle\""));

        let anual = resumen_190(&db, 2026);
        assert_eq!(anual.numero("01"), 5);
        assert_eq!(anual.importe("02"), 900_000);
        assert_eq!(anual.importe("03"), 120_000);
        assert_eq!(anual.numero("P"), 2);
        assert!(anual.resumen().contains("5 nóminas"));
        assert!(previsible("111") && previsible("190"));
        assert!(trimestral("111"));
        assert!(!trimestral("190"));
        assert!(generar(&db, "111", 2026, 1).is_some());
        assert!(generar(&db, "190", 2026, 0).is_some());
    }
}
