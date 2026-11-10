//! Instalador de la aplicación, **por usuario** y sin privilegios.
//!
//! No necesita administrador ni herramientas externas: lleva la aplicación
//! embebida, se copia a `%LOCALAPPDATA%\Programs\Facturacion`, crea los accesos
//! directos y se registra en "Agregar o quitar programas" de HKCU.
//!
//!   Facturacion-Setup.exe              instala (con confirmación)
//!   Facturacion-Setup.exe --silent     instala sin preguntar
//!   Facturacion-Setup.exe --uninstall  desinstala
//!
//! Los accesos directos se crean con `WScript.Shell` desde PowerShell: es la
//! forma de generar un `.lnk` sin escribir COM a mano ni añadir dependencias.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Binario de la aplicación, incrustado al compilar (`empaquetar.sh` lo copia
/// antes a `dist/`).
const APP: &[u8] = include_bytes!("../../dist/facturacion.exe");

const NOMBRE: &str = "Facturacion";
/// Versión que se escribe en el registro. Cargo no admite cuatro campos en
/// `package.version`, así que la fija `empaquetar.sh` leyendo `src/version.rs`;
/// si se compila a mano se usa la del paquete.
const VERSION: &str = match option_env!("FACTURACION_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};
const CLAVE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Facturacion";
const ACCESO: &str = "Facturacion.lnk";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let silencioso = args.iter().any(|a| a == "--silent" || a == "/S");

    if args.iter().any(|a| a == "--uninstall" || a == "/uninstall") {
        desinstalar();
        return;
    }

    if let Err(e) = instalar(silencioso) {
        eprintln!("ERROR: {e}");
        let _ = std::io::stdin().read_line(&mut String::new());
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Rutas
// ---------------------------------------------------------------------------

fn local_appdata() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn dir_instalacion() -> PathBuf {
    local_appdata().join("Programs").join(NOMBRE)
}

fn exe_instalado() -> PathBuf {
    dir_instalacion().join(format!("{}.exe", NOMBRE))
}

fn dir_inicio() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|a| {
        PathBuf::from(a)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
    })
}

fn escritorio() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .ok()
        .map(|u| PathBuf::from(u).join("Desktop"))
}

// ---------------------------------------------------------------------------
// Instalación
// ---------------------------------------------------------------------------

fn instalar(silencioso: bool) -> Result<(), String> {
    let dir = dir_instalacion();
    std::fs::create_dir_all(&dir).map_err(|e| format!("crear {}: {e}", dir.display()))?;

    if !silencioso {
        println!("Facturacion {VERSION}");
        println!("Se instalara en: {}", dir.display());
        print!("Continuar? [S/n] ");
        let _ = std::io::stdout().flush();
        let mut linea = String::new();
        let _ = std::io::stdin().read_line(&mut linea);
        if matches!(linea.trim(), "n" | "N" | "no" | "NO") {
            println!("Cancelado.");
            return Ok(());
        }
    }

    let exe = exe_instalado();
    std::fs::write(&exe, APP).map_err(|e| format!("escribir la aplicacion: {e}"))?;
    println!("  aplicacion   {}", exe.display());

    // El desinstalador es este mismo binario, que ya lleva la app dentro.
    let desinstalador = dir.join("Desinstalar.exe");
    if let Ok(propio) = std::env::current_exe() {
        let _ = std::fs::copy(&propio, &desinstalador);
    }

    let mut accesos = 0;
    if let Some(inicio) = dir_inicio() {
        let _ = std::fs::create_dir_all(&inicio);
        if crear_acceso(&inicio.join(ACCESO), &exe).is_ok() {
            accesos += 1;
        }
    }
    if let Some(esc) = escritorio() {
        if crear_acceso(&esc.join(ACCESO), &exe).is_ok() {
            accesos += 1;
        }
    }
    println!("  accesos      {accesos} (menu Inicio y escritorio)");

    registrar(&exe, &desinstalador, &dir);
    println!("  desinstalar  {}", desinstalador.display());
    println!();
    println!("Listo. Busca \"Facturacion\" en el menu Inicio.");

    if !silencioso {
        print!("\nPulsa Enter para cerrar...");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    Ok(())
}

/// Crea un `.lnk` con `WScript.Shell`. PowerShell recibe los argumentos como
/// UTF-16 desde Windows, así que los acentos del nombre no se rompen.
fn crear_acceso(lnk: &Path, exe: &Path) -> Result<(), String> {
    let limpio = |p: &Path| p.display().to_string().replace('\'', "''");
    let trabajo = exe.parent().unwrap_or(Path::new("."));
    let script = format!(
        "$w = New-Object -ComObject WScript.Shell; \
         $s = $w.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.WorkingDirectory = '{}'; \
         $s.IconLocation = '{},0'; \
         $s.Description = 'Facturacion: presupuestos y facturas'; \
         $s.Save()",
        limpio(lnk),
        limpio(exe),
        limpio(trabajo),
        limpio(exe)
    );

    let salida = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|e| format!("powershell: {e}"))?;

    if salida.status.success() && lnk.exists() {
        Ok(())
    } else {
        Err(format!(
            "no se pudo crear {}: {}",
            lnk.display(),
            String::from_utf8_lossy(&salida.stderr).trim()
        ))
    }
}

