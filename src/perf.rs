//! Perfil de rendimiento: la interfaz se adapta al equipo donde corre.
//!
//! Al arrancar se mira la tarjeta gráfica (renderer y versión de OpenGL), el
//! número de núcleos y, si el usuario lo fuerza, la variable de entorno
//! `AUTONOMOFACIL_PERFIL`. Con eso se deciden las animaciones, las sombras y el
//! suavizado de bordes.
//!
//! La idea: en un equipo con OpenGL antiguo (las integradas Intel de Sandy
//! Bridge llegan a 3.1) se apaga lo que cuesta y se queda lo que se usa; en un
//! equipo moderno se deja todo el adorno. Nunca se degrada la legibilidad:
//! sólo efectos.

use eframe::egui;
use eframe::glow::HasContext;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nivel {
    /// Gráfica por software u OpenGL viejo: sin animaciones ni sombras.
    Bajo,
    /// OpenGL 3.3-3.9: animaciones cortas, sin sombras.
    Medio,
    /// OpenGL 4.0 o superior: todo.
    Alto,
}

impl Nivel {
    pub fn etiqueta(self) -> &'static str {
        match self {
            Nivel::Bajo => "bajo",
            Nivel::Medio => "medio",
            Nivel::Alto => "alto",
        }
    }
}

pub struct Perfil {
    pub nivel: Nivel,
    pub gpu: String,
    pub gl_version: String,
    pub nucleos: usize,
    /// Por qué se eligió ese nivel, para poder enseñarlo en Ajustes.
    pub motivo: String,
    pub memoria_mb: Option<u64>,
}

impl Perfil {
    /// Línea corta para Ajustes: qué equipo se ha detectado y qué se aplicó.
    pub fn resumen(&self) -> String {
        format!(
            "{} · OpenGL {} · {} núcleos · perfil {} ({})",
            self.gpu.trim(),
            self.gl_version.trim(),
            self.nucleos,
            self.nivel.etiqueta(),
            self.motivo
        )
    }
}

/// Lee la versión mayor/menor de una cadena tipo `"3.1.0 - Build 9.17..."`.
fn version_gl(texto: &str) -> Option<(u32, u32)> {
    let primera = texto.split_whitespace().next()?;
    let mut partes = primera.split('.');
    let mayor = partes.next()?.parse().ok()?;
    let menor = partes.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((mayor, menor))
}

/// Detecta el perfil. `gl` sólo está disponible si la ventana se creó con
/// OpenGL (renderer glow), que es nuestro caso.
pub fn detectar(gl: Option<&eframe::glow::Context>) -> Perfil {
    let nucleos = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let (gpu, gl_version) = match gl {
        // El contexto está activo cuando eframe llama al creador de la app,
        // así que glGetString es válido aquí.
        Some(gl) => unsafe {
            (
                gl.get_parameter_string(eframe::glow::RENDERER),
                gl.get_parameter_string(eframe::glow::VERSION),
            )
        },
        None => ("sin OpenGL".to_string(), "?".to_string()),
    };

    let mut perfil = Perfil {
        nivel: Nivel::Medio,
        gpu,
        gl_version,
        nucleos,
        motivo: String::new(),
        memoria_mb: None,
    };

    let version = version_gl(&perfil.gl_version);
    let texto = format!(
        "{} {}",
        perfil.gpu.to_lowercase(),
        perfil.gl_version.to_lowercase()
    );
    let software = [
        "llvmpipe",
        "softpipe",
        "swrast",
        "gdi generic",
        "microsoft basic",
        "software",
    ]
    .iter()
    .any(|marca| texto.contains(marca));

    perfil.nivel = if software {
        perfil.motivo = "gráfica por software".into();
        Nivel::Bajo
    } else {
        match version {
            Some(v) if v < (3, 3) => {
                perfil.motivo = format!("OpenGL {}.{} antiguo", v.0, v.1);
                Nivel::Bajo
            }
            Some(v) if v >= (4, 0) => {
                perfil.motivo = format!("OpenGL {}.{} moderno", v.0, v.1);
                Nivel::Alto
            }
            Some(v) => {
                perfil.motivo = format!("OpenGL {}.{} intermedio", v.0, v.1);
                Nivel::Medio
            }
            None => {
                perfil.motivo = "versión de OpenGL desconocida".into();
                Nivel::Medio
            }
        }
    };

    // Un solo núcleo no sostiene el perfil alto por mucho que la gráfica ayude.
    if perfil.nivel == Nivel::Alto && perfil.nucleos <= 2 {
        perfil.nivel = Nivel::Medio;
        perfil.motivo = format!("{}; sólo {} núcleos", perfil.motivo, perfil.nucleos);
    }

    // Escape manual, útil para probar o para forzar en un equipo raro.
    if let Ok(forzado) = std::env::var("AUTONOMOFACIL_PERFIL") {
        perfil.nivel = match forzado.to_lowercase().as_str() {
            "bajo" => Nivel::Bajo,
            "medio" => Nivel::Medio,
            "alto" => Nivel::Alto,
            _ => perfil.nivel,
        };
        perfil.motivo = format!("forzado por AUTONOMOFACIL_PERFIL={forzado}");
    }

    perfil
}

