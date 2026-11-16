#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! AutónomoFACIL: presupuestos, facturas y nóminas para España.
//!
//! · Dinero en céntimos (`i64`), nunca coma flotante.
//! · Registro Verifactu con huella SHA-256 encadenada y QR de la AEAT.
//! · Exportación FacturaE 3.2.2 y PDF A4 sin dependencias nativas.

mod asistente;
mod contab;
mod empleados;
mod export;
mod fiscal;
mod graficas;
mod logo;
mod modelos;
mod nomina_paquetes;
mod facturae;
mod model;
mod pdf;
mod perf;
mod presentacion;
mod recursos;
mod version;
mod store;
mod tema;
mod ui;
mod verifactu;
mod voz;

use eframe::egui;

fn main() -> eframe::Result {
    // Modo sin interfaz: genera un juego de ejemplo (PDF + FacturaE) y sale.
    // Sirve para comprobar el motor sin abrir ventana.
    if std::env::args().any(|a| a == "--demo") {
        demo();
        return Ok(());
    }

    let (db, aviso) = store::Db::cargar();

    let opciones = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1260.0, 820.0])
            .with_min_inner_size([820.0, 540.0])
            .with_title(format!(
                "AutónomoFACIL {} — presupuestos, facturas y nóminas",
                version::VERSION
            )),
        ..Default::default()
    };

    eframe::run_native(
        "autonomofacil",
        opciones,
        Box::new(move |cc| {
            // El contexto GL ya está activo aquí, así que se puede preguntar al
            // driver qué tarjeta hay y adaptar la interfaz a ella.
            let perfil = perf::detectar(cc.gl.as_deref());
            eprintln!(
                "perfil de rendimiento: {} · OpenGL {} · {} núcleos · nivel {}",
                perfil.gpu.trim(),
                perfil.gl_version.trim(),
                perfil.nucleos,
                perfil.nivel.etiqueta()
            );
            configurar(&cc.egui_ctx, &perfil);
            Ok(Box::new(ui::App::new(
                db,
                aviso,
                perfil,
                perf::Benchmark::desde_entorno(),
            )))
        }),
    )
}

/// Genera una factura de ejemplo con dos tipos de IVA y retención de IRPF.
fn demo() {
    use crate::model::*;

    let empresa = Empresa {
        nombre: "Estudio Ñandú S.L.".to_string(),
        nif: "B12345674".to_string(),
        direccion: "Calle Mayor 1".to_string(),
        cp: "28013".to_string(),
        ciudad: "Madrid".to_string(),
        provincia: "Madrid".to_string(),
        email: "hola@nandu.es".to_string(),
        telefono: "910 000 000".to_string(),
        iban: "ES9121000418450200051332".to_string(),
        serie: "2026".to_string(),
        ..Default::default()
    };
    let cliente = Cliente {
        id: 1,
        nombre: "Agencia Creativa S.L.".to_string(),
        nif: "B76281996".to_string(),
        direccion: "Gran Vía 28".to_string(),
        cp: "28013".to_string(),
        ciudad: "Madrid".to_string(),
        provincia: "Madrid".to_string(),
        email: "facturacion@agenciacreativasl.es".to_string(),
        ..Default::default()
    };

    let mut doc = Documento::nuevo(TipoDoc::Factura, cliente, 1500, "2026".to_string());
    doc.id = 1;
    doc.numero = 1;
    doc.estado = Estado::Emitido;
    doc.lineas = vec![
        Linea {
            descripcion: "Diseño de identidad corporativa".to_string(),
            cantidad_milli: 1000,
            precio_cents: 200_000,
            descuento_pb: 0,
            iva_pb: 2100,
        },
        Linea {
            descripcion: "Manual de marca, 120 páginas".to_string(),
            cantidad_milli: 1000,
            precio_cents: 85_000,
            descuento_pb: 1000,
            iva_pb: 2100,
        },
        Linea {
            descripcion: "Formación al equipo (5 h)".to_string(),
            cantidad_milli: 5000,
            precio_cents: 9_000,
            descuento_pb: 0,
            iva_pb: 1000,
        },
    ];
    doc.verifactu = Some(verifactu::registro_alta(&doc, &empresa.nif, ""));

    let t = doc.totales();
    println!("Factura {}", doc.etiqueta_numero());
    for (tipo, base, cuota) in &t.bases {
        println!(
            "  IVA {:>5} %  base {:>10}  cuota {:>10}",
            fmt_pb(*tipo),
            fmt_cents(*base),
            fmt_cents(*cuota)
        );
    }
    println!("  IRPF        {:>10}", format!("-{}", fmt_cents(t.retencion)));
    println!("  TOTAL       {:>10}", fmt_cents_eur(t.total));
    if let Some(r) = &doc.verifactu {
        println!("  Huella      {}", r.huella);
        println!("  QR          {}", r.qr_url);
    }

    let dir = export::dir_documento(&doc, &empresa);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("No se pudo crear {}: {e}", dir.display());
        return;
    }
    let ruta_pdf = dir.join(pdf::nombre_fichero(&doc));
    let ruta_xml = dir.join(facturae::nombre_fichero(&doc));
    match std::fs::write(&ruta_pdf, pdf::documento(&doc, &empresa)) {
        Ok(()) => println!("  PDF         {}", ruta_pdf.display()),
        Err(e) => eprintln!("PDF: {e}"),
    }
    match std::fs::write(&ruta_xml, facturae::generar(&doc, &empresa)) {
        Ok(()) => println!("  FacturaE    {}", ruta_xml.display()),
        Err(e) => eprintln!("XML: {e}"),
    }
}

fn configurar(ctx: &egui::Context, perfil: &perf::Perfil) {
    let mut estilo = (*ctx.style()).clone();
    tema::aplicar_estilo(&mut estilo);
    // Animaciones, sombras y suavizado según el equipo detectado.
    perf::aplicar(&mut estilo, ctx, perfil);
    ctx.set_style(estilo);
}
