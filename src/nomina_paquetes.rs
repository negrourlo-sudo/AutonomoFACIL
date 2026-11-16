//! Paquetes anuales de parámetros de nómina.
//!
//! Los tipos y las bases no viven en el código: cada ejercicio tiene un paquete
//! JSON versionado en `parametros/nomina/`. Se embebe en el ejecutable para que
//! la instalación siga siendo un único fichero, pero el motor sólo consume los
//! datos del paquete. Cada nómina guarda una copia del paquete que utilizó.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const PAQUETES: [(&str, &str); 3] = [
    ("2024", include_str!("../parametros/nomina/2024.json")),
    ("2025", include_str!("../parametros/nomina/2025.json")),
    ("2026", include_str!("../parametros/nomina/2026.json")),
];

/// Bases mínima y máxima aplicables a un grupo.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BasesCotizacion {
    pub minima_cents: i64,
    pub maxima_cents: i64,
}

/// Fila de bases de un grupo dentro del paquete anual.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BaseGrupo {
    pub grupo: u32,
    /// Cierto para los grupos 8 a 11, que se publican en euros/día.
    pub diaria: bool,
    pub minima_cents: i64,
    pub maxima_cents: i64,
}

/// Tipos generales del Régimen General, en puntos base (470 = 4,70 %).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TiposCotizacion {
    pub contingencias_comunes_empleado_pb: i32,
    pub contingencias_comunes_empresa_pb: i32,
    pub desempleo_indefinido_empleado_pb: i32,
    pub desempleo_indefinido_empresa_pb: i32,
    pub desempleo_temporal_empleado_pb: i32,
    pub desempleo_temporal_empresa_pb: i32,
    pub formacion_empleado_pb: i32,
    pub formacion_empresa_pb: i32,
    pub fogasa_empresa_pb: i32,
    pub mei_empleado_pb: i32,
    pub mei_empresa_pb: i32,
}

/// Paquete anual versionado de parámetros de nómina.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PaqueteNomina {
    pub anyo: i32,
    pub version: String,
    pub vigente_desde: String,
    pub publicado: String,
    pub fuente: String,
    pub url_fuente: String,
    pub tope_maximo_mensual_cents: i64,
    pub bases_por_grupo: Vec<BaseGrupo>,
    pub tipos: TiposCotizacion,
    /// AT/EP es una estimación de oficina: la tarifa oficial depende de la
    /// actividad y no se puede fijar en un paquete general.
    pub at_ep_empresa_pb: i32,
    pub at_ep_estimado: bool,
    pub notas: Vec<String>,
}

impl PaqueteNomina {
    pub fn base_grupo(&self, grupo: u32) -> Option<BasesCotizacion> {
        self.bases_por_grupo
            .iter()
            .find(|b| b.grupo == grupo)
            .map(|b| BasesCotizacion {
                minima_cents: b.minima_cents,
                maxima_cents: b.maxima_cents,
            })
    }

    pub fn desempleo_empleado_pb(&self, indefinido: bool) -> i32 {
        if indefinido {
            self.tipos.desempleo_indefinido_empleado_pb
        } else {
            self.tipos.desempleo_temporal_empleado_pb
        }
    }

    pub fn desempleo_empresa_pb(&self, indefinido: bool) -> i32 {
        if indefinido {
            self.tipos.desempleo_indefinido_empresa_pb
        } else {
            self.tipos.desempleo_temporal_empresa_pb
        }
    }

    /// Cotización del trabajador: contingencias comunes, desempleo, formación
    /// y MEI.
    pub fn ss_empleado_pb(&self, indefinido: bool) -> i32 {
        self.tipos.contingencias_comunes_empleado_pb
            + self.desempleo_empleado_pb(indefinido)
            + self.tipos.formacion_empleado_pb
            + self.tipos.mei_empleado_pb
    }

    /// Cotización de la empresa sin la prima de AT/EP, que es una estimación.
    pub fn ss_empresa_sin_at_ep_pb(&self, indefinido: bool) -> i32 {
        self.tipos.contingencias_comunes_empresa_pb
            + self.desempleo_empresa_pb(indefinido)
            + self.tipos.fogasa_empresa_pb
            + self.tipos.formacion_empresa_pb
            + self.tipos.mei_empresa_pb
    }

    /// Cotización total de la empresa, con la estimación de AT/EP incluida.
    pub fn ss_empresa_pb(&self, indefinido: bool) -> i32 {
        self.ss_empresa_sin_at_ep_pb(indefinido) + self.at_ep_empresa_pb
    }

