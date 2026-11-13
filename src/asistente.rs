//! Asistente de puesta en marcha.
//!
//! Cuatro preguntas para deducir el régimen, el IVA y la retención, y mostrar en
//! lenguaje llano qué modelos hay que presentar y cómo facturar. No es asesoría
//! fiscal: es una recomendación razonada que después se puede cambiar en
//! Ajustes. Toda la lógica vive aquí, sin interfaz, para poder probarla.

use crate::fiscal::Obligacion;
use crate::model::*;

/// Lo que el usuario ha respondido en el asistente.
#[derive(Clone, Debug, Default)]
pub struct Perfil {
    pub nombre: String,
    pub nif: String,
    /// Sociedad limitada, cooperativa… En caso contrario, autónomo persona
    /// física.
    pub es_sociedad: bool,
    pub actividad: Actividad,
    pub regimen_irpf: RegimenIrpf,
    /// Previsión de facturación anual, en céntimos.
    pub facturacion_cents: i64,
    /// A quién factura.
    pub a_particulares: bool,
    pub a_empresas: bool,
    pub a_profesionales: bool,
    pub a_minoristas: bool,
    pub a_ue: bool,
    pub a_isp: bool,
    /// El propio negocio es minorista: compra a mayoristas y vende al público.
    pub recargo_equivalencia: bool,
    pub sii: bool,
    pub empleados: bool,
    pub alquiler_local: bool,
    pub verifactu: bool,
}

impl Perfil {
    /// ¿Ha marcado al menos un tipo de cliente?
    pub fn tiene_clientes(&self) -> bool {
        self.a_particulares
            || self.a_empresas
            || self.a_profesionales
            || self.a_minoristas
            || self.a_ue
            || self.a_isp
    }
}

/// Lo que el programa recomienda para ese perfil.
pub struct Plan {
    pub iva_pb: i32,
    pub irpf_pb: i32,
    pub recargo_pb: i32,
    pub obligaciones: Vec<Obligacion>,
    pub resumen: Vec<String>,
    pub consejos: Vec<String>,
}