fn reg(args: &[&str]) {
    let _ = Command::new("reg").args(args).output();
}

fn registrar(exe: &Path, desinstalador: &Path, dir: &Path) {
    let s = |p: &Path| p.display().to_string();
    let desinstalar = format!("\"{}\" --uninstall", s(desinstalador));
    let tamano = (APP.len() / 1024).to_string();

    reg(&["add", CLAVE, "/v", "DisplayName", "/t", "REG_SZ", "/d", "Facturacion", "/f"]);
    reg(&["add", CLAVE, "/v", "DisplayVersion", "/t", "REG_SZ", "/d", VERSION, "/f"]);
    reg(&["add", CLAVE, "/v", "Publisher", "/t", "REG_SZ", "/d", "Facturacion", "/f"]);
    reg(&["add", CLAVE, "/v", "InstallLocation", "/t", "REG_SZ", "/d", &s(dir), "/f"]);
    reg(&["add", CLAVE, "/v", "DisplayIcon", "/t", "REG_SZ", "/d", &s(exe), "/f"]);
    reg(&["add", CLAVE, "/v", "UninstallString", "/t", "REG_SZ", "/d", &desinstalar, "/f"]);
    reg(&["add", CLAVE, "/v", "EstimatedSize", "/t", "REG_DWORD", "/d", &tamano, "/f"]);
    reg(&["add", CLAVE, "/v", "NoModify", "/t", "REG_DWORD", "/d", "1", "/f"]);
    reg(&["add", CLAVE, "/v", "NoRepair", "/t", "REG_DWORD", "/d", "1", "/f"]);
}

// ---------------------------------------------------------------------------
// Desinstalación
// ---------------------------------------------------------------------------

fn desinstalar() {
    let dir = dir_instalacion();

    if let Some(inicio) = dir_inicio() {
        let _ = std::fs::remove_file(inicio.join(ACCESO));
    }
    if let Some(esc) = escritorio() {
        let _ = std::fs::remove_file(esc.join(ACCESO));
    }
    reg(&["delete", CLAVE, "/f"]);

    // Un .exe en ejecución no se puede borrar desde sí mismo: se difiere.
    // `rmdir` espera a que el proceso termine.
    let orden = format!(
        "timeout /t 2 >nul & rmdir /s /q \"{}\"",
        dir.display()
    );
    let _ = Command::new("cmd").args(["/C", &orden]).spawn();

    println!("Desinstalado. La carpeta se borrara en unos segundos.");
    println!("Tus datos NO se han tocado: siguen en %APPDATA%\\Facturacion");
}