/// Aplica el perfil al estilo: animaciones, sombras y suavizado de bordes.
pub fn aplicar(estilo: &mut egui::Style, ctx: &egui::Context, perfil: &Perfil) {
    let sin_sombra = egui::epaint::Shadow {
        offset: egui::Vec2::ZERO,
        blur: 0.0,
        spread: 0.0,
        color: egui::Color32::TRANSPARENT,
    };

    match perfil.nivel {
        Nivel::Bajo => {
            // Sin transiciones: cada fotograma cuesta lo mismo y no se repinta
            // de más por una animación a medias.
            estilo.animation_time = 0.0;
            estilo.visuals.window_shadow = sin_sombra;
            estilo.visuals.popup_shadow = sin_sombra;
            // El suavizado de bordes triangula cada forma; en software se nota.
            ctx.tessellation_options_mut(|t| t.feathering = false);
        }
        Nivel::Medio => {
            estilo.animation_time = 0.12;
            estilo.visuals.window_shadow = sin_sombra;
        }
        Nivel::Alto => {
            estilo.animation_time = 0.25;
        }
    }
}

/// Medición opcional para comparar configuraciones con datos, no a ojo.
///
/// Con `AUTONOMOFACIL_PERF=1` la app repinta en bucle, escribe fps por stderr y
/// se cierra sola a los `AUTONOMOFACIL_PERF_SEGUNDOS` (10 por defecto).
pub struct Benchmark {
    inicio: Instant,
    ultimo_informe: Instant,
    frames: u64,
    frames_informe: u64,
    duracion: Duration,
    peor_ms: f64,
}

impl Benchmark {
    pub fn desde_entorno() -> Option<Self> {
        if std::env::var("AUTONOMOFACIL_PERF").ok().as_deref() != Some("1") {
            return None;
        }
        let segundos: f64 = std::env::var("AUTONOMOFACIL_PERF_SEGUNDOS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10.0);
        let ahora = Instant::now();
        Some(Self {
            inicio: ahora,
            ultimo_informe: ahora,
            frames: 0,
            frames_informe: 0,
            duracion: Duration::from_secs_f64(segundos.max(1.0)),
            peor_ms: 0.0,
        })
    }

    /// Cuenta un fotograma. Devuelve `true` cuando ya toca cerrar.
    pub fn tick(&mut self, ctx: &egui::Context) -> bool {
        ctx.request_repaint();
        self.frames += 1;
        self.frames_informe += 1;

        let ahora = Instant::now();
        if ahora.duration_since(self.ultimo_informe).as_secs_f64() >= 2.0 {
            let fps = self.frames_informe as f64
                / ahora.duration_since(self.ultimo_informe).as_secs_f64();
            self.peor_ms = self.peor_ms.max(1000.0 / fps.max(0.001));
            eprintln!("PERF fps={fps:.1} frames_acumulados={}", self.frames);
            self.ultimo_informe = ahora;
            self.frames_informe = 0;
        }

        let total = ahora.duration_since(self.inicio);
        if total >= self.duracion {
            let segundos = total.as_secs_f64();
            eprintln!(
                "PERF_TOTAL fps_medio={:.1} frames={} segundos={:.1}",
                self.frames as f64 / segundos,
                self.frames,
                segundos
            );
            return true;
        }
        false
    }
}
