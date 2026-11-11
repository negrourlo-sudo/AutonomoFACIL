//! Lectura en voz alta del tutorial.
//!
//! Usa la voz de Windows (SAPI) a través de PowerShell, sin dependencias: se
//! elige la primera voz **española y femenina** instalada (Microsoft Helena en
//! un Windows en español) y, si no hay, cualquier voz femenina.
//!
//! Cada frase es un proceso aparte: así se puede parar al instante matando sólo
//! ese proceso y no hay que sincronizar una cola. El texto viaja en base64 para
//! que acentos, comillas y puntos y coma no rompan el comando.

use std::process::Child;

pub struct Voz {
    hijo: Option<Child>,
}

impl Default for Voz {
    fn default() -> Self {
        Self { hijo: None }
    }
}

impl Voz {
    /// Empieza a leer `texto`, cortando lo que estuviera diciendo.
    pub fn decir(&mut self, texto: &str) {
        self.parar();
        let limpio = texto.trim();
        if limpio.is_empty() {
            return;
        }
        let corto: String = limpio.chars().take(1500).collect();
        if let Ok(hijo) = lanzar(&corto) {
            self.hijo = Some(hijo);
        }
    }

    /// Corta la lectura actual, si la hay.
    pub fn parar(&mut self) {
        if let Some(mut c) = self.hijo.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    /// ¿Sigue leyendo? Limpia el proceso cuando ya ha terminado.
    pub fn hablando(&mut self) -> bool {
        let terminado = match &mut self.hijo {
            Some(c) => matches!(c.try_wait(), Ok(Some(_)) | Err(_)),
            None => false,
        };
        if terminado {
            self.hijo = None;
        }
        self.hijo.is_some()
    }
}

/// Base64 sin dependencias: el texto se pasa al script sin problemas de comillas.
fn base64(datos: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity((datos.len() + 2) / 3 * 4);
    for trozo in datos.chunks(3) {
        let b = [
            trozo[0],
            *trozo.get(1).unwrap_or(&0),
            *trozo.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if trozo.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if trozo.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

#[cfg(windows)]
fn lanzar(texto: &str) -> Result<Child, String> {
    use std::os::windows::process::CommandExt;
    let codificado = base64(texto.as_bytes());
    let script = format!(
        "Add-Type -AssemblyName System.Speech; \
         $t = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{codificado}')); \
         $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
         $v = $s.GetInstalledVoices() | Where-Object {{ $_.Enabled -and $_.VoiceInfo.Culture.Name -like 'es*' -and $_.VoiceInfo.Gender -eq 'Female' }} | Select-Object -First 1; \
         if (-not $v) {{ $v = $s.GetInstalledVoices() | Where-Object {{ $_.Enabled -and $_.VoiceInfo.Gender -eq 'Female' }} | Select-Object -First 1 }}; \
         if ($v) {{ $s.SelectVoice($v.VoiceInfo.Name) }}; \
         $s.Rate = -1; $s.Volume = 100; $s.Speak($t); $s.Dispose()"
    );
    // CREATE_NO_WINDOW: no asoma ninguna consola al hablar.
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(0x0800_0000)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("No se pudo iniciar la voz: {e}"))
}

#[cfg(not(windows))]
fn lanzar(_texto: &str) -> Result<Child, String> {
    Err("La voz sólo está disponible en Windows.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_base64_codifica_acentos_y_comillas() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64("ñ".as_bytes()), "w7E=");
        // Comillas y punto y coma no pueden romper el script.
        assert_eq!(base64(b"';\""), "Jzsi");
    }

    #[test]
    fn parar_sin_voz_no_hace_nada() {
        let mut v = Voz::default();
        v.parar();
        assert!(!v.hablando());
        v.decir("   ");
        assert!(!v.hablando());
    }
}
