//! Persistencia.
//!
//! Un único fichero JSON con escritura atómica (temporal + rename) y copia de
//! seguridad del anterior. Es deliberado: cero dependencias nativas, cero
//! toolchain de C, y el fichero se puede abrir, copiar e inspeccionar a mano.
//! Para miles de facturas es de sobra; si algún día hace falta SQL, el modelo
//! ya está separado y se migra sin tocar la interfaz.

use crate::empleados::{Empleado, Nomina};
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
    /// Plantilla y nóminas. Los datos antiguos se abren con vectores vacíos.
    #[serde(default)]
    pub empleados: Vec<Empleado>,
    #[serde(default)]
    pub nominas: Vec<Nomina>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contab, fiscal, modelos, verifactu};
    use chrono::{Datelike, Duration, NaiveDate};
    use std::collections::BTreeMap;
    use std::time::Instant;

    const CLIENTES: usize = 500;
    const FACTURAS: usize = 4_000;
    const PRESUPUESTOS: usize = 400;
    const PROFORMAS: usize = 300;
    const GASTOS: usize = 2_500;
    const ARTICULOS: usize = 300;
    const EMISOR: &str = "B12345674";

    fn con_hora(mut r: RegistroVerifactu, hora: &str) -> RegistroVerifactu {
        r.fecha_hora_gen = hora.to_string();
        r.huella = verifactu::calcular_huella(&r);
        r.qr_url = verifactu::construir_qr_url(&r);
        r
    }

    #[test]
    fn carga_un_datos_json_antiguo_real_sin_empleados_ni_nominas() {
        // Fichero de una versión anterior: conserva la empresa y el contador de
        // IDs, pero no tiene los campos de plantilla ni de nóminas.
        const ANTIGUO: &str = r#"{
  "empresa": {
    "nombre": "Taller Antiguo SL",
    "nif": "B12345674",
    "persona_fisica": false,
    "apellidos": "",
    "direccion": "Calle Mayor 1",
    "cp": "28001",
    "ciudad": "Madrid",
    "provincia": "Madrid",
    "email": "taller@example.com",
    "telefono": "910000000",
    "iban": "ES9121000418450200051332",
    "serie": "2025",
    "iva_defecto": 2100,
    "irpf_defecto": 0,
    "condiciones": "Transferencia a 30 días.",
    "verifactu_activo": false
  },
  "clientes": [],
  "articulos": [],
  "documentos": [],
  "next_id": 42
}"#;

        let sello = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "autonomofacil-datos-antiguo-{}-{sello}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("datos.json");
        std::fs::write(&ruta, ANTIGUO).unwrap();

        let (db, aviso) = Db::cargar_desde(ruta.clone());
        let _ = std::fs::remove_dir_all(&dir);

        assert!(aviso.is_none(), "aviso de carga: {aviso:?}");
        assert_eq!(db.ruta, ruta);
        assert_eq!(db.empresa.nombre, "Taller Antiguo SL");
        assert_eq!(db.next_id, 42);
        assert!(db.empleados.is_empty());
        assert!(db.nominas.is_empty());
    }

    /// Genera una base de trabajo realista y la deja en `.toolchain/carga`
    /// para abrirla con `APPDATA` apuntando ahí. No se ejecuta en la suite
    /// normal: es una prueba de carga, no un test de cada cambio.
    #[test]
    #[ignore = "genera datos de carga; ejecutar con --ignored --nocapture"]
    fn genera_carga_masiva() {
        let mut db = Db::default();
        let mut e = Empresa::default();
        e.nombre = "CARGA MASIVA SL".into();
        e.nif = EMISOR.into();
        e.persona_fisica = false;
        e.direccion = "Calle de la Prueba 1".into();
        e.cp = "28001".into();
        e.ciudad = "Madrid".into();
        e.provincia = "Madrid".into();
        e.email = "carga@example.com".into();
        e.iban = "ES9121000418450200051332".into();
        e.serie = "2026".into();
        e.verifactu_activo = true;
        db.empresa = e;

        for i in 0..CLIENTES {
            let id = db.id();
            let regimen = match i % 20 {
                0 => RegimenCliente::Profesional,
                1 => RegimenCliente::Minorista,
                2 => RegimenCliente::Intracomunitario,
                _ => RegimenCliente::General,
            };
            let sin_nif = i % 13 == 0 && regimen == RegimenCliente::General;
            db.clientes.push(Cliente {
                id,
                nombre: format!("Cliente de carga {i:03}"),
                nif: if sin_nif {
                    String::new()
                } else {
                    format!("B{:08}", 10_000_000 + i)
                },
                persona_fisica: i % 3 == 0,
                apellidos: if i % 3 == 0 { "Prueba" } else { "" }.to_string(),
                direccion: format!("Calle {i} 1"),
                cp: "28001".to_string(),
                ciudad: "Madrid".to_string(),
                provincia: "Madrid".to_string(),
                email: format!("cliente{i}@example.com"),
                telefono: format!("600{:06}", i),
                notas: "Generado para la prueba de carga".to_string(),
                regimen,
            });
        }

        for i in 0..ARTICULOS {
            let id = db.id();
            db.articulos.push(Articulo {
                id,
                descripcion: format!("Artículo o servicio {i:03}"),
                precio_cents: 5_000 + (i as i64 * 137) % 300_000,
                iva_pb: [2100, 1000, 400][i % 3],
                unidad: if i % 2 == 0 { "ud" } else { "h" }.to_string(),
            });
        }

        let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let mut numeros: BTreeMap<i32, u64> = BTreeMap::new();
        let mut prev = String::new();
        let mut anuladas = 0usize;

        for i in 0..FACTURAS {
            let fecha = base + Duration::days((i / 4) as i64);
            let anyo = fecha.year();
            let n = numeros.entry(anyo).or_insert(0);
            *n += 1;
            let cliente = db.clientes[i % CLIENTES].clone();
            let irpf = if cliente.regimen == RegimenCliente::Profesional {
                1500
            } else {
                0
            };
            let mut d = Documento::nuevo(TipoDoc::Factura, cliente.clone(), irpf, anyo.to_string());
            d.id = db.id();
            d.numero = *n;
            d.fecha = fecha;
            d.vencimiento = fecha + Duration::days(30);
            d.operacion = if i % 29 == 0 {
                TipoOperacion::Exportacion
            } else if i % 31 == 0 {
                TipoOperacion::Exenta
            } else {
                cliente.regimen.operacion()
            };
            d.recargo_pb = if d.operacion.lleva_iva() {
                cliente.regimen.recargo_pb()
            } else {
                0
            };
            for l in 0..(1 + i % 3) {
                let iva = if d.operacion.lleva_iva() {
                    [2100, 1000, 400][l % 3]
                } else {
                    0
                };
                d.lineas.push(Linea {
                    descripcion: format!("Servicio {i} · línea {l}"),
                    cantidad_milli: 1000 + l as i64 * 500,
                    precio_cents: 15_000 + ((i * 137 + l * 1_000) % 200_000) as i64,
                    descuento_pb: if l == 1 { 500 } else { 0 },
                    iva_pb: iva,
                });
            }
            d.notas = if i % 7 == 0 {
                "Pago por transferencia; gracias por confiar.".to_string()
            } else {
                String::new()
            };

            let hora = format!(
                "{}T{:02}:00:00+01:00",
                fecha.format("%Y-%m-%d"),
                8 + (i % 4) * 3
            );
            let alta = con_hora(
                verifactu::registro_alta(&d, EMISOR, &prev),
                &hora,
            );
            prev = alta.huella.clone();
            d.verifactu = Some(alta);

            let anulada = i % 37 == 0;
            if anulada {
                anuladas += 1;
                d.estado = Estado::Anulado;
                d.motivo_anulacion = "Prueba de carga".to_string();
                let hora_n = format!(
                    "{}T{:02}:00:00+01:00",
                    fecha.format("%Y-%m-%d"),
                    9 + (i % 4) * 3
                );
                let n = con_hora(
                    verifactu::registro_anulacion(&d, EMISOR, &prev),
                    &hora_n,
                );
                prev = n.huella.clone();
                d.verifactu_anulacion = Some(n);
            } else if i % 5 == 0 {
                d.estado = Estado::Pagado;
                d.pagos.push(Pago {
                    fecha: fecha + Duration::days(15),
                    importe_cents: d.totales().total,
                });
            } else {
                d.estado = Estado::Emitido;
            }
            db.documentos.push(d);
        }

        for i in 0..PRESUPUESTOS {
            let fecha = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Duration::days((i % 300) as i64);
            let cliente = db.clientes[i % CLIENTES].clone();
            let mut d = Documento::nuevo(TipoDoc::Presupuesto, cliente, 0, "P2026".to_string());
            d.id = db.id();
            d.numero = i as u64 + 1;
            d.fecha = fecha;
            d.vencimiento = fecha + Duration::days(30);
            d.estado = Estado::Emitido;
            d.lineas.push(Linea {
                descripcion: format!("Presupuesto de carga {i}"),
                cantidad_milli: 1000,
                precio_cents: 20_000 + (i as i64 * 97) % 150_000,
                descuento_pb: 0,
                iva_pb: 2100,
            });
            db.documentos.push(d);
        }

        for i in 0..PROFORMAS {
            let fecha = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + Duration::days((i % 300) as i64);
            let cliente = db.clientes[(i * 3) % CLIENTES].clone();
            let mut d = Documento::nuevo(TipoDoc::Proforma, cliente, 0, "PRO2026".to_string());
            d.id = db.id();
            d.numero = i as u64 + 1;
            d.fecha = fecha;
            d.vencimiento = fecha + Duration::days(30);
            d.estado = Estado::Emitido;
            d.lineas.push(Linea {
                descripcion: format!("Proforma de carga {i}"),
                cantidad_milli: 1000,
                precio_cents: 30_000 + (i as i64 * 53) % 100_000,
                descuento_pb: 0,
                iva_pb: 2100,
            });
            db.documentos.push(d);
        }

        for i in 0..GASTOS {
            let id = db.id();
            db.gastos.push(Gasto {
                id,
                fecha: base + Duration::days((i * 2 / 5) as i64),
                proveedor: format!("Proveedor de carga {i:04}"),
                nif: format!("B{:08}", 20_000_000 + i),
                concepto: format!("Gasto de explotación {i}"),
                base_cents: 5_000 + (i as i64 * 211) % 400_000,
                iva_pb: [2100, 1000, 400][i % 3],
                irpf_pb: 0,
                pagado: i % 3 == 0,
                categoria: ["Compras", "Software", "Alquiler", "Suministros"][i % 4]
                    .to_string(),
                pago_personal: false,
            });
        }

        for anyo in 2024..=2026 {
            for t in 1..=4 {
                if anyo < 2026 || t <= 3 {
                    db.presentados.push(format!("{anyo}-303-{t}T"));
                    db.presentados.push(format!("{anyo}-130-{t}T"));
                }
            }
        }
        db.cerrados.push(2024);
        db.ajustes_modelos.insert(
            "303-2026-1T".to_string(),
            BTreeMap::from([("69".to_string(), "1234.56".to_string())]),
        );
        db.asistente_completado = true;
        db.voz = false;

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".toolchain")
            .join("carga")
            .join("AutonomoFACIL");
        std::fs::create_dir_all(&dir).unwrap();
        db.ruta = dir.join("datos.json");

        let t = Instant::now();
        db.guardar().unwrap();
        let guardar = t.elapsed();
        let bytes = std::fs::metadata(&db.ruta).unwrap().len();

        let txt = std::fs::read_to_string(&db.ruta).unwrap();
        let t = Instant::now();
        let mut leida: Db = serde_json::from_str(&txt).unwrap();
        let cargar = t.elapsed();
        leida.ruta = db.ruta.clone();

        let t = Instant::now();
        let problemas = verifactu::verificar_cadena(&leida.documentos);
        let cadena = t.elapsed();
        assert!(problemas.is_empty(), "cadena rota: {problemas:?}");

        let t = Instant::now();
        let _ = contab::resumen(&leida, 2026, 0);
        let resumen = t.elapsed();

        let t = Instant::now();
        assert!(modelos::generar(&leida, "303", 2026, 1).is_some());
        let m303 = t.elapsed();

        let t = Instant::now();
        assert!(modelos::generar(&leida, "130", 2026, 1).is_some());
        let m130 = t.elapsed();

        let t = Instant::now();
        let filas347 = contab::modelo_347(&leida, 2026).len();
        let m347 = t.elapsed();

        let t = Instant::now();
        let filas349 = fiscal::resumen_349(&leida, 2026).len();
        let m349 = t.elapsed();

        let registros = leida.registros().len();
        assert_eq!(leida.clientes.len(), CLIENTES);
        assert_eq!(leida.facturas().len(), FACTURAS);
        assert_eq!(leida.gastos.len(), GASTOS);
        assert_eq!(registros, FACTURAS + anuladas);

        println!("CARGA: {CLIENTES} clientes, {FACTURAS} facturas, {PRESUPUESTOS} presupuestos, {PROFORMAS} proformas, {GASTOS} gastos, {ARTICULOS} artículos");
        println!("REGISTROS VERIFACTU: {registros} ({} altas + {anuladas} anulaciones)", registros - anuladas);
        println!("JSON: {:.2} MB", bytes as f64 / 1_048_576.0);
        println!("guardar: {} ms", guardar.as_millis());
        println!("leer y parsear: {} ms", cargar.as_millis());
        println!("verificar cadena: {} ms ({} problemas)", cadena.as_millis(), problemas.len());
        println!("resumen 2026: {} ms", resumen.as_millis());
        println!("303 1T 2026: {} ms", m303.as_millis());
        println!("130 1T 2026: {} ms", m130.as_millis());
        println!("347 2026: {} ms ({filas347} filas)", m347.as_millis());
        println!("349 2026: {} ms ({filas349} filas)", m349.as_millis());
        println!("pendiente de cobro: {}", crate::model::fmt_cents_eur(leida.pendientes_cobro()));
    }
}