    /// Huella SHA-256 del paquete, útil para comprobar la copia guardada en
    /// una nómina frente al paquete de referencia.
    pub fn huella(&self) -> String {
        let texto = serde_json::to_string(self).expect("el paquete es serializable");
        Sha256::digest(texto.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

}

static CACHE: OnceLock<BTreeMap<i32, PaqueteNomina>> = OnceLock::new();

fn paquetes() -> &'static BTreeMap<i32, PaqueteNomina> {
    CACHE.get_or_init(|| {
        let mut mapa = BTreeMap::new();
        for (nombre, texto) in PAQUETES {
            let paquete: PaqueteNomina = serde_json::from_str(texto)
                .unwrap_or_else(|e| panic!("paquete de nómina {nombre} inválido: {e}"));
            assert_eq!(
                paquete.anyo.to_string(),
                nombre,
                "el año del paquete no coincide con su fichero"
            );
            validar_paquete(&paquete)
                .unwrap_or_else(|e| panic!("paquete de nómina {nombre} incompleto: {e}"));
            mapa.insert(paquete.anyo, paquete);
        }
        mapa
    })
}

fn validar_paquete(paquete: &PaqueteNomina) -> Result<(), String> {
    if paquete.version.trim().is_empty() {
        return Err("falta la versión".into());
    }
    if paquete.fuente.trim().is_empty() || !paquete.url_fuente.starts_with("https://") {
        return Err("faltan la fuente o su URL".into());
    }
    if paquete.tope_maximo_mensual_cents <= 0 {
        return Err("el tope mensual debe ser positivo".into());
    }
    if paquete.bases_por_grupo.len() != 11 {
        return Err("deben existir exactamente 11 grupos".into());
    }
    let mut vistos = [false; 11];
    for base in &paquete.bases_por_grupo {
        if !(1..=11).contains(&base.grupo) {
            return Err(format!("grupo fuera de rango: {}", base.grupo));
        }
        let indice = base.grupo as usize - 1;
        if vistos[indice] {
            return Err(format!("grupo repetido: {}", base.grupo));
        }
        vistos[indice] = true;
        if base.minima_cents <= 0 || base.maxima_cents < base.minima_cents {
            return Err(format!("bases no válidas en el grupo {}", base.grupo));
        }
        if base.diaria != (base.grupo >= 8) {
            return Err(format!(
                "el grupo {} tiene una modalidad de base incorrecta",
                base.grupo
            ));
        }
    }
    if vistos.iter().any(|v| !v) {
        return Err("faltan grupos en el paquete".into());
    }
    if paquete.at_ep_empresa_pb < 0 {
        return Err("el tipo de AT/EP no puede ser negativo".into());
    }
    Ok(())
}

/// Paquete del ejercicio, si está publicado en esta versión de la app.
pub fn paquete_nomina(anyo: i32) -> Option<&'static PaqueteNomina> {
    paquetes().get(&anyo)
}

/// Años con paquete cargado, de menor a mayor.
pub fn anyos_disponibles() -> Vec<i32> {
    paquetes().keys().copied().collect()
}

/// Último paquete publicado; sirve para calcular ejercicios sin paquete
/// específico, marcando el resultado como estimado.
pub fn ultimo_paquete() -> Option<&'static PaqueteNomina> {
    paquetes().values().next_back()
}

/// Paquete que debe usar el motor. Para un año sin paquete devuelve una copia
/// del último publicado, sin bases y con la versión marcada como estimada:
/// así no se aplican topes de otro ejercicio ni se oculta que faltan datos.
pub fn paquete_para_calculo(anyo: i32) -> PaqueteNomina {
    if let Some(paquete) = paquete_nomina(anyo) {
        return paquete.clone();
    }
    let mut paquete = ultimo_paquete()
        .expect("debe existir al menos un paquete de nómina")
        .clone();
    let version_origen = paquete.version.clone();
    paquete.anyo = anyo;
    paquete.version = format!("{version_origen}+estimado");
    paquete.vigente_desde.clear();
    paquete.publicado.clear();
    paquete.fuente = format!(
        "Sin paquete anual para {anyo}; se usan los últimos tipos publicados ({version_origen})"
    );
    paquete.url_fuente.clear();
    paquete.bases_por_grupo.clear();
    paquete.tope_maximo_mensual_cents = 0;
    paquete
        .notas
        .push("Cálculo estimado: no hay bases de cotización publicadas para este ejercicio.".into());
    paquete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_paquetes_anuales_estan_completos_y_versionados() {
        let anyos = anyos_disponibles();
        assert_eq!(anyos, vec![2024, 2025, 2026]);
        for anyo in anyos {
            let paquete = paquete_nomina(anyo).expect("paquete del año");
            assert_eq!(paquete.anyo, anyo);
            assert!(!paquete.version.is_empty());
            assert!(paquete.url_fuente.starts_with("https://"));
            assert_eq!(paquete.bases_por_grupo.len(), 11);
            assert!(paquete.base_grupo(7).unwrap().minima_cents > 0);
            assert!(paquete.ss_empleado_pb(true) > 0);
            assert!(paquete.ss_empresa_pb(false) > paquete.ss_empresa_sin_at_ep_pb(false));
            assert_eq!(paquete.huella().len(), 64);
        }
    }

    #[test]
    fn un_ejercicio_sin_paquete_no_hereda_las_bases_del_ultimo() {
        let paquete = paquete_para_calculo(2027);
        assert_eq!(paquete.anyo, 2027);
        assert!(paquete.version.ends_with("+estimado"));
        assert!(paquete.bases_por_grupo.is_empty());
        assert_eq!(paquete.tope_maximo_mensual_cents, 0);
        assert!(paquete.fuente.contains("Sin paquete anual"));
    }
}
