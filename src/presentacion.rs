//! Cómo se entrega cada modelo: la página oficial de la Sede, los pasos,
//! qué fichero admite y qué se puede automatizar de verdad.
//!
//! Es documentación viva metida en la app: los enlaces son los de la Sede de
//! la AEAT y los pasos siguen su formulario web. No hay API pública de
//! presentación, así que aquí no se finge: se explica qué se teclea, qué se
//! sube y qué haría falta para automatizarlo.

pub struct Guia {
    pub codigo: &'static str,
    /// Página de ayuda de la AEAT para presentar este modelo.
    pub sede: &'static str,
    /// Con qué se puede identificar el autónomo.
    pub identificacion: &'static [&'static str],
    /// Pasos, en orden, desde que sale el borrador hasta el justificante.
    pub pasos: &'static [&'static str],
    /// Qué admite la Sede además del formulario.
    pub fichero: &'static str,
    /// Lo que hay que vigilar en este modelo en concreto.
    pub aviso: &'static str,
}

/// Índice de la Sede para elegir modelo y ejercicio.
pub const SEDE: &str = "https://sede.agenciatributaria.gob.es/Sede/presentar-consultar-declaraciones-modelo.html";

const GUIA_303: Guia = Guia {
    codigo: "303",
    sede: "https://sede.agenciatributaria.gob.es/Sede/ayuda/consultas-informaticas/presentacion-declaraciones-ayuda-tecnica/modelo-303.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
        "eIDAS, para ciudadanos de la UE",
    ],
    pasos: &[
        "Genera el borrador con «Rellenar modelo» y revisa el PDF casilla a casilla.",
        "En la Sede elige «Modelo 303. Ejercicio actual. Presentación».",
        "Identifícate con certificado, DNIe o Cl@ve.",
        "«Nueva declaración» y teclea las casillas del borrador: 01/04/07 las bases, 03/06/09 y 150 las cuotas, 28/29 el IVA soportado. El formulario suma solo el 27, el 45 y el 46.",
        "Si facturaste a empresas de la UE o fuera de España, esa base va en la casilla 59 (operaciones no sujetas por reglas de localización o con inversión del sujeto pasivo), no en las casillas de IVA repercutido.",
        "Comprueba que la casilla 61 coincide con el resultado del borrador.",
        "«Presentar declaración», marca «Conforme» y confirma.",
        "Descarga el justificante (PDF con CSV y nº de registro) y guárdalo. Vuelve aquí y marca el trimestre como presentado.",
    ],
    fichero: "Desde 2023 el 303 se presenta sólo por formulario web: ya no admite fichero. Sí puedes importar los libros registro de IVA desde «Pre 303» si los llevas en un fichero compatible.",
    aviso: "El 303 se presenta aunque salga a cero o a devolver. Si rectificas uno ya presentado, usa la declaración rectificativa, no una nueva.",
};

const GUIA_130: Guia = Guia {
    codigo: "130",
    sede: "https://sede.agenciatributaria.gob.es/Sede/ayuda/consultas-informaticas/presentacion-declaraciones-ayuda-tecnica/modelo-130.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Genera el borrador del trimestre y revisa las casillas 01 a 07.",
        "En la Sede entra en «Modelo 130. Ejercicio actual. Presentación y servicio de ayuda Pre 130».",
        "Identifícate y elige «Nueva declaración».",
        "Teclea el 01 y el 02 acumulados del año: el 03, el 04, el 07 y el 12 se calculan solos. El 05 y el 06 los rellenas con lo ya pagado y las retenciones del trimestre.",
        "Comprueba el resultado (casilla 07 o 12) contra el borrador.",
        "«Formalizar ingreso/devolución» y confirma. Guarda el justificante.",
        "Marca el trimestre como presentado en la guía.",
    ],
    fichero: "El 130 no admite fichero de declaración: formulario web, o importación de los libros registro si los tienes en el formato de la AEAT.",
    aviso: "Si el 70 % de tus ingresos ya lleva retención, no tienes que presentarlo: márcalo como no aplicable y no lo generes.",
};

