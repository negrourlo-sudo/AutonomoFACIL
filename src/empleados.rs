//! Empleados y nóminas.
//!
//! La nómina se calcula con los tipos generales de cotización de 2026 y el
//! IRPF que indique la ficha del empleado. Es una estimación de trabajo: no
//! sustituye el recibo oficial, el boletín de cotización ni el cálculo de la
//! AEAT. El objetivo es que el autónomo o la sociedad sepan cuánto cuesta una
//! persona y cuánto se retiene cada trimestre para el 111 y el 190.

use crate::model::div_round;
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Tipos generales de cotización usados en la estimación (puntos base).
pub const SS_CC_EMPLEADO_PB: i32 = 470; // 4,70 %
pub const SS_FORMACION_EMPLEADO_PB: i32 = 10; // 0,10 %
pub const SS_MEI_EMPLEADO_PB: i32 = 13; // 0,13 %
pub const SS_CC_EMPRESA_PB: i32 = 2360; // 23,60 %
pub const SS_FOGASA_PB: i32 = 20; // 0,20 %
pub const SS_FORMACION_EMPRESA_PB: i32 = 60; // 0,60 %
pub const SS_MEI_EMPRESA_PB: i32 = 67; // 0,67 %
pub const SS_AT_EP_PB: i32 = 150; // 1,50 % orientativo

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TipoContrato {
    #[default]
    Indefinido,
    Temporal,
    Formacion,
    Practicas,
}

impl TipoContrato {
    pub const TODOS: [TipoContrato; 4] = [
        TipoContrato::Indefinido,
        TipoContrato::Temporal,
        TipoContrato::Formacion,
        TipoContrato::Practicas,
    ];

