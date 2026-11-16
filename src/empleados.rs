//! Empleados y nóminas.
//!
//! La nómina se calcula con el paquete anual de parámetros del ejercicio y el
//! IRPF que indique la ficha del empleado. Es una estimación de trabajo: no
//! sustituye el recibo oficial, el boletín de cotización ni el cálculo de la
//! AEAT. El objetivo es que el autónomo o la sociedad sepan cuánto cuesta una
//! persona y cuánto se retiene cada trimestre para el 111 y el 190.
//!
//! Prorrateo: el salario usa días naturales del mes, contando tanto el día de
//! alta como el de baja. El divisor es el número real de días del mes (28, 29,
//! 30 o 31). Un alta el día 1 o una baja el último día dan el mes completo.
//! Los límites de base no siguen siempre ese divisor: los grupos 1 a 7 son
//! mensuales y, en un mes incompleto, se prorratean con 30 días; los grupos 8 a
//! 11 tienen bases diarias y, con salario mensual, usan los días de alta en un
//! mes incompleto y la equivalencia de 30 días con el tope mensual en un mes
//! completo.

use crate::model::{div_round, fmt_cents, fmt_pb};
pub use crate::nomina_paquetes::BasesCotizacion;
use crate::nomina_paquetes::PaqueteNomina;
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Grupo de cotización del Régimen General. Los datos antiguos sin grupo se
/// interpretan como grupo 7 para no alterar sus importes más de lo necesario.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrupoCotizacion {
    Grupo1,
    Grupo2,
    Grupo3,
    Grupo4,
    Grupo5,
    Grupo6,
    Grupo7,
    Grupo8,
    Grupo9,
    Grupo10,
    Grupo11,
}

impl Default for GrupoCotizacion {
    fn default() -> Self {
        Self::Grupo7
    }
}

impl GrupoCotizacion {
    pub const TODOS: [GrupoCotizacion; 11] = [
        GrupoCotizacion::Grupo1,
        GrupoCotizacion::Grupo2,
        GrupoCotizacion::Grupo3,
        GrupoCotizacion::Grupo4,
        GrupoCotizacion::Grupo5,
        GrupoCotizacion::Grupo6,
        GrupoCotizacion::Grupo7,
        GrupoCotizacion::Grupo8,
        GrupoCotizacion::Grupo9,
        GrupoCotizacion::Grupo10,
        GrupoCotizacion::Grupo11,
    ];

    pub fn numero(self) -> u32 {
        self as u32 + 1
    }

    /// Los grupos 8 a 11 se expresan con bases diarias; los grupos 1 a 7, con
    /// bases mensuales. La app calcula salario mensual y ajusta el mes completo
    /// de los grupos diarios a 30 días.
    pub fn es_diaria(self) -> bool {
        self.numero() >= 8
    }

    pub fn etiqueta(self) -> &'static str {
        match self {
            GrupoCotizacion::Grupo1 => "1 · Ingenieros y licenciados",
            GrupoCotizacion::Grupo2 => "2 · Ingenieros técnicos y peritos",
            GrupoCotizacion::Grupo3 => "3 · Jefes administrativos y de taller",
            GrupoCotizacion::Grupo4 => "4 · Ayudantes no titulados",
            GrupoCotizacion::Grupo5 => "5 · Oficiales administrativos",
            GrupoCotizacion::Grupo6 => "6 · Subalternos",
            GrupoCotizacion::Grupo7 => "7 · Auxiliares administrativos",
            GrupoCotizacion::Grupo8 => "8 · Oficiales de primera y segunda",
            GrupoCotizacion::Grupo9 => "9 · Oficiales de tercera y especialistas",
            GrupoCotizacion::Grupo10 => "10 · Peones",
            GrupoCotizacion::Grupo11 => "11 · Menores de 18 años",
        }
    }
}

/// Bases mínimas y máximas de un grupo y ejercicio. Para los grupos 1 a 7 son
/// mensuales; para los grupos 8 a 11 son diarias. La nómina ajusta estas
/// últimas a 30 días cuando el mes está completo y hay salario mensual.
/// Devuelve `None` para ejercicios sin tabla cargada; en ese caso la nómina no
/// aplica topes.
pub fn bases_cotizacion(anyo: i32, grupo: GrupoCotizacion) -> Option<BasesCotizacion> {
    crate::nomina_paquetes::paquete_nomina(anyo)?.base_grupo(grupo.numero())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BaseAplicada {
    aplicada_cents: i64,
    minima_cents: i64,
    maxima_cents: i64,
    tabla: Option<BasesCotizacion>,
}

fn base_cotizacion_aplicada(
    paquete: &PaqueteNomina,
    bruto_cents: i64,
    grupo: GrupoCotizacion,
    dias_trabajados: u32,
    dias_mes: u32,
) -> BaseAplicada {
    let Some(limites) = paquete.base_grupo(grupo.numero()) else {
        return BaseAplicada {
            aplicada_cents: bruto_cents,
            minima_cents: 0,
            maxima_cents: 0,
            tabla: None,
        };
    };
    if dias_mes == 0 {
        return BaseAplicada {
            aplicada_cents: bruto_cents,
            minima_cents: 0,
            maxima_cents: 0,
            tabla: Some(limites),
        };
    }
    // Los límites se prorratean según el grupo y la modalidad de salario.
    // La app calcula nóminas mensuales: en los grupos diarios, el mes completo
    // se ajusta a 30 días y queda sujeto al tope mensual; el límite diario se
    // usa en los meses incompletos.
    let (minima, maxima) = if grupo.es_diaria() {
        if dias_trabajados >= dias_mes {
            (
                limites.minima_cents * 30,
                (limites.maxima_cents * 30).min(paquete.tope_maximo_mensual_cents),
            )
        } else {
            (
                limites.minima_cents * dias_trabajados as i64,
                limites.maxima_cents * dias_trabajados as i64,
            )
        }
    } else if dias_trabajados < dias_mes {
        // Grupos mensuales en mes incompleto: divisor oficial de 30 días.
        (
            div_round(limites.minima_cents * dias_trabajados as i64, 30),
            div_round(limites.maxima_cents * dias_trabajados as i64, 30),
        )
    } else {
        (limites.minima_cents, limites.maxima_cents)
    };
    BaseAplicada {
        aplicada_cents: bruto_cents.clamp(minima, maxima),
        minima_cents: minima,
        maxima_cents: maxima,
        tabla: Some(limites),
    }
}

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

    /// Los contratos formativos y de prácticas usan el desempleo temporal a
    /// efectos de esta estimación.
    pub fn es_indefinido(self) -> bool {
        self == TipoContrato::Indefinido
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
    /// Grupo de cotización que fija las bases mínimas y máximas.
    #[serde(default)]
    pub grupo_cotizacion: GrupoCotizacion,
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
            grupo_cotizacion: GrupoCotizacion::Grupo7,
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

    /// ¿Está de alta en algún momento del mes indicado?
    pub fn activo_en_mes(&self, anyo: i32, mes: u32) -> bool {
        self.dias_trabajados_en_mes(anyo, mes) > 0
    }

    /// Días naturales trabajados en el mes, contando el alta y la baja.
    /// Devuelve 0 si el empleado no coincide con el mes.
    pub fn dias_trabajados_en_mes(&self, anyo: i32, mes: u32) -> u32 {
        let Some((inicio, fin)) = rango_mes(anyo, mes) else {
            return 0;
        };
        let desde = self.fecha_alta.max(inicio);
        let hasta = self.fecha_baja.unwrap_or(fin).min(fin);
        if desde > hasta {
            0
        } else {
            (hasta - desde).num_days() as u32 + 1
        }
    }

    /// Salario bruto mensual estimado: el anual repartido en 12 pagas.
    pub fn bruto_mensual_cents(&self) -> i64 {
        div_round(self.salario_bruto_anual_cents, 12)
    }
}

/// Días naturales del mes. Febrero devuelve 28 o 29 según el año.
pub fn dias_del_mes(anyo: i32, mes: u32) -> u32 {
    let Some((inicio, fin)) = rango_mes(anyo, mes) else {
        return 0;
    };
    (fin - inicio).num_days() as u32 + 1
}

fn rango_mes(anyo: i32, mes: u32) -> Option<(NaiveDate, NaiveDate)> {
    if !(1..=12).contains(&mes) {
        return None;
    }
    let inicio = NaiveDate::from_ymd_opt(anyo, mes, 1)?;
    let siguiente = if mes == 12 {
        NaiveDate::from_ymd_opt(anyo + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(anyo, mes + 1, 1)?
    };
    Some((inicio, siguiente - chrono::Duration::days(1)))
}

/// Versión del formato del expediente. Si cambian los campos de cálculo, se
/// sube este número para poder distinguir expedientes antiguos.
pub const VERSION_EXPEDIENTE_NOMINA: u32 = 2;

/// Estado del ciclo de vida de una nómina. Una nómina cerrada es inmutable:
/// para corregirla se crea un recibo de diferencias enlazado a ella.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EstadoNomina {
    #[default]
    Borrador,
    Revisada,
    Aprobada,
    Pagada,
    Cerrada,
}

impl EstadoNomina {
    pub fn nombre(self) -> &'static str {
        match self {
            EstadoNomina::Borrador => "Borrador",
            EstadoNomina::Revisada => "Revisada",
            EstadoNomina::Aprobada => "Aprobada",
            EstadoNomina::Pagada => "Pagada",
            EstadoNomina::Cerrada => "Cerrada",
        }
    }

    /// Siguiente paso del ciclo. No hay atajos: cada estado se confirma antes
    /// de pasar al siguiente y una nómina cerrada ya no avanza más.
    pub fn siguiente(self) -> Option<Self> {
        match self {
            EstadoNomina::Borrador => Some(EstadoNomina::Revisada),
            EstadoNomina::Revisada => Some(EstadoNomina::Aprobada),
            EstadoNomina::Aprobada => Some(EstadoNomina::Pagada),
            EstadoNomina::Pagada => Some(EstadoNomina::Cerrada),
            EstadoNomina::Cerrada => None,
        }
    }

    pub fn accion(self) -> &'static str {
        match self {
            EstadoNomina::Borrador => "Marcar revisada",
            EstadoNomina::Revisada => "Aprobar",
            EstadoNomina::Aprobada => "Marcar pagada",
            EstadoNomina::Pagada => "Cerrar",
            EstadoNomina::Cerrada => "Cerrada",
        }
    }

    pub fn cerrada(self) -> bool {
        self == EstadoNomina::Cerrada
    }
}