const GUIA_390: Guia = Guia {
    codigo: "390",
    sede: "https://sede.agenciatributaria.gob.es/Sede/ayuda/consultas-informaticas/presentacion-declaraciones-ayuda-tecnica/modelo-390/modelo-390-presentacion-mediante-formulario.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil (personas físicas)",
    ],
    pasos: &[
        "Genera el 390: es la suma de los cuatro 303 del año.",
        "En la Sede elige «Modelo 390. IVA. Declaración resumen anual».",
        "Identifícate y escoge «Nueva declaración».",
        "Teclea las casillas por apartados: devengado por tipo, deducible y liquidación. Los totales del borrador deben cuadrar con lo presentado en los 303.",
        "«Validar»; si no hay errores, «Presentar declaración» y «Conforme».",
        "Guarda el justificante y marca el 390 como presentado.",
    ],
    fichero: "El 390 sí admite «Presentación (con fichero)»: se sube un fichero con el diseño de registro oficial del ejercicio. El formulario web es más rápido para un autónomo.",
    aviso: "Plazo: hasta el 30 de enero. Hay supuestos de exoneración (por ejemplo, estar en el SII o con volumen bajo); compruébalo antes de presentarlo.",
};

const GUIA_347: Guia = Guia {
    codigo: "347",
    sede: "https://sede.agenciatributaria.gob.es/Sede/ayuda/consultas-informaticas/declaraciones-informativas-ayuda-tecnica/modelos-291-347/modelo-347-presentacion-mediante-formulario.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Genera el 347: sólo salen los terceros que pasan de 3.005,06 € con IVA en el año.",
        "En la Sede elige «Modelo 347. Declaración anual de operaciones con terceras personas».",
        "Identifícate y abre el formulario; puedes «Importar año anterior» para reaprovechar los registros.",
        "Da de alta un registro por cliente (clave A) y otro por proveedor (clave B) con NIF, nombre e importe con IVA del año.",
        "«Validar»; corrige los NIF que dé por erróneos el servicio de identificación fiscal.",
        "Presenta y guarda el justificante. Marca el 347 como presentado.",
    ],
    fichero: "Admite presentación por fichero (TGVI Online) con el diseño de registro del ejercicio, útil si tienes muchos terceros; el formulario web llega hasta 40.000 registros.",
    aviso: "Plazo: hasta el 28 de febrero. No entran operaciones con el extranjero, alquileres ni las que ya van en el 349.",
};

const GUIA_349: Guia = Guia {
    codigo: "349",
    sede: "https://sede.agenciatributaria.gob.es/Sede/ayuda/consultas-informaticas/declaraciones-informativas-ayuda-tecnica/modelos-349-720/modelo-349-presentacion-mediante-fichero.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Genera el 349 del trimestre: un renglón por NIF-IVA de cliente con la base intracomunitaria.",
        "Comprueba antes en VIES que cada NIF-IVA sigue válido.",
        "En la Sede entra en «Modelo 349. Declaración recapitulativa de operaciones intracomunitarias».",
        "Identifícate y elige la presentación por formulario o por fichero.",
        "Si vas por fichero, súbelo con «Leer fichero»: primero valida, después presenta y descarga el justificante.",
        "Marca el trimestre como presentado.",
    ],
    fichero: "Admite fichero por TGVI Online (cualquier número de registros) con el diseño de registro del modelo. Si presentas varios registros y alguno falla, se pueden presentar los correctos y subsanar el resto.",
    aviso: "Plazo: 1-20 del mes siguiente al trimestre. Si superas 50.000 € de operaciones intracomunitarias, pasa a presentarlo cada mes.",
};

const GUIA_131: Guia = Guia {
    codigo: "131",
    sede: "https://sede.agenciatributaria.gob.es/Sede/presentar-consultar-declaraciones-modelo.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Comprueba los módulos que te aplican en la orden anual de módulos de tu actividad.",
        "En la Sede entra en «Modelo 131. Pago fraccionado en estimación objetiva».",
        "Identifícate y elige «Nueva declaración».",
        "Teclea las unidades de cada módulo (personal, local, vehículo…) y el formulario aplica el rendimiento por módulo.",
        "Comprueba el resultado y preséntalo; en el 4T se regulariza el año.",
        "Guarda el justificante y marca el trimestre como presentado.",
    ],
    fichero: "El 131 se presenta por formulario web; también admite los ficheros de ayuda que genera la AEAT.",
    aviso: "El 131 no se calcula con tus facturas: sale de los módulos. En estimación objetiva el 130 no se presenta.",
};