    pub fn etiqueta(self) -> &'static str {
        match self {
            TipoContrato::Indefinido => "Indefinido",
            TipoContrato::Temporal => "Temporal",
            TipoContrato::Formacion => "Formación en alternancia",
            TipoContrato::Practicas => "Prácticas",
        }
    }

    /// Desempleo a cargo del trabajador.
    pub fn desempleo_empleado_pb(self) -> i32 {
        match self {
            TipoContrato::Indefinido => 155,
            _ => 160,
        }
    }

    /// Desempleo a cargo de la empresa.
    pub fn desempleo_empresa_pb(self) -> i32 {
        match self {
            TipoContrato::Indefinido => 550,
            _ => 670,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Empleado {
    pub id: u64,
    pub nombre: String,
    pub apellidos: String,
    pub nif: String,
    /// Número de afiliación a la Seguridad Social.
    pub naf: String,
    pub puesto: String,
    pub email: String,
    pub telefono: String,
    pub fecha_alta: NaiveDate,
    #[serde(default)]
    pub fecha_baja: Option<NaiveDate>,
    #[serde(default)]
    pub tipo_contrato: TipoContrato,
    /// Salario bruto anual en céntimos, antes de pagas extras.
    #[serde(default)]
    pub salario_bruto_anual_cents: i64,
    /// Retención de IRPF que se aplica en la nómina.
    #[serde(default)]
    pub irpf_pb: i32,
    #[serde(default)]
    pub notas: String,
}

impl Default for Empleado {
    fn default() -> Self {
        Self {
            id: 0,
            nombre: String::new(),
            apellidos: String::new(),
            nif: String::new(),
            naf: String::new(),
            puesto: String::new(),
            email: String::new(),
            telefono: String::new(),
            fecha_alta: Local::now().date_naive(),
            fecha_baja: None,
            tipo_contrato: TipoContrato::Indefinido,
            salario_bruto_anual_cents: 0,
            irpf_pb: 1500,
            notas: String::new(),
        }
    }
}

impl Empleado {
    pub fn nombre_completo(&self) -> String {
        if self.apellidos.trim().is_empty() {
            self.nombre.clone()
        } else {
            format!("{} {}", self.nombre, self.apellidos)
        }
    }

    pub fn etiqueta(&self) -> String {
        if self.nombre.trim().is_empty() {
            "(sin nombre)".to_string()
        } else if self.nif.trim().is_empty() {
            self.nombre_completo()
        } else {
            format!("{} ({})", self.nombre_completo(), self.nif)
        }
    }

    pub fn activo(&self) -> bool {
        self.fecha_baja.is_none()
    }

    /// ¿Está de alta en algún momento del mes indicado? La nómina es de mes
    /// completo; no se prorratea una alta o baja a mitad de mes.
    pub fn activo_en_mes(&self, anyo: i32, mes: u32) -> bool {
        let Some(inicio) = NaiveDate::from_ymd_opt(anyo, mes, 1) else {
            return false;
        };
        let siguiente = if mes == 12 {
            NaiveDate::from_ymd_opt(anyo + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(anyo, mes + 1, 1)
        };
        let Some(fin) = siguiente.map(|f| f - chrono::Duration::days(1)) else {
            return false;
        };
        self.fecha_alta <= fin && self.fecha_baja.map(|b| b >= inicio).unwrap_or(true)
    }

    /// Salario bruto mensual estimado: el anual repartido en 12 pagas.
    pub fn bruto_mensual_cents(&self) -> i64 {
        div_round(self.salario_bruto_anual_cents, 12)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Nomina {
    pub id: u64,
    pub empleado_id: u64,
    /// Copia de los datos laborales en el momento de generar la nómina, para
    /// que editarlos después no cambie un recibo ya calculado.
    #[serde(default)]
    pub empleado_nombre: String,
    #[serde(default)]
    pub empleado_nif: String,
    #[serde(default)]
    pub empleado_naf: String,
    #[serde(default)]
    pub puesto: String,
    #[serde(default)]
    pub tipo_contrato: Option<TipoContrato>,
    #[serde(default)]
    pub irpf_pb: Option<i32>,
    pub anyo: i32,
    pub mes: u32,
    pub bruto_cents: i64,
    pub ss_empleado_cents: i64,
    pub irpf_cents: i64,
    pub neto_cents: i64,
    pub ss_empresa_cents: i64,
    pub coste_empresa_cents: i64,
    #[serde(default)]
    pub pagada: bool,
}

impl Nomina {
    pub fn calcular(empleado: &Empleado, id: u64, anyo: i32, mes: u32) -> Self {
        let bruto = empleado.bruto_mensual_cents();
        let ss_empleado_pb = SS_CC_EMPLEADO_PB
            + empleado.tipo_contrato.desempleo_empleado_pb()
            + SS_FORMACION_EMPLEADO_PB
            + SS_MEI_EMPLEADO_PB;
        let ss_empresa_pb = SS_CC_EMPRESA_PB
            + empleado.tipo_contrato.desempleo_empresa_pb()
            + SS_FOGASA_PB
            + SS_FORMACION_EMPRESA_PB
            + SS_MEI_EMPRESA_PB
            + SS_AT_EP_PB;
        let ss_empleado = div_round(bruto * ss_empleado_pb as i64, 10_000);
        let irpf = div_round(bruto * empleado.irpf_pb as i64, 10_000);
        let ss_empresa = div_round(bruto * ss_empresa_pb as i64, 10_000);
        Self {
            id,
            empleado_id: empleado.id,
            empleado_nombre: empleado.nombre_completo(),
            empleado_nif: empleado.nif.clone(),
            empleado_naf: empleado.naf.clone(),
            puesto: empleado.puesto.clone(),
            tipo_contrato: Some(empleado.tipo_contrato),
            irpf_pb: Some(empleado.irpf_pb),
            anyo,
            mes,
            bruto_cents: bruto,
            ss_empleado_cents: ss_empleado,
            irpf_cents: irpf,
            neto_cents: bruto - ss_empleado - irpf,
            ss_empresa_cents: ss_empresa,
            coste_empresa_cents: bruto + ss_empresa,
            pagada: false,
        }
    }

    pub fn trimestre(&self) -> u32 {
        ((self.mes.saturating_sub(1)) / 3) + 1
    }
}

/// Genera las nóminas del mes sólo para los empleados activos con salario.
///
/// Es idempotente: si un empleado ya tiene una nómina de ese año y mes, no se
/// crea otra; se cuenta como saltada. No guarda el fichero ni toca la UI: eso
/// lo hace la capa que llama.
pub fn generar_nominas_mes(
    nominas: &mut Vec<Nomina>,
    empleados: &[Empleado],
    siguiente_id: &mut u64,
    anyo: i32,
    mes: u32,
) -> Result<(usize, usize), String> {
    if !(1..=12).contains(&mes) {
        return Err("Mes no válido.".into());
    }
    let empleados: Vec<Empleado> = empleados
        .iter()
        .filter(|e| e.activo_en_mes(anyo, mes) && e.salario_bruto_anual_cents > 0)
        .cloned()
        .collect();
    if empleados.is_empty() {
        return Err(
            "No hay empleados activos con salario ese mes. Revisa las fichas y las fechas."
                .into(),
        );
    }
    let mut creadas = 0;
    let mut saltadas = 0;
    for e in empleados {
        if nominas
            .iter()
            .any(|n| n.empleado_id == e.id && n.anyo == anyo && n.mes == mes)
        {
            saltadas += 1;
            continue;
        }
        let id = *siguiente_id;
        *siguiente_id += 1;
        nominas.push(Nomina::calcular(&e, id, anyo, mes));
        creadas += 1;
    }
    Ok((creadas, saltadas))
}

/// Una persona con nóminas en el periodo, agrupada para el 111 y el 190.
#[derive(Clone, Debug, Default)]
pub struct PerceptorRetencion {
    pub empleado_id: u64,
    pub nif: String,
    pub nombre: String,
    pub nominas: usize,
    pub bruto_cents: i64,
    pub irpf_cents: i64,
}

/// Datos de retenciones de nóminas de un trimestre o de un año completo.
#[derive(Clone, Debug, Default)]
pub struct ResumenRetenciones {
    pub anyo: i32,
    /// 1–4 para un trimestre; 0 para el año completo del 190.
    pub trimestre: u32,
    pub perceptores: Vec<PerceptorRetencion>,
    pub num_nominas: usize,
    pub total_bruto_cents: i64,
    pub total_irpf_cents: i64,
}

impl ResumenRetenciones {
    pub fn sin_nif(&self) -> usize {
        self.perceptores
            .iter()
            .filter(|p| p.nif.trim().is_empty())
            .count()
    }
}

/// Agrupa las nóminas por trabajador. Para el 111 se pasa un trimestre; para
/// el 190, `trimestre = 0`. No mezcla facturas de profesionales ni otros
/// rendimientos: sólo lo que hay en las nóminas generadas.
pub fn resumen_retenciones(
    nominas: &[Nomina],
    empleados: &[Empleado],
    anyo: i32,
    trimestre: u32,
) -> ResumenRetenciones {
    let mut por_persona: BTreeMap<u64, PerceptorRetencion> = BTreeMap::new();
    let mut num_nominas = 0usize;

    for n in nominas {
        if n.anyo != anyo || (trimestre != 0 && n.trimestre() != trimestre) {
            continue;
        }
        num_nominas += 1;
        let empleado = empleados.iter().find(|e| e.id == n.empleado_id);
        let entrada = por_persona
            .entry(n.empleado_id)
            .or_insert_with(|| PerceptorRetencion {
                empleado_id: n.empleado_id,
                nif: String::new(),
                nombre: String::new(),
                nominas: 0,
                bruto_cents: 0,
                irpf_cents: 0,
            });
        entrada.nominas += 1;
        entrada.bruto_cents += n.bruto_cents;
        entrada.irpf_cents += n.irpf_cents;
        if entrada.nif.trim().is_empty() {
            entrada.nif = if !n.empleado_nif.trim().is_empty() {
                n.empleado_nif.trim().to_string()
            } else {
                empleado.map(|e| e.nif.trim().to_string()).unwrap_or_default()
            };
        }
        if entrada.nombre.trim().is_empty() {
            entrada.nombre = if !n.empleado_nombre.trim().is_empty() {
                n.empleado_nombre.trim().to_string()
            } else {
                empleado
                    .map(|e| e.nombre_completo())
                    .unwrap_or_else(|| "(sin nombre)".to_string())
            };
        }
    }

    let mut perceptores: Vec<PerceptorRetencion> = por_persona.into_values().collect();
    perceptores.sort_by(|a, b| {
        a.nombre
            .to_lowercase()
            .cmp(&b.nombre.to_lowercase())
            .then_with(|| a.nif.cmp(&b.nif))
    });
    let total_bruto_cents = perceptores.iter().map(|p| p.bruto_cents).sum();
    let total_irpf_cents = perceptores.iter().map(|p| p.irpf_cents).sum();

    ResumenRetenciones {
        anyo,
        trimestre,
        perceptores,
        num_nominas,
        total_bruto_cents,
        total_irpf_cents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empleado() -> Empleado {
        Empleado {
            id: 7,
            nombre: "Ana".to_string(),
            apellidos: "García Ruiz".to_string(),
            nif: "12345678Z".to_string(),
            fecha_alta: NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
            tipo_contrato: TipoContrato::Indefinido,
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        }
    }

    #[test]
    fn la_nomina_calcula_neto_y_coste_de_empresa() {
        let n = Nomina::calcular(&empleado(), 9, 2026, 1);
        assert_eq!(n.bruto_cents, 200_000);
        // 4,70 + 1,55 + 0,10 + 0,13 = 6,48 %
        assert_eq!(n.ss_empleado_cents, 12_960);
        assert_eq!(n.irpf_cents, 30_000);
        assert_eq!(n.irpf_pb, Some(1500));
        assert_eq!(n.empleado_nif, "12345678Z");
        assert_eq!(n.neto_cents, 157_040);
        // 23,60 + 5,50 + 0,20 + 0,60 + 0,67 + 1,50 = 32,07 %
        assert_eq!(n.ss_empresa_cents, 64_140);
        assert_eq!(n.coste_empresa_cents, 264_140);
        assert_eq!(n.trimestre(), 1);
    }

    #[test]
    fn el_temporal_cotiza_un_poco_mas() {
        let mut e = empleado();
        e.tipo_contrato = TipoContrato::Temporal;
        let n = Nomina::calcular(&e, 10, 2026, 1);
        // 4,70 + 1,60 + 0,10 + 0,13 = 6,53 %
        assert_eq!(n.ss_empleado_cents, 13_060);
        assert!(n.neto_cents < 157_040);
    }

    #[test]
    fn la_actividad_en_el_mes_respeta_alta_y_baja() {
        let mut e = empleado();
        assert!(e.activo_en_mes(2026, 1));
        assert!(!e.activo_en_mes(2025, 12));
        e.fecha_baja = Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
        assert!(e.activo_en_mes(2026, 3));
        assert!(!e.activo_en_mes(2026, 4));
        assert!(!e.activo());
    }

    #[test]
    fn el_nombre_y_la_etiqueta_no_duplican_apellidos_vacios() {
        let mut e = empleado();
        assert_eq!(e.nombre_completo(), "Ana García Ruiz");
        e.apellidos.clear();
        assert_eq!(e.nombre_completo(), "Ana");
        assert_eq!(e.etiqueta(), "Ana (12345678Z)");
    }

    #[test]
    fn generar_dos_veces_el_mismo_mes_no_duplica_las_nominas() {
        let mut otra = empleado();
        otra.id = 8;
        otra.nombre = "Luis".into();
        otra.apellidos = "Pérez".into();
        let equipo = [empleado(), otra];
        let mut nominas = Vec::new();
        let mut siguiente_id = 100;

        let (creadas, saltadas) = generar_nominas_mes(
            &mut nominas,
            &equipo,
            &mut siguiente_id,
            2026,
            1,
        )
        .expect("la primera generación debe crear las nóminas");
        assert_eq!((creadas, saltadas), (2, 0));
        assert_eq!(nominas.len(), 2);
        assert_eq!(siguiente_id, 102);
        let ids = [nominas[0].id, nominas[1].id];

        let (creadas, saltadas) = generar_nominas_mes(
            &mut nominas,
            &equipo,
            &mut siguiente_id,
            2026,
            1,
        )
        .expect("la segunda generación debe saltar las existentes");
        assert_eq!((creadas, saltadas), (0, 2));
        assert_eq!(nominas.len(), 2);
        assert_eq!(siguiente_id, 102);
        assert_eq!([nominas[0].id, nominas[1].id], ids);
    }

    #[test]
    fn generar_nominas_mes_rechaza_meses_invalidos_sin_consumir_ids() {
        for mes in [0, 13] {
            let mut nominas = Vec::new();
            let mut siguiente_id = 100;
            let r = generar_nominas_mes(
                &mut nominas,
                &[empleado()],
                &mut siguiente_id,
                2026,
                mes,
            );
            assert!(r.is_err(), "el mes {mes} debería rechazarse");
            assert!(nominas.is_empty());
            assert_eq!(siguiente_id, 100);
        }
    }

    #[test]
    fn generar_nominas_mes_falla_sin_empleados_validos_sin_consumir_ids() {
        let mut inactivo = empleado();
        inactivo.fecha_alta = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        inactivo.fecha_baja = Some(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
        let mut sin_salario = empleado();
        sin_salario.salario_bruto_anual_cents = 0;
        let casos: [Vec<Empleado>; 2] = [vec![inactivo], vec![sin_salario]];

        for equipo in casos {
            let mut nominas = Vec::new();
            let mut siguiente_id = 100;
            let r = generar_nominas_mes(
                &mut nominas,
                &equipo,
                &mut siguiente_id,
                2026,
                1,
            );
            assert!(r.is_err(), "no debería generar con {equipo:?}");
            assert!(nominas.is_empty());
            assert_eq!(siguiente_id, 100);
        }
    }

    #[test]
    fn generar_nominas_mes_mezcla_nuevas_y_existentes_sin_duplicar() {
        let existente = Nomina::calcular(&empleado(), 50, 2026, 1);
        let mut nominas = vec![existente];
        let mut siguiente_id = 100;
        let mut nuevo = empleado();
        nuevo.id = 8;

        let (creadas, saltadas) = generar_nominas_mes(
            &mut nominas,
            &[empleado(), nuevo],
            &mut siguiente_id,
            2026,
            1,
        )
        .expect("la mezcla debe crear sólo la nómina que falta");

        assert_eq!((creadas, saltadas), (1, 1));
        assert_eq!(nominas.len(), 2);
        assert_eq!(nominas[0].id, 50);
        assert_eq!(nominas[0].empleado_id, 7);
        assert_eq!(nominas[1].id, 100);
        assert_eq!(nominas[1].empleado_id, 8);
        assert_eq!(siguiente_id, 101);
    }

    #[test]
    fn el_resumen_agrupa_nominas_por_persona_y_trimestre() {
        let e = empleado();
        let mut otra = empleado();
        otra.id = 8;
        otra.nombre = "Luis".into();
        otra.apellidos = "Pérez".into();
        otra.nif = "87654321X".into();
        otra.salario_bruto_anual_cents = 1_800_000;
        otra.irpf_pb = 1000;

        let nominas = vec![
            Nomina::calcular(&e, 1, 2026, 1),
            Nomina::calcular(&e, 2, 2026, 2),
            Nomina::calcular(&e, 3, 2026, 4),
            Nomina::calcular(&otra, 4, 2026, 2),
        ];
        let q1 = resumen_retenciones(&nominas, &[e, otra], 2026, 1);
        assert_eq!(q1.num_nominas, 3);
        assert_eq!(q1.perceptores.len(), 2);
        assert_eq!(q1.total_bruto_cents, 550_000);
        assert_eq!(q1.total_irpf_cents, 75_000);

        let q2 = resumen_retenciones(&nominas, &[], 2026, 2);
        assert_eq!(q2.num_nominas, 1);
        assert_eq!(q2.perceptores.len(), 1);
        assert_eq!(q2.total_bruto_cents, 200_000);
        assert_eq!(q2.total_irpf_cents, 30_000);
        assert!(q2.perceptores.iter().all(|p| !p.nif.is_empty()));

        let anual = resumen_retenciones(&nominas, &[], 2026, 0);
        assert_eq!(anual.num_nominas, 4);
        assert_eq!(anual.perceptores.len(), 2);
        assert_eq!(anual.total_bruto_cents, 750_000);
        assert_eq!(anual.total_irpf_cents, 105_000);
    }

    #[test]
    fn el_resumen_usa_el_nif_actual_si_la_nomina_es_antigua() {
        let e = empleado();
        let mut n = Nomina::calcular(&e, 1, 2026, 1);
        n.empleado_nif.clear();
        let r = resumen_retenciones(&[n], &[e.clone()], 2026, 1);
        assert_eq!(r.perceptores[0].nif, "12345678Z");
        assert_eq!(r.sin_nif(), 0);
    }
}
