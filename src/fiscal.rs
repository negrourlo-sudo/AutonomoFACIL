//! Qué tiene que presentar un autónomo en España según su actividad, en
//! lenguaje claro, y las operaciones que no llevan IVA.
//!
//! No es asesoría fiscal: es una lista de comprobación con las fechas
//! habituales para que no se olvide ningún modelo. Las fechas exactas las
//! publica la AEAT cada año y se desplazan si caen en festivo.

use crate::model::*;
use crate::store::Db;
use chrono::Datelike;

pub struct Obligacion {
    pub codigo: &'static str,
    pub nombre: &'static str,
    pub plazo: &'static str,
    pub detalle: &'static str,
    /// Si se marca por trimestres (1T..4T) o una sola vez al año.
    pub trimestral: bool,
}

/// Modelos que le tocan a esta actividad. Los condicionales se listan igual:
/// el usuario decide si le aplican.
pub fn obligaciones(act: Actividad) -> Vec<Obligacion> {
    let mut v = vec![
        Obligacion {
            codigo: "303",
            nombre: "IVA trimestral",
            plazo: "1–20 de abril, julio y octubre; 1–30 de enero el 4T",
            detalle: "IVA repercutido menos el soportado en gastos. Se presenta aunque salga a devolver o a cero.",
            trimestral: true,
        },
        Obligacion {
            codigo: "130",
            nombre: "Pago a cuenta del IRPF",
            plazo: "Mismas fechas que el 303",
            detalle: "20 % del beneficio acumulado del año. No hace falta si el 70 % de tus ingresos ya llevan retención.",
            trimestral: true,
        },
        Obligacion {
            codigo: "390",
            nombre: "Resumen anual de IVA",
            plazo: "Hasta el 30 de enero",
            detalle: "Resume el año entero; es la suma de los cuatro 303.",
            trimestral: false,
        },
        Obligacion {
            codigo: "100",
            nombre: "Declaración de la Renta",
            plazo: "Campaña de abril a junio",
            detalle: "La renta del año anterior. Todos los autónomos están obligados, aunque no lleguen al mínimo.",
            trimestral: false,
        },
        Obligacion {
            codigo: "347",
            nombre: "Operaciones con terceros",
            plazo: "Hasta el 28 de febrero",
            detalle: "Sólo si con un mismo cliente o proveedor superaste 3.005,06 € (con IVA) en el año.",
            trimestral: false,
        },
        Obligacion {
            codigo: "349",
            nombre: "Operaciones intracomunitarias",
            plazo: "1–20 del mes siguiente al trimestre (mensual si superas 50.000 €)",
            detalle: "Facturas a empresas de la UE con NIF-IVA válido en VIES y compras a proveedores UE. No incluye particulares ni países de fuera de la UE.",
            trimestral: true,
        },
        Obligacion {
            codigo: "111",
            nombre: "Retenciones de trabajo",
            plazo: "1–20 del mes siguiente a cada trimestre",
            detalle: "Sólo si tienes empleados o pagas nóminas.",
            trimestral: true,
        },
        Obligacion {
            codigo: "190",
            nombre: "Resumen anual de retenciones de trabajo",
            plazo: "Hasta el 31 de enero",
            detalle: "Sólo si has presentado el 111.",
            trimestral: false,
        },
        Obligacion {
            codigo: "115",
            nombre: "Retención por alquiler de local",
            plazo: "1–20 del mes siguiente a cada trimestre",
            detalle: "Sólo si pagas alquiler de un local por el que te retienen el 19 %.",
            trimestral: true,
        },
        Obligacion {
            codigo: "180",
            nombre: "Resumen anual de alquileres",
            plazo: "Hasta el 31 de enero",
            detalle: "Sólo si has presentado el 115.",
            trimestral: false,
        },
    ];

    if act == Actividad::Digital {
        v.push(Obligacion {
            codigo: "368",
            nombre: "Ventanilla única (OSS)",
            plazo: "Último día del mes siguiente al trimestre",
            detalle: "Sólo si vendes tú directamente a particulares de la UE más de 10.000 € al año. Vendiendo a través de Steam no hace falta: Valve es el vendedor.",
            trimestral: true,
        });
    }

    v
}