/// Deduce el plan: usa el mismo motor de obligaciones que la vista de modelos,
/// así el asistente y el programa nunca dicen cosas distintas.
pub fn plan(p: &Perfil) -> Plan {
    // Empresa simulada para reutilizar `obligaciones_con`.
    let mut e = Empresa::default();
    e.actividad = p.actividad;
    e.regimen_irpf = p.regimen_irpf;
    e.recargo_equivalencia = p.recargo_equivalencia;
    e.sii = p.sii;
    e.persona_fisica = !p.es_sociedad;

    let mut obligaciones = crate::fiscal::obligaciones_con(p.actividad, &e);
    obligaciones.retain(|o| match o.codigo {
        // Nóminas y alquiler sólo si los hay.
        "111" | "190" => p.empleados,
        "115" | "180" => p.alquiler_local,
        // El 349 sólo tiene sentido si hay clientes o proveedores de la UE.
        "349" => p.a_ue || p.a_isp,
        _ => true,
    });

    let iva_pb = p.actividad.iva_pct() * 100;
    let irpf_pb = if p.a_profesionales { 1500 } else { 0 };
    let recargo_pb = if p.a_minoristas { 520 } else { 0 };

    let mut resumen = Vec::new();
    resumen.push(if p.es_sociedad {
        "Tributas como sociedad: el Impuesto de Sociedades (200) sustituye a la Renta y los pagos a cuenta son el 202, no el 130 ni el 131.".to_string()
    } else {
        format!(
            "Eres autónomo persona física en {}.",
            p.regimen_irpf.etiqueta().to_lowercase()
        )
    });
    resumen.push(format!("Tu actividad: {}.", p.actividad.etiqueta().to_lowercase()));
    resumen.push(format!(
        "IVA habitual en tus facturas: {} %{}.",
        fmt_pb(iva_pb),
        if p.recargo_equivalencia {
            " (tú no lo ingresas: lo hace tu proveedor mayorista)"
        } else {
            ""
        }
    ));
    if !p.es_sociedad {
        resumen.push(if irpf_pb > 0 {
            "A tus clientes profesionales y empresas les retienes el 15 % de IRPF (el 7 % los tres primeros años de actividad).".to_string()
        } else {
            "En tus facturas no hay retención de IRPF: tus clientes te pagan el total.".to_string()
        });
    }
    if recargo_pb > 0 {
        resumen.push(format!(
            "A los comercios minoristas les repercutes un recargo de equivalencia del {} % además del IVA.",
            fmt_pb(recargo_pb)
        ));
    }
    if matches!(p.actividad, Actividad::Digital) {
        resumen.push(
            "Venta digital, según quién compre: a un particular español, el 21 % en el 303. A un particular de la UE, hasta 10.000 € al año puedes aplicar ese mismo 21 % en el 303; por encima, IVA del país del cliente y 369, con alta en el 035. A una empresa de la UE con VIES, sin IVA y al 349. Fuera de la UE, exportación sin IVA en el 303. Si vendes solo a través de plataformas, ellas liquidan el IVA del jugador y no presentas el 369; con plataformas de la UE (Apple, Google) sí va el 349."
                .to_string(),
        );
    }
    let modelos: Vec<&str> = obligaciones.iter().map(|o| o.codigo).collect();
    resumen.push(format!("Modelos que te tocan: {}.", modelos.join(", ")));

    let mut consejos = Vec::new();
    if p.recargo_equivalencia {
        consejos.push(
            "Recargo de equivalencia: tus proveedores te cobran IVA + recargo y tú no deduces ese IVA ni presentas 303 ni 390. Sólo el 309 si compras a la UE o hay inversión del sujeto pasivo."
                .to_string(),
        );
    } else {
        consejos.push(
            "En el 303 ingresas el IVA que has repercutido menos el que has soportado en gastos con factura. Se presenta aunque salga cero o a devolver."
                .to_string(),
        );
    }
    if irpf_pb > 0 {
        consejos.push(
            "La retención no es un impuesto extra: se descuenta de lo que te pagan y se resta en tu Renta. Si el 70 % de tus ingresos ya lleva retención, no tienes que presentar el 130."
                .to_string(),
        );
    }
    if p.a_ue {
        consejos.push(
            "Antes de facturar sin IVA a una empresa de la UE, comprueba su NIF-IVA en VIES y guárdalo: la operación va al 349 y el cliente autoliquida el IVA en su país."
                .to_string(),
        );
    }
    if p.a_isp {
        consejos.push(
            "En inversión del sujeto pasivo la factura sale sin IVA y con la leyenda «operación con inversión del sujeto pasivo»; lo declara el cliente."
                .to_string(),
        );
    }
    if matches!(p.actividad, Actividad::Comercio | Actividad::Digital) {
        consejos.push(
            "Si vendes tú directamente a particulares de la UE más de 10.000 € al año, el IVA es el del país del cliente y se declara en el 369 (ventanilla única), con alta previa en el 035. Por debajo de esa cifra puedes aplicar el 21 % español en el 303. A través de un marketplace o de Steam, el vendedor es la plataforma y no presentas el 369."
                .to_string(),
        );
    }
    if p.facturacion_cents > 3_005_06 {
        consejos.push(
            "Con más de 3.005,06 € facturados a un mismo cliente o proveedor en el año, ese cliente entra en tu 347 (se presenta en febrero)."
                .to_string(),
        );
    }
    if p.sii {
        consejos.push(
            "Con el SII el 303 es mensual y los libros se envían en cuatro días; el 390 no se presenta.".to_string(),
        );
    }
    if p.verifactu {
        consejos.push(
            "Con Verifactu activo, cada factura emitida guarda un registro encadenado y un QR tributario de cotejo (no verificable). No se remite a la AEAT. Una factura emitida ya no se edita: se anula y se hace otra."
                .to_string(),
        );
    }
    consejos.push(
        "Guarda todas las facturas de gasto con tu NIF: sin factura no hay IVA deducible ni gasto que reste en el 130."
            .to_string(),
    );
    consejos.push(
        "Numera las facturas sin saltos y por series. Una factura emitida no se borra: se corrige con una rectificativa."
            .to_string(),
    );
    consejos.push(
        "Separa la cuenta del negocio de la personal y guarda facturas, justificantes y extractos cuatro años: Hacienda puede pedirlos."
            .to_string(),
    );

    Plan {
        iva_pb,
        irpf_pb,
        recargo_pb,
        obligaciones,
        resumen,
        consejos,
    }
}