impl Default for Db {
    fn default() -> Self {
        Self {
            empresa: Empresa::default(),
            clientes: Vec::new(),
            articulos: Vec::new(),
            documentos: Vec::new(),
            gastos: Vec::new(),
            empleados: Vec::new(),
            nominas: Vec::new(),
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
        Self::cargar_desde(ruta_datos())
    }

    /// Carga el fichero indicado con el mismo lector que usa la aplicación.
    /// Se separa de `cargar` para poder probar la lectura de un `datos.json`
    /// concreto sin cambiar `APPDATA` ni tocar los datos reales del usuario.
    fn cargar_desde(ruta: PathBuf) -> (Self, Option<String>) {
        if !ruta.exists() {
            let mut db = Db::default();
            db.ruta = ruta;
            return (db, None);
        }
        match std::fs::read_to_string(&ruta) {
            Ok(txt) => match serde_json::from_str::<Db>(&txt) {
                Ok(mut db) => {
                    db.ruta = ruta;
                    // Los registros anteriores usaban el QR de un sistema que
                    // remite; esta app nunca remitió, así que el cotejo correcto
                    // es el no verificable. No se toca ningún hash.
                    db.migrar_qr_verifactu();
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

    /// Actualiza en memoria el QR de los registros antiguos al modo no
    /// verificable. La próxima vez que se guarde la base quedará persistido;
    /// la huella y la cadena no se modifican.
    fn migrar_qr_verifactu(&mut self) {
        for d in &mut self.documentos {
            if let Some(r) = &mut d.verifactu {
                if r.version < crate::verifactu::VERSION_REGISTRO {
                    r.qr_url = crate::verifactu::construir_qr_url(r);
                }
            }
            if let Some(r) = &mut d.verifactu_anulacion {
                if r.version < crate::verifactu::VERSION_REGISTRO {
                    r.qr_url = crate::verifactu::construir_qr_url(r);
                }
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

    pub fn empleado(&self, id: u64) -> Option<&Empleado> {
        self.empleados.iter().find(|e| e.id == id)
    }

    pub fn empleado_mut(&mut self, id: u64) -> Option<&mut Empleado> {
        self.empleados.iter_mut().find(|e| e.id == id)
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
    /// serie: la AEAT encadena por obligado tributario). Se ordena por instante
    /// real, con el huso de cada marca, y no por el texto ISO.
    pub fn registros(&self) -> Vec<&RegistroVerifactu> {
        let mut v: Vec<&RegistroVerifactu> = Vec::new();
        for d in &self.documentos {
            if let Some(r) = &d.verifactu {
                v.push(r);
            }
            if let Some(r) = &d.verifactu_anulacion {
                v.push(r);
            }
        }
        v.sort_by_key(|r| crate::verifactu::clave_orden(r));
        v
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