/// Explicación de la actividad, con los casos que más se atascan.
pub fn guia(act: Actividad) -> Vec<(&'static str, &'static str)> {
    match act {
        Actividad::Servicios => vec![
            (
                "Facturar a empresas españolas",
                "El cliente te retiene el 15 % de IRPF en la factura (7 % los tres primeros años). Ponlo en el campo IRPF y la factura ya descuenta.",
            ),
            (
                "¿Puedo librarme del 130?",
                "Sí, si al menos el 70 % de tus ingresos del año ya vienen con retención. Márcalo en la lista de obligaciones como no aplicable.",
            ),
            (
                "Qué no entra en el 303",
                "Las facturas con retención sí entran: el IVA se declara igual. Lo que no entra son las operaciones exentas o de fuera de España.",
            ),
        ],
        Actividad::Comercio => vec![
            (
                "Si vendes al público y tu proveedor te aplica recargo de equivalencia",
                "Entonces no liquidas IVA de esas ventas en el 303: lo ingresa el proveedor. Consúltalo con tu gestor antes de configurar el IVA por defecto a 0.",
            ),
            (
                "Compras y gastos",
                "El IVA de las facturas de gasto se deduce en el 303 del trimestre en que están fechadas, no cuando las pagas.",
            ),
            (
                "Stock e inventario",
                "Al cerrar el año, el valor del stock forma parte del resultado en la Renta. Anótalo antes de hacer el 390.",
            ),
        ],
        Actividad::Digital => vec![
            (
                "En Steam tú no vendes al jugador",
                "El vendedor frente al jugador es Valve: cobra el precio, liquida el IVA o sales tax de cada país y te paga tu parte. Tú sólo facturas a Valve.",
            ),
            (
                "La factura a Valve es exportación",
                "Valve Corporation está en EE. UU.: marca la factura como «Exportación» y el programa la deja sin IVA, sin retención y fuera del 349. Se declara en el 303 como operación sin IVA.",
            ),
            (
                "Ojo con la retención de EE. UU.",
                "Valve clasifica el pago como royalty y retiene un 30 % si no tiene tu W-8BEN con NIF. Con el formulario se aplica el convenio España-EE. UU. Guarda el 1042-S de cada año: el bruto es tu ingreso y lo retenido se resta en la Renta.",
            ),
            (
                "Una factura por cada pago",
                "Usa el informe mensual de Steamworks como base y emite una factura a Valve por el importe del mes. Si el pago llega neto de retención, factura el bruto y anota la retención en las notas.",
            ),
            (
                "Si además vendes tú a particulares de la UE",
                "Por tu propia web o itch.io, con clientes de la UE: hasta 10.000 € al año puedes cobrar IVA español; por encima, alta en OSS (modelo 035) y declarar en el 368.",
            ),
            (
                "Clientes empresa de la UE",
                "Si facturas a un estudio o publisher con NIF-IVA, marca la factura como «Intracomunitaria»: sin IVA, con la leyenda de inversión del sujeto pasivo y suma en el 349.",
            ),
        ],
        Actividad::Formacion => vec![
            (
                "La formación reglada suele estar exenta",
                "Los cursos incluidos en un plan de estudios están exentos de IVA (art. 20.1.9 LIVA). Por eso el IVA por defecto sale al 0 %: si tu curso no es reglado, súbelo al 21 %.",
            ),
            (
                "Aun exenta, el 303 y el 130 siguen",
                "El 303 se presenta a cero si no repercutes IVA, y el 130 se calcula sobre el beneficio igual.",
            ),
            (
                "Retención de IRPF",
                "Si das formación a empresas, te retienen el 15 % como a cualquier profesional. Los cursos a particulares no llevan retención.",
            ),
        ],
        Actividad::Alquiler => vec![
            (
                "Alquiler de vivienda",
                "Exento de IVA (art. 20.1.23 LIVA): facturas al 0 % con la leyenda de exención.",
            ),
            (
                "Alquiler de local a una empresa",
                "Lleva IVA del 21 % y el inquilino te retiene el 19 % de IRPF: ponlo en el campo IRPF y declara el 115 trimestral.",
            ),
            (
                "Gastos deducibles",
                "Comunidad, IBI, seguros, reparaciones y amortización. Regístralos todos: son los que bajan el rendimiento en la Renta.",
            ),
        ],
        Actividad::Otra => vec![
            (
                "Empieza por el 303 y el 130",
                "Son los dos que se presentan cada trimestre en casi cualquier actividad en estimación directa.",
            ),
            (
                "Cierra el año con el 390 y el 100",
                "El 390 resume el IVA y el 100 la renta. El 347 sólo si superaste 3.005,06 € con alguien.",
            ),
        ],
    }
}

pub fn esta_presentado(db: &Db, clave: &str) -> bool {
    db.presentados.iter().any(|k| k == clave)
}

