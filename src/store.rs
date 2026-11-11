//! Persistencia.
//!
//! Un único fichero JSON con escritura atómica (temporal + rename) y copia de
//! seguridad del anterior. Es deliberado: cero dependencias nativas, cero
//! toolchain de C, y el fichero se puede abrir, copiar e inspeccionar a mano.
//! Para miles de facturas es de sobra; si algún día hace falta SQL, el modelo
//! ya está separado y se migra sin tocar la interfaz.

use crate::model::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Db {
    pub empresa: Empresa,
    pub clientes: Vec<Cliente>,
    pub articulos: Vec<Articulo>,
    pub documentos: Vec<Documento>,
    /// Gastos del negocio. `serde(default)` para que los ficheros de datos
    /// anteriores a esta función sigan abriéndose sin tocar nada.
    #[serde(default)]
    pub gastos: Vec<Gasto>,
    /// Ejercicios cerrados: no se emiten documentos con fecha de ese año.
    #[serde(default)]
    pub cerrados: Vec<i32>,
    /// Último cliente usado: se propone al crear el siguiente documento.
    #[serde(default)]
    pub ultimo_cliente_id: u64,
    /// Modelos marcados como presentados, con clave `año-modelo` o
    /// `año-modelo-1T` para los trimestrales.
    #[serde(default)]
    pub presentados: Vec<String>,
    /// Zoom de la interfaz (1.0 = 100 %). Se guarda para no reajustarlo cada
    /// vez; con 0 (datos antiguos) se usa 1.0.
    #[serde(default = "zoom_uno")]
    pub zoom: f32,
    /// Valores escritos a mano en las casillas: clave `303-2026-1T` o
    /// `390-2026-anual` → número de casilla → lo que se tecleó. Se guarda el
    /// texto para no reformatear mientras se escribe.
    #[serde(default)]
    pub ajustes_modelos: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, String>,
    >,
    /// Justificante de cada modelo presentado (nº de registro, CSV o lo que
    /// dé la Sede), con la misma clave que `presentados`.
    #[serde(default)]
    pub justificantes: std::collections::BTreeMap<String, String>,
    /// El asistente de puesta en marcha ya se ha completado. Mientras sea
    /// falso, el programa abre directamente el asistente.
    #[serde(default)]
    pub asistente_completado: bool,
    /// El tutorial se lee en voz alta con una voz femenina del sistema.
    #[serde(default = "voz_si")]
    pub voz: bool,
    pub next_id: u64,
    #[serde(skip)]
    pub ruta: PathBuf,
}

impl Default for Db {
    fn default() -> Self {
        Self {
            empresa: Empresa::default(),
            clientes: Vec::new(),
            articulos: Vec::new(),
            documentos: Vec::new(),
            gastos: Vec::new(),
            cerrados: Vec::new(),
            ultimo_cliente_id: 0,
            presentados: Vec::new(),
            zoom: 1.0,
            ajustes_modelos: Default::default(),
            justificantes: Default::default(),
            asistente_completado: false,
            voz: true,
            next_id: 1,
            ruta: ruta_datos(),
        }
    }
}

fn zoom_uno() -> f32 {
    1.0
}

fn voz_si() -> bool {
    true
}

/// Carpeta de datos de la aplicación.
pub fn dir_base() -> PathBuf {
    match std::env::var("APPDATA") {
        Ok(base) if !base.is_empty() => PathBuf::from(base).join("AutonomoFACIL"),
        _ => PathBuf::from("."),
    }
}

/// Carpeta de la versión anterior, sólo para traer los datos una vez.
fn dir_anterior() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .filter(|b| !b.is_empty())
        .map(|b| PathBuf::from(b).join("Facturacion"))
}

/// La primera vez copia `datos.json`, las copias de seguridad y las
/// exportaciones de «Facturación». No borra nada: si algo falla, los datos
/// siguen en la carpeta antigua.
fn migrar_si_hace_falta(destino: &Path) {
    let Some(viejo) = dir_anterior() else {
        return;
    };
    if destino.join("datos.json").exists() || !viejo.join("datos.json").exists() {
        return;
    }
    let _ = std::fs::create_dir_all(destino);
    let _ = std::fs::copy(viejo.join("datos.json"), destino.join("datos.json"));
    for sub in ["copias", "export"] {
        let origen = viejo.join(sub);
        if !origen.is_dir() {
            continue;
        }
        let meta = destino.join(sub);
        let _ = std::fs::create_dir_all(&meta);
        if let Ok(entradas) = std::fs::read_dir(&origen) {
            for e in entradas.flatten() {
                let p = e.path();
                if p.is_file() {
                    if let Some(n) = p.file_name() {
                        let _ = std::fs::copy(&p, meta.join(n));
                    }
                }
            }
        }
    }
}

