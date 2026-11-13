//! Verifactu: registro de facturación encadenado.
//!
//! Base legal y técnica:
//!   * RD 1007/2023 (Reglamento de sistemas informáticos de facturación).
//!   * Orden HAC/1177/2024, de 17 de octubre: especificaciones técnicas de
//!     huella, QR, registros y eventos (en vigor desde el 29/10/2024).
//!
//! Plazos vigentes (RDL 15/2025): 1-ene-2027 para contribuyentes del Impuesto
//! sobre Sociedades y 1-jul-2027 para el resto, autónomos incluidos. Hacienda
//! anunció en octubre de 2026 su intención de aplazar a 2028, pero a fecha de
//! hoy no hay norma publicada en el BOE, así que se implementa contra 2027.
//!
//! AVISO IMPORTANTE: este módulo genera el registro, la huella y el QR, pero
//! **no remite nada a la AEAT**. La remisión (modalidad VERI*FACTU) exige
//! certificado electrónico cualificado, y el productor del software debe haber
//! presentado la declaración responsable. Por eso la app funciona como un SIF
//! **NO VERI*FACTU**: usa el QR de cotejo y no la frase «factura verificable».
//!
//! La huella de alta usa los ocho campos del artículo 13 de la Orden
//! HAC/1177/2024. La de anulación usa **cinco** campos, no ocho: NIF, número y
//! fecha de la factura anulada, huella anterior y fecha/hora. Los importes de
//! `CuotaTotal` incluyen el recargo de equivalencia y `ImporteTotal` es la suma
//! de bases, IVA y recargo, **sin restar la retención de IRPF**. Los registros
//! antiguos se conservan con su formato (versión 0) para no romper cadenas ya
//! guardadas.

use crate::model::{div_round, fmt_cents, nif_normalizado, Documento, RegistroVerifactu};
use chrono::{DateTime, Local, SecondsFormat};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Versión actual del registro. Los ficheros antiguos, sin este campo, se
/// interpretan como 0 y se verifican con el algoritmo anterior.
pub const VERSION_REGISTRO: u8 = 2;

/// URL pública de cotejo para un SIF que **no remite** los registros. La app
/// no es un sistema VERI*FACTU: no debe usar `ValidarQR` ni la leyenda
/// «Factura verificable en la sede electrónica de la AEAT».
pub const URL_QR: &str =
    "https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQRNoVerifactu";

fn sha256_hex_mayusculas(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let out = h.finalize();
    let mut hex = String::with_capacity(64);
    for b in out {
        hex.push_str(&format!("{:02X}", b));
    }
    hex
}

/// Formato de importe que usa la AEAT en la huella: `1234.56`.
fn importe_aeat(cents: i64) -> String {
    let negativo = cents < 0;
    let v = cents.abs();
    format!("{}{}.{:02}", if negativo { "-" } else { "" }, v / 100, v % 100)
}

