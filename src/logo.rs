//! Logo de la empresa en los PDF.
//!
//! El PDF embebe el JPEG tal cual (`/DCTDecode`), así que no hace falta ninguna
//! biblioteca de imagen ni decodificar píxeles en Rust. Al elegir la imagen se
//! normaliza con las API de Windows a un JPEG sobre fondo blanco y con un
//! máximo de 900 px, que es lo que después se imprime.

use std::path::PathBuf;

pub struct Jpeg {
    pub datos: Vec<u8>,
    pub ancho: u32,
    pub alto: u32,
}

fn es_jpeg(d: &[u8]) -> bool {
    d.len() > 4 && d[0] == 0xFF && d[1] == 0xD8 && d[2] == 0xFF
}

/// Lee ancho y alto del marcador SOF del JPEG (sin decodificar la imagen).
fn dimensiones(d: &[u8]) -> Option<(u32, u32)> {
    if !es_jpeg(d) {
        return None;
    }
    let mut i = 2usize;
    while i + 9 < d.len() {
        if d[i] != 0xFF {
            i += 1;
            continue;
        }
        let marcador = d[i + 1];
        // Marcadores sin campo de longitud.
        if (0xD0..=0xD9).contains(&marcador) || marcador == 0x01 {
            i += 2;
            continue;
        }
        let len = ((d[i + 2] as usize) << 8) | d[i + 3] as usize;
        let es_sof = (0xC0..=0xCF).contains(&marcador)
            && marcador != 0xC4
            && marcador != 0xC8
            && marcador != 0xCC;
        if es_sof {
            let alto = ((d[i + 5] as u32) << 8) | d[i + 6] as u32;
            let ancho = ((d[i + 7] as u32) << 8) | d[i + 8] as u32;
            if ancho == 0 || alto == 0 {
                return None;
            }
            return Some((ancho, alto));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

/// Carga el logo listo para embeber. Un logo vacío o ilegible no es un error
/// grave: el documento se genera sin él.
pub fn cargar(ruta: &str) -> Result<Jpeg, String> {
    let ruta = ruta.trim();
    if ruta.is_empty() {
        return Err("Sin logo.".to_string());
    }
    let datos = std::fs::read(ruta).map_err(|e| format!("No se pudo leer el logo: {e}"))?;
    if datos.len() > 4 * 1024 * 1024 {
        return Err("La imagen es demasiado grande (máximo 4 MB).".to_string());
    }
    let (ancho, alto) = dimensiones(&datos)
        .ok_or_else(|| "El logo debe ser un JPEG válido.".to_string())?;
    Ok(Jpeg { datos, ancho, alto })
}

pub fn destino() -> PathBuf {
    crate::store::dir_base().join("logo.jpg")
}

/// Abre el diálogo de Windows para elegir una imagen.
#[cfg(windows)]
pub fn elegir() -> Option<String> {
    let script = "Add-Type -AssemblyName System.Windows.Forms; \
        $d = New-Object System.Windows.Forms.OpenFileDialog; \
        $d.Title = 'Elige el logo de tu empresa'; \
        $d.Filter = 'Imagenes|*.jpg;*.jpeg;*.png;*.bmp;*.gif|Todos los archivos|*.*'; \
        if ($d.ShowDialog() -eq 'OK') { $d.FileName }";
    let salida = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .output()
        .ok()?;
    let ruta = String::from_utf8_lossy(&salida.stdout).trim().to_string();
    if ruta.is_empty() {
        None
    } else {
        Some(ruta)
    }
}

#[cfg(not(windows))]
pub fn elegir() -> Option<String> {
    None
}

/// Convierte cualquier imagen a JPEG sobre fondo blanco, con 900 px de lado
/// como máximo, y devuelve la ruta del fichero normalizado.
#[cfg(windows)]
pub fn normalizar(origen: &str) -> Result<String, String> {
    let destino = destino();
    let origen_ps = origen.replace('\'', "''");
    let destino_ps = destino.display().to_string().replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Drawing; \
         $img = [System.Drawing.Image]::FromFile('{origen_ps}'); \
         $max = 900.0; \
         if ($img.Width -gt $max -or $img.Height -gt $max) {{ \
             if ($img.Width -ge $img.Height) {{ $nw = $max; $nh = [int]($img.Height * $max / $img.Width) }} \
             else {{ $nh = $max; $nw = [int]($img.Width * $max / $img.Height) }} \
         }} else {{ $nw = $img.Width; $nh = $img.Height }}; \
         $bmp = New-Object System.Drawing.Bitmap([int]$nw, [int]$nh); \
         $g = [System.Drawing.Graphics]::FromImage($bmp); \
         $g.Clear([System.Drawing.Color]::White); \
         $g.DrawImage($img, 0, 0, [int]$nw, [int]$nh); \
         $bmp.Save('{destino_ps}', [System.Drawing.Imaging.ImageFormat]::Jpeg); \
         $g.Dispose(); $bmp.Dispose(); $img.Dispose()"
    );
    let salida = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|e| format!("No se pudo abrir la imagen: {e}"))?;
    if !salida.status.success() {
        let err = String::from_utf8_lossy(&salida.stderr);
        return Err(format!(
            "No se pudo preparar el logo: {}",
            err.lines().next().unwrap_or("error desconocido")
        ));
    }
    if !destino.exists() {
        return Err("La imagen no se pudo convertir.".to_string());
    }
    Ok(destino.display().to_string())
}

#[cfg(not(windows))]
pub fn normalizar(origen: &str) -> Result<String, String> {
    cargar(origen)?;
    Ok(origen.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SOI + APP0 mínimo + SOF0 con 120x40 + EOI.
    fn jpeg_falso(ancho: u16, alto: u16) -> Vec<u8> {
        // APP0 con longitud 7 = los dos bytes de longitud + "JFIF\0".
        let mut d = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x07];
        d.extend_from_slice(b"JFIF\0");
        d.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        d.extend_from_slice(&alto.to_be_bytes());
        d.extend_from_slice(&ancho.to_be_bytes());
        d.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
        d.extend_from_slice(&[0xFF, 0xD9]);
        d
    }

    #[test]
    fn lee_las_dimensiones_del_sof() {
        let d = jpeg_falso(120, 40);
        assert_eq!(dimensiones(&d), Some((120, 40)));
    }

    #[test]
    fn rechaza_lo_que_no_es_jpeg() {
        assert_eq!(dimensiones(b"PNG..."), None);
        assert!(cargar("").is_err());
    }

    #[test]
    fn un_logo_inexistente_no_rompe_el_documento() {
        assert!(cargar("C:/ruta/que/no/existe.png").is_err());
    }
}
