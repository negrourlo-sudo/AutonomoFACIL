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
//! presentado la declaración responsable. El orden exacto de campos y la
//! codificación de la huella deben contrastarse con la especificación vigente
//! antes de un uso real; aquí se sigue el orden publicado, con test que fija el
//! algoritmo para que cualquier cambio sea un diff explícito.

use crate::model::{fmt_cents, div_round, Documento, RegistroVerifactu};
use chrono::{Local, SecondsFormat};
use sha2::{Digest, Sha256};

/// URL pública de validación del QR que exige el reglamento.
pub const URL_QR: &str = "https://www2.agenciatributaria.gob.es/wlpl/TIKE-CONT/ValidarQR";

/// Endpoint SOAP de remisión (modalidad VERI*FACTU, entorno real).
pub const ENDPOINT_SOAP_PRODUCCION: &str =
    "https://www1.agenciatributaria.gob.es/wlpl/TIKE-CONT/ws/SistemaFacturacion/VerifactuSOAP";
/// Endpoint SOAP de pruebas.
pub const ENDPOINT_SOAP_PRUEBAS: &str =
    "https://prewww1.aeat.es/wlpl/TIKE-CONT/ws/SistemaFacturacion/VerifactuSOAP";

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
/// Los ocho campos van en este orden, unidos por `&`, con el nombre del campo,
/// `=` y el valor. `Huella` es la del registro anterior y va vacía en el
/// primero de la cadena.
pub fn cadena_huella(r: &RegistroVerifactu) -> String {
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
    let t = doc.totales();
    let mut r = RegistroVerifactu {
        tipo_registro: "A".to_string(),
        nif: nif_emisor.to_string(),
        num_serie: doc.num_serie_aeat(),
        fecha_expedicion: fecha_aeat(doc.fecha),
        tipo_factura: doc.tipo_factura_aeat().to_string(),
        cuota_total: t.cuota_iva,
        importe_total: t.total,
        huella: String::new(),
        huella_anterior: huella_anterior.to_string(),
        fecha_hora_gen: Local::now().to_rfc3339_opts(SecondsFormat::Secs, false),
        qr_url: String::new(),
        encadenamiento: if huella_anterior.is_empty() {
            "Primero".to_string()
        } else {
            "RegistroAnterior".to_string()
        },
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
        nif: nif_emisor.to_string(),
        num_serie: doc.num_serie_aeat(),
        fecha_expedicion: fecha_aeat(doc.fecha),
        tipo_factura: doc.tipo_factura_aeat().to_string(),
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
    };
    r.huella = calcular_huella(&r);
    r.qr_url = construir_qr_url(&r);
    r
}

/// Recalcula toda la cadena y devuelve los problemas encontrados.
/// Se ejecuta al arrancar: si alguien ha editado el JSON a mano, se ve.
pub fn verificar_cadena(docs: &[Documento]) -> Vec<String> {
    let mut problemas = Vec::new();
    let mut anterior = String::new();
    let mut ordenados: Vec<&Documento> = docs.iter().filter(|d| d.verifactu.is_some()).collect();
    ordenados.sort_by(|a, b| {
        let fa = a.verifactu.as_ref().map(|r| r.fecha_hora_gen.clone()).unwrap_or_default();
        let fb = b.verifactu.as_ref().map(|r| r.fecha_hora_gen.clone()).unwrap_or_default();
        fa.cmp(&fb)
    });

    for d in ordenados {
        let r = d.verifactu.as_ref().unwrap();
        let recalculada = calcular_huella(r);
        if recalculada != r.huella {
            problemas.push(format!(
                "{}: huella no coincide (guardada {}…, recalculada {}…)",
                d.etiqueta_numero(),
                &r.huella[..8.min(r.huella.len())],
                &recalculada[..8.min(recalculada.len())]
            ));
        }
        if r.huella_anterior != anterior {
            problemas.push(format!(
                "{}: el encadenamiento no enlaza con el registro anterior",
                d.etiqueta_numero()
            ));
        }
        if r.importe_total != d.totales().total && r.tipo_registro == "A" {
            problemas.push(format!(
                "{}: el importe del registro ({}) no coincide con la factura ({})",
                d.etiqueta_numero(),
                fmt_cents(r.importe_total),
                fmt_cents(d.totales().total)
            ));
        }
        anterior = r.huella.clone();
    }
    problemas
}

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
    use crate::model::{Cliente, Estado, Linea, TipoDoc};
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
    fn qr_apunta_a_la_sede_de_la_aeat() {
        let d = doc_de_prueba();
        let r = registro_alta(&d, "51234567H", "");
        assert!(r.qr_url.starts_with(URL_QR));
        // El QR identifica al **emisor**, no al cliente.
        assert!(r.qr_url.contains("nif=51234567H"));
        assert!(r.qr_url.contains("fecha=15-03-2026"));
        assert!(r.qr_url.contains("importe=2420.00"));
    }
}
