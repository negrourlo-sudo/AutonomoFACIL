//! FacturaE 3.2.2 (formato oficial, `facturae.gob.es`).
//!
//! El orden de los elementos no es decorativo: el XSD es una `sequence` y un
//! nodo fuera de sitio invalida el fichero. La estructura replicada aquí está
//! tomada de un documento validado contra `Facturaev3_2_2.xsd`.

use crate::model::{Documento, Empresa};

/// `1234.56`, sin separador de miles: en XML el punto es el decimal.
fn imp(c: i64) -> String {
    let neg = c < 0;
    let v = c.abs();
    format!("{}{}.{:02}", if neg { "-" } else { "" }, v / 100, v % 100)
}

/// Cantidad con dos decimales: `1.25`, `2.00`.
fn cant(m: i64) -> String {
    let neg = m < 0;
    let v = m.abs();
    format!("{}{}.{:02}", if neg { "-" } else { "" }, v / 1000, (v % 1000) / 10)
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

    x.push_str("      <Items>\n");
    for l in &doc.lineas {
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
            imp(crate::model::div_round(lc.importe * l.iva_pb as i64, 10_000))
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
    fn escapa_caracteres_especiales() {
        let (mut d, e) = ejemplo();
        d.lineas[0].descripcion = "A & B <Cía> \"S.L.\"".into();
        let xml = generar(&d, &e);
        assert!(xml.contains("A &amp; B &lt;Cía&gt; &quot;S.L.&quot;"));
        assert!(!xml.contains("A & B <Cía>"));
    }
}