pub fn alternar(db: &mut Db, clave: &str) {
    if let Some(i) = db.presentados.iter().position(|k| k == clave) {
        db.presentados.remove(i);
    } else {
        db.presentados.push(clave.to_string());
    }
}

pub struct Fila349 {
    pub nif: String,
    pub nombre: String,
    pub base: i64,
}

/// Operaciones intracomunitarias del año, agrupadas por NIF-IVA del cliente.
/// Sólo cuentan facturas emitidas con ese tipo de operación.
pub fn resumen_349(db: &Db, anyo: i32) -> Vec<Fila349> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, (String, i64)> = BTreeMap::new();
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if d.fecha.year() != anyo || d.operacion != TipoOperacion::Intracomunitaria {
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
    acc.into_iter()
        .map(|(nif, (nombre, base))| Fila349 { nif, nombre, base })
        .collect()
}

/// Bases facturadas que no llevan IVA español, para las casillas del 303.
pub fn bases_por_operacion(db: &Db, anyo: i32, trimestre: u32) -> Vec<(TipoOperacion, i64)> {
    let mut intra = 0i64;
    let mut export = 0i64;
    let mut exenta = 0i64;
    for d in &db.documentos {
        if !d.tipo.es_fiscal() || d.numero == 0 || d.estado == Estado::Anulado {
            continue;
        }
        if !crate::contab::en_rango(d.fecha, anyo, trimestre) {
            continue;
        }
        match d.operacion {
            TipoOperacion::Nacional => continue,
            TipoOperacion::Intracomunitaria => intra += d.totales().base_total,
            TipoOperacion::Exportacion => export += d.totales().base_total,
            TipoOperacion::Exenta => exenta += d.totales().base_total,
        }
    }
    let mut v = Vec::new();
    if intra != 0 {
        v.push((TipoOperacion::Intracomunitaria, intra));
    }
    if export != 0 {
        v.push((TipoOperacion::Exportacion, export));
    }
    if exenta != 0 {
        v.push((TipoOperacion::Exenta, exenta));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn cliente() -> Cliente {
        Cliente {
            id: 1,
            nombre: "Agencia".to_string(),
            nif: "B12345678".to_string(),
            ..Default::default()
        }
    }

    fn factura(op: TipoOperacion, iva_pb: i32, base: i64) -> Documento {
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente(), 0, "2026".to_string());
        d.numero = 1;
        d.estado = Estado::Emitido;
        d.fecha = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        d.operacion = op;
        d.lineas.push(Linea {
            cantidad_milli: 1000,
            precio_cents: base,
            iva_pb,
            ..Default::default()
        });
        d
    }

    #[test]
    fn exportacion_no_repercute_iva() {
        let d = factura(TipoOperacion::Exportacion, 2100, 100_00);
        let t = d.totales();
        assert_eq!(t.cuota_iva, 0);
        assert_eq!(t.total, 100_00);
    }

    #[test]
    fn nacional_si_repercute_iva() {
        let t = factura(TipoOperacion::Nacional, 2100, 100_00).totales();
        assert_eq!(t.cuota_iva, 21_00);
        assert_eq!(t.total, 121_00);
    }

    #[test]
    fn el_349_solo_suma_intracomunitarias() {
        let mut db = Db::default();
        db.documentos.push(factura(TipoOperacion::Nacional, 2100, 100_00));
        db.documentos
            .push(factura(TipoOperacion::Intracomunitaria, 0, 200_00));
        let filas = resumen_349(&db, 2026);
        assert_eq!(filas.len(), 1);
        assert_eq!(filas[0].base, 200_00);
    }

    #[test]
    fn el_oss_solo_sale_en_actividad_digital() {
        let digital: Vec<&str> = obligaciones(Actividad::Digital)
            .iter()
            .map(|o| o.codigo)
            .collect();
        let servicios: Vec<&str> = obligaciones(Actividad::Servicios)
            .iter()
            .map(|o| o.codigo)
            .collect();
        assert!(digital.contains(&"368"));
        assert!(!servicios.contains(&"368"));
    }

    #[test]
    fn las_bases_sin_iva_cuentan_solo_lo_suyo() {
        let mut db = Db::default();
        db.documentos.push(factura(TipoOperacion::Nacional, 2100, 100_00));
        db.documentos
            .push(factura(TipoOperacion::Exportacion, 0, 70_00));
        let bases = bases_por_operacion(&db, 2026, 1);
        assert_eq!(bases.len(), 1);
        assert_eq!(bases[0].0, TipoOperacion::Exportacion);
        assert_eq!(bases[0].1, 70_00);
    }
}