/// Una nómina normal o un recibo de diferencias. El recibo de diferencias no
/// duplica el recibo original: guarda sólo los importes de la corrección y
/// queda enlazado al original mediante `Nomina::nomina_origen`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TipoNomina {
    #[default]
    Ordinaria,
    Diferencia,
}

impl TipoNomina {
    pub fn nombre(self) -> &'static str {
        match self {
            TipoNomina::Ordinaria => "Nómina ordinaria",
            TipoNomina::Diferencia => "Recibo de diferencias",
        }
    }

    pub fn diferencia(self) -> bool {
        self == TipoNomina::Diferencia
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SeccionExpediente {
    #[default]
    Informativo,
    Devengo,
    /// Conceptos e incidencias introducidos para el mes (extras, variables,
    /// vacaciones, bajas y deducciones).
    Conceptos,
    Base,
    Deduccion,
    Coste,
    Diferencia,
}

impl SeccionExpediente {
    pub fn nombre(self) -> &'static str {
        match self {
            SeccionExpediente::Informativo => "Datos del cálculo",
            SeccionExpediente::Devengo => "Devengos",
            SeccionExpediente::Conceptos => "Conceptos e incidencias",
            SeccionExpediente::Base => "Base de cotización",
            SeccionExpediente::Deduccion => "Deducciones",
            SeccionExpediente::Coste => "Coste de empresa",
            SeccionExpediente::Diferencia => "Diferencias",
        }
    }
}

/// Una línea del expediente: qué se calculó, con qué fórmula y qué importe.
/// La fórmula es texto legible, no una expresión evaluable: el objetivo es que
/// el usuario y su asesor puedan revisar el cálculo sin abrir el código.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LineaExpediente {
    pub seccion: SeccionExpediente,
    pub etiqueta: String,
    pub formula: String,
    pub importe_cents: i64,
}

/// Copia inmutable de todo lo necesario para explicar y reproducir una nómina:
/// entradas, topes aplicados, líneas de cotización/deducción/coste y huella.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpedienteNomina {
    pub version: u32,
    pub generado: String,
    pub anyo: i32,
    pub mes: u32,
    pub grupo_cotizacion: GrupoCotizacion,
    pub tipo_contrato: TipoContrato,
    pub reglas: String,
    pub salario_anual_cents: i64,
    pub salario_mensual_cents: i64,
    pub dias_trabajados: u32,
    pub dias_mes: u32,
    pub bruto_cents: i64,
    pub tabla: Option<BasesCotizacion>,
    pub base_minima_cents: i64,
    pub base_maxima_cents: i64,
    pub base_cotizacion_cents: i64,
    pub irpf_pb: i32,
    pub ss_empleado_cents: i64,
    pub irpf_cents: i64,
    pub neto_cents: i64,
    pub ss_empresa_cents: i64,
    pub coste_empresa_cents: i64,
    pub lineas: Vec<LineaExpediente>,
    /// SHA-256 en hexadecimal de todos los campos anteriores y de las líneas.
    /// No es una firma: sirve para detectar cambios accidentales o ediciones
    /// manuales del JSON.
    pub huella: String,
}

impl ExpedienteNomina {
    fn texto_huella(&self) -> String {
        let tabla = match self.tabla {
            Some(t) => format!("{}|{}", t.minima_cents, t.maxima_cents),
            None => "sin-tabla".to_string(),
        };
        let mut texto = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.version,
            self.generado,
            self.anyo,
            self.mes,
            format!("{:?}", self.grupo_cotizacion),
            format!("{:?}", self.tipo_contrato),
            self.reglas,
            self.salario_anual_cents,
            self.salario_mensual_cents,
            self.dias_trabajados,
            self.dias_mes,
            self.bruto_cents,
            tabla,
            self.base_minima_cents,
            self.base_maxima_cents,
            self.base_cotizacion_cents,
            self.irpf_pb,
            self.ss_empleado_cents,
            self.irpf_cents,
            self.neto_cents,
            self.ss_empresa_cents,
            self.coste_empresa_cents
        );
        for l in &self.lineas {
            texto.push_str(&format!(
                "\n{:?}|{}|{}|{}",
                l.seccion, l.etiqueta, l.formula, l.importe_cents
            ));
        }
        texto
    }

    pub fn calcular_huella(&self) -> String {
        Sha256::digest(self.texto_huella().as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Comprueba que el expediente no se ha modificado desde que se generó.
    pub fn huella_valida(&self) -> bool {
        !self.huella.is_empty() && self.huella == self.calcular_huella()
    }
}

fn linea_expediente(
    seccion: SeccionExpediente,
    etiqueta: impl Into<String>,
    formula: impl Into<String>,
    importe_cents: i64,
) -> LineaExpediente {
    LineaExpediente {
        seccion,
        etiqueta: etiqueta.into(),
        formula: formula.into(),
        importe_cents,
    }
}

fn linea_cotizacion(
    seccion: SeccionExpediente,
    etiqueta: &str,
    base_cents: i64,
    tipo_pb: i32,
) -> LineaExpediente {
    linea_expediente(
        seccion,
        etiqueta,
        format!("{} × {} %", fmt_cents(base_cents), fmt_pb(tipo_pb)),
        div_round(base_cents * tipo_pb as i64, 10_000),
    )
}

/// Conceptos e incidencias de un mes concreto. Los importes son cantidades
/// monetarias en céntimos; los días de vacaciones y baja se guardan como
/// información para el expediente y el recibo. Para no inventar tratamientos
/// legales que el motor no modela, el importe de la baja es un descuento
/// explícito y el de vacaciones, una compensación explícita: si se dejan a
/// cero, los días no alteran el salario.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ConceptosNomina {
    /// Paga o pagas extras devengadas en el mes.
    #[serde(default)]
    pub pagas_extras_cents: i64,
    /// Importe de horas extra. Se aplican los tipos generales del paquete.
    #[serde(default)]
    pub horas_extra_cents: i64,
    /// Variables, bonus o complementos variables del mes.
    #[serde(default)]
    pub variables_cents: i64,
    #[serde(default)]
    pub vacaciones_dias: u32,
    /// Compensación económica de vacaciones, si procede.
    #[serde(default)]
    pub vacaciones_cents: i64,
    #[serde(default)]
    pub bajas_dias: u32,
    /// Descuento aplicado al bruto por la baja. Positivo = resta.
    #[serde(default)]
    pub bajas_cents: i64,
    /// Otras deducciones que restan del neto pero no del bruto ni de la base.
    #[serde(default)]
    pub deducciones_cents: i64,
    #[serde(default)]
    pub nota: String,
}

