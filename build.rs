//! Incrusta el icono de la aplicación en el ejecutable de Windows.
//!
//! Rust no mete recursos en el PE por su cuenta, así que se compila el `.rc`
//! con `windres` (viene con MinGW) y se enlaza el objeto resultante. No se
//! depende sólo del PATH: también se busca `WINDRES`, el toolchain local del
//! proyecto y las rutas habituales de MinGW/MSYS2. Si no aparece, el build
//! continúa y deja un aviso: el instalador conserva `icono.ico` como respaldo,
//! pero el `.exe` no tendrá icono.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=recursos/icono.rc");
    println!("cargo:rerun-if-changed=instalador/icono.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Ok(out) = std::env::var("OUT_DIR") else {
        return;
    };
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let Some(windres) = buscar_windres(&manifest) else {
        println!(
            "cargo:warning=No se encontró windres; el ejecutable se compilará sin icono. \
             Instala MinGW o define WINDRES con la ruta a windres.exe."
        );
        return;
    };

    let objeto = Path::new(&out).join("icono.res.o");
    let resultado = Command::new(&windres)
        .current_dir(&manifest)
        .args(["recursos/icono.rc", "-O", "coff", "-o"])
        .arg(&objeto)
        .status();

    match resultado {
        Ok(estado) if estado.success() => {
            println!("cargo:rustc-link-arg={}", objeto.display());
        }
        Ok(estado) => println!(
            "cargo:warning=windres terminó con {}; el ejecutable se compilará sin icono.",
            estado
        ),
        Err(e) => println!(
            "cargo:warning=No se pudo ejecutar {}: {e}; el ejecutable se compilará sin icono.",
            windres.display()
        ),
    }
}

fn buscar_windres(manifest: &Path) -> Option<PathBuf> {
    if let Some(ruta) = std::env::var_os("WINDRES") {
        let ruta = PathBuf::from(ruta);
        if ruta.is_file() {
            return Some(ruta);
        }
    }

    let local = manifest.join(".toolchain/mingw64/bin/windres.exe");
    if local.is_file() {
        return Some(local);
    }

    if Command::new("windres")
        .arg("--version")
        .output()
        .map(|salida| salida.status.success())
        .unwrap_or(false)
    {
        return Some(PathBuf::from("windres"));
    }

    for ruta in [
        "C:/msys64/mingw64/bin/windres.exe",
        "C:/msys64/ucrt64/bin/windres.exe",
        "C:/Program Files/mingw64/bin/windres.exe",
    ] {
        let ruta = PathBuf::from(ruta);
        if ruta.is_file() {
            return Some(ruta);
        }
    }

    None
}