/// `%APPDATA%\AutonomoFACIL\datos.json` en Windows; `./datos.json` en el resto.
pub fn ruta_datos() -> PathBuf {
    let dir = dir_base();
    migrar_si_hace_falta(&dir);
    dir.join("datos.json")
}

impl Db {
    pub fn cargar() -> (Self, Option<String>) {
        let ruta = ruta_datos();
        if !ruta.exists() {
            let mut db = Db::default();
            db.ruta = ruta;
            return (db, None);
        }
        match std::fs::read_to_string(&ruta) {
            Ok(txt) => match serde_json::from_str::<Db>(&txt) {
                Ok(mut db) => {
                    db.ruta = ruta;
                    (db, None)
                }
                Err(e) => {
                    let mut db = Db::default();
                    db.ruta = ruta.clone();
                    (
                        db,
                        Some(format!(
                            "No se pudo leer {}: {}. Se ha empezado con una base vacía; \
                             el fichero anterior NO se ha borrado.",
                            ruta.display(),
                            e
                        )),
                    )
                }
            },
            Err(e) => {
                let mut db = Db::default();
                db.ruta = ruta.clone();
                (db, Some(format!("No se pudo abrir {}: {}", ruta.display(), e)))
            }
        }
    }

    pub fn guardar(&self) -> Result<(), String> {
        if let Some(dir) = self.ruta.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("crear carpeta: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serializar: {e}"))?;
        let tmp = self.ruta.with_extension("json.tmp");
        let bak = self.ruta.with_extension("json.bak");

        if self.ruta.exists() {
            // Si la copia falla no abortamos: es una red de seguridad, no un
            // requisito para guardar.
            let _ = std::fs::copy(&self.ruta, &bak);
        }
        std::fs::write(&tmp, json).map_err(|e| format!("escribir temporal: {e}"))?;
        std::fs::rename(&tmp, &self.ruta).map_err(|e| format!("reemplazar: {e}"))?;
        Ok(())
    }

    pub fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn cliente(&self, id: u64) -> Option<&Cliente> {
        self.clientes.iter().find(|c| c.id == id)
    }

    pub fn cliente_mut(&mut self, id: u64) -> Option<&mut Cliente> {
        self.clientes.iter_mut().find(|c| c.id == id)
    }

    pub fn documento(&self, id: u64) -> Option<&Documento> {
        self.documentos.iter().find(|d| d.id == id)
    }

    pub fn documento_mut(&mut self, id: u64) -> Option<&mut Documento> {
        self.documentos.iter_mut().find(|d| d.id == id)
    }

    /// Siguiente número libre de una serie. Nunca reutiliza: toma el máximo
    /// histórico, no la cantidad de documentos, para que borrar un borrador no
    /// deje huecos en la numeración.
    pub fn siguiente_numero(&self, tipo: TipoDoc, serie: &str) -> u64 {
        self.documentos
            .iter()
            .filter(|d| d.tipo == tipo && d.serie == serie && d.numero > 0)
            .map(|d| d.numero)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn facturas(&self) -> Vec<&Documento> {
        self.documentos
            .iter()
            .filter(|d| d.tipo == TipoDoc::Factura)
            .collect()
    }

    pub fn presupuestos(&self) -> Vec<&Documento> {
        self.documentos
            .iter()
            .filter(|d| d.tipo == TipoDoc::Presupuesto)
            .collect()
    }

    pub fn proformas(&self) -> Vec<&Documento> {
        self.documentos
            .iter()
            .filter(|d| d.tipo == TipoDoc::Proforma)
            .collect()
    }

    /// Registros Verifactu en orden de generación (la cadena es global, no por
    /// serie: la AEAT encadena por obligado tributario).
    pub fn registros(&self) -> Vec<&RegistroVerifactu> {
        let mut v: Vec<&Documento> = self
            .documentos
            .iter()
            .filter(|d| d.verifactu.is_some())
            .collect();
        v.sort_by(|a, b| {
            let fa = a.verifactu.as_ref().map(|r| r.fecha_hora_gen.clone()).unwrap_or_default();
            let fb = b.verifactu.as_ref().map(|r| r.fecha_hora_gen.clone()).unwrap_or_default();
            fa.cmp(&fb)
        });
        v.into_iter().filter_map(|d| d.verifactu.as_ref()).collect()
    }

    pub fn esta_cerrado(&self, anyo: i32) -> bool {
        self.cerrados.contains(&anyo)
    }

    /// Carpeta de copias de seguridad, junto al fichero de datos.
    pub fn dir_copias(&self) -> PathBuf {
        match self.ruta.parent() {
            Some(dir) => dir.join("copias"),
            None => PathBuf::from("copias"),
        }
    }

    /// Lo que realmente queda por cobrar: descuenta los cobros parciales.
    pub fn pendientes_cobro(&self) -> i64 {
        self.documentos.iter().map(|d| d.pendiente_cents()).sum()
    }
}