impl ConceptosNomina {
    /// Efecto directo en el bruto: los conceptos retributivos suman y el
    /// descuento por baja resta. Las deducciones no entran aquí porque no son
    /// salario ni cotizan.
    pub fn efecto_bruto_cents(&self) -> i64 {
        self.pagas_extras_cents
            + self.horas_extra_cents
            + self.variables_cents
            + self.vacaciones_cents
            - self.bajas_cents
    }

    pub fn vacia(&self) -> bool {
        self.pagas_extras_cents == 0
            && self.horas_extra_cents == 0
            && self.variables_cents == 0
            && self.vacaciones_dias == 0
            && self.vacaciones_cents == 0
            && self.bajas_dias == 0
            && self.bajas_cents == 0
            && self.deducciones_cents == 0
            && self.nota.trim().is_empty()
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
    /// Grupo aplicado en el momento de generar la nómina.
    #[serde(default)]
    pub grupo_cotizacion: GrupoCotizacion,
    #[serde(default)]
    pub irpf_pb: Option<i32>,
    /// Copia exacta del paquete anual de parámetros usado al calcular la
    /// nómina. Las nóminas antiguas no lo tienen; los recibos de diferencias
    /// conservan el paquete de la nómina original.
    #[serde(default)]
    pub paquete: Option<PaqueteNomina>,
    /// Salario base del periodo antes de aplicar conceptos e incidencias.
    #[serde(default)]
    pub salario_base_cents: i64,
    /// Copia de los conceptos introducidos al calcular o recalcular la nómina.
    #[serde(default)]
    pub conceptos: ConceptosNomina,
    /// Indica si es una nómina ordinaria o un recibo de diferencias.
    #[serde(default)]
    pub tipo: TipoNomina,
    /// Nómina cerrada que corrige este recibo de diferencias.
    #[serde(default)]
    pub nomina_origen: Option<u64>,
    #[serde(default)]
    pub motivo_diferencia: String,
    pub anyo: i32,
    pub mes: u32,
    /// Días naturales del mes que se han pagado (alta y baja incluidos).
    #[serde(default)]
    pub dias_trabajados: u32,
    /// Días naturales del mes usado como divisor (28, 29, 30 o 31).
    #[serde(default)]
    pub dias_mes: u32,
    pub bruto_cents: i64,
    /// Base de cotización aplicada después de mínimos y máximos.
    #[serde(default)]
    pub base_cotizacion_cents: i64,
    pub ss_empleado_cents: i64,
    pub irpf_cents: i64,
    pub neto_cents: i64,
    pub ss_empresa_cents: i64,
    pub coste_empresa_cents: i64,
    #[serde(default)]
    pub pagada: bool,
    #[serde(default)]
    pub estado: EstadoNomina,
    /// Expediente de cálculo generado al crear la nómina. Las nóminas
    /// anteriores a esta función no lo tienen y se muestran como tales.
    #[serde(default)]
    pub expediente: Option<ExpedienteNomina>,
}

impl Nomina {
    /// Calcula la nómina de un mes sin conceptos adicionales. Si el alta o la
    /// baja caen dentro del mes, el bruto mensual se prorratea por días
    /// naturales, contando ambos extremos y usando los días reales del mes
    /// como divisor. La base de cotización se limita por el grupo y el
    /// ejercicio; el IRPF se calcula sobre el bruto prorrateado.
    pub fn calcular(empleado: &Empleado, id: u64, anyo: i32, mes: u32) -> Self {
        Self::calcular_con_conceptos(
            empleado,
            id,
            anyo,
            mes,
            &ConceptosNomina::default(),
        )
    }

