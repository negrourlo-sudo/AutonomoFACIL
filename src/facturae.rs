//! FacturaE 3.2.2 (formato oficial, `facturae.gob.es`).
//!
//! El orden de los elementos no es decorativo: el XSD es una `sequence` y un
//! nodo fuera de sitio invalida el fichero. La estructura replicada aquí está
//! tomada de un documento validado contra `Facturaev3_2_2.xsd`.

use crate::model::{div_round, Documento, Empresa};
use std::collections::BTreeMap;

/// `1234.56`, sin separador de miles: en XML el punto es el decimal.
fn imp(c: i64) -> String {
    let neg = c < 0;
    let v = c.abs();
    format!("{}{}.{:02}", if neg { "-" } else { "" }, v / 100, v % 100)
}

/// Cantidad con dos decimales, o tres si el tercero no es cero: `1.25`,
/// `1.255`, `0.125`.
fn cant(m: i64) -> String {
    let neg = m < 0;
    let v = m.abs();
    let ent = v / 1000;
    let frac = v % 1000;
    let signo = if neg { "-" } else { "" };
    if frac % 10 == 0 {
        format!("{}{}.{:02}", signo, ent, frac / 10)
    } else {
        format!("{}{}.{:03}", signo, ent, frac)
    }
}

/// Porcentaje con dos decimales: `21.00`.
fn pct2(pb: i32) -> String {
    format!("{}.{:02}", pb / 100, (pb % 100).abs())
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// NIF con prefijo de país para `TaxIdentificationNumber`.
fn nif_pais(nif: &str) -> String {
    let n = nif.trim().to_uppercase().replace([' ', '-', '.'], "");
    if n.starts_with("ES") || n.len() < 2 {
        n
    } else {
        format!("ES{}", n)
    }
}

fn direccion_esp(direccion: &str, cp: &str, ciudad: &str, provincia: &str, sangria: &str) -> String {
    format!(
        "{s}<AddressInSpain>\n\
         {s}  <Address>{}</Address>\n\
         {s}  <PostCode>{}</PostCode>\n\
         {s}  <Town>{}</Town>\n\
         {s}  <Province>{}</Province>\n\
         {s}  <CountryCode>ESP</CountryCode>\n\
         {s}</AddressInSpain>\n",
        esc(direccion),
        esc(cp),
        esc(ciudad),
        esc(provincia),
        s = sangria
    )
}

/// Nombre del fichero: `facturae-2026-0042-agencia-creativa-s-l.xml`.
pub fn nombre_fichero(doc: &Documento) -> String {
    let serie = crate::pdf::slug(&doc.serie);
    let serie = if serie.is_empty() {
        "sin-serie".to_string()
    } else {
        serie
    };
    let cliente = crate::pdf::slug(&doc.cliente.nombre_completo());
    let cliente = if cliente.is_empty() {
        "sin-cliente".to_string()
    } else {
        cliente
    };
    format!("facturae-{serie}-{:04}-{cliente}.xml", doc.numero)
}

/// Cuota de IVA de cada línea, calculada sobre la base acumulada de su tipo.
/// La suma de las cuotas coincide exactamente con la cuota agrupada de
/// `Documento::totales()`; no hace falta repartir diferencias después.
fn cuotas_iva_lineas(doc: &Documento) -> Vec<i64> {
    if !doc.operacion.lleva_iva() {
        return vec![0; doc.lineas.len()];
    }

    let mut acumulado: BTreeMap<i32, (i64, i64)> = BTreeMap::new();
    doc.lineas
        .iter()
        .map(|l| {
            let (base, cuota_previa) = acumulado.entry(l.iva_pb).or_insert((0, 0));
            *base += Documento::calc_linea(l).importe;
            let cuota_acumulada = div_round(*base * l.iva_pb as i64, 10_000);
            let cuota_linea = cuota_acumulada - *cuota_previa;
            *cuota_previa = cuota_acumulada;
            cuota_linea
        })
        .collect()
}

pub fn generar(doc: &Documento, empresa: &Empresa) -> String {
    let t = doc.totales();
    let mut x = String::with_capacity(4096);

    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(
        "<fe:Facturae xmlns:fe=\"http://www.facturae.gob.es/formato/Versiones/Facturaev3_2_2.xml\" \
         xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\">\n",
    );

    // ---- FileHeader -------------------------------------------------------
    x.push_str("  <FileHeader>\n");
    x.push_str("    <SchemaVersion>3.2.2</SchemaVersion>\n");
    x.push_str("    <Modality>I</Modality>\n");
    x.push_str("    <InvoiceIssuerType>EM</InvoiceIssuerType>\n");
    x.push_str("    <Batch>\n");
    x.push_str(&format!(
        "      <BatchIdentifier>{}</BatchIdentifier>\n",
        esc(&format!("{}{:04}", doc.serie, doc.numero))
    ));
    x.push_str("      <InvoicesCount>1</InvoicesCount>\n");
    x.push_str(&format!(
        "      <TotalInvoicesAmount><TotalAmount>{}</TotalAmount></TotalInvoicesAmount>\n",
        imp(t.total)
    ));
    x.push_str(&format!(
        "      <TotalOutstandingAmount><TotalAmount>{}</TotalAmount></TotalOutstandingAmount>\n",
        imp(t.total)
    ));
    x.push_str(&format!(
        "      <TotalExecutableAmount><TotalAmount>{}</TotalAmount></TotalExecutableAmount>\n",
        imp(t.total)
    ));
    x.push_str("      <InvoiceCurrencyCode>EUR</InvoiceCurrencyCode>\n");
    x.push_str("    </Batch>\n");
    x.push_str("  </FileHeader>\n");

    // ---- Parties ----------------------------------------------------------
    x.push_str("  <Parties>\n");
    x.push_str("    <SellerParty>\n");
    x.push_str("      <TaxIdentification>\n");
    x.push_str(&format!(
        "        <PersonTypeCode>{}</PersonTypeCode>\n",
        if empresa.persona_fisica { "F" } else { "J" }
    ));
    x.push_str("        <ResidenceTypeCode>R</ResidenceTypeCode>\n");
    x.push_str(&format!(
        "        <TaxIdentificationNumber>{}</TaxIdentificationNumber>\n",
        esc(&nif_pais(&empresa.nif))
    ));
    x.push_str("      </TaxIdentification>\n");
    if empresa.persona_fisica {
        x.push_str("      <Individual>\n");
        x.push_str(&format!("        <Name>{}</Name>\n", esc(&empresa.nombre)));
        if !empresa.apellidos.trim().is_empty() {
            x.push_str(&format!(
                "        <FirstSurname>{}</FirstSurname>\n",
                esc(&empresa.apellidos)
            ));
        }
        x.push_str(&direccion_esp(
            &empresa.direccion,
            &empresa.cp,
            &empresa.ciudad,
            &empresa.provincia,
            "        ",
        ));
        x.push_str("      </Individual>\n");
    } else {
        x.push_str("      <LegalEntity>\n");
        x.push_str(&format!(
            "        <CorporateName>{}</CorporateName>\n",
            esc(&empresa.nombre)
        ));
        x.push_str(&direccion_esp(
            &empresa.direccion,
            &empresa.cp,
            &empresa.ciudad,
            &empresa.provincia,
            "        ",
        ));
        if !empresa.email.trim().is_empty() {
            x.push_str("        <ContactDetails>\n");
            x.push_str(&format!(
                "          <ElectronicMail>{}</ElectronicMail>\n",
                esc(&empresa.email)
            ));
            x.push_str("        </ContactDetails>\n");
        }
        x.push_str("      </LegalEntity>\n");
    }
    x.push_str("    </SellerParty>\n");

    let c = &doc.cliente;
    x.push_str("    <BuyerParty>\n");
    x.push_str("      <TaxIdentification>\n");
    x.push_str(&format!(
        "        <PersonTypeCode>{}</PersonTypeCode>\n",
        if c.persona_fisica { "F" } else { "J" }
    ));
    x.push_str("        <ResidenceTypeCode>R</ResidenceTypeCode>\n");
    x.push_str(&format!(
        "        <TaxIdentificationNumber>{}</TaxIdentificationNumber>\n",
        esc(&nif_pais(&c.nif))
    ));
    x.push_str("      </TaxIdentification>\n");
    if c.persona_fisica {
        x.push_str("      <Individual>\n");
        x.push_str(&format!("        <Name>{}</Name>\n", esc(&c.nombre)));
        if !c.apellidos.trim().is_empty() {
            x.push_str(&format!(
                "        <FirstSurname>{}</FirstSurname>\n",
                esc(&c.apellidos)
            ));
        }
        x.push_str(&direccion_esp(&c.direccion, &c.cp, &c.ciudad, &c.provincia, "        "));
        x.push_str("      </Individual>\n");
    } else {
        x.push_str("      <LegalEntity>\n");
        x.push_str(&format!(
            "        <CorporateName>{}</CorporateName>\n",
            esc(&c.nombre_completo())
        ));
        x.push_str(&direccion_esp(&c.direccion, &c.cp, &c.ciudad, &c.provincia, "        "));
        if !c.email.trim().is_empty() {
            x.push_str("        <ContactDetails>\n");
            x.push_str(&format!(
                "          <ElectronicMail>{}</ElectronicMail>\n",
                esc(&c.email)
            ));
            x.push_str("        </ContactDetails>\n");
        }
        x.push_str("      </LegalEntity>\n");
    }
    x.push_str("    </BuyerParty>\n");
    x.push_str("  </Parties>\n");

    // ---- Invoices ---------------------------------------------------------
    x.push_str("  <Invoices>\n    <Invoice>\n");
    x.push_str("      <InvoiceHeader>\n");
    x.push_str(&format!(
        "        <InvoiceNumber>{:04}</InvoiceNumber>\n",
        doc.numero
    ));
    x.push_str(&format!(
        "        <InvoiceSeriesCode>{}</InvoiceSeriesCode>\n",
        esc(&doc.serie)
    ));
    x.push_str(&format!(
        "        <InvoiceDocumentType>{}</InvoiceDocumentType>\n",
        if doc.es_simplificada() { "FA" } else { "FC" }
    ));
    x.push_str("        <InvoiceClass>OO</InvoiceClass>\n");
    x.push_str("      </InvoiceHeader>\n");

    x.push_str("      <InvoiceIssueData>\n");
    x.push_str(&format!(
        "        <IssueDate>{}</IssueDate>\n",
        doc.fecha.format("%Y-%m-%d")
    ));
    x.push_str("        <InvoiceCurrencyCode>EUR</InvoiceCurrencyCode>\n");
    x.push_str("        <TaxCurrencyCode>EUR</TaxCurrencyCode>\n");
    x.push_str("        <LanguageName>es</LanguageName>\n");
    x.push_str("      </InvoiceIssueData>\n");

    // Impuestos repercutidos, un <Tax> por tipo.
    x.push_str("      <TaxesOutputs>\n");
    for (tipo, base, cuota) in &t.bases {
        x.push_str("        <Tax>\n");
        x.push_str("          <TaxTypeCode>01</TaxTypeCode>\n");
        x.push_str(&format!("          <TaxRate>{}</TaxRate>\n", pct2(*tipo)));
        x.push_str(&format!(
            "          <TaxableBase><TotalAmount>{}</TotalAmount></TaxableBase>\n",
            imp(*base)
        ));
        x.push_str(&format!(
            "          <TaxAmount><TotalAmount>{}</TotalAmount></TaxAmount>\n",
            imp(*cuota)
        ));
        x.push_str("        </Tax>\n");
    }
    if doc.recargo_pb > 0 {
        // FacturaE no tiene un código propio de recargo de equivalencia: se
        // declara como «Otros» (05) para que los totales cuadren.
        x.push_str("        <Tax>\n");
        x.push_str("          <TaxTypeCode>05</TaxTypeCode>\n");
        x.push_str(&format!("          <TaxRate>{}</TaxRate>\n", pct2(doc.recargo_pb)));
        x.push_str(&format!(
            "          <TaxableBase><TotalAmount>{}</TotalAmount></TaxableBase>\n",
            imp(t.base_total)
        ));
        x.push_str(&format!(
            "          <TaxAmount><TotalAmount>{}</TotalAmount></TaxAmount>\n",
            imp(t.cuota_recargo)
        ));
        x.push_str("        </Tax>\n");
    }
    x.push_str("      </TaxesOutputs>\n");

    if doc.irpf_pb > 0 {
        x.push_str("      <TaxesWithheld>\n        <Tax>\n");
        x.push_str("          <TaxTypeCode>04</TaxTypeCode>\n");
        x.push_str(&format!("          <TaxRate>{}</TaxRate>\n", pct2(doc.irpf_pb)));
        x.push_str(&format!(
            "          <TaxableBase><TotalAmount>{}</TotalAmount></TaxableBase>\n",
            imp(t.base_total)
        ));
        x.push_str(&format!(
            "          <TaxAmount><TotalAmount>{}</TotalAmount></TaxAmount>\n",
            imp(t.retencion)
        ));
        x.push_str("        </Tax>\n      </TaxesWithheld>\n");
    }

    x.push_str("      <InvoiceTotals>\n");
    x.push_str(&format!(
        "        <TotalGrossAmount>{}</TotalGrossAmount>\n",
        imp(t.base_total)
    ));
    x.push_str(&format!(
        "        <TotalGrossAmountBeforeTaxes>{}</TotalGrossAmountBeforeTaxes>\n",
        imp(t.base_total)
    ));
    x.push_str(&format!(
        "        <TotalTaxOutputs>{}</TotalTaxOutputs>\n",
        imp(t.cuota_iva + t.cuota_recargo)
    ));
    x.push_str(&format!(
        "        <TotalTaxesWithheld>{}</TotalTaxesWithheld>\n",
        imp(t.retencion)
    ));
    x.push_str(&format!(
        "        <InvoiceTotal>{}</InvoiceTotal>\n",
        imp(t.total)
    ));
    x.push_str(&format!(
        "        <TotalOutstandingAmount>{}</TotalOutstandingAmount>\n",
        imp(t.total)
    ));
    x.push_str(&format!(
        "        <TotalExecutableAmount>{}</TotalExecutableAmount>\n",
        imp(t.total)
    ));
    x.push_str("      </InvoiceTotals>\n");

    let cuotas_linea = cuotas_iva_lineas(doc);

    x.push_str("      <Items>\n");
    for (l, cuota) in doc.lineas.iter().zip(&cuotas_linea) {
        let lc = Documento::calc_linea(l);
        x.push_str("        <InvoiceLine>\n");
        x.push_str(&format!(
            "          <ItemDescription>{}</ItemDescription>\n",
            esc(&l.descripcion)
        ));
        x.push_str(&format!("          <Quantity>{}</Quantity>\n", cant(l.cantidad_milli)));
        x.push_str(&format!(
            "          <UnitPriceWithoutTax>{}</UnitPriceWithoutTax>\n",
            imp(l.precio_cents)
        ));
        x.push_str(&format!("          <TotalCost>{}</TotalCost>\n", imp(lc.bruto)));
        if l.descuento_pb > 0 {
            x.push_str("          <DiscountsAndRebates>\n            <Discount>\n");
            x.push_str("              <DiscountReason>Descuento</DiscountReason>\n");
            x.push_str(&format!(
                "              <DiscountRate>{}</DiscountRate>\n",
                pct2(l.descuento_pb)
            ));
            x.push_str(&format!(
                "              <DiscountAmount>{}</DiscountAmount>\n",
                imp(lc.descuento)
            ));
            x.push_str("            </Discount>\n          </DiscountsAndRebates>\n");
        }
        x.push_str(&format!("          <GrossAmount>{}</GrossAmount>\n", imp(lc.importe)));
        x.push_str("          <TaxesOutputs>\n            <Tax>\n");
        x.push_str("              <TaxTypeCode>01</TaxTypeCode>\n");
        x.push_str(&format!("              <TaxRate>{}</TaxRate>\n", pct2(l.iva_pb)));
        x.push_str(&format!(
            "              <TaxableBase><TotalAmount>{}</TotalAmount></TaxableBase>\n",
            imp(lc.importe)
        ));
        x.push_str(&format!(
            "              <TaxAmount><TotalAmount>{}</TotalAmount></TaxAmount>\n",
            imp(*cuota)
        ));
        x.push_str("            </Tax>\n          </TaxesOutputs>\n");
        x.push_str("        </InvoiceLine>\n");
    }
    x.push_str("      </Items>\n");

    // Datos de pago: sólo si hay IBAN configurado.
    if !empresa.iban.trim().is_empty() {
        x.push_str("      <PaymentDetails>\n        <Installment>\n");
        x.push_str(&format!(
            "          <InstallmentDueDate>{}</InstallmentDueDate>\n",
            doc.vencimiento.format("%Y-%m-%d")
        ));
        x.push_str(&format!(
            "          <InstallmentAmount>{}</InstallmentAmount>\n",
            imp(t.total)
        ));
        x.push_str("          <PaymentMeans>04</PaymentMeans>\n");
        x.push_str("          <AccountToBeCredited>\n");
        x.push_str(&format!(
            "            <IBAN>{}</IBAN>\n",
            esc(&empresa.iban.replace(' ', ""))
        ));
        x.push_str("          </AccountToBeCredited>\n");
        x.push_str("        </Installment>\n      </PaymentDetails>\n");
    }

    x.push_str("    </Invoice>\n  </Invoices>\n");
    x.push_str("</fe:Facturae>\n");
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cliente, Linea, TipoDoc};
    use chrono::NaiveDate;

    fn ejemplo() -> (Documento, Empresa) {
        let mut empresa = Empresa {
            nombre: "Laura Gómez".into(),
            nif: "51234567H".into(),
            persona_fisica: true,
            apellidos: "Gómez Ruiz".into(),
            direccion: "Carrer de Sants 12".into(),
            cp: "08014".into(),
            ciudad: "Barcelona".into(),
            provincia: "Barcelona".into(),
            ..Default::default()
        };
        empresa.iban.clear();

        let cliente = Cliente {
            id: 1,
            nombre: "Agencia Creativa S.L.".into(),
            nif: "B76281996".into(),
            direccion: "Gran Via 28".into(),
            cp: "28013".into(),
            ciudad: "Madrid".into(),
            provincia: "Madrid".into(),
            ..Default::default()
        };
        let mut d = Documento::nuevo(TipoDoc::Factura, cliente, 1500, "2026".into());
        d.numero = 14;
        d.fecha = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        d.vencimiento = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        d.lineas = vec![Linea {
            descripcion: "Diseño de identidad corporativa".into(),
            cantidad_milli: 1000,
            precio_cents: 200_000,
            descuento_pb: 0,
            iva_pb: 2100,
        }];
        (d, empresa)
    }

    #[test]
    fn reproducen_los_totales_del_ejemplo_oficial() {
        // 2.000 base + 420 IVA - 300 IRPF = 2.120, igual que el fichero de
        // referencia validado contra el XSD.
        let (d, _) = ejemplo();
        let t = d.totales();
        assert_eq!(t.base_total, 200_000);
        assert_eq!(t.cuota_iva, 42_000);
        assert_eq!(t.retencion, 30_000);
        assert_eq!(t.total, 212_000);
    }

    #[test]
    fn xml_tiene_orden_y_valores_correctos() {
        let (d, e) = ejemplo();
        let xml = generar(&d, &e);
        assert!(xml.contains("<SchemaVersion>3.2.2</SchemaVersion>"));
        assert!(xml.contains("<InvoiceNumber>0014</InvoiceNumber>"));
        assert!(xml.contains("<InvoiceSeriesCode>2026</InvoiceSeriesCode>"));
        assert!(xml.contains("<PersonTypeCode>F</PersonTypeCode>"));
        assert!(xml.contains("<TaxIdentificationNumber>ES51234567H</TaxIdentificationNumber>"));
        assert!(xml.contains("<TaxTypeCode>04</TaxTypeCode>"));
        assert!(xml.contains("<InvoiceTotal>2120.00</InvoiceTotal>"));
        // El orden de InvoiceTotals: bruto -> antes de impuestos -> IVA -> IRPF
        let i_bruto = xml.find("<TotalGrossAmount>2000.00").unwrap();
        let i_ant = xml.find("<TotalGrossAmountBeforeTaxes>").unwrap();
        let i_iva = xml.find("<TotalTaxOutputs>420.00").unwrap();
        let i_total = xml.find("<InvoiceTotal>2120.00").unwrap();
        assert!(i_bruto < i_ant && i_ant < i_iva && i_iva < i_total);
    }

    #[test]
    fn la_cantidad_del_xml_conserva_tres_decimales() {
        let (mut d, e) = ejemplo();
        for (milli, esperado) in [
            (1_255, "1.255"),
            (125, "0.125"),
            (1_999, "1.999"),
            (1_250, "1.25"),
            (1_000, "1.00"),
        ] {
            d.lineas[0].cantidad_milli = milli;
            let xml = generar(&d, &e);
            assert!(
                xml.contains(&format!("<Quantity>{esperado}</Quantity>")),
                "cantidad {milli}"
            );
        }
    }

    fn cuotas_de_lineas(xml: &str) -> Vec<i64> {
        let items = xml
            .split("<Items>")
            .nth(1)
            .unwrap()
            .split("</Items>")
            .next()
            .unwrap();
        let etiqueta = "<TaxAmount><TotalAmount>";
        let mut cuotas = Vec::new();
        for (inicio, _) in items.match_indices(etiqueta) {
            let desde = inicio + etiqueta.len();
            let hasta = desde + items[desde..].find("</TotalAmount>").unwrap();
            cuotas.push(crate::model::parse_cents(&items[desde..hasta]).unwrap());
        }
        cuotas
    }

    fn entre<'a>(texto: &'a str, inicio: &str, fin: &str) -> &'a str {
        let desde = texto.find(inicio).unwrap() + inicio.len();
        let hasta = desde + texto[desde..].find(fin).unwrap();
        &texto[desde..hasta]
    }

    fn lineas_del_xml(xml: &str) -> Vec<(i32, i64)> {
        let items = xml
            .split("<Items>")
            .nth(1)
            .unwrap()
            .split("</Items>")
            .next()
            .unwrap();
        items
            .split("<InvoiceLine>")
            .skip(1)
            .map(|linea| {
                let tipo = entre(linea, "<TaxRate>", "</TaxRate>");
                let cuota = entre(
                    linea,
                    "<TaxAmount><TotalAmount>",
                    "</TotalAmount>",
                );
                (
                    crate::model::parse_cents(tipo).unwrap() as i32,
                    crate::model::parse_cents(cuota).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn las_cuotas_de_las_lineas_suman_el_total_por_tipo() {
        let (mut d, e) = ejemplo();
        // Cinco líneas de un céntimo: cada cuota individual redondea a cero,
        // pero la cuota agrupada al 21 % es un céntimo.
        d.lineas = (0..5)
            .map(|i| Linea {
                descripcion: format!("Línea {i}"),
                cantidad_milli: 1000,
                precio_cents: 1,
                iva_pb: 2100,
                ..Default::default()
            })
            .collect();
        let xml = generar(&d, &e);
        assert_eq!(d.totales().cuota_iva, 1);
        assert!(xml.contains("<TotalTaxOutputs>0.01</TotalTaxOutputs>"));
        let cuotas = cuotas_de_lineas(&xml);
        assert_eq!(cuotas.iter().sum::<i64>(), d.totales().cuota_iva);
    }

    #[test]
    fn las_cuotas_de_las_lineas_no_quedan_negativas() {
        let (mut d, e) = ejemplo();
        // Cinco líneas de 3 céntimos: cada cuota individual redondea a uno,
        // pero la cuota agrupada al 21 % es 3 céntimos. El ajuste debe
        // repartirse sin dejar ninguna cuota en negativo.
        d.lineas = (0..5)
            .map(|i| Linea {
                descripcion: format!("Línea {i}"),
                cantidad_milli: 1000,
                precio_cents: 3,
                iva_pb: 2100,
                ..Default::default()
            })
            .collect();
        let xml = generar(&d, &e);
        assert_eq!(d.totales().cuota_iva, 3);
        assert!(xml.contains("<TotalTaxOutputs>0.03</TotalTaxOutputs>"));
        let cuotas = cuotas_de_lineas(&xml);
        assert_eq!(cuotas.iter().sum::<i64>(), 3, "{cuotas:?}");
        assert!(cuotas.iter().all(|&cuota| cuota >= 0), "{cuotas:?}");
        assert!(
            cuotas.iter().filter(|&&cuota| cuota > 0).count() >= 2,
            "el ajuste no debe acumularse en una sola línea: {cuotas:?}"
        );
    }

    #[test]
    fn dos_tipos_intercalados_suman_su_cuota_por_grupo() {
        let (mut d, e) = ejemplo();
        // Cinco líneas de 3 céntimos al 21 % y cinco al 10 %, alternadas.
        // El 21 % agrupa 3 céntimos aunque sus líneas sumen 5; el 10 % agrupa
        // 2 aunque sus líneas redondeen a cero.
        d.lineas = (0..10)
            .map(|i| Linea {
                descripcion: format!("Línea {i}"),
                cantidad_milli: 1000,
                precio_cents: 3,
                iva_pb: if i % 2 == 0 { 2100 } else { 1000 },
                ..Default::default()
            })
            .collect();

        let xml = generar(&d, &e);
        assert!(xml.contains("<TotalTaxOutputs>0.05</TotalTaxOutputs>"));

        let lineas = lineas_del_xml(&xml);
        assert_eq!(
            lineas.iter().map(|(tipo, _)| *tipo).collect::<Vec<_>>(),
            (0..10)
                .map(|i| if i % 2 == 0 { 2100 } else { 1000 })
                .collect::<Vec<_>>()
        );

        for (tipo, _, cuota_agrupada) in &d.totales().bases {
            let suma: i64 = lineas
                .iter()
                .filter(|(linea_tipo, _)| linea_tipo == tipo)
                .map(|(_, cuota)| *cuota)
                .sum();
            assert_eq!(suma, *cuota_agrupada, "tipo {tipo}");
        }
        assert!(lineas.iter().all(|(_, cuota)| *cuota >= 0), "{lineas:?}");
    }

    #[test]
    fn escapa_caracteres_especiales() {
        let (mut d, e) = ejemplo();
        d.lineas[0].descripcion = "A & B <Cía> \"S.L.\"".into();
        let xml = generar(&d, &e);
        assert!(xml.contains("A &amp; B &lt;Cía&gt; &quot;S.L.&quot;"));
        assert!(!xml.contains("A & B <Cía>"));
    }
}