/// Fecha `dd-mm-yyyy` que usan tanto la huella como el QR.
pub fn fecha_aeat(d: chrono::NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

/// Cadena canónica de la huella.
///
/// Alta: ocho campos, unidos por `&`, con el nombre del campo, `=` y el valor.
/// Anulación: **cinco** campos, con los nombres terminados en `Anulada`; no
/// lleva tipo de factura, cuota ni importe. `Huella` es la del registro
/// anterior y va vacía en el primero de la cadena.
pub fn cadena_huella(r: &RegistroVerifactu) -> String {
    if r.tipo_registro == "N" {
        if r.version >= VERSION_REGISTRO {
            return format!(
                "IDEmisorFacturaAnulada={}&NumSerieFacturaAnulada={}&FechaExpedicionFacturaAnulada={}&Huella={}&FechaHoraHusoGenRegistro={}",
                r.nif,
                r.num_serie,
                r.fecha_expedicion,
                r.huella_anterior,
                r.fecha_hora_gen
            );
        }
        // Compatibilidad con los registros N guardados antes de corregir el
        // algoritmo. No se reescriben: su huella se conserva y se comprueba
        // con la cadena de ocho campos que se usó entonces.
        return cadena_huella_legacy(r);
    }
    format!(
        "IDEmisorFactura={}&NumSerieFactura={}&FechaExpedicionFactura={}&TipoFactura={}\
         &CuotaTotal={}&ImporteTotal={}&Huella={}&FechaHoraHusoGenRegistro={}",
        r.nif,
        r.num_serie,
        r.fecha_expedicion,
        r.tipo_factura,
        importe_aeat(r.cuota_total),
        importe_aeat(r.importe_total),
        r.huella_anterior,
        r.fecha_hora_gen
    )
}

fn cadena_huella_legacy(r: &RegistroVerifactu) -> String {
    format!(
        "IDEmisorFactura={}&NumSerieFactura={}&FechaExpedicionFactura={}&TipoFactura={}\
         &CuotaTotal={}&ImporteTotal={}&Huella={}&FechaHoraHusoGenRegistro={}",
        r.nif,
        r.num_serie,
        r.fecha_expedicion,
        r.tipo_factura,
        importe_aeat(r.cuota_total),
        importe_aeat(r.importe_total),
        r.huella_anterior,
        r.fecha_hora_gen
    )
}

pub fn calcular_huella(r: &RegistroVerifactu) -> String {
    sha256_hex_mayusculas(&cadena_huella(r))
}

/// Importes que exige la AEAT en el registro. `CuotaTotal` suma IVA y recargo
/// de equivalencia; `ImporteTotal` no resta la retención de IRPF. El total del
/// recibo que ve el cliente sigue siendo `Documento::totales().total`.
pub fn totales_aeat(doc: &Documento) -> (i64, i64) {
    let t = doc.totales();
    let cuota = t.cuota_iva + t.cuota_recargo;
    (cuota, t.base_total + t.cuota_iva + t.cuota_recargo)
}

/// Marca temporal en milisegundos desde la época, para ordenar la cadena como
/// una secuencia real y no como texto. Si no se puede interpretar, devuelve
/// `i64::MIN` y la verificación lo avisa.
pub fn clave_tiempo(r: &RegistroVerifactu) -> i64 {
    DateTime::parse_from_rfc3339(&r.fecha_hora_gen)
        .map(|f| f.timestamp_millis())
        .unwrap_or(i64::MIN)
}

pub fn fecha_hora_valida(r: &RegistroVerifactu) -> bool {
    DateTime::parse_from_rfc3339(&r.fecha_hora_gen).is_ok()
}

/// Los registros llevan la hora con segundos, así que dos operaciones
/// seguidas pueden compartir marca. Este contador local, que no entra en la
/// huella, desempata sin añadir fracciones al formato oficial.
static ULTIMO_ORDEN: AtomicU64 = AtomicU64::new(0);

fn orden_local() -> u64 {
    let ahora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        .max(1);
    let mut previo = ULTIMO_ORDEN.load(Ordering::Relaxed);
    loop {
        let siguiente = ahora.max(previo.saturating_add(1));
        match ULTIMO_ORDEN.compare_exchange_weak(
            previo,
            siguiente,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return siguiente,
            Err(actual) => previo = actual,
        }
    }
}

pub fn clave_orden(r: &RegistroVerifactu) -> (i64, u64) {
    (clave_tiempo(r), r.orden)
}

/// Percent-encoding RFC 3986 para los valores del QR.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// URL que se codifica en el QR obligatorio.
pub fn construir_qr_url(r: &RegistroVerifactu) -> String {
    format!(
        "{}?nif={}&numserie={}&fecha={}&importe={}",
        URL_QR,
        urlencode(&r.nif),
        urlencode(&r.num_serie),
        urlencode(&r.fecha_expedicion),
        urlencode(&importe_aeat(r.importe_total))
    )
}

/// Crea el registro de alta de un documento ya numerado.
/// `nif_emisor` es el NIF de **quien emite**, no el del cliente: la huella se
/// calcula sobre el obligado tributario. `huella_anterior` es la del último
/// registro de la cadena, o vacía si es el primero.
pub fn registro_alta(doc: &Documento, nif_emisor: &str, huella_anterior: &str) -> RegistroVerifactu {
    let (cuota_total, importe_total) = totales_aeat(doc);
    let mut r = RegistroVerifactu {
        tipo_registro: "A".to_string(),
        nif: nif_normalizado(nif_emisor),
        num_serie: doc.num_serie_aeat(),
        fecha_expedicion: fecha_aeat(doc.fecha),
        tipo_factura: doc.tipo_factura_aeat().to_string(),
        cuota_total,
        importe_total,
        huella: String::new(),
        huella_anterior: huella_anterior.to_string(),
        fecha_hora_gen: Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
        qr_url: String::new(),
        encadenamiento: if huella_anterior.is_empty() {
            "Primero".to_string()
        } else {
            "RegistroAnterior".to_string()
        },
        facturas_rectificadas: doc
            .rectifica
            .as_ref()
            .map(|r| vec![r.num_serie.clone()])
            .unwrap_or_default(),
        tipo_rectificativa: doc
            .rectifica
            .as_ref()
            .map(|r| r.tipo.clone())
            .unwrap_or_default(),
        version: VERSION_REGISTRO,
        orden: orden_local(),
    };
    r.huella = calcular_huella(&r);
    r.qr_url = construir_qr_url(&r);
    r
}

/// Registro de anulación: mismo encadenamiento, importes a cero.
pub fn registro_anulacion(
    doc: &Documento,
    nif_emisor: &str,
    huella_anterior: &str,
) -> RegistroVerifactu {
    let mut r = RegistroVerifactu {
        tipo_registro: "N".to_string(),
        nif: nif_normalizado(nif_emisor),
        num_serie: doc.num_serie_aeat(),
        fecha_expedicion: fecha_aeat(doc.fecha),
        // El registro de anulación no lleva TipoFactura en el XML oficial;
        // se deja vacío en los nuevos registros. La huella tampoco lo usa.
        tipo_factura: String::new(),
        cuota_total: 0,
        importe_total: 0,
        huella: String::new(),
        huella_anterior: huella_anterior.to_string(),
        fecha_hora_gen: Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
        qr_url: String::new(),
        encadenamiento: if huella_anterior.is_empty() {
            "Primero".to_string()
        } else {
            "RegistroAnterior".to_string()
        },
        facturas_rectificadas: doc
            .rectifica
            .as_ref()
            .map(|r| vec![r.num_serie.clone()])
            .unwrap_or_default(),
        tipo_rectificativa: doc
            .rectifica
            .as_ref()
            .map(|r| r.tipo.clone())
            .unwrap_or_default(),
        version: VERSION_REGISTRO,
        orden: orden_local(),
    };
    r.huella = calcular_huella(&r);
    r.qr_url = construir_qr_url(&r);
    r
}

/// Recalcula toda la cadena y devuelve los problemas encontrados.
/// Se ejecuta al arrancar: si alguien ha editado el JSON a mano, se ve.
///
/// Ordena por la marca temporal real (con su huso), no por el texto: entre
/// `+01:00` y `+02:00` el orden alfabético puede ser distinto del cronológico.
pub fn verificar_cadena(docs: &[Documento]) -> Vec<String> {
    let mut problemas = Vec::new();
    // El alta y la anulación de una misma factura son dos eslabones: la
    // anulación no sustituye al alta, se añade después.
    let mut ordenados: Vec<(usize, &Documento, &RegistroVerifactu)> = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        if let Some(r) = &d.verifactu {
            ordenados.push((i, d, r));
        }
        if let Some(r) = &d.verifactu_anulacion {
            ordenados.push((i, d, r));
        }
    }
    ordenados.sort_by(|a, b| {
        clave_orden(a.2)
            .cmp(&clave_orden(b.2))
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut anterior = String::new();
    let mut anterior_tiempo: Option<i64> = None;
    // Desde qué factura empieza la cadena. Las facturas anteriores a la
    // primera alta con registro son de antes de activar Verifactu y no se
    // deben marcar como si faltara un eslabón.
    let primer_alta_id = ordenados
        .iter()
        .filter(|(_, _, r)| r.tipo_registro == "A")
        .map(|(_, d, _)| d.id)
        .min();
    for (posicion, (_indice, d, r)) in ordenados.iter().enumerate() {
        let etiqueta = if r.tipo_registro == "N" {
            format!("{} (anulación)", d.etiqueta_numero())
        } else {
            format!("{} (alta)", d.etiqueta_numero())
        };
        let t = clave_tiempo(r);
        if !fecha_hora_valida(r) {
            problemas.push(format!(
                "{etiqueta}: «{}» no es una fecha/hora ISO-8601 con huso",
                r.fecha_hora_gen
            ));
        }
        if r.version > VERSION_REGISTRO {
            problemas.push(format!(
                "{etiqueta}: versión de registro {} desconocida; actualiza la app",
                r.version
            ));
        }
        if let Some(previo) = anterior_tiempo {
            if t < previo {
                problemas.push(format!(
                    "{etiqueta}: la marca de generación es anterior al registro que le precede"
                ));
            }
        }

        let recalculada = calcular_huella(r);
        if recalculada != r.huella {
            problemas.push(format!(
                "{etiqueta}: huella no coincide (guardada {}…, recalculada {}…)",
                &r.huella[..8.min(r.huella.len())],
                &recalculada[..8.min(recalculada.len())]
            ));
        }
        if posicion == 0 {
            if !r.huella_anterior.is_empty() {
                problemas.push(format!(
                    "{etiqueta}: es el primer registro pero dice tener una huella anterior"
                ));
            }
        } else if r.huella_anterior != anterior {
            problemas.push(format!(
                "{etiqueta}: el encadenamiento no enlaza con el registro anterior"
            ));
        }

        // La identidad de la factura también forma parte del registro, no
        // sólo los importes: así se detecta un número o una fecha cambiados.
        if r.num_serie != d.num_serie_aeat() {
            problemas.push(format!(
                "{etiqueta}: el número de serie del registro ({}) no coincide con la factura ({})",
                r.num_serie,
                d.num_serie_aeat()
            ));
        }
        if r.fecha_expedicion != fecha_aeat(d.fecha) {
            problemas.push(format!(
                "{etiqueta}: la fecha del registro ({}) no coincide con la factura ({})",
                r.fecha_expedicion,
                fecha_aeat(d.fecha)
            ));
        }

        if r.tipo_registro == "A" {
            let t = d.totales();
            // Los registros antiguos usaban cuota_iva y el total con IRPF; los
            // nuevos usan los importes oficiales de la AEAT.
            let (cuota, importe) = if r.version >= VERSION_REGISTRO {
                totales_aeat(d)
            } else {
                (t.cuota_iva, t.total)
            };
            if r.cuota_total != cuota {
                problemas.push(format!(
                    "{etiqueta}: la cuota del registro ({}) no coincide con la factura ({})",
                    fmt_cents(r.cuota_total),
                    fmt_cents(cuota)
                ));
            }
            if r.importe_total != importe {
                problemas.push(format!(
                    "{etiqueta}: el importe del registro ({}) no coincide con la factura ({})",
                    fmt_cents(r.importe_total),
                    fmt_cents(importe)
                ));
            }
            if r.tipo_factura != d.tipo_factura_aeat() {
                problemas.push(format!(
                    "{etiqueta}: el registro dice tipo {} pero el documento es {}",
                    r.tipo_factura,
                    d.tipo_factura_aeat()
                ));
            }
            if let Some(rect) = &d.rectifica {
                if !r.facturas_rectificadas.iter().any(|n| n == &rect.num_serie) {
                    problemas.push(format!(
                        "{etiqueta}: el registro R1 no cita la factura rectificada {}",
                        rect.num_serie
                    ));
                }
                if !matches!(r.tipo_rectificativa.as_str(), "I" | "S") {
                    problemas.push(format!(
                        "{etiqueta}: el registro R1 no lleva un TipoRectificativa válido (I o S)"
                    ));
                }
            } else if !r.facturas_rectificadas.is_empty() {
                problemas.push(format!(
                    "{etiqueta}: el registro cita facturas rectificadas pero la factura no es R1"
                ));
            }
        } else if r.tipo_registro == "N" {
            if d.verifactu.is_none() {
                problemas.push(format!(
                    "{etiqueta}: hay anulación pero falta el registro de alta de la factura"
                ));
            }
            if r.version >= VERSION_REGISTRO
                && (r.cuota_total != 0 || r.importe_total != 0)
            {
                problemas.push(format!(
                    "{etiqueta}: una anulación debe llevar cuota e importe a cero"
                ));
            }
            if d.estado != crate::model::Estado::Anulado {
                problemas.push(format!(
                    "{etiqueta}: hay un registro de anulación pero la factura no está anulada"
                ));
            }
        } else {
            problemas.push(format!(
                "{etiqueta}: tipo de registro «{}» desconocido",
                r.tipo_registro
            ));
        }
        if r.qr_url.trim().is_empty() {
            problemas.push(format!("{etiqueta}: falta la URL del QR"));
        } else if r.version >= VERSION_REGISTRO && r.qr_url != construir_qr_url(r) {
            problemas.push(format!(
                "{etiqueta}: el QR no coincide con los datos del registro"
            ));
        }

        anterior = r.huella.clone();
        anterior_tiempo = Some(t);
    }

    // Si hay una cadena, las facturas emitidas a partir de la primera alta
    // deben estar dentro; una intermedia sin registro rompe la trazabilidad.
    if let Some(inicio) = primer_alta_id {
        for d in docs {
            if d.id > inicio && d.tipo.es_fiscal() && d.numero > 0 && d.verifactu.is_none() {
                problemas.push(format!(
                    "{}: factura emitida después de iniciar la cadena pero sin registro de alta",
                    d.etiqueta_numero()
                ));
            }
        }
    }
    problemas
}

/// Texto de la ayuda «Cómo funciona Verifactu», en orden. Cada entrada es
/// (título, párrafo). Está aquí, junto al código que lo hace, para que la
/// explicación y el comportamiento no se separen.
pub const AYUDA: &[(&str, &str)] = &[
    (
        "Qué es, en una frase",
        "Verifactu es un registro de facturación que no se puede cambiar sin que se note. Cada factura emitida guarda una huella digital (SHA-256) encadenada con la anterior. En el alta se usan ocho campos: NIF, número, fecha, tipo de factura, cuota, importe, huella anterior y fecha/hora. En la anulación se usan sólo cinco: NIF, número y fecha de la factura anulada, huella anterior y fecha/hora. Si alguien altera una factura o borra una intermedia, la cadena deja de cuadrar y la app avisa.",
    ),
    (
        "La huella, campo a campo",
        "Alta (ocho campos, en este orden): IDEmisorFactura, NumSerieFactura, FechaExpedicionFactura, TipoFactura, CuotaTotal, ImporteTotal, Huella y FechaHoraHusoGenRegistro.\nAnulación (cinco campos): IDEmisorFacturaAnulada, NumSerieFacturaAnulada, FechaExpedicionFacturaAnulada, Huella y FechaHoraHusoGenRegistro. No lleva tipo, cuota ni importe.\nLa cadena se calcula en UTF-8, con los importes en euros y dos decimales, y se escribe en mayúsculas. El primer registro lleva Huella vacía; los demás, la huella del anterior.",
    ),
    (
        "Qué hace esta app por ti",
        "· Al pulsar «Guardar y emitir» asigna número y crea un registro de alta (tipo A) con fecha y hora.\n· Calcula la huella con los campos oficiales y la enlaza con el registro anterior.\n· Usa CuotaTotal = IVA + recargo de equivalencia e ImporteTotal = base + IVA + recargo, sin restar la retención de IRPF.\n· Genera un QR tributario de 34 mm, nivel M y zona de silencio de 2 mm, con la etiqueta «QR tributario:»; lo imprime una sola vez, al principio de la primera página, y lo enseña en la ficha.\n· Comprueba al arrancar la huella, el encadenamiento, la identidad, los importes y que no falte ninguna factura emitida.\n· Si anulas, crea un registro de anulación (tipo N) con la huella de cinco campos y con la factura marcada como anulada; no la borra.\n· Las facturas emitidas ya no se editan: se corrigen con una rectificativa R1 (botón «Rectificar R1») o se anulan.",
    ),
    (
        "Lo que NO hace (y conviene saber)",
        "· No es un sistema VERI*FACTU ni un SIF certificado: no envía los registros a la AEAT. La remisión exige certificado cualificado y que el fabricante haya presentado la declaración responsable.\n· El QR es de cotejo, no verificable: apunta a la validación para sistemas que no remiten (`ValidarQRNoVerifactu`). No lleva la leyenda «Factura verificable en la sede electrónica de la AEAT».\n· Como no se remite, la Sede no tendrá el registro y puede responder que no consta. El QR no crea ni envía nada por sí solo.\n· No firma el FacturaE (XAdES) ni lo valida contra el XSD oficial.\n· No sustituye a tu asesor: calcula casillas y plazos, pero presentar es cosa tuya.",
    ),
    (
        "Cómo se usa, paso a paso",
        "1. Actívalo antes de emitir la primera factura: Ajustes > Verifactu > «Generar registro encadenado y QR». Si lo activas después, las facturas anteriores no se encadenan hacia atrás.\n2. Rellena bien nombre, NIF y dirección. El NIF va en la huella y en el QR; la app lo guarda en mayúsculas y sin espacios, guiones ni puntos. Si la letra de control no cuadra, no deja emitir con Verifactu activo.\n3. Emite con «Guardar y emitir». No uses «Guardar» para una factura que ya tiene número.\n4. En el PDF verás «QR tributario:». Es un QR de cotejo no verificable: no remite nada. En pantalla aparece el mismo QR y el aviso de registro local.\n5. Para corregir sin anular, abre la factura y pulsa «Rectificar R1»: indicas la diferencia de base, el IVA y el motivo; se emite una rectificativa encadenada y la original queda marcada como rectificada. Para anular, usa «Anular»: la factura se conserva y se añade el registro N.\n6. Si Inicio dice «La cadena de registros no cuadra», no sigas emitiendo: restaura la última copia buena.\n7. Guarda una copia antes de cada cierre y no edites datos.json a mano.\nPlazos: 1 de enero de 2027 para sociedades y 1 de julio de 2027 para autónomos; el posible aplazamiento a 2028 anunciado en octubre de 2026 no está en el BOE.",
    ),
    (
        "Rectificativas R1: corregir sin borrar",
        "Si una factura emitida tiene un importe, un IVA o un dato equivocado, no la edites ni la borres: abre su ficha y pulsa «Rectificar R1». Indicas la diferencia de base, el IVA de esa diferencia y el motivo. La app crea una factura nueva en la serie R<aaaa>, con TipoFactura=R1 y TipoRectificativa=I (por diferencias), y la encadena con el registro anterior. La original no se anula: queda marcada como rectificada y sigue contando en la cadena y en los modelos; la R1 suma la corrección con su signo, así que el 303, el 347 y el 349 cuadran sin excluirla.\n· La R1 es por diferencias: si bajas 100 €, la R1 lleva base -100 €; no repite la factura entera.\n· Si la original mezcla varios tipos de IVA, la ventana usa un solo tipo; si la corrección afecta a varios, crea una R1 por cada tipo sobre la original.\n· Una R1 no se rectifica: si te equivocas, anula la R1 y crea otra desde la original. Mientras la R1 esté viva, la original no se puede anular.\n· El PDF de la R1 se titula «FACTURA RECTIFICATIVA», cita la original y el motivo. FacturaE no se genera para R1: el XML correctivo no está implementado.",
    ),
    (
        "Errores típicos y qué hacer",
        "· «Quiero cambiar el importe de una factura emitida»: no se edita; usa «Rectificar R1» y la original se conserva.\n· «He borrado una factura»: la cadena se rompe; restaura la copia anterior. La app no tiene papelera.\n· «He activado Verifactu tarde»: las facturas anteriores no tienen registro y no se reescriben; empieza a encadenar desde la siguiente. Si anulas una de ellas, no se le inventa una anulación suelta: queda anulada sin registro N.\n· «El QR no coincide»: lleva NIF, número, fecha e importe oficial; si cambiaste la factura, ya no vale.\n· «Número repetido»: no reutilices números; deja que la app numere por serie.\n· «La Sede no encuentra la factura»: es normal en este modo; el QR es de cotejo no verificable y no crea el registro en la AEAT.\n· «La cuota o el importe no cuadran»: en el registro, CuotaTotal incluye IVA y recargo, e ImporteTotal no resta el IRPF; el TOTAL del recibo sí lo resta.",
    ),
    (
        "Plazos y a quién obliga",
        "· Sociedades (Impuesto de Sociedades): 1 de enero de 2027.\n· Autónomos y resto: 1 de julio de 2027.\n· Hacienda anunció en octubre de 2026 un posible aplazamiento a 2028, pero a 12 de noviembre de 2026 no hay norma publicada en el BOE: la app se guía por 2027. Compruébalo antes de fiarte.\n· Si aún no estás obligado, puedes activarlo igualmente: no molesta y deja las facturas preparadas.",
    ),
    (
        "Qué guardar y cuánto tiempo",
        "Conserva datos.json y las copias. La normativa fiscal general habla de 4 años y la mercantil de 6; ante la duda, no borres: cambia de equipo copiando la carpeta entera. Restaurar una copia no fusiona registros: sustituye el estado, así que no mezcles dos copias del mismo periodo.",
    ),
];

/// Preguntas cortas de la misma ayuda. Se muestran detrás de las secciones.
pub const PREGUNTAS: &[(&str, &str)] = &[
    (
        "Tengo que activarlo ya",
        "Si estás obligado en 2027, sí. Si aún no, es opcional, pero recomendable: el registro y el QR no estorban y evitan tener que rehacer facturas después.",
    ),
    (
        "Puedo hacer una factura rectificativa",
        "Sí. Abre la factura emitida y pulsa «Rectificar R1»: indicas la diferencia de base, el IVA y el motivo. Se crea una R1 encadenada y la original no se anula.",
    ),
    (
        "Cuándo rectifico y cuándo anulo",
        "Rectifica con una R1 si la factura ya se entregó o se registró y hay que corregir importe, IVA o datos sin borrarla. Anula solo si la factura no debió existir o si la R1 que hiciste no es válida; la anulación conserva la factura y añade un registro N.",
    ),
    (
        "Puedo rectificar una R1",
        "No desde la app. Si una R1 está mal, anula esa R1 y crea otra corrección sobre la factura original. Así no se encadenan rectificativas de rectificativas.",
    ),
    (
        "Una R1 cambia el 303 y el 347",
        "Sí, con su signo. La original sigue contando y la R1 suma la diferencia: si la R1 es negativa, resta base y cuota en el trimestre en que se emite. El 347 y el 349 también la incluyen.",
    ),
    (
        "Qué pasa con el FacturaE de una R1",
        "No se genera: el XML correctivo de FacturaE no está implementado. El PDF y el registro Verifactu sí se generan. Si necesitas FacturaE para una R1, prepáralo fuera y revísalo con tu asesor.",
    ),
    (
        "Sirve para presupuestos y proformas",
        "No. Solo las facturas entran en la cadena; presupuestos y proformas no son fiscales y no llevan registro ni QR.",
    ),
    (
        "Puedo emitir sin NIF del cliente",
        "Sí: sale una factura simplificada (F2). Para que el cliente deduzca el IVA necesitará la completa con su NIF (F1).",
    ),
    (
        "Qué pasa si anulo una factura",
        "No se borra: queda anulada y se añade un registro de anulación (N) con importes a cero, enlazado con el anterior.",
    ),
    (
        "Puedo cambiar la numeración o la serie",
        "Puedes usar series distintas, cada una con su contador. Lo que no debes hacer es repetir un número ni renumerar facturas ya emitidas.",
    ),
    (
        "El QR es obligatorio",
        "En las facturas con registro, sí. La app imprime «QR tributario:» de 34 mm, nivel M y zona de silencio de 2 mm, y lo enseña en la ficha. Es un QR de cotejo no verificable: no remite nada. En presupuestos y proformas no se pone.",
    ),
    (
        "Por qué el QR no dice «verificable»",
        "Porque esta app no es un sistema VERI*FACTU y no remite los registros. La leyenda «Factura verificable en la sede electrónica de la AEAT» se reserva a los sistemas que sí los envían. Aquí el QR apunta a la validación de sistemas que no remiten y sirve para cotejar los datos, no para acreditar que la AEAT los tenga.",
    ),
    (
        "La huella de anulación es distinta",
        "Sí: la anulación no copia los ocho campos del alta. Usa cinco (NIF, número y fecha de la factura anulada, huella anterior y fecha/hora) y deja fuera tipo, cuota e importe. La app ya lo calcula así; los registros antiguos se conservan con su formato para no romper cadenas ya guardadas.",
    ),
    (
        "Puedo usar la app en dos ordenadores",
        "La cadena es global y vive en datos.json. Usa una sola carpeta o restaura una copia; no mezcles dos archivos del mismo periodo.",
    ),
    (
        "Y si el reloj del ordenador está mal",
        "La fecha y hora de generación van en la huella. Pon bien la hora de Windows antes de emitir; los registros ya creados no cambian.",
    ),
];

/// Comprueba que la cuota del registro cuadra con el desglose (útil en tests).
pub fn cuadra_cuota(doc: &Documento) -> bool {
    let t = doc.totales();
    let teoricas: i64 = t
        .bases
        .iter()
        .map(|(tipo, base, _)| div_round(*base * *tipo as i64, 10_000))
        .sum();
    teoricas == t.cuota_iva
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cliente, Estado, Linea, Rectificacion, TipoDoc};
    use chrono::NaiveDate;

    fn doc_de_prueba() -> Documento {
        let cliente = Cliente {
            id: 1,
            nombre: "Agencia Creativa S.L.".to_string(),
            nif: "B76281996".to_string(),
            ..Default::default()
        };
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente, 0, "2026".to_string());
        d.numero = 14;
        d.fecha = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        d.lineas = vec![Linea {
            descripcion: "Identidad corporativa".to_string(),
            cantidad_milli: 1000,
            precio_cents: 200_000,
            descuento_pb: 0,
            iva_pb: 2100,
        }];
        d
    }

    #[test]
    fn huella_es_estable_y_de_64_hex() {
        let d = doc_de_prueba();
        let r = registro_alta(&d, "51234567H", "");
        assert_eq!(r.huella.len(), 64);
        assert!(r.huella.chars().all(|c| c.is_ascii_hexdigit()));
        // Mismo registro -> misma huella.
        assert_eq!(calcular_huella(&r), r.huella);
        assert_eq!(r.encadenamiento, "Primero");
        assert_eq!(r.tipo_factura, "F1");
    }

    #[test]
    fn cadena_canonica_tiene_los_ocho_campos_en_orden() {
        let d = doc_de_prueba();
        let r = registro_alta(&d, "51234567H", "");
        let cadena = cadena_huella(&r);
        let claves: Vec<&str> = cadena
            .split('&')
            .map(|p| p.split('=').next().unwrap())
            .collect();
        assert_eq!(
            claves,
            vec![
                "IDEmisorFactura",
                "NumSerieFactura",
                "FechaExpedicionFactura",
                "TipoFactura",
                "CuotaTotal",
                "ImporteTotal",
                "Huella",
                "FechaHoraHusoGenRegistro"
            ]
        );
    }

    #[test]
    fn totales_cuadran_al_centimo() {
        let d = doc_de_prueba();
        let t = d.totales();
        assert_eq!(t.base_total, 200_000);
        assert_eq!(t.cuota_iva, 42_000);
        assert_eq!(t.total, 242_000);
        assert!(cuadra_cuota(&d));
    }

    #[test]
    fn la_cadena_detecta_manipulacion() {
        let mut a = doc_de_prueba();
        a.id = 1;
        a.estado = Estado::Emitido;
        let r = registro_alta(&a, "51234567H", "");
        a.verifactu = Some(r.clone());

        let mut b = doc_de_prueba();
        b.id = 2;
        b.numero = 15;
        b.estado = Estado::Emitido;
        b.verifactu = Some(registro_alta(&b, "51234567H", &r.huella));

        assert!(verificar_cadena(&[a.clone(), b.clone()]).is_empty());

        let mut manipulado = b.clone();
        if let Some(rf) = manipulado.verifactu.as_mut() {
            rf.importe_total += 100;
        }
        assert!(!verificar_cadena(&[a, manipulado]).is_empty());
    }

    #[test]
    fn la_rectificativa_r1_lleva_su_tipo_y_la_factura_rectificada() {
        let mut d = doc_de_prueba();
        d.rectifica = Some(Rectificacion {
            documento_id: 3,
            num_serie: "20260014".to_string(),
            etiqueta: "2026/0014".to_string(),
            fecha: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
            tipo: "I".to_string(),
        });
        d.lineas[0].precio_cents = -20_000;
        let r = registro_alta(&d, "51234567H", "");
        assert_eq!(r.tipo_factura, "R1");
        assert_eq!(r.facturas_rectificadas, vec!["20260014".to_string()]);
        assert_eq!(r.tipo_rectificativa, "I");
        assert_eq!(r.cuota_total, -4_200);
        assert_eq!(r.importe_total, -24_200);
        assert!(r.qr_url.contains("importe=-242.00"));
    }

    #[test]
    fn la_r1_encadena_con_la_original_y_no_la_sustituye() {
        let mut original = doc_de_prueba();
        original.estado = Estado::Emitido;
        let alta = registro_alta(&original, "51234567H", "");
        original.verifactu = Some(alta.clone());

        let mut r1 = doc_de_prueba();
        r1.numero = 15;
        r1.estado = Estado::Emitido;
        r1.rectifica = Some(Rectificacion {
            documento_id: original.id,
            num_serie: original.num_serie_aeat(),
            etiqueta: original.etiqueta_numero(),
            fecha: original.fecha,
            tipo: "I".to_string(),
        });
        r1.lineas[0].precio_cents = -10_000;
        r1.verifactu = Some(registro_alta(&r1, "51234567H", &alta.huella));

        let problemas = verificar_cadena(&[original, r1]);
        assert!(problemas.is_empty(), "cadena rota: {problemas:?}");
    }

    #[test]
    fn la_anulacion_se_anade_sin_borrar_el_alta() {
        let mut d = doc_de_prueba();
        d.estado = Estado::Emitido;
        let alta = registro_alta(&d, "51234567H", "");
        d.verifactu = Some(alta.clone());
        d.estado = Estado::Anulado;
        d.verifactu_anulacion = Some(registro_anulacion(&d, "51234567H", &alta.huella));

        let problemas = verificar_cadena(&[d.clone()]);
        assert!(problemas.is_empty(), "cadena rota: {problemas:?}");
        assert_eq!(d.verifactu.as_ref().unwrap().tipo_registro, "A");
        assert_eq!(
            d.verifactu_anulacion.as_ref().unwrap().tipo_registro,
            "N"
        );
        assert_eq!(
            d.verifactu_anulacion.as_ref().unwrap().importe_total,
            0
        );
    }

    /// La anulación oficial no usa los ocho campos del alta: son cinco, con
    /// los nombres terminados en Anulada. El vector es el publicado por la
    /// AEAT para comprobar el algoritmo.
    #[test]
    fn la_huella_de_anulacion_coincide_con_el_vector_oficial() {
        let r = RegistroVerifactu {
            tipo_registro: "N".to_string(),
            nif: "89890001K".to_string(),
            num_serie: "12345679/G34".to_string(),
            fecha_expedicion: "01-01-2024".to_string(),
            tipo_factura: String::new(),
            cuota_total: 0,
            importe_total: 0,
            huella: String::new(),
            huella_anterior: "F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97"
                .to_string(),
            fecha_hora_gen: "2024-01-01T19:20:40+01:00".to_string(),
            qr_url: String::new(),
            encadenamiento: "Registro anterior".to_string(),
            facturas_rectificadas: Vec::new(),
            tipo_rectificativa: String::new(),
            version: VERSION_REGISTRO,
            orden: 1,
        };
        // El vector oficial usa esta huella anterior abreviada; se comprueba
        // sobre la cadena canónica, no sobre un hash inventado.
        let cadena = cadena_huella(&r);
        assert!(cadena.starts_with("IDEmisorFacturaAnulada=89890001K&"));
        assert!(!cadena.contains("TipoFactura="));
        assert!(!cadena.contains("CuotaTotal="));
        let claves: Vec<&str> = cadena
            .split('&')
            .map(|p| p.split('=').next().unwrap())
            .collect();
        assert_eq!(
            claves,
            vec![
                "IDEmisorFacturaAnulada",
                "NumSerieFacturaAnulada",
                "FechaExpedicionFacturaAnulada",
                "Huella",
                "FechaHoraHusoGenRegistro"
            ]
        );
    }

    /// Segundo vector publicado: el SHA-256 completo de la anulación.
    #[test]
    fn el_hash_de_anulacion_oficial_es_el_publicado() {
        let r = RegistroVerifactu {
            tipo_registro: "N".to_string(),
            nif: "89890001K".to_string(),
            num_serie: "12345679/G34".to_string(),
            fecha_expedicion: "01-01-2024".to_string(),
            tipo_factura: String::new(),
            cuota_total: 0,
            importe_total: 0,
            huella: String::new(),
            huella_anterior: "F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97"
                .to_string(),
            fecha_hora_gen: "2024-01-01T19:20:40+01:00".to_string(),
            qr_url: String::new(),
            encadenamiento: "Registro anterior".to_string(),
            facturas_rectificadas: Vec::new(),
            tipo_rectificativa: String::new(),
            version: VERSION_REGISTRO,
            orden: 1,
        };
        assert_eq!(
            calcular_huella(&r),
            "177547C0D57AC74748561D054A9CEC14B4C4EA23D1BEFD6F2E69E3A388F90C68"
        );
    }

    #[test]
    fn la_huella_de_alta_coincide_con_el_vector_oficial() {
        let r = RegistroVerifactu {
            tipo_registro: "A".to_string(),
            nif: "89890001K".to_string(),
            num_serie: "12345678/G33".to_string(),
            fecha_expedicion: "01-01-2024".to_string(),
            tipo_factura: "F1".to_string(),
            cuota_total: 1_235,
            importe_total: 12_345,
            huella: String::new(),
            huella_anterior: String::new(),
            fecha_hora_gen: "2024-01-01T19:20:30+01:00".to_string(),
            qr_url: String::new(),
            encadenamiento: "Primero".to_string(),
            facturas_rectificadas: Vec::new(),
            tipo_rectificativa: String::new(),
            version: VERSION_REGISTRO,
            orden: 1,
        };
        assert_eq!(
            calcular_huella(&r),
            "3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60"
        );
    }

    #[test]
    fn cuota_e_importe_del_registro_son_los_oficiales() {
        let mut d = doc_de_prueba();
        d.estado = Estado::Emitido;
        d.irpf_pb = 1_500;
        d.recargo_pb = 520;
        let t = d.totales();
        let r = registro_alta(&d, "51234567H", "");
        assert_eq!(r.cuota_total, t.cuota_iva + t.cuota_recargo);
        assert_eq!(
            r.importe_total,
            t.base_total + t.cuota_iva + t.cuota_recargo
        );
        assert_ne!(r.importe_total, t.total);
        d.verifactu = Some(r);
        assert!(verificar_cadena(&[d]).is_empty());
    }

    #[test]
    fn json_antiguo_sin_version_ni_orden_se_lee_sin_romperse() {
        let d = doc_de_prueba();
        let r = registro_alta(&d, "51234567H", "");
        let mut valor = serde_json::to_value(&r).expect("serializar el registro");
        let obj = valor.as_object_mut().expect("objeto JSON");
        obj.remove("version");
        obj.remove("orden");
        let antiguo: RegistroVerifactu =
            serde_json::from_value(valor).expect("leer un registro sin los campos nuevos");
        assert_eq!(antiguo.version, 0);
        assert_eq!(antiguo.orden, 0);
        assert_eq!(calcular_huella(&antiguo), antiguo.huella);
    }

    #[test]
    fn el_orden_local_desempata_registros_del_mismo_segundo() {
        let mut a = doc_de_prueba();
        a.id = 1;
        a.numero = 1;
        a.estado = Estado::Emitido;
        let mut alta = registro_alta(&a, "51234567H", "");
        alta.fecha_hora_gen = "2026-03-15T12:00:00+01:00".to_string();
        alta.orden = 1;
        alta.huella = calcular_huella(&alta);
        alta.qr_url = construir_qr_url(&alta);
        a.verifactu = Some(alta.clone());

        let mut b = doc_de_prueba();
        b.id = 2;
        b.numero = 2;
        b.estado = Estado::Emitido;
        let mut segunda = registro_alta(&b, "51234567H", &alta.huella);
        segunda.fecha_hora_gen = alta.fecha_hora_gen.clone();
        segunda.orden = 2;
        segunda.huella = calcular_huella(&segunda);
        segunda.qr_url = construir_qr_url(&segunda);
        b.verifactu = Some(segunda.clone());

        // La anulación de la primera se generó después de la segunda, pero con
        // la misma marca de un segundo: sólo el orden local la coloca bien.
        a.estado = Estado::Anulado;
        let mut anulacion = registro_anulacion(&a, "51234567H", &segunda.huella);
        anulacion.fecha_hora_gen = alta.fecha_hora_gen.clone();
        anulacion.orden = 3;
        anulacion.huella = calcular_huella(&anulacion);
        anulacion.qr_url = construir_qr_url(&anulacion);
        a.verifactu_anulacion = Some(anulacion);

        let problemas = verificar_cadena(&[a, b]);
        assert!(problemas.is_empty(), "{problemas:?}");
    }

    #[test]
    fn una_factura_anterior_a_la_activacion_no_rompe_la_cadena() {
        let mut vieja = doc_de_prueba();
        vieja.id = 1;
        vieja.numero = 1;
        vieja.estado = Estado::Emitido;

        let mut nueva = doc_de_prueba();
        nueva.id = 2;
        nueva.numero = 15;
        nueva.estado = Estado::Emitido;
        nueva.verifactu = Some(registro_alta(&nueva, "51234567H", ""));

        assert!(
            verificar_cadena(&[vieja, nueva]).is_empty(),
            "una factura previa sin registro no debe contar como eslabón perdido"
        );
    }

    #[test]
    fn la_cadena_detecta_una_factura_posterior_sin_registro() {
        let mut primera = doc_de_prueba();
        primera.id = 1;
        primera.numero = 1;
        primera.estado = Estado::Emitido;
        primera.verifactu = Some(registro_alta(&primera, "51234567H", ""));

        let mut huerfana = doc_de_prueba();
        huerfana.id = 2;
        huerfana.numero = 2;
        huerfana.estado = Estado::Emitido;

        let problemas = verificar_cadena(&[primera, huerfana]);
        assert!(
            problemas.iter().any(|p| p.contains("sin registro de alta")),
            "{problemas:?}"
        );
    }

    #[test]
    fn el_nif_se_normaliza_antes_de_la_huella_y_el_qr() {
        let d = doc_de_prueba();
        let r = registro_alta(&d, " 12345678-z ", "");
        assert_eq!(r.nif, "12345678Z");
        assert!(r.qr_url.contains("nif=12345678Z"));
    }

    #[test]
    fn una_anulacion_sin_alta_es_un_error_de_cadena() {
        let mut d = doc_de_prueba();
        d.estado = Estado::Anulado;
        d.verifactu_anulacion = Some(registro_anulacion(&d, "51234567H", ""));
        let problemas = verificar_cadena(&[d]);
        assert!(
            problemas.iter().any(|p| p.contains("falta el registro de alta")),
            "{problemas:?}"
        );
    }

    #[test]
    fn la_cadena_detecta_una_anulacion_manipulada() {
        let mut d = doc_de_prueba();
        d.estado = Estado::Emitido;
        let alta = registro_alta(&d, "51234567H", "");
        d.verifactu = Some(alta.clone());
        d.estado = Estado::Anulado;
        let mut n = registro_anulacion(&d, "51234567H", &alta.huella);
        n.fecha_expedicion = "16-03-2026".to_string();
        d.verifactu_anulacion = Some(n);
        let problemas = verificar_cadena(&[d]);
        assert!(
            problemas.iter().any(|p| p.contains("fecha del registro")),
            "{problemas:?}"
        );
    }

    #[test]
    fn el_qr_no_es_verificable_y_escapa_los_datos() {
        let mut d = doc_de_prueba();
        d.serie = "A/B&".to_string();
        let r = registro_alta(&d, "51234567H", "");
        assert!(r.qr_url.starts_with(URL_QR));
        assert!(r.qr_url.contains("ValidarQRNoVerifactu"));
        assert!(r.qr_url.contains("A%2FB%26"));
    }

    #[test]
    fn la_cadena_detecta_un_tipo_de_factura_cambiado() {
        let mut d = doc_de_prueba();
        let mut r = registro_alta(&d, "51234567H", "");
        r.tipo_factura = "F2".to_string();
        r.huella = calcular_huella(&r);
        d.verifactu = Some(r);
        let problemas = verificar_cadena(&[d]);
        assert!(
            problemas.iter().any(|p| p.contains("tipo F2")),
            "{problemas:?}"
        );
    }

    #[test]
    fn la_cadena_detecta_una_r1_sin_referencia_a_la_original() {
        let mut d = doc_de_prueba();
        d.rectifica = Some(Rectificacion {
            documento_id: 3,
            num_serie: d.num_serie_aeat(),
            etiqueta: d.etiqueta_numero(),
            fecha: d.fecha,
            tipo: "I".to_string(),
        });
        let mut r = registro_alta(&d, "51234567H", "");
        r.facturas_rectificadas.clear();
        r.huella = calcular_huella(&r);
        d.verifactu = Some(r);
        let problemas = verificar_cadena(&[d]);
        assert!(
            problemas.iter().any(|p| p.contains("no cita la factura rectificada")),
            "{problemas:?}"
        );
    }

    #[test]
    fn la_ayuda_cubre_los_puntos_clave() {
        let todo: String = AYUDA
            .iter()
            .map(|(t, c)| format!("{t}\n{c}\n"))
            .collect();
        for clave in [
            "SHA-256",
            "No es un sistema VERI*FACTU",
            "cinco campos",
            "CuotaTotal",
            "ImporteTotal",
            "recargo de equivalencia",
            "no verificable",
            "no consta",
            "rectificativa",
            "Rectificativas R1",
            "TipoRectificativa",
            "303",
            "FacturaE",
            "registro de anulación",
            "1 de enero de 2027",
            "1 de julio de 2027",
            "datos.json",
        ] {
            assert!(todo.contains(clave), "la ayuda no menciona: {clave}");
        }
        assert!(PREGUNTAS.len() >= 6);
        assert!(AYUDA.len() >= 6);
    }

    #[test]
    fn qr_apunta_al_cotejo_no_verificable() {
        let d = doc_de_prueba();
        let r = registro_alta(&d, "51234567H", "");
        assert!(r.qr_url.starts_with(URL_QR));
        assert!(r.qr_url.contains("ValidarQRNoVerifactu"));
        assert!(!r.qr_url.contains("/ValidarQR?"));
        // El QR identifica al **emisor**, no al cliente.
        assert!(r.qr_url.contains("nif=51234567H"));
        assert!(r.qr_url.contains("fecha=15-03-2026"));
        assert!(r.qr_url.contains("importe=2420.00"));
    }

    #[test]
    fn la_huella_encadenada_coincide_con_el_vector_oficial() {
        let r = RegistroVerifactu {
            tipo_registro: "A".to_string(),
            nif: "89890001K".to_string(),
            num_serie: "12345679/G34".to_string(),
            fecha_expedicion: "01-01-2024".to_string(),
            tipo_factura: "F1".to_string(),
            cuota_total: 1_235,
            importe_total: 12_345,
            huella: String::new(),
            huella_anterior: "3C464DAF61ACB827C65FDA19F352A4E3BDC2C640E9E9FC4CC058073F38F12F60"
                .to_string(),
            fecha_hora_gen: "2024-01-01T19:20:35+01:00".to_string(),
            qr_url: String::new(),
            encadenamiento: "Registro anterior".to_string(),
            facturas_rectificadas: Vec::new(),
            tipo_rectificativa: String::new(),
            version: VERSION_REGISTRO,
            orden: 1,
        };
        assert_eq!(
            calcular_huella(&r),
            "F7B94CFD8924EDFF273501B01EE5153E4CE8F259766F88CF6ACB8935802A2B97"
        );
    }
}