    /// Calcula la nómina aplicando los conceptos e incidencias del mes. Los
    /// conceptos retributivos suman al bruto y, por tanto, también a la base y
    /// al IRPF; el descuento por baja resta del bruto; las deducciones restan
    /// sólo del neto.
    pub fn calcular_con_conceptos(
        empleado: &Empleado,
        id: u64,
        anyo: i32,
        mes: u32,
        conceptos: &ConceptosNomina,
    ) -> Self {
        let dias_mes = dias_del_mes(anyo, mes);
        let dias_trabajados = empleado.dias_trabajados_en_mes(anyo, mes);
        let bruto_mensual = empleado.bruto_mensual_cents();
        let salario_base_cents = if dias_mes == 0 || dias_trabajados == 0 {
            0
        } else if dias_trabajados >= dias_mes {
            bruto_mensual
        } else {
            div_round(bruto_mensual * dias_trabajados as i64, dias_mes as i64)
        };
        let bruto_sin_limite = salario_base_cents + conceptos.efecto_bruto_cents();
        let bruto = bruto_sin_limite.max(0);
        let paquete = crate::nomina_paquetes::paquete_para_calculo(anyo);
        let indefinido = empleado.tipo_contrato.es_indefinido();
        let ss_empleado_pb = paquete.ss_empleado_pb(indefinido);
        let ss_empresa_pb = paquete.ss_empresa_pb(indefinido);
        let base = base_cotizacion_aplicada(
            &paquete,
            bruto,
            empleado.grupo_cotizacion,
            dias_trabajados,
            dias_mes,
        );
        let base_cotizacion_cents = base.aplicada_cents;
        let ss_empleado = div_round(base_cotizacion_cents * ss_empleado_pb as i64, 10_000);
        let irpf = div_round(bruto * empleado.irpf_pb as i64, 10_000);
        let ss_empresa = div_round(base_cotizacion_cents * ss_empresa_pb as i64, 10_000);
        let neto = bruto - ss_empleado - irpf - conceptos.deducciones_cents;
        let coste_empresa = bruto + ss_empresa;

        let mut expediente = ExpedienteNomina {
            version: VERSION_EXPEDIENTE_NOMINA,
            generado: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            anyo,
            mes,
            grupo_cotizacion: empleado.grupo_cotizacion,
            tipo_contrato: empleado.tipo_contrato,
            reglas: format!(
                "Salario mensual en 12 pagas · bases {} · paquete {}{} · sin bonificaciones",
                if base.tabla.is_some() {
                    "de cotización cargadas"
                } else {
                    "sin cargar para el ejercicio"
                },
                paquete.version,
                if conceptos.vacia() {
                    ""
                } else {
                    " · conceptos e incidencias del mes"
                }
            ),
            salario_anual_cents: empleado.salario_bruto_anual_cents,
            salario_mensual_cents: bruto_mensual,
            dias_trabajados,
            dias_mes,
            bruto_cents: bruto,
            tabla: base.tabla,
            base_minima_cents: base.minima_cents,
            base_maxima_cents: base.maxima_cents,
            base_cotizacion_cents,
            irpf_pb: empleado.irpf_pb,
            ss_empleado_cents: ss_empleado,
            irpf_cents: irpf,
            neto_cents: neto,
            ss_empresa_cents: ss_empresa,
            coste_empresa_cents: coste_empresa,
            lineas: Vec::new(),
            huella: String::new(),
        };

        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Informativo,
            "Salario bruto anual",
            "Dato de la ficha del empleado",
            empleado.salario_bruto_anual_cents,
        ));
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Informativo,
            "Paquete anual de nómina",
            format!("{} · {}", paquete.version, paquete.fuente),
            0,
        ));
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Informativo,
            "Salario bruto mensual",
            format!("{} / 12", fmt_cents(empleado.salario_bruto_anual_cents)),
            bruto_mensual,
        ));
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Devengo,
            "Salario bruto del periodo",
            if dias_mes > 0 && dias_trabajados < dias_mes {
                format!(
                    "{} × {}/{} días naturales del mes",
                    fmt_cents(bruto_mensual),
                    dias_trabajados,
                    dias_mes
                )
            } else {
                format!("{} (mes completo)", fmt_cents(bruto_mensual))
            },
            salario_base_cents,
        ));
        if !conceptos.vacia() {
            if conceptos.pagas_extras_cents != 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    "Pagas extras",
                    "Importe introducido en los conceptos del mes",
                    conceptos.pagas_extras_cents,
                ));
            }
            if conceptos.horas_extra_cents != 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    "Horas extra",
                    "Importe introducido; se aplican los tipos generales del paquete",
                    conceptos.horas_extra_cents,
                ));
            }
            if conceptos.variables_cents != 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    "Variables y complementos",
                    "Importe introducido en los conceptos del mes",
                    conceptos.variables_cents,
                ));
            }
            if conceptos.vacaciones_dias != 0 || conceptos.vacaciones_cents != 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    format!("Vacaciones ({} días)", conceptos.vacaciones_dias),
                    "Días informativos; el importe es la compensación aplicada",
                    conceptos.vacaciones_cents,
                ));
            }
            if conceptos.bajas_dias != 0 || conceptos.bajas_cents != 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    format!("Baja ({} días)", conceptos.bajas_dias),
                    "Días informativos; el importe es el descuento aplicado al bruto",
                    -conceptos.bajas_cents,
                ));
            }
            if conceptos.deducciones_cents != 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    "Otras deducciones",
                    "Se restan del neto; no reducen el bruto ni la base",
                    -conceptos.deducciones_cents,
                ));
            }
            if !conceptos.nota.trim().is_empty() {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    "Nota de conceptos",
                    conceptos.nota.trim(),
                    0,
                ));
            }
            if bruto_sin_limite < 0 {
                expediente.lineas.push(linea_expediente(
                    SeccionExpediente::Conceptos,
                    "Ajuste a bruto no negativo",
                    "El descuento superaba el salario base del periodo",
                    bruto - bruto_sin_limite,
                ));
            }
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Conceptos,
                "Bruto total del periodo",
                "Salario base + conceptos retributivos - descuento por baja",
                bruto,
            ));
        }

        if base.tabla.is_some() {
            let limite = if empleado.grupo_cotizacion.es_diaria() {
                if dias_trabajados < dias_mes {
                    format!("diario × {} días", dias_trabajados)
                } else {
                    "diario × 30 días".to_string()
                }
            } else if dias_trabajados < dias_mes {
                "mensual × días / 30".to_string()
            } else {
                "mensual".to_string()
            };
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Base,
                "Base mínima aplicable",
                format!(
                    "Grupo {} · tabla {} · {} días",
                    empleado.grupo_cotizacion.numero(),
                    anyo,
                    if dias_trabajados < dias_mes { "parcial" } else { "completa" }
                ),
                base.minima_cents,
            ));
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Base,
                "Base máxima aplicable",
                format!(
                    "Grupo {} · tabla {} · límite {}",
                    empleado.grupo_cotizacion.numero(),
                    anyo,
                    limite
                ),
                base.maxima_cents,
            ));
        } else {
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Base,
                "Sin tabla de cotización",
                format!("No hay bases cargadas para {}; no se aplican topes", anyo),
                0,
            ));
        }
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Base,
            "Base de cotización aplicada",
            "Bruto limitado entre la base mínima y la máxima",
            base_cotizacion_cents,
        ));

        let mut suma_empleado = 0;
        for (nombre, tipo_pb) in [
            (
                "Contingencias comunes",
                paquete.tipos.contingencias_comunes_empleado_pb,
            ),
            (
                "Desempleo del trabajador",
                paquete.desempleo_empleado_pb(indefinido),
            ),
            ("Formación profesional", paquete.tipos.formacion_empleado_pb),
            ("MEI", paquete.tipos.mei_empleado_pb),
        ] {
            let linea = linea_cotizacion(
                SeccionExpediente::Deduccion,
                nombre,
                base_cotizacion_cents,
                tipo_pb,
            );
            suma_empleado += linea.importe_cents;
            expediente.lineas.push(linea);
        }
        if suma_empleado != ss_empleado {
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Deduccion,
                "Ajuste de redondeo de cotizaciones",
                "Diferencia entre la suma de componentes y el cálculo global",
                ss_empleado - suma_empleado,
            ));
        }
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Deduccion,
            "Retención IRPF",
            format!("{} × {} %", fmt_cents(bruto), fmt_pb(empleado.irpf_pb)),
            irpf,
        ));
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Informativo,
            "Neto a percibir",
            if conceptos.deducciones_cents != 0 {
                "Bruto - Seguridad Social del trabajador - IRPF - otras deducciones"
            } else {
                "Bruto - Seguridad Social del trabajador - IRPF"
            },
            neto,
        ));

        let mut suma_empresa = 0;
        for (nombre, tipo_pb) in [
            (
                "Contingencias comunes",
                paquete.tipos.contingencias_comunes_empresa_pb,
            ),
            (
                "Desempleo de la empresa",
                paquete.desempleo_empresa_pb(indefinido),
            ),
            ("FOGASA", paquete.tipos.fogasa_empresa_pb),
            ("Formación profesional", paquete.tipos.formacion_empresa_pb),
            ("MEI", paquete.tipos.mei_empresa_pb),
            ("AT y EP (estimación)", paquete.at_ep_empresa_pb),
        ] {
            let linea = linea_cotizacion(
                SeccionExpediente::Coste,
                nombre,
                base_cotizacion_cents,
                tipo_pb,
            );
            suma_empresa += linea.importe_cents;
            expediente.lineas.push(linea);
        }
        if suma_empresa != ss_empresa {
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Coste,
                "Ajuste de redondeo de cotizaciones",
                "Diferencia entre la suma de componentes y el cálculo global",
                ss_empresa - suma_empresa,
            ));
        }
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Coste,
            "Coste total para la empresa",
            "Bruto + Seguridad Social de la empresa",
            coste_empresa,
        ));
        expediente.huella = expediente.calcular_huella();

        Self {
            id,
            empleado_id: empleado.id,
            empleado_nombre: empleado.nombre_completo(),
            empleado_nif: empleado.nif.clone(),
            empleado_naf: empleado.naf.clone(),
            puesto: empleado.puesto.clone(),
            tipo_contrato: Some(empleado.tipo_contrato),
            grupo_cotizacion: empleado.grupo_cotizacion,
            irpf_pb: Some(empleado.irpf_pb),
            paquete: Some(paquete),
            salario_base_cents,
            conceptos: conceptos.clone(),
            tipo: TipoNomina::Ordinaria,
            nomina_origen: None,
            motivo_diferencia: String::new(),
            anyo,
            mes,
            dias_trabajados,
            dias_mes,
            bruto_cents: bruto,
            base_cotizacion_cents,
            ss_empleado_cents: ss_empleado,
            irpf_cents: irpf,
            neto_cents: neto,
            ss_empresa_cents: ss_empresa,
            coste_empresa_cents: coste_empresa,
            pagada: false,
            estado: EstadoNomina::Borrador,
            expediente: Some(expediente),
        }
    }

    /// Crea un recibo de diferencias enlazado a una nómina cerrada. Los
    /// importes son diferencias, no el total del recibo original: pueden ser
    /// positivos o negativos. El original no se toca.
    pub fn diferencia(
        origen: &Nomina,
        id: u64,
        anyo: i32,
        mes: u32,
        motivo: String,
        bruto_cents: i64,
        ss_empleado_cents: i64,
        irpf_cents: i64,
        ss_empresa_cents: i64,
    ) -> Self {
        let neto_cents = bruto_cents - ss_empleado_cents - irpf_cents;
        let coste_empresa_cents = bruto_cents + ss_empresa_cents;
        let irpf_pb = origen.irpf_pb.unwrap_or(0);
        let reglas = format!(
            "Recibo de diferencias de la nómina #{} · {}",
            origen.id, motivo
        );
        let mut expediente = ExpedienteNomina {
            version: VERSION_EXPEDIENTE_NOMINA,
            generado: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            anyo,
            mes,
            grupo_cotizacion: origen.grupo_cotizacion,
            tipo_contrato: origen.tipo_contrato.unwrap_or_default(),
            reglas,
            salario_anual_cents: 0,
            salario_mensual_cents: 0,
            dias_trabajados: 0,
            dias_mes: 0,
            bruto_cents,
            tabla: None,
            base_minima_cents: 0,
            base_maxima_cents: 0,
            // En un recibo manual no hay una base calculada: la Seguridad
            // Social se indica aparte y no se deriva del bruto.
            base_cotizacion_cents: 0,
            irpf_pb,
            ss_empleado_cents,
            irpf_cents,
            neto_cents,
            ss_empresa_cents,
            coste_empresa_cents,
            lineas: Vec::new(),
            huella: String::new(),
        };
        if let Some(paquete) = &origen.paquete {
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Informativo,
                "Paquete anual de referencia",
                format!("{} · {}", paquete.version, paquete.fuente),
                0,
            ));
        }
        for (etiqueta, importe) in [
            ("Diferencia de bruto", bruto_cents),
            ("Diferencia de Seguridad Social del trabajador", ss_empleado_cents),
            ("Diferencia de retención de IRPF", irpf_cents),
        ] {
            expediente.lineas.push(linea_expediente(
                SeccionExpediente::Diferencia,
                etiqueta,
                "Importe indicado en la corrección",
                importe,
            ));
        }
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Diferencia,
            "Diferencia de neto",
            "Bruto - SS trabajador - IRPF",
            neto_cents,
        ));
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Diferencia,
            "Diferencia de Seguridad Social de la empresa",
            "Importe indicado en la corrección",
            ss_empresa_cents,
        ));
        expediente.lineas.push(linea_expediente(
            SeccionExpediente::Diferencia,
            "Diferencia de coste de empresa",
            "Bruto + SS empresa",
            coste_empresa_cents,
        ));
        expediente.huella = expediente.calcular_huella();

        Self {
            id,
            empleado_id: origen.empleado_id,
            empleado_nombre: origen.empleado_nombre.clone(),
            empleado_nif: origen.empleado_nif.clone(),
            empleado_naf: origen.empleado_naf.clone(),
            puesto: origen.puesto.clone(),
            tipo_contrato: origen.tipo_contrato,
            grupo_cotizacion: origen.grupo_cotizacion,
            irpf_pb: Some(irpf_pb),
            paquete: origen.paquete.clone(),
            salario_base_cents: 0,
            conceptos: ConceptosNomina::default(),
            tipo: TipoNomina::Diferencia,
            nomina_origen: Some(origen.id),
            motivo_diferencia: motivo,
            anyo,
            mes,
            dias_trabajados: 0,
            dias_mes: 0,
            bruto_cents,
            base_cotizacion_cents: 0,
            ss_empleado_cents,
            irpf_cents,
            neto_cents,
            ss_empresa_cents,
            coste_empresa_cents,
            pagada: false,
            estado: EstadoNomina::Borrador,
            expediente: Some(expediente),
        }
    }

    /// Salario base que se muestra en el PDF. Las nóminas antiguas no tienen
    /// el campo y usan el bruto completo; las nuevas con conceptos usan la
    /// base guardada, aunque sea cero (por ejemplo, un mes sin salario base
    /// pero con una paga extra).
    pub fn salario_base_efectivo(&self) -> i64 {
        if self.salario_base_cents != 0 || !self.conceptos.vacia() {
            self.salario_base_cents
        } else {
            self.bruto_cents
        }
    }

    /// Recalcula una nómina ordinaria con nuevos conceptos. Sólo se permite
    /// mientras siga en Borrador: a partir de Revisada la nómina es inmutable
    /// y las correcciones van por un recibo de diferencias.
    pub fn actualizar_conceptos(
        &mut self,
        empleado: &Empleado,
        conceptos: ConceptosNomina,
    ) -> Result<(), String> {
        if self.empleado_id != empleado.id {
            return Err("El empleado no coincide con la nómina.".into());
        }
        if self.tipo != TipoNomina::Ordinaria {
            return Err("Los conceptos sólo se aplican a nóminas ordinarias.".into());
        }
        if self.estado != EstadoNomina::Borrador {
            return Err(
                "Sólo se pueden editar los conceptos de una nómina en Borrador. Para corregir una nómina revisada o cerrada, crea un recibo de diferencias."
                    .into(),
            );
        }
        let nueva = Self::calcular_con_conceptos(
            empleado,
            self.id,
            self.anyo,
            self.mes,
            &conceptos,
        );
        *self = nueva;
        Ok(())
    }

    pub fn trimestre(&self) -> u32 {
        ((self.mes.saturating_sub(1)) / 3) + 1
    }

    /// Sólo se puede borrar una nómina que todavía es un borrador. A partir de
    /// "Revisada" se conserva para que el ciclo quede auditable.
    pub fn puede_borrar(&self) -> bool {
        self.estado == EstadoNomina::Borrador
    }

    pub fn cerrada(&self) -> bool {
        self.estado.cerrada()
    }

    /// Avanza un único paso. Devuelve error si ya está cerrada, que es la
    /// garantía de que no se puede modificar después del cierre.
    pub fn avanzar_estado(&mut self) -> Result<(), String> {
        if self.cerrada() {
            return Err("La nómina está cerrada: no se puede modificar. Crea un recibo de diferencias.".into());
        }
        let siguiente = self
            .estado
            .siguiente()
            .ok_or_else(|| "La nómina no tiene un estado siguiente.".to_string())?;
        self.estado = siguiente;
        self.pagada = matches!(siguiente, EstadoNomina::Pagada | EstadoNomina::Cerrada);
        Ok(())
    }
}

