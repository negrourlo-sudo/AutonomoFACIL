//! Incrusta el icono de la aplicación en el ejecutable de Windows.
//!
//! Rust no mete recursos en el PE por su cuenta, así que se compila el `.rc`
//! con `windres` (viene con MinGW) y se enlaza el objeto resultante. Si
//! `windres` no está en el PATH el build sigue adelante sin icono: es un
//! detalle estético, no un requisito para compilar.

fn main() {
    println!("cargo:rerun-if-changed=recursos/icono.rc");
    println!("cargo:rerun-if-changed=instalador/icono.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let Ok(out) = std::env::var("OUT_DIR") else {
        return;
    };
    let objeto = format!("{out}/icono.res.o");
    let ok = std::process::Command::new("windres")
        .args(["recursos/icono.rc", "-O", "coff", "-o", &objeto])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        println!("cargo:rustc-link-arg={objeto}");
    }
}
