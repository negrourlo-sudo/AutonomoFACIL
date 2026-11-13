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
    obligaciones_con(act, &Empresa::default())
}

/// Modelos según actividad y régimen: en módulos el pago a cuenta es el 131,
/// en recargo de equivalencia no hay 303 ni 390, y el SII quita el 390.
pub fn obligaciones_con(act: Actividad, empresa: &Empresa) -> Vec<Obligacion> {
    let mut v = obligaciones_base(act);

    // Una sociedad no presenta IRPF personal: ni Renta (100) ni pagos a cuenta
    // 130/131. En su lugar va el Impuesto de Sociedades (200) y su pago
    // fraccionado (202). Así el listado no confunde a quien tiene una SL.
    if !empresa.persona_fisica {
        v.retain(|o| !matches!(o.codigo, "130" | "131" | "100"));
        v.push(Obligacion {
            codigo: "200",
            nombre: "Impuesto sobre Sociedades",
            plazo: "1–25 de julio (o 25 días tras el cierre)",
            detalle: "Grava el beneficio del ejercicio en lugar de la Renta. Lo presenta toda sociedad, aunque el resultado sea cero; el autónomo persona física no lo presenta.",
            trimestral: false,
        });
        v.push(Obligacion {
            codigo: "202",
            nombre: "Pago fraccionado de Sociedades",
            plazo: "1–20 de abril, octubre y diciembre",
            detalle: "A cuenta del 200: 18 % del beneficio del año en curso, o 17 % del resultado del ejercicio anterior si eliges esa modalidad.",
            trimestral: false,
        });
    }

    if empresa.recargo_equivalencia {
        v.retain(|o| o.codigo != "303" && o.codigo != "390");
        v.insert(
            0,
            Obligacion {
                codigo: "309",
                nombre: "IVA · declaración no periódica",
                plazo: "30 días desde la operación",
                detalle: "En recargo de equivalencia no se presentan 303 ni 390. El 309 sólo hace falta por adquisiciones intracomunitarias, inversión del sujeto pasivo o venta de inmuebles.",
                trimestral: false,
            },
        );
    }

    if empresa.sii {
        v.retain(|o| o.codigo != "390");
        if let Some(o) = v.iter_mut().find(|o| o.codigo == "303") {
            o.plazo = "Mensual: 1–20 del mes siguiente (30 de enero el de diciembre)";
            o.detalle = "Con SII el 303 es mensual y los libros se llevan en tiempo real; el 390 no se presenta.";
        }
    }

    if empresa.regimen_irpf.es_objetiva() {
        if let Some(o) = v.iter_mut().find(|o| o.codigo == "130") {
            o.codigo = "131";
            o.nombre = "Pago fraccionado en módulos";
            o.detalle = "Sustituye al 130 en estimación objetiva: se calcula con los módulos de tu actividad, no con tus facturas.";
        }
    }

    v
}