/// Genera las nóminas del mes sólo para los empleados con alta en el mes y
/// salario. Un alta o baja parcial también genera nómina.
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
    if crate::nomina_paquetes::paquete_nomina(anyo).is_none() {
        return Err(format!(
            "No hay paquete anual de parámetros de nómina para {anyo}. Añade su fichero en parametros/nomina."
        ));
    }
    let empleados: Vec<Empleado> = empleados
        .iter()
        .filter(|e| {
            e.dias_trabajados_en_mes(anyo, mes) > 0 && e.salario_bruto_anual_cents > 0
        })
        .cloned()
        .collect();
    if empleados.is_empty() {
        return Err(
            "No hay empleados con alta y salario ese mes. Revisa las fichas y las fechas."
                .into(),
        );
    }
    let mut creadas = 0;
    let mut saltadas = 0;
    for e in empleados {
        if nominas
            .iter()
            .any(|n| {
                n.empleado_id == e.id
                    && n.anyo == anyo
                    && n.mes == mes
                    && n.tipo == TipoNomina::Ordinaria
            })
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

/// Crea un recibo de diferencias enlazado a una nómina cerrada. La operación
/// no toca el original y no consume un ID si la validación falla.
#[allow(clippy::too_many_arguments)]
pub fn crear_diferencia(
    nominas: &mut Vec<Nomina>,
    origen_id: u64,
    siguiente_id: &mut u64,
    anyo: i32,
    mes: u32,
    motivo: &str,
    bruto_cents: i64,
    ss_empleado_cents: i64,
    irpf_cents: i64,
    ss_empresa_cents: i64,
) -> Result<u64, String> {
    if !(1..=12).contains(&mes) {
        return Err("Mes no válido para el recibo de diferencias.".into());
    }
    let motivo = motivo.trim();
    if motivo.is_empty() {
        return Err("Escribe el motivo del recibo de diferencias.".into());
    }
    if bruto_cents == 0
        && ss_empleado_cents == 0
        && irpf_cents == 0
        && ss_empresa_cents == 0
    {
        return Err("El recibo de diferencias no puede tener todos los importes a cero.".into());
    }
    let origen = nominas
        .iter()
        .find(|n| n.id == origen_id)
        .cloned()
        .ok_or("La nómina original no existe.")?;
    if origen.tipo != TipoNomina::Ordinaria {
        return Err("Sólo se puede corregir una nómina ordinaria cerrada.".into());
    }
    if !origen.cerrada() {
        return Err(
            "La nómina original debe estar cerrada antes de crear el recibo de diferencias."
                .into(),
        );
    }
    let id = *siguiente_id;
    let recibo = Nomina::diferencia(
        &origen,
        id,
        anyo,
        mes,
        motivo.to_string(),
        bruto_cents,
        ss_empleado_cents,
        irpf_cents,
        ss_empresa_cents,
    );
    *siguiente_id += 1;
    nominas.push(recibo);
    Ok(id)
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
            fecha_alta: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tipo_contrato: TipoContrato::Indefinido,
            salario_bruto_anual_cents: 2_400_000,
            irpf_pb: 1500,
            ..Default::default()
        }
    }

    #[test]
    fn los_conceptos_afectan_bruto_base_neto_y_coste() {
        let conceptos = ConceptosNomina {
            pagas_extras_cents: 10_000,
            horas_extra_cents: 5_000,
            variables_cents: 2_500,
            vacaciones_dias: 3,
            vacaciones_cents: 1_000,
            bajas_dias: 2,
            bajas_cents: 4_000,
            deducciones_cents: 3_000,
            nota: "Paga extra y regularización de variables".into(),
        };
        let n = Nomina::calcular_con_conceptos(&empleado(), 9, 2026, 1, &conceptos);

        assert_eq!(n.salario_base_cents, 200_000);
        assert_eq!(n.conceptos, conceptos);
        // 200.000 + 10.000 + 5.000 + 2.500 + 1.000 - 4.000.
        assert_eq!(n.bruto_cents, 214_500);
        assert_eq!(n.base_cotizacion_cents, 214_500);
        assert_eq!(n.ss_empleado_cents, div_round(214_500 * 650, 10_000));
        assert_eq!(n.irpf_cents, div_round(214_500 * 1500, 10_000));
        assert_eq!(
            n.neto_cents,
            214_500 - n.ss_empleado_cents - n.irpf_cents - 3_000
        );
        assert_eq!(
            n.coste_empresa_cents,
            214_500 + div_round(214_500 * 3215, 10_000)
        );
        assert_eq!(n.salario_base_efectivo(), 200_000);

        let exp = n.expediente.as_ref().expect("expediente");
        assert!(exp.huella_valida());
        assert_eq!(
            exp.lineas
                .iter()
                .filter(|l| l.seccion == SeccionExpediente::Conceptos)
                .count(),
            8
        );
        for etiqueta in [
            "Pagas extras",
            "Horas extra",
            "Variables y complementos",
            "Vacaciones (3 días)",
            "Baja (2 días)",
            "Otras deducciones",
        ] {
            assert!(
                exp.lineas.iter().any(|l| l.etiqueta == etiqueta),
                "falta la línea {etiqueta}"
            );
        }
    }

    #[test]
    fn las_deducciones_reducen_el_neto_pero_no_el_bruto_ni_la_base() {
        let sin = Nomina::calcular(&empleado(), 9, 2026, 1);
        let con = Nomina::calcular_con_conceptos(
            &empleado(),
            9,
            2026,
            1,
            &ConceptosNomina {
                deducciones_cents: 12_500,
                ..Default::default()
            },
        );

        assert_eq!(con.bruto_cents, sin.bruto_cents);
        assert_eq!(con.base_cotizacion_cents, sin.base_cotizacion_cents);
        assert_eq!(con.ss_empleado_cents, sin.ss_empleado_cents);
        assert_eq!(con.ss_empresa_cents, sin.ss_empresa_cents);
        assert_eq!(con.irpf_cents, sin.irpf_cents);
        assert_eq!(con.coste_empresa_cents, sin.coste_empresa_cents);
        assert_eq!(con.neto_cents, sin.neto_cents - 12_500);
    }

    #[test]
    fn los_conceptos_solo_se_recalculan_en_borrador() {
        let e = empleado();
        let mut n = Nomina::calcular(&e, 9, 2026, 1);
        let conceptos = ConceptosNomina {
            variables_cents: 5_000,
            ..Default::default()
        };
        n.actualizar_conceptos(&e, conceptos.clone())
            .expect("un borrador se puede recalcular");
        assert_eq!(n.id, 9);
        assert_eq!(n.estado, EstadoNomina::Borrador);
        assert_eq!(n.conceptos, conceptos);
        assert_eq!(n.bruto_cents, 205_000);

        n.avanzar_estado().expect("pasa a revisada");
        assert!(n
            .actualizar_conceptos(&e, ConceptosNomina::default())
            .is_err());
        assert_eq!(n.bruto_cents, 205_000);
        assert_eq!(n.conceptos, conceptos);
    }

    #[test]
    fn la_nomina_calcula_neto_y_coste_de_empresa() {
        let n = Nomina::calcular(&empleado(), 9, 2026, 1);
        assert_eq!(n.bruto_cents, 200_000);
        // 4,70 + 1,55 + 0,10 + 0,15 = 6,50 %
        assert_eq!(n.ss_empleado_cents, 13_000);
        assert_eq!(n.irpf_cents, 30_000);
        assert_eq!(n.irpf_pb, Some(1500));
        assert_eq!(n.empleado_nif, "12345678Z");
        assert_eq!(n.neto_cents, 157_000);
        // 23,60 + 5,50 + 0,20 + 0,60 + 0,75 + 1,50 = 32,15 %
        assert_eq!(n.ss_empresa_cents, 64_300);
        assert_eq!(n.coste_empresa_cents, 264_300);
        assert_eq!(n.grupo_cotizacion, GrupoCotizacion::Grupo7);
        assert_eq!(n.base_cotizacion_cents, 200_000);
        assert_eq!(n.trimestre(), 1);
    }

    #[test]
    fn la_nomina_guarda_un_expediente_de_calculo_verificable() {
        let n = Nomina::calcular(&empleado(), 9, 2026, 1);
        let exp = n.expediente.as_ref().expect("expediente guardado");

        assert_eq!(exp.version, VERSION_EXPEDIENTE_NOMINA);
        assert_eq!(exp.bruto_cents, n.bruto_cents);
        assert_eq!(exp.base_cotizacion_cents, n.base_cotizacion_cents);
        assert_eq!(exp.neto_cents, n.neto_cents);
        assert!(!exp.generado.is_empty());
        assert!(exp.huella_valida());
        assert!(exp.lineas.iter().any(|l| l.etiqueta == "Retención IRPF"));
        assert!(exp
            .lineas
            .iter()
            .any(|l| l.seccion == SeccionExpediente::Coste));
        assert!(exp
            .lineas
            .iter()
            .any(|l| l.etiqueta == "Paquete anual de nómina"));
        let paquete = n.paquete.as_ref().expect("paquete guardado");
        assert_eq!(paquete.version, "2026.1");
        assert_eq!(paquete.anyo, 2026);
        assert_eq!(paquete.base_grupo(7).unwrap().minima_cents, 142_440);
        assert_eq!(n.estado, EstadoNomina::Borrador);
        assert!(n.puede_borrar());
    }

    #[test]
    fn el_expediente_sobrevive_al_json_y_detecta_manipulacion() {
        let n = Nomina::calcular(&empleado(), 9, 2026, 1);
        let texto = serde_json::to_string(&n).expect("serializa la nómina");
        let leida: Nomina = serde_json::from_str(&texto).expect("lee la nómina");
        assert!(leida
            .expediente
            .as_ref()
            .expect("expediente")
            .huella_valida());
        assert_eq!(
            leida.paquete.as_ref().expect("paquete").version,
            "2026.1"
        );

        let mut manipulado = n;
        manipulado
            .expediente
            .as_mut()
            .expect("expediente")
            .lineas[0]
            .importe_cents += 1;
        assert!(!manipulado
            .expediente
            .as_ref()
            .expect("expediente")
            .huella_valida());
    }

    #[test]
    fn los_estados_avanzan_en_orden_y_la_cerrada_no_se_puede_modificar() {
        let mut n = Nomina::calcular(&empleado(), 9, 2026, 1);
        let estados = [
            EstadoNomina::Revisada,
            EstadoNomina::Aprobada,
            EstadoNomina::Pagada,
            EstadoNomina::Cerrada,
        ];
        for estado in estados {
            n.avanzar_estado().expect("transición válida");
            assert_eq!(n.estado, estado);
        }
        assert!(n.cerrada());
        assert!(n.pagada);
        assert!(!n.puede_borrar());
        assert!(n.avanzar_estado().is_err());
    }

    #[test]
    fn el_recibo_de_diferencias_corrige_sin_tocar_el_original() {
        let mut original = Nomina::calcular(&empleado(), 9, 2026, 1);
        for _ in 0..4 {
            original.avanzar_estado().expect("se cierra");
        }
        let bruto_original = original.bruto_cents;
        let mut nominas = vec![original.clone()];
        let mut siguiente_id = 100;

        let id = crear_diferencia(
            &mut nominas,
            original.id,
            &mut siguiente_id,
            2026,
            2,
            "Atraso de convenio",
            10_000,
            648,
            1_500,
            3_207,
        )
        .expect("se crea el recibo de diferencias");

        assert_eq!(id, 100);
        assert_eq!(siguiente_id, 101);
        assert_eq!(nominas.len(), 2);
        let diferencia = &nominas[1];
        assert_eq!(diferencia.tipo, TipoNomina::Diferencia);
        assert_eq!(diferencia.nomina_origen, Some(original.id));
        assert_eq!(diferencia.estado, EstadoNomina::Borrador);
        assert_eq!(
            diferencia.paquete.as_ref().expect("paquete del recibo").version,
            "2026.1"
        );
        assert_eq!(diferencia.bruto_cents, 10_000);
        assert_eq!(diferencia.ss_empleado_cents, 648);
        assert_eq!(diferencia.irpf_cents, 1_500);
        assert_eq!(diferencia.neto_cents, 7_852);
        assert_eq!(diferencia.coste_empresa_cents, 13_207);
        assert!(diferencia
            .expediente
            .as_ref()
            .expect("expediente del recibo")
            .huella_valida());
        assert_eq!(nominas[0].bruto_cents, bruto_original);
        assert_eq!(nominas[0].estado, EstadoNomina::Cerrada);

        // Un recibo de diferencias no impide generar la nómina ordinaria del
        // mismo mes: no se trata de un recibo ordinario duplicado.
        let (creadas, saltadas) = generar_nominas_mes(
            &mut nominas,
            &[empleado()],
            &mut siguiente_id,
            2026,
            2,
        )
        .expect("la nómina ordinaria se genera aparte");
        assert_eq!((creadas, saltadas), (1, 0));
        assert_eq!(nominas.len(), 3);
        assert_eq!(nominas[2].tipo, TipoNomina::Ordinaria);
    }

    #[test]
    fn no_se_crea_un_recibo_de_diferencias_de_una_nomina_abierta() {
        let original = Nomina::calcular(&empleado(), 9, 2026, 1);
        let mut nominas = vec![original.clone()];
        let mut siguiente_id = 100;
        let r = crear_diferencia(
            &mut nominas,
            original.id,
            &mut siguiente_id,
            2026,
            2,
            "Prueba",
            1_000,
            0,
            0,
            0,
        );
        assert!(r.is_err());
        assert_eq!(siguiente_id, 100);
        assert_eq!(nominas.len(), 1);
    }

    #[test]
    fn las_bases_por_grupo_y_ejercicio_estan_cargadas() {
        let bases = |anyo, grupo| bases_cotizacion(anyo, grupo).expect("tabla cargada");

        assert_eq!(
            bases(2026, GrupoCotizacion::Grupo1),
            BasesCotizacion {
                minima_cents: 198_930,
                maxima_cents: 510_120,
            }
        );
        assert_eq!(bases(2026, GrupoCotizacion::Grupo2).minima_cents, 164_970);
        assert_eq!(bases(2026, GrupoCotizacion::Grupo3).minima_cents, 143_520);
        assert_eq!(bases(2026, GrupoCotizacion::Grupo7).minima_cents, 142_440);
        assert_eq!(bases(2026, GrupoCotizacion::Grupo8).minima_cents, 4_748);
        assert_eq!(bases(2026, GrupoCotizacion::Grupo8).maxima_cents, 17_004);

        assert_eq!(bases(2025, GrupoCotizacion::Grupo1).minima_cents, 192_900);
        assert_eq!(bases(2025, GrupoCotizacion::Grupo2).minima_cents, 159_960);
        assert_eq!(bases(2025, GrupoCotizacion::Grupo3).minima_cents, 139_170);
        assert_eq!(bases(2025, GrupoCotizacion::Grupo7).minima_cents, 138_120);
        assert_eq!(bases(2025, GrupoCotizacion::Grupo7).maxima_cents, 490_950);
        assert_eq!(bases(2025, GrupoCotizacion::Grupo8).minima_cents, 4_604);

        assert_eq!(bases(2024, GrupoCotizacion::Grupo1).minima_cents, 184_740);
        assert_eq!(bases(2024, GrupoCotizacion::Grupo3).minima_cents, 133_290);
        assert_eq!(bases(2024, GrupoCotizacion::Grupo7).minima_cents, 132_300);
        assert_eq!(bases(2024, GrupoCotizacion::Grupo7).maxima_cents, 472_050);
        assert_eq!(bases(2024, GrupoCotizacion::Grupo8).minima_cents, 4_410);
        assert_eq!(bases_cotizacion(2023, GrupoCotizacion::Grupo7), None);
    }

    #[test]
    fn la_base_no_baja_del_minimo_del_grupo() {
        let mut e = empleado();
        e.grupo_cotizacion = GrupoCotizacion::Grupo7;
        e.salario_bruto_anual_cents = 1_200_000; // 100.000 céntimos al mes
        let n = Nomina::calcular(&e, 19, 2026, 1);

        assert_eq!(n.bruto_cents, 100_000);
        assert_eq!(n.base_cotizacion_cents, 142_440);
        assert_eq!(
            n.ss_empleado_cents,
            div_round(142_440 * 650, 10_000)
        );
        assert_eq!(n.irpf_cents, 15_000);
        assert_eq!(
            n.neto_cents,
            100_000 - n.ss_empleado_cents - n.irpf_cents
        );
        assert_eq!(
            n.coste_empresa_cents,
            100_000 + div_round(142_440 * 3215, 10_000)
        );

        e.grupo_cotizacion = GrupoCotizacion::Grupo1;
        e.salario_bruto_anual_cents = 1_500_000; // 125.000 céntimos al mes
        let n = Nomina::calcular(&e, 20, 2026, 1);
        assert_eq!(n.base_cotizacion_cents, 198_930);
    }

    #[test]
    fn la_base_no_supera_el_maximo_del_ejercicio() {
        let mut e = empleado();
        e.grupo_cotizacion = GrupoCotizacion::Grupo1;
        e.salario_bruto_anual_cents = 12_000_000; // 1.000.000 céntimos al mes
        let n = Nomina::calcular(&e, 21, 2026, 1);

        assert_eq!(n.bruto_cents, 1_000_000);
        assert_eq!(n.base_cotizacion_cents, 510_120);
        assert_eq!(
            n.ss_empleado_cents,
            div_round(510_120 * 650, 10_000)
        );
        assert_eq!(n.irpf_cents, 150_000);
    }

    #[test]
    fn los_limites_mensuales_usan_divisor_treinta_dias() {
        let mut e = empleado();
        e.grupo_cotizacion = GrupoCotizacion::Grupo7;
        e.salario_bruto_anual_cents = 1_200_000; // 100.000 céntimos al mes
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let n = Nomina::calcular(&e, 22, 2026, 2);

        // El salario se prorratea por 14/28 días, pero la base mínima mensual
        // usa el divisor oficial de 30 días: 142.440 × 14 / 30 = 66.472.
        assert_eq!(n.bruto_cents, 50_000);
        assert_eq!(n.base_cotizacion_cents, 66_472);
        assert_eq!((n.dias_trabajados, n.dias_mes), (14, 28));
    }

    #[test]
    fn los_grupos_diarios_usan_la_base_diaria_en_meses_incompletos() {
        let mut e = empleado();
        e.grupo_cotizacion = GrupoCotizacion::Grupo8;
        e.salario_bruto_anual_cents = 1_200_000; // 100.000 céntimos al mes
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let n = Nomina::calcular(&e, 23, 2026, 2);
        assert_eq!(n.base_cotizacion_cents, 4_748 * 14);

        // En un mes incompleto también se aplica el máximo diario, no el tope
        // mensual completo.
        e.salario_bruto_anual_cents = 12_000_000;
        let n = Nomina::calcular(&e, 24, 2026, 2);
        assert_eq!(n.base_cotizacion_cents, 17_004 * 14);
    }

    #[test]
    fn los_grupos_diarios_respetan_el_tope_mensual_en_mes_completo() {
        let mut e = empleado();
        e.grupo_cotizacion = GrupoCotizacion::Grupo8;
        e.salario_bruto_anual_cents = 1_200_000; // 100.000 céntimos al mes

        // Enero tiene 31 días, pero con salario mensual la base mínima se
        // ajusta a 30 días, no a 31.
        let n = Nomina::calcular(&e, 25, 2026, 1);
        assert_eq!(n.base_cotizacion_cents, 4_748 * 30);

        // El máximo mensual es 5.101,20: no se multiplica 170,04 por 31.
        e.salario_bruto_anual_cents = 12_000_000; // 1.000.000 céntimos al mes
        for (anyo, tope) in [(2026, 510_120), (2025, 490_950), (2024, 472_050)] {
            e.fecha_alta = NaiveDate::from_ymd_opt(anyo, 1, 1).unwrap();
            let n = Nomina::calcular(&e, 26, anyo, 1);
            assert_eq!(n.base_cotizacion_cents, tope);
        }

        // También en febrero completo se aplica el tope mensual, no el diario
        // multiplicado por 28.
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let n = Nomina::calcular(&e, 27, 2026, 2);
        assert_eq!(n.base_cotizacion_cents, 510_120);
    }

    #[test]
    fn un_empleado_antiguo_sin_grupo_se_lee_como_grupo_siete() {
        let mut valor = serde_json::to_value(empleado()).expect("serializa el empleado");
        valor
            .as_object_mut()
            .expect("objeto JSON")
            .remove("grupo_cotizacion");
        let leido: Empleado = serde_json::from_value(valor).expect("lee el empleado antiguo");
        assert_eq!(leido.grupo_cotizacion, GrupoCotizacion::Grupo7);
    }

    #[test]
    fn una_nomina_antigua_sin_base_se_lee_sin_romperse() {
        let mut valor =
            serde_json::to_value(Nomina::calcular(&empleado(), 30, 2026, 1)).expect("serializa");
        let objeto = valor.as_object_mut().expect("objeto JSON");
        objeto.remove("base_cotizacion_cents");
        objeto.remove("grupo_cotizacion");
        objeto.remove("expediente");
        objeto.remove("estado");
        objeto.remove("tipo");
        objeto.remove("paquete");
        objeto.remove("salario_base_cents");
        objeto.remove("conceptos");
        let leida: Nomina = serde_json::from_value(valor).expect("lee la nómina antigua");
        assert_eq!(leida.base_cotizacion_cents, 0);
        assert_eq!(leida.grupo_cotizacion, GrupoCotizacion::Grupo7);
        assert_eq!(leida.estado, EstadoNomina::Borrador);
        assert_eq!(leida.tipo, TipoNomina::Ordinaria);
        assert!(leida.paquete.is_none());
        assert!(leida.expediente.is_none());
        assert_eq!(leida.salario_base_cents, 0);
        assert!(leida.conceptos.vacia());
    }

    #[test]
    fn prorratea_febrero_por_dias_naturales() {
        let mut e = empleado();
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let n = Nomina::calcular(&e, 11, 2026, 2);

        assert_eq!((n.dias_trabajados, n.dias_mes), (14, 28));
        assert_eq!(n.bruto_cents, 100_000);
        assert_eq!(n.ss_empleado_cents, 6_500);
        assert_eq!(n.irpf_cents, 15_000);
        assert_eq!(n.neto_cents, 78_500);
        assert_eq!(n.ss_empresa_cents, 32_150);
        assert_eq!(n.coste_empresa_cents, 132_150);
    }

    #[test]
    fn febrero_bisiesto_usa_veintinueve_dias() {
        let mut e = empleado();
        e.fecha_alta = NaiveDate::from_ymd_opt(2024, 2, 15).unwrap();
        let n = Nomina::calcular(&e, 12, 2024, 2);

        assert_eq!((n.dias_trabajados, n.dias_mes), (15, 29));
        assert_eq!(n.bruto_cents, div_round(200_000 * 15, 29));
    }

    #[test]
    fn alta_y_baja_en_los_extremos_incluyen_ambos_dias() {
        // Alta el primer día y sin baja: mes completo de febrero.
        let mut e = empleado();
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let n = Nomina::calcular(&e, 13, 2026, 2);
        assert_eq!((n.dias_trabajados, n.dias_mes), (28, 28));
        assert_eq!(n.bruto_cents, 200_000);

        // Baja el último día: también mes completo.
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        e.fecha_baja = Some(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        let n = Nomina::calcular(&e, 14, 2026, 2);
        assert_eq!((n.dias_trabajados, n.dias_mes), (28, 28));
        assert_eq!(n.bruto_cents, 200_000);

        // Alta el último día: un solo día.
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 2, 28).unwrap();
        e.fecha_baja = None;
        let n = Nomina::calcular(&e, 15, 2026, 2);
        assert_eq!((n.dias_trabajados, n.dias_mes), (1, 28));
        assert_eq!(n.bruto_cents, div_round(200_000, 28));

        // Baja el primer día: un solo día.
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        e.fecha_baja = Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
        let n = Nomina::calcular(&e, 16, 2026, 2);
        assert_eq!((n.dias_trabajados, n.dias_mes), (1, 28));
        assert_eq!(n.bruto_cents, div_round(200_000, 28));
    }

    #[test]
    fn el_cambio_de_ano_prorratea_diciembre_y_enero_por_separado() {
        let mut e = empleado();
        e.fecha_alta = NaiveDate::from_ymd_opt(2025, 12, 20).unwrap();
        e.fecha_baja = Some(NaiveDate::from_ymd_opt(2026, 1, 10).unwrap());

        let diciembre = Nomina::calcular(&e, 17, 2025, 12);
        assert_eq!((diciembre.dias_trabajados, diciembre.dias_mes), (12, 31));
        assert_eq!(
            diciembre.bruto_cents,
            div_round(200_000 * 12, 31)
        );

        let enero = Nomina::calcular(&e, 18, 2026, 1);
        assert_eq!((enero.dias_trabajados, enero.dias_mes), (10, 31));
        assert_eq!(enero.bruto_cents, div_round(200_000 * 10, 31));
    }

    #[test]
    fn generar_nominas_mes_aplica_el_prorrateo() {
        let mut e = empleado();
        e.fecha_alta = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let mut nominas = Vec::new();
        let mut siguiente_id = 100;

        let (creadas, saltadas) =
            generar_nominas_mes(&mut nominas, &[e], &mut siguiente_id, 2026, 2)
                .expect("debe generar la nómina parcial");

        assert_eq!((creadas, saltadas), (1, 0));
        assert_eq!((nominas[0].dias_trabajados, nominas[0].dias_mes), (14, 28));
        assert_eq!(nominas[0].bruto_cents, 100_000);
    }

    #[test]
    fn el_temporal_cotiza_un_poco_mas() {
        let mut e = empleado();
        e.tipo_contrato = TipoContrato::Temporal;
        let n = Nomina::calcular(&e, 10, 2026, 1);
        // 4,70 + 1,60 + 0,10 + 0,15 = 6,55 %
        assert_eq!(n.ss_empleado_cents, 13_100);
        assert!(n.neto_cents < 157_000);
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

    #[derive(serde::Deserialize)]
    struct CasosValidados {
        casos: Vec<CasoValidado>,
    }

    #[derive(serde::Deserialize)]
    struct CasoValidado {
        id: String,
        anyo: i32,
        paquete_version: String,
        grupo: GrupoCotizacion,
        tipo_contrato: TipoContrato,
        salario_bruto_anual_cents: i64,
        fecha_alta: String,
        mes: u32,
        base_esperada_cents: i64,
        ss_trabajador_esperada_cents: i64,
        ss_empresa_sin_at_ep_esperada_cents: i64,
        at_ep_estimado_pb: i32,
        ss_empresa_total_esperada_cents: i64,
        fuentes: Vec<String>,
    }

    #[test]
    fn los_paquetes_superan_los_casos_validados_externamente() {
        let casos: CasosValidados = serde_json::from_str(include_str!(
            "../parametros/nomina/casos-validados.json"
        ))
        .expect("casos validados legibles");
        assert!(casos.casos.len() >= 3);

        for caso in casos.casos {
            assert!(
                caso.fuentes.iter().all(|url| url.starts_with("https://")),
                "{} debe citar fuentes oficiales",
                caso.id
            );
            let e = Empleado {
                id: 1,
                nombre: "Caso".into(),
                apellidos: "Validado".into(),
                nif: "12345678Z".into(),
                fecha_alta: NaiveDate::parse_from_str(&caso.fecha_alta, "%Y-%m-%d")
                    .expect("fecha del caso"),
                grupo_cotizacion: caso.grupo,
                tipo_contrato: caso.tipo_contrato,
                salario_bruto_anual_cents: caso.salario_bruto_anual_cents,
                irpf_pb: 0,
                ..Default::default()
            };
            let n = Nomina::calcular(&e, 1, caso.anyo, caso.mes);
            let paquete = n.paquete.as_ref().expect("paquete del caso");
            assert_eq!(paquete.version, caso.paquete_version, "{}", caso.id);
            assert_eq!(n.base_cotizacion_cents, caso.base_esperada_cents, "{}", caso.id);
            assert_eq!(
                n.ss_empleado_cents, caso.ss_trabajador_esperada_cents,
                "{}", caso.id
            );
            let indefinido = caso.tipo_contrato.es_indefinido();
            assert_eq!(
                div_round(
                    n.base_cotizacion_cents * paquete.ss_empresa_sin_at_ep_pb(indefinido) as i64,
                    10_000
                ),
                caso.ss_empresa_sin_at_ep_esperada_cents,
                "{}",
                caso.id
            );
            assert_eq!(
                paquete.at_ep_empresa_pb, caso.at_ep_estimado_pb,
                "{}", caso.id
            );
            assert_eq!(
                n.ss_empresa_cents, caso.ss_empresa_total_esperada_cents,
                "{}", caso.id
            );
            assert!(n.expediente.as_ref().expect("expediente").huella_valida());
        }
    }

    #[test]
    fn no_se_generan_nominas_sin_paquete_anual() {
        let mut e = empleado();
        e.fecha_alta = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        let mut nominas = Vec::new();
        let mut siguiente_id = 100;
        let error = generar_nominas_mes(&mut nominas, &[e], &mut siguiente_id, 2027, 1)
            .expect_err("2027 no tiene paquete");
        assert!(error.contains("paquete anual"));
        assert!(nominas.is_empty());
        assert_eq!(siguiente_id, 100);
    }
}