const GUIA_309: Guia = Guia {
    codigo: "309",
    sede: "https://sede.agenciatributaria.gob.es/Sede/presentar-consultar-declaraciones-modelo.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Identifica la operación: adquisición intracomunitaria, inversión del sujeto pasivo o venta de inmueble.",
        "En la Sede busca «Modelo 309. Declaración no periódica».",
        "Identifícate y rellena la base y la cuota de esa operación concreta.",
        "Preséntalo en los 30 días siguientes a la operación y guarda el justificante.",
    ],
    fichero: "Por formulario web; no hay fichero de declaración para el 309.",
    aviso: "Es la excepción del recargo de equivalencia: aunque no se presenten 303 ni 390, el 309 sí hay que presentarlo en esos casos.",
};

const GUIA_369: Guia = Guia {
    codigo: "369",
    sede: "https://sede.agenciatributaria.gob.es/Sede/iva/iva-comercio-electronico/presentacion-autoliquidaciones-periodicas-modelo-369.html",
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Comprueba que tienes el alta en ventanilla única (formulario 035); sin ella el 369 no se puede presentar.",
        "Reúne las ventas a particulares de cada país de la UE del periodo, con la base y el IVA del tipo de ese país.",
        "En la Sede entra en «Modelo 369. Declaración-liquidación periódica de los regímenes de ventanilla única».",
        "Identifícate, elige el régimen (Unión o importación) y el periodo.",
        "Teclea un renglón por país con la base y la cuota; el modelo suma y calcula el resultado a ingresar.",
        "Presenta y guarda el justificante; marca el trimestre como presentado en la guía.",
    ],
    fichero: "Admite presentación por fichero XML con el formato oficial, útil si hay muchos países o movimientos; el formulario web basta para un autónomo.",
    aviso: "OSS es trimestral (hasta el día 20 del mes siguiente); IOSS es mensual. Si no superas 10.000 € al año en ventas a particulares de la UE, puedes aplicar el 21 % español en el 303 en su lugar, pero la opción se mantiene y conviene revisarla cada año.",
};

const GUIA_035: Guia = Guia {
    codigo: "035",
    sede: "https://sede.agenciatributaria.gob.es/Sede/iva/iva-comercio-electronico/registro-regimen-unionregimen-exterior-regimen_intermediario.html",
    identificacion: &["Certificado electrónico o DNIe", "Cl@ve"],
    pasos: &[
        "Decide el régimen: Unión (OSS) si vendes desde España a particulares de la UE; importación (IOSS) si envías desde fuera de la UE paquetes de hasta 150 €.",
        "En la Sede entra en «Registro en el régimen de la Unión, régimen exterior y régimen de importación».",
        "Identifícate y elige el alta.",
        "Rellena tus datos y marca los regímenes en los que te registras; España será el Estado miembro de identificación.",
        "Presenta el formulario y guarda el justificante: el alta surte efecto desde el trimestre siguiente.",
        "A partir de ahí, presenta el 369 cada trimestre (o cada mes si es IOSS).",
    ],
    fichero: "Formulario web: el 035 no admite fichero de declaración.",
    aviso: "Sin el 035 no puedes usar el 369. Para cambiar datos o darte de baja se presenta el mismo formulario 035.",
};

const GUIA_200: Guia = Guia {
    codigo: "200",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN",
    ],
    pasos: &[
        "Cierra las cuentas del ejercicio con la contabilidad al día: el 200 se calcula sobre el resultado contable.",
        "En la Sede elige «Modelo 200. Impuesto sobre Sociedades. Ejercicio cerrado».",
        "Identifícate y elige «Nueva declaración»; puedes importar el borrador si lo generas con tu programa de contabilidad.",
        "Revisa el resultado, los ajustes y las casillas de liquidación.",
        "Valida, presenta y descarga el justificante con el nº de registro.",
    ],
    fichero: "Admite presentación por fichero con el diseño de registro oficial; el formulario web cubre a una pyme sin problema.",
    aviso: "Plazo: 1–25 de julio si el ejercicio cierra el 31 de diciembre (25 días naturales tras el cierre en los demás casos). No lo presenta el autónomo persona física: ese va con el 100.",
};