/// Rutina práctica del año, con lo que hay que hacer y cuándo.
/// Se adapta a régimen, sociedad y tipo de clientes.
pub fn rutina(p: &Perfil) -> Vec<String> {
    let mut v = Vec::new();
    if p.recargo_equivalencia {
        v.push(
            "Cada trimestre: revisa compras y ventas. En recargo de equivalencia no presentas 303; el 309 sólo si hay compra a la UE o inversión del sujeto pasivo."
                .to_string(),
        );
        // El recargo exime del IVA, no del IRPF: si es persona física, el pago
        // a cuenta sigue siendo suyo. Una sociedad no lo presenta.
        if !p.es_sociedad {
            v.push(
                "Cada trimestre, además: el pago a cuenta del IRPF, 130 o 131, con las mismas fechas."
                    .to_string(),
            );
        }
    } else {
        // Con SII el 303 es mensual; el 130/131 sigue siendo trimestral y sólo
        // lo presenta la persona física. Una sociedad nunca ve el 130/131.
        v.push(if p.sii {
            "Cada mes: cierra facturas y gastos y presenta el 303 antes del día 20; con SII los libros van en cuatro días."
                .to_string()
        } else {
            "Cada trimestre: cierra facturas y gastos y presenta el 303 antes del día 20 del mes siguiente."
                .to_string()
        });
        if !p.es_sociedad {
            v.push(
                "Cada trimestre, además: el pago a cuenta del IRPF, 130 o 131, con las mismas fechas que el 303."
                    .to_string(),
            );
        }
    }
    if p.es_sociedad {
        v.push("Abril, octubre y diciembre: pago fraccionado del 200 con el 202.".to_string());
        v.push("Julio: Impuesto de Sociedades (200) del ejercicio cerrado.".to_string());
    } else {
        v.push("Abril a junio: Renta (100); las retenciones y los 130 ya pagados se restan.".to_string());
    }
    v.push(
        "Enero: 4T y resumen anual del IVA (390, si no estás en SII ni exonerado); 190 y 180 antes del 31 si tienes nóminas o alquiler con retención."
            .to_string(),
    );
    v.push(
        "Febrero: 347 si con un mismo cliente o proveedor superaste 3.005,06 € con IVA en el año."
            .to_string(),
    );
    if p.a_ue || p.a_isp {
        v.push(
            "Cada trimestre (o cada mes si superas 50.000 €): 349 con las operaciones intracomunitarias."
                .to_string(),
        );
    }
    if matches!(p.actividad, Actividad::Digital | Actividad::Comercio) {
        v.push(
            "Si vendes a particulares de la UE por encima de 10.000 € al año: 369 trimestral (mensual si es IOSS), con alta previa en el 035."
                .to_string(),
        );
    }
    v.push(
        "Siempre: numeración sin saltos, rectificativa para corregir y justificantes guardados cuatro años."
            .to_string(),
    );
    v
}