fn obligaciones_base(act: Actividad) -> Vec<Obligacion> {
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
            detalle: "Sólo si tienes empleados o pagas nóminas. AutónomoFACIL prepara un borrador con las nóminas generadas; revisa perceptores, brutos e IRPF.",
            trimestral: true,
        },
        Obligacion {
            codigo: "190",
            nombre: "Resumen anual de retenciones de trabajo",
            plazo: "Hasta el 31 de enero",
            detalle: "Sólo si has presentado el 111. Resume las retenciones de las nóminas del año y debe cuadrar con los cuatro 111.",
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

    if matches!(act, Actividad::Digital | Actividad::Comercio) {
        v.push(Obligacion {
            codigo: "035",
            nombre: "Alta en la ventanilla única (OSS/IOSS)",
            plazo: "Antes de empezar; efectos desde el trimestre siguiente",
            detalle: "Formulario de alta, cambio o baja en la ventanilla única. Se presenta una vez por cada régimen y es obligatorio antes de usar el 369. Si vendes a través de Steam, Amazon o similar, la plataforma es el vendedor y no lo necesitas.",
            trimestral: false,
        });
        v.push(Obligacion {
            codigo: "369",
            nombre: "IVA · ventanilla única (OSS/IOSS)",
            plazo: "OSS: 1–20 del mes siguiente al trimestre · IOSS: mensual",
            detalle: "Declara de una vez el IVA de cada país de la UE donde tengas particulares. A partir de 10.000 € al año es obligatorio si vendes tú directamente. Sustituye al 303 sólo por esas ventas; el resto sigue en el 303.",
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
                "Vendes a particulares de la UE: dónde se paga el IVA",
                "El IVA es del país donde vive el cliente, no el español. Hasta 10.000 € al año (sin IVA) puedes seguir aplicando el 21 % y declararlo en el 303; al pasarlos, tienes que cobrar el IVA de cada país y declararlo en el 369, con alta previa en el 035.",
            ),
            (
                "Qué es y qué no es el 369",
                "No es un impuesto nuevo: es el mismo IVA, pero declarado una sola vez para toda la UE. La AEAT lo reparte a cada país y tú evitas registrarte en cada uno. Es trimestral (mensual si usas la IOSS).",
            ),
            (
                "Justo en el límite de los 10.000 €",
                "El umbral se cuenta sumando las ventas a particulares de todos los países de la UE, sin IVA, y se revisa cada año. Al superarlo en el año en curso, el 369 se aplica desde ese momento; puedes elegir el 369 antes de llegar si te compensa.",
            ),
            (
                "Clientes empresa de la UE",
                "Con NIF-IVA válido en VIES, la factura va sin IVA y con la leyenda de inversión del sujeto pasivo: el cliente autoliquida en su país. Esa base va al 349, no al 369.",
            ),
            (
                "Clientes de fuera de la UE",
                "A un particular de EE. UU., Reino Unido o Japón se le factura sin IVA español: es exportación (art. 21 LIVA). Va al 303 como operación sin IVA y no entra ni en el 349 ni en el 369.",
            ),
            (
                "Vendes solo en plataformas: el 369 no te toca",
                "La plataforma es el vendedor frente al jugador y liquida el IVA de cada país. Tú no cobras IVA al jugador ni presentas el 369: le haces una factura B2B a la plataforma. El 303 y, si es de la UE, el 349 siguen siendo tuyos.",
            ),
            (
                "Plataforma en la UE: Apple o Google (Irlanda)",
                "La factura va a una empresa irlandesa con su NIF-IVA (IE...): base al 0 % con la cláusula de inversión del sujeto pasivo. Esa base va al 349 y, en el 303, a la casilla 59 (operaciones no sujetas por reglas de localización). Necesitas el alta en el ROI con el 036 o el 037.",
            ),
            (
                "itch.io: la sede manda",
                "itch.io opera desde EE. UU.: es exportación de servicios, sin IVA y sin 349. Si en algún momento te factura una filial europea, comprueba el NIF-IVA que figure en la factura: si empieza por IE, pasa a ser intracomunitaria y sí va al 349.",
            ),
            (
                "Plataforma fuera de la UE: Valve o Epic (EE. UU.)",
                "Es exportación de servicios (art. 69 LIVA): factura sin IVA con la leyenda de exportación. Va al 303 en la casilla 59 como operación no sujeta, no entra en el 349 ni en el 369. La retención del 30 % de EE. UU. es otra cosa: se reduce con el W-8BEN y se justifica con el 1042-S.",
            ),
            (
                "Resumen: qué presentas según la sede de la plataforma",
                "UE (Apple, Google): 303 con la base en la casilla 59 y 349 obligatorio con su NIF-IVA; antes, alta en el ROI. Fuera de la UE (Valve, Epic, itch.io): 303 con la base en la casilla 59 y sin 349. En ambos casos el IVA de la factura es 0 % y deduces el IVA de tus gastos. IRPF: si eres persona física, el beneficio va al 130 y a la Renta; si eres sociedad, al 200 y al 202.",
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

/// Fecha límite de un modelo para el periodo indicado (`tri` 0 = anual).
/// Son las fechas habituales; si caen en festivo la AEAT las desplaza.
pub fn vence(codigo: &str, trimestral: bool, anyo: i32, tri: u32) -> Option<chrono::NaiveDate> {
    let f = |y: i32, m: u32, d: u32| chrono::NaiveDate::from_ymd_opt(y, m, d);
    if trimestral {
        match tri {
            1 => f(anyo, 4, 20),
            2 => f(anyo, 7, 20),
            3 => f(anyo, 10, 20),
            4 => f(anyo + 1, 1, 30),
            _ => None,
        }
    } else {
        match codigo {
            "390" => f(anyo + 1, 1, 30),
            "190" | "180" => f(anyo + 1, 1, 31),
            "347" => f(anyo + 1, 2, 28),
            "100" => f(anyo + 1, 6, 30),
            _ => None,
        }
    }
}

/// El «por qué» de cada modelo para este perfil concreto: la razón práctica
/// por la que le toca, no la norma completa. Es lo que se pinta en la tarjeta
/// para que nadie tenga que adivinar de dónde sale.
pub fn motivo(e: &Empresa, codigo: &str) -> &'static str {
    match codigo {
        "303" => {
            if e.sii {
                "Porque repercutes IVA y, con SII, lo liquidas cada mes: 1–20 del mes siguiente."
            } else {
                "Porque repercutes IVA a tus clientes e ingresas la diferencia con el IVA de tus gastos."
            }
        }
        "130" => {
            "Porque eres autónomo persona física y adelantas el 20 % del beneficio; las retenciones y lo ya pagado se restan."
        }
        "131" => {
            "Porque eres autónomo en estimación objetiva: el pago a cuenta se calcula por módulos, no por tus facturas."
        }
        "100" => {
            "Porque la Renta es personal: la presenta la persona física, no la sociedad."
        }
        "200" => {
            "Porque es una sociedad: tributa por el Impuesto de Sociedades sobre el beneficio, no por el IRPF."
        }
        "202" => {
            "Porque una sociedad adelanta el Impuesto de Sociedades tres veces al año: abril, octubre y diciembre."
        }
        "390" => {
            if e.sii {
                "Con SII no se presenta: el resumen anual de IVA va dentro del propio SII."
            } else {
                "Porque resume el IVA del año entero, aunque ya hayas presentado los cuatro 303."
            }
        }
        "347" => {
            "Porque con un mismo cliente o proveedor superaste 3.005,06 € con IVA en el año."
        }
        "349" => {
            "Porque facturas o compras a empresas de la UE con NIF-IVA; los particulares no entran."
        }
        "111" => "Porque tienes empleados o pagas retenciones a profesionales.",
        "190" => {
            "Porque resume el año de las retenciones que ya declaraste en los 111."
        }
        "115" => "Porque pagas alquiler de local y retienes el 19 % al arrendador.",
        "180" => {
            "Porque resume el año de las retenciones del alquiler declaradas en los 115."
        }
        "309" => {
            "Porque en recargo de equivalencia hay operaciones (UE, ISP o inmuebles) que no caben en el 303."
        }
        "369" => {
            "Porque vendes a particulares de la UE por encima de 10.000 € al año y liquidas el IVA de cada país."
        }
        "035" => {
            "Porque es el alta obligatoria en la ventanilla única antes de presentar el 369."
        }
        _ => "Está en la lista por tu actividad y régimen; revisa si te aplica antes de marcarlo.",
    }
}

/// ¿Este modelo se presenta cada mes? El 303 con SII, sí. El 130/131 sigue
/// siendo trimestral aunque la empresa esté en SII.
pub fn es_mensual(db: &Db, ob: &Obligacion) -> bool {
    db.empresa.sii && ob.codigo == "303"
}

/// Vencimiento del 303 mensual: día 20 del mes siguiente; el de diciembre,
/// el 30 de enero del año siguiente.
pub fn vence_mes(anyo: i32, mes: u32) -> Option<chrono::NaiveDate> {
    let f = |y: i32, m: u32, d: u32| chrono::NaiveDate::from_ymd_opt(y, m, d);
    match mes {
        1..=11 => f(anyo, mes + 1, 20),
        12 => f(anyo + 1, 1, 30),
        _ => None,
    }
}

/// Próximo plazo pendiente de una obligación: (periodo, fecha, días que
/// faltan). Si todos los plazos del año ya pasaron, devuelve el más antiguo
/// con días negativos para poder avisar de que está sin presentar.
pub fn proximo_plazo(
    db: &Db,
    ob: &Obligacion,
    anyo: i32,
    hoy: chrono::NaiveDate,
) -> Option<(String, chrono::NaiveDate, i64)> {
    let mut pendientes: Vec<(String, chrono::NaiveDate)> = Vec::new();
    if es_mensual(db, ob) {
        for m in 1..=12u32 {
            let clave = format!("{anyo}-{}-{m:02}M", ob.codigo);
            if esta_presentado(db, &clave) {
                continue;
            }
            if let Some(f) = vence_mes(anyo, m) {
                pendientes.push((format!("{anyo} · {m:02}M"), f));
            }
        }
    } else if ob.trimestral {
        for q in 1..=4u32 {
            let clave = format!("{anyo}-{}-{q}T", ob.codigo);
            if esta_presentado(db, &clave) {
                continue;
            }
            if let Some(f) = vence(ob.codigo, true, anyo, q) {
                pendientes.push((format!("{anyo} · {q}T"), f));
            }
        }
    } else {
        let clave = format!("{anyo}-{}", ob.codigo);
        if !esta_presentado(db, &clave) {
            if let Some(f) = vence(ob.codigo, false, anyo, 0) {
                pendientes.push((format!("{anyo} · anual"), f));
            }
        }
    }
    pendientes.sort_by_key(|(_, f)| *f);
    let elegido = pendientes
        .iter()
        .find(|(_, f)| *f >= hoy)
        .or_else(|| pendientes.first())
        .cloned();
    elegido.map(|(etiqueta, fecha)| (etiqueta, fecha, (fecha - hoy).num_days()))
}

/// Lo que más se atasca al vender por internet en España, con la norma que
/// hay detrás. Se muestra en la guía para comercio y venta digital.
pub fn guia_comercio() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Vender a particulares de la UE: el umbral de 10.000 €",
            "Si vendes a distancia a consumidores de otros países de la UE más de 10.000 € al año (IVA excluido), el IVA se paga en el país del cliente y se declara en el 369 (OSS). Por debajo puedes seguir aplicando el IVA español en el 303. El alta en la ventanilla única se pide con el formulario 035.",
        ),
        (
            "Vender desde fuera de la UE: IOSS",
            "Si importas y envías a la UE paquetes de hasta 150 €, puedes registrarte en la IOSS (formulario 035) y declarar en el 369, que en este régimen es mensual, cobrando el IVA al cliente en la propia venta.",
        ),
        (
            "Marketplaces: quién es el vendedor",
            "Si vendes por Amazon, Steam, Apple, Google o similar, la plataforma es el vendedor a efectos de IVA y liquida el impuesto al cliente final. Tú le facturas a ella: si está en la UE (Apple o Google Irlanda), factura intracomunitaria con su NIF-IVA, 349 obligatorio y base en la casilla 59 del 303; si está fuera (Valve, Epic), exportación de servicios, también en la casilla 59 y sin 349. Por estas ventas no se presenta el 369.",
        ),
        (
            "Factura simplificada",
            "Puedes emitir simplificada hasta 400 € (IVA incluido) en general y hasta 3.000 € en ventas al por menor. No vale para entregas intracomunitarias ni para ventas a distancia desde otro Estado miembro.",
        ),
        (
            "Devoluciones y abonos",
            "Una devolución se documenta con factura rectificativa, no borrando la original: el registro de Verifactu ya está encadenado.",
        ),
        (
            "Pasarelas de pago",
            "Stripe, PayPal o Redsys ingresan el neto con sus comisiones: la comisión es un gasto deducible con su factura, y el ingreso que declaras es el bruto que paga el cliente.",
        ),
        (
            "Recargo de equivalencia",
            "Si eres minorista en recargo de equivalencia, no liquidas IVA de tus ventas ni deduces el soportado: tus proveedores te repercuten IVA + recargo y lo ingresan ellos.",
        ),
        (
            "Criterio de caja y SII",
            "El criterio de caja retrasa el devengo al cobro; el SII obliga a llevar los libros en tiempo real y exime del 390.",
        ),
    ]
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
        // El 369 es la declaración; el 035, el alta previa. El 368 ya no se usa.
        assert!(digital.contains(&"369"), "{digital:?}");
        assert!(digital.contains(&"035"), "{digital:?}");
        assert!(!digital.contains(&"368"), "{digital:?}");
        assert!(!servicios.contains(&"369"));
    }

    #[test]
    fn la_guia_digital_explica_el_modelo_por_tipo_de_cliente() {
        let g = guia(Actividad::Digital);
        let textos: String = g.iter().map(|(t, d)| format!("{t} {d}")).collect();
        assert!(textos.contains("369"), "{textos}");
        assert!(textos.contains("035"), "{textos}");
        assert!(textos.contains("10.000"));
        assert!(textos.contains("349"));
        assert!(textos.contains("exportación"));
        assert!(textos.contains("casilla 59"), "{textos}");
        assert!(textos.contains("ROI"), "{textos}");
        assert!(textos.contains("Resumen"), "{textos}");
        assert!(textos.contains("130") && textos.contains("200"), "{textos}");
    }

    #[test]
    fn el_motivo_explica_el_porque_segun_el_perfil() {
        let mut e = Empresa::default();
        assert!(motivo(&e, "130").contains("autónomo"));
        assert!(motivo(&e, "100").contains("personal"));

        e.persona_fisica = false;
        assert!(motivo(&e, "200").contains("sociedad"));
        assert!(motivo(&e, "202").contains("tres veces"));

        let mut sii = Empresa::default();
        sii.sii = true;
        assert!(motivo(&sii, "303").contains("cada mes"));
        assert!(motivo(&sii, "390").contains("SII"));

        let mut recargo = Empresa::default();
        recargo.recargo_equivalencia = true;
        assert!(motivo(&recargo, "309").contains("recargo"));
    }

    #[test]
    fn con_sii_el_303_es_mensual_y_el_130_no() {
        let mut db = Db::default();
        db.empresa.sii = true;
        let obs = obligaciones_con(Actividad::Servicios, &db.empresa);
        let ob303 = obs.iter().find(|o| o.codigo == "303").unwrap();
        assert!(ob303.plazo.starts_with("Mensual"), "{}", ob303.plazo);
        assert!(es_mensual(&db, ob303));

        let hoy = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        // El 1 de abril aún está pendiente el 303 de marzo (vence el 20 de abril).
        let (periodo, fecha, dias) = proximo_plazo(&db, ob303, 2026, hoy).unwrap();
        assert_eq!(periodo, "2026 · 03M");
        assert_eq!(fecha, NaiveDate::from_ymd_opt(2026, 4, 20).unwrap());
        assert_eq!(dias, 19);

        // Marcar marzo hace que el siguiente sea abril, no el 2T.
        alternar(&mut db, "2026-303-03M");
        let (periodo, _, _) = proximo_plazo(&db, ob303, 2026, hoy).unwrap();
        assert_eq!(periodo, "2026 · 04M");

        // El 130 de una persona física sigue siendo trimestral.
        let ob130 = obs.iter().find(|o| o.codigo == "130").unwrap();
        assert!(!es_mensual(&db, ob130));
        let (periodo, _, _) = proximo_plazo(&db, ob130, 2026, hoy).unwrap();
        assert_eq!(periodo, "2026 · 1T");

        // Diciembre vence el 30 de enero del año siguiente.
        assert_eq!(
            vence_mes(2026, 12),
            NaiveDate::from_ymd_opt(2027, 1, 30)
        );
    }

    #[test]
    fn el_plazo_salta_los_trimestres_ya_marcados() {
        let mut db = Db::default();
        let ob = obligaciones(Actividad::Servicios)
            .into_iter()
            .find(|o| o.codigo == "303")
            .unwrap();
        let hoy = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let (periodo, fecha, dias) = proximo_plazo(&db, &ob, 2026, hoy).unwrap();
        assert_eq!(periodo, "2026 · 1T");
        assert_eq!(fecha, NaiveDate::from_ymd_opt(2026, 4, 20).unwrap());
        assert_eq!(dias, 19);

        alternar(&mut db, "2026-303-1T");
        let (periodo, _, _) = proximo_plazo(&db, &ob, 2026, hoy).unwrap();
        assert_eq!(periodo, "2026 · 2T");
    }

    #[test]
    fn el_347_vence_el_28_de_febrero_del_ano_siguiente() {
        let ob = obligaciones(Actividad::Servicios)
            .into_iter()
            .find(|o| o.codigo == "347")
            .unwrap();
        let fecha = vence(ob.codigo, false, 2026, 0).unwrap();
        assert_eq!(fecha, NaiveDate::from_ymd_opt(2027, 2, 28).unwrap());
    }

    #[test]
    fn todas_las_obligaciones_tienen_ficha_de_presentacion() {
        let mut soc = Empresa::default();
        soc.persona_fisica = false;
        let mut recargo = Empresa::default();
        recargo.recargo_equivalencia = true;
        let mut casos: Vec<Obligacion> = Vec::new();
        for act in Actividad::TODAS {
            casos.extend(obligaciones(act));
            casos.extend(obligaciones_con(act, &soc));
            casos.extend(obligaciones_con(act, &recargo));
        }
        for o in casos {
            assert!(
                crate::presentacion::guia(o.codigo).is_some(),
                "falta la ficha de presentación del modelo {}",
                o.codigo
            );
        }
    }

    #[test]
    fn una_sociedad_cambia_irpf_por_sociedades() {
        let mut e = Empresa::default();
        e.persona_fisica = false;
        let c: Vec<&str> = obligaciones_con(Actividad::Servicios, &e)
            .iter()
            .map(|o| o.codigo)
            .collect();
        assert!(!c.contains(&"130"), "{c:?}");
        assert!(!c.contains(&"131"), "{c:?}");
        assert!(!c.contains(&"100"), "{c:?}");
        assert!(c.contains(&"200"), "{c:?}");
        assert!(c.contains(&"202"), "{c:?}");
    }

    #[test]
    fn el_regimen_cambia_los_modelos() {
        let mut objetiva = Empresa::default();
        objetiva.regimen_irpf = RegimenIrpf::Objetiva;
        let codigos: Vec<&str> = obligaciones_con(Actividad::Servicios, &objetiva)
            .iter()
            .map(|o| o.codigo)
            .collect();
        assert!(codigos.contains(&"131"), "{codigos:?}");
        assert!(!codigos.contains(&"130"), "{codigos:?}");

        let mut recargo = Empresa::default();
        recargo.recargo_equivalencia = true;
        let codigos: Vec<&str> = obligaciones_con(Actividad::Comercio, &recargo)
            .iter()
            .map(|o| o.codigo)
            .collect();
        assert!(!codigos.contains(&"303"), "{codigos:?}");
        assert!(!codigos.contains(&"390"), "{codigos:?}");
        assert!(codigos.contains(&"309"), "{codigos:?}");

        let mut sii = Empresa::default();
        sii.sii = true;
        let codigos: Vec<&str> = obligaciones_con(Actividad::Servicios, &sii)
            .iter()
            .map(|o| o.codigo)
            .collect();
        assert!(!codigos.contains(&"390"), "{codigos:?}");
        assert!(codigos.contains(&"303"), "{codigos:?}");
    }

    #[test]
    fn la_guia_de_comercio_cubre_la_venta_online() {
        let g = guia_comercio();
        assert!(g.iter().any(|(t, _)| t.contains("10.000")));
        assert!(g.iter().any(|(_, d)| d.contains("IOSS")));
        assert!(g.iter().any(|(_, d)| d.contains("rectificativa")));
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