const GUIA_202: Guia = Guia {
    codigo: "202",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN",
    ],
    pasos: &[
        "Elige la modalidad: 18 % del beneficio del año en curso, o 17 % del resultado del ejercicio anterior.",
        "En la Sede busca «Modelo 202. Pago fraccionado del Impuesto sobre Sociedades».",
        "Identifícate y abre el periodo (abril, octubre o diciembre).",
        "Rellena la base y el porcentaje de la modalidad elegida; el modelo calcula el ingreso.",
        "Presenta y guarda el justificante.",
    ],
    fichero: "Formulario web; también admite fichero con el diseño de registro.",
    aviso: "Hay que presentarlo tres veces al año aunque no haya beneficio (a cero). Si el 200 sale a devolver, se compensa en la liquidación anual.",
};

const GUIA_100: Guia = Guia {
    codigo: "100",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN",
    ],
    pasos: &[
        "Reúne ingresos, gastos, retenciones y los pagos del 130 ya hechos en el año.",
        "Entra en Renta WEB con certificado o Cl@ve y abre el borrador.",
        "Revisa importes, situación personal y deducciones; no des por bueno el borrador sin mirarlo.",
        "Comprueba que las retenciones y los 130 cuadran con tus cuentas.",
        "Presenta y guarda el justificante; si sale a pagar, puedes domiciliarlo.",
    ],
    fichero: "No se presenta por fichero propio: se usa Renta WEB o la app de la AEAT.",
    aviso: "Campaña de abril a junio. Se presenta aunque no salga a pagar. Las sociedades no usan el 100: van con el 200.",
};

const GUIA_111: Guia = Guia {
    codigo: "111",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Reúne las nóminas del trimestre y, si las hay, las facturas de profesionales con retención.",
        "Calcula las percepciones íntegras (el bruto) y las retenciones de IRPF de cada perceptor. No uses la base de cotización para el 111.",
        "En la Sede elige «Modelo 111. Retenciones e ingresos a cuenta del IRPF».",
        "Identifícate, abre «Nueva declaración» y teclea perceptores, base de retenciones y cuota retenida.",
        "Presenta y guarda el justificante; el 190 resume el año y debe cuadrar con los cuatro 111.",
    ],
    fichero: "Admite fichero con el diseño de registro oficial, cómodo si hay muchos empleados; el formulario sirve para pocos.",
    aviso: "Plazo: 1–20 del mes siguiente a cada trimestre. Este borrador de AutónomoFACIL sólo suma las nóminas generadas: 01 perceptores, 02 percepciones íntegras (bruto) y 03 retenciones. La base de cotización no se usa en el 111. No incluye facturas de profesionales, rentas en especie, regularizaciones ni otros pagos; si no hay nóminas ni retenciones, no se presenta.",
};

const GUIA_115: Guia = Guia {
    codigo: "115",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Sólo si pagas alquiler de un local a un arrendador español y le retienes el 19 %.",
        "En la Sede elige «Modelo 115. Retenciones por rendimientos de inmuebles urbanos».",
        "Identifícate y abre «Nueva declaración».",
        "Teclea el número de inmuebles, la base y la retención del trimestre.",
        "Presenta y guarda el justificante; el 180 resume el año.",
    ],
    fichero: "Formulario web; también admite fichero con el diseño de registro.",
    aviso: "Plazo: 1–20 del mes siguiente a cada trimestre. Si el arrendador es extranjero sin establecimiento, el 115 no aplica igual: consúltalo.",
};

const GUIA_190: Guia = Guia {
    codigo: "190",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Reúne los cuatro 111 del año: perceptores, percepciones íntegras y retenciones.",
        "Comprueba que las nóminas generadas y sus NIF coinciden con lo presentado en cada 111.",
        "En la Sede elige «Modelo 190. Resumen anual de retenciones».",
        "Identifícate y abre «Nueva declaración»; puedes importar los 111 si los tienes en fichero.",
        "Comprueba que los totales cuadran con la suma de los 111 presentados.",
        "Presenta y guarda el justificante antes del 31 de enero.",
    ],
    fichero: "Admite fichero TGVI con el diseño de registro anual.",
    aviso: "Sólo si has presentado el 111. Este borrador se calcula con las nóminas generadas: 01 percepciones (nóminas), 02 percepciones íntegras (bruto), 03 retenciones y P personas distintas. La base de cotización no se usa. No incluye facturas de profesionales, rentas en especie ni otras rentas; revisa NIF y que cuadre con los cuatro 111 del año.",
};