/// Texto que lee la voz en cada paso del asistente. Se escribe para el oído:
/// frases cortas, sin listas y sin abreviaturas que suenen raras.
pub fn guion(paso: usize, p: &Perfil) -> String {
    match paso {
        0 => "Vamos a empezar. Escribe tu nombre o la razón social y tu NIF: con ellos sabré \
              quién declara y podré ponerlos en tus facturas. Marca si eres persona física \
              o sociedad, porque de eso depende que tributes en el IRPF o en el Impuesto \
              sobre Sociedades, y que tu pago a cuenta sea el ciento treinta o no exista."
            .to_string(),
        1 => "Cuéntame a qué te dedicas. La actividad decide el IVA que te propongo y los \
              modelos que aparecen; el régimen del IRPF decide si tu pago a cuenta es el \
              ciento treinta o el ciento treinta y uno. Marca también si tienes empleados, \
              si pagas el alquiler de un local o si estás en el SII, porque cada uno añade \
              modelos y plazos distintos."
            .to_string(),
        2 => "Ahora, ¿a quién facturas? Esto cambia el IVA y la retención. A empresas y \
              profesionales españoles se les retiene el quince por ciento; a particulares, no. \
              Si vendes a particulares de la Unión Europea, a partir de diez mil euros al año \
              el IVA es el del país del comprador y se declara en el trescientos sesenta y nueve. \
              Si vendes a empresas de la UE, la factura va sin IVA y al trescientos cuarenta y nueve."
            .to_string(),
        _ => {
            let plan = plan(p);
            let modelos: Vec<&str> = plan.obligaciones.iter().map(|o| o.codigo).collect();
            let trimestral = if p.recargo_equivalencia && p.es_sociedad {
                "cada trimestre revisa compras y ventas, sin trescientos tres"
            } else if p.recargo_equivalencia {
                "cada trimestre revisa compras y ventas, sin trescientos tres, y presenta el ciento treinta"
            } else if p.sii {
                "cada mes el trescientos tres, porque estás en el SII"
            } else if p.es_sociedad {
                "cada trimestre el trescientos tres"
            } else {
                "cada trimestre el trescientos tres y el ciento treinta"
            };
            let anual = if p.es_sociedad {
                "en abril, octubre y diciembre el doscientos dos, y en julio el doscientos"
            } else {
                "en primavera la Renta"
            };
            format!(
                "Este es tu plan. {} Tendrás que presentar los modelos {}. \
                 La rutina es sencilla: {trimestral}; en enero el resumen anual; {anual}. \
                 Guarda cada justificante. Te dejo los consejos en pantalla, y puedes cambiar \
                 todo esto en Ajustes o repetir el asistente cuando quieras.",
                plan.resumen.join(" "),
                modelos.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perfil(actividad: Actividad) -> Perfil {
        Perfil {
            actividad,
            a_empresas: true,
            ..Default::default()
        }
    }

    fn codigos(p: &Perfil) -> Vec<&'static str> {
        plan(p).obligaciones.iter().map(|o| o.codigo).collect()
    }

    #[test]
    fn en_modulos_el_pago_a_cuenta_es_el_131() {
        let mut p = perfil(Actividad::Servicios);
        p.regimen_irpf = RegimenIrpf::Objetiva;
        let c = codigos(&p);
        assert!(c.contains(&"131"), "{c:?}");
        assert!(!c.contains(&"130"), "{c:?}");
    }

    #[test]
    fn el_recargo_de_equivalencia_quita_303_y_390() {
        let mut p = perfil(Actividad::Comercio);
        p.recargo_equivalencia = true;
        let c = codigos(&p);
        assert!(!c.contains(&"303"), "{c:?}");
        assert!(!c.contains(&"390"), "{c:?}");
        assert!(c.contains(&"309"), "{c:?}");
    }

    #[test]
    fn el_349_solo_sale_si_hay_clientes_de_la_ue() {
        let mut p = perfil(Actividad::Servicios);
        assert!(!codigos(&p).contains(&"349"));
        p.a_ue = true;
        assert!(codigos(&p).contains(&"349"));
    }

    #[test]
    fn nominas_y_alquiler_solo_si_los_hay() {
        let mut p = perfil(Actividad::Servicios);
        let c = codigos(&p);
        assert!(!c.contains(&"111") && !c.contains(&"115"));
        p.empleados = true;
        p.alquiler_local = true;
        let c = codigos(&p);
        assert!(c.contains(&"111") && c.contains(&"190"));
        assert!(c.contains(&"115") && c.contains(&"180"));
    }

    #[test]
    fn la_retencion_la_marcan_los_clientes_profesionales() {
        let mut p = perfil(Actividad::Servicios);
        assert_eq!(plan(&p).irpf_pb, 0);
        p.a_profesionales = true;
        assert_eq!(plan(&p).irpf_pb, 1500);
    }

    #[test]
    fn el_guion_suena_a_lenguaje_hablado() {
        let p = perfil(Actividad::Servicios);
        assert!(guion(0, &p).contains("NIF"));
        assert!(guion(2, &p).contains("quince por ciento"));
        let cierre = guion(3, &p);
        assert!(cierre.contains("Este es tu plan"), "{cierre}");
        assert!(cierre.contains("303"), "{cierre}");
        assert!(!guion(3, &p).contains("·"), "sin listas para el oído");
    }

    #[test]
    fn la_rutina_cambia_entre_autonomo_y_sociedad() {
        let mut p = perfil(Actividad::Servicios);
        let autonomo = rutina(&p).join(" ");
        assert!(autonomo.contains("303"), "{autonomo}");
        assert!(autonomo.contains("130"), "{autonomo}");
        assert!(autonomo.contains("Renta"), "{autonomo}");
        p.es_sociedad = true;
        let sociedad = rutina(&p).join(" ");
        assert!(sociedad.contains("200"), "{sociedad}");
        assert!(sociedad.contains("202"), "{sociedad}");
        assert!(!sociedad.contains("Renta (100)"), "{sociedad}");
        // Una sociedad no presenta el pago a cuenta del IRPF.
        assert!(!sociedad.contains("130"), "{sociedad}");
        assert!(!sociedad.contains("131"), "{sociedad}");
    }

    #[test]
    fn con_sii_el_303_es_mensual_y_el_130_sigue_trimestral() {
        let mut p = perfil(Actividad::Servicios);
        p.sii = true;
        let autonomo = rutina(&p).join(" ");
        assert!(autonomo.contains("Cada mes"), "{autonomo}");
        assert!(!autonomo.contains("Cada trimestre: cierra"), "{autonomo}");
        assert!(autonomo.contains("130 o 131"), "{autonomo}");

        // Y con SII, una sociedad sigue sin ver el 130/131.
        p.es_sociedad = true;
        let sociedad = rutina(&p).join(" ");
        assert!(sociedad.contains("Cada mes"), "{sociedad}");
        assert!(!sociedad.contains("130"), "{sociedad}");
        assert!(!sociedad.contains("131"), "{sociedad}");
    }

    #[test]
    fn el_recargo_no_quita_el_130_al_autonomo() {
        let mut p = perfil(Actividad::Comercio);
        p.recargo_equivalencia = true;
        let texto = rutina(&p).join(" ");
        assert!(texto.contains("130 o 131"), "{texto}");
        assert!(guion(3, &p).contains("ciento treinta"), "{}", guion(3, &p));

        // La sociedad en recargo sigue sin 130/131.
        p.es_sociedad = true;
        let texto = rutina(&p).join(" ");
        assert!(!texto.contains("130"), "{texto}");
        assert!(!texto.contains("131"), "{texto}");
        assert!(!guion(3, &p).contains("ciento treinta"));
    }

    #[test]
    fn la_voz_de_la_sociedad_no_nombra_el_130() {
        let mut p = perfil(Actividad::Servicios);
        p.es_sociedad = true;
        let voz = guion(3, &p);
        assert!(voz.contains("doscientos"), "{voz}");
        assert!(!voz.contains("ciento treinta"), "{voz}");

        p.sii = true;
        let voz = guion(3, &p);
        assert!(voz.contains("cada mes"), "{voz}");
        assert!(!voz.contains("ciento treinta"), "{voz}");
    }

    #[test]
    fn el_minorista_suma_recargo_y_el_ue_lo_quita() {
        let mut p = perfil(Actividad::Comercio);
        p.a_minoristas = true;
        assert_eq!(plan(&p).recargo_pb, 520);
        p.a_ue = true;
        assert_eq!(plan(&p).recargo_pb, 520);
    }
}