const GUIA_180: Guia = Guia {
    codigo: "180",
    sede: SEDE,
    identificacion: &[
        "Certificado electrónico o DNIe",
        "Cl@ve Móvil o Cl@ve PIN (personas físicas)",
    ],
    pasos: &[
        "Reúne los cuatro 115 del año: arrendadores, bases y retenciones.",
        "En la Sede elige «Modelo 180. Resumen anual de retenciones por inmuebles».",
        "Identifícate y abre «Nueva declaración».",
        "Comprueba que los totales cuadran con los 115 presentados.",
        "Presenta y guarda el justificante antes del 31 de enero.",
    ],
    fichero: "Admite fichero TGVI con el diseño de registro anual.",
    aviso: "Sólo si has presentado el 115. El arrendador declara ese alquiler como ingreso: los importes deben coincidir.",
};

const GUIAS: &[&Guia] = &[
    &GUIA_303, &GUIA_130, &GUIA_390, &GUIA_347, &GUIA_349, &GUIA_131, &GUIA_309, &GUIA_369,
    &GUIA_035, &GUIA_200, &GUIA_202, &GUIA_100, &GUIA_111, &GUIA_115, &GUIA_190, &GUIA_180,
];

pub fn guia(codigo: &str) -> Option<&'static Guia> {
    GUIAS.iter().copied().find(|g| g.codigo == codigo)
}

/// Las vías reales de automatización, de menos a más esfuerzo.
pub const AUTOMATICO: &[(&str, &str)] = &[
    (
        "Guiado en la Sede (lo que hace la app)",
        "El borrador da los importes y la app te lleva a la página oficial del modelo. Tú te identificas y tecleas: es lo más seguro y no depende de nadie.",
    ),
    (
        "Fichero con diseño de registro",
        "390, 347 y 349 admiten subir un fichero con el formato oficial del ejercicio. La app ya tiene todas las casillas en el JSON; falta traducirlo a ese formato, que la AEAT cambia cada año.",
    ),
    (
        "Colaborador social o API de un tercero",
        "Plataformas tipo TaxDown o asesorías presentan como colaborador social autorizado por la AEAT. Automatizarlo así exige un acuerdo con ellos y enviarles el JSON de casillas por su API: la app ya lo genera.",
    ),
    (
        "Robot sobre la Sede (RPA)",
        "Un programa que maneje el navegador con el certificado del equipo. Funciona, pero la Sede cambia sin avisar y hay que revisarlo cada campaña; el certificado debe seguir en poder del autónomo.",
    ),
];

/// Enlace oficial con el diseño de registro de cada modelo.
pub fn diseno_registro(codigo: &str) -> Option<&'static str> {
    match codigo {
        "390" | "347" | "349" | "369" | "200" | "202" | "111" | "115" | "190" | "180" => Some(
            "https://sede.agenciatributaria.gob.es/Sede/ayuda/consultas-informaticas/presentacion-declaraciones-ayuda-tecnica.html",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_modelos_listados_tienen_guia() {
        // Los que la app calcula exigen pasos completos.
        for codigo in ["303", "130", "390", "347", "349"] {
            let g = guia(codigo).unwrap_or_else(|| panic!("falta la guía del {codigo}"));
            assert!(g.sede.starts_with("https://sede.agenciatributaria.gob.es/"));
            assert!(g.pasos.len() >= 5, "el {codigo} necesita pasos de verdad");
            assert!(!g.fichero.is_empty());
        }
        // Los que sólo se guían (sin cálculo) también tienen su ficha.
        for codigo in [
            "100", "111", "115", "180", "190", "200", "202", "369", "035",
        ] {
            let g = guia(codigo).unwrap_or_else(|| panic!("falta la guía del {codigo}"));
            assert!(g.sede.starts_with("https://sede.agenciatributaria.gob.es/"));
            assert!(g.pasos.len() >= 4, "el {codigo} necesita pasos de verdad");
            assert!(!g.fichero.is_empty());
        }
    }

    #[test]
    fn solo_los_que_admiten_fichero_tienen_diseno() {
        assert!(diseno_registro("347").is_some());
        assert!(diseno_registro("349").is_some());
        assert!(diseno_registro("390").is_some());
        assert!(diseno_registro("303").is_none());
        assert!(diseno_registro("130").is_none());
    }
}
