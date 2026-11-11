//! Ayuda para conseguir un logo cuando todavía no hay uno.
//!
//! Enlaces oficiales a generadores de imagen y de logos, más un prompt listo
//! para copiar. Igual que el resto de la app, sin dependencias: sólo texto y
//! URLs que se abren en el navegador del sistema.

pub struct Recurso {
    pub nombre: &'static str,
    pub url: &'static str,
    /// Qué aporta y para qué usarlo.
    pub nota: &'static str,
}

/// Prompt para pedir un logo a una IA de imagen (Nano Banana, ChatGPT, etc.).
/// Los corchetes son lo único que hay que cambiar.
pub const PROMPT_LOGO: &str = "Diseña un logotipo para [nombre de mi empresa], que se dedica a \
[sector o servicio]. Estilo minimalista y profesional, plano, con dos colores (azul #2B59C8 y \
gris oscuro), sin degradados ni sombras, centrado en un lienzo cuadrado de fondo blanco liso, \
sin texto pequeño ni marcas de agua. Que se reconozca a 32 píxeles y funcione impreso en blanco \
y negro. Entrégamelo en alta resolución.";

pub const LOGO: [Recurso; 8] = [
    Recurso {
        nombre: "Nano Banana · Gemini",
        url: "https://gemini.google.com/",
        nota: "Pega el prompt en el chat de Gemini (modelo de imagen Nano Banana) y descarga el resultado.",
    },
    Recurso {
        nombre: "ChatGPT",
        url: "https://chatgpt.com/",
        nota: "Pide el logo con el prompt de abajo y pide también una versión en blanco y negro.",
    },
    Recurso {
        nombre: "Microsoft Designer",
        url: "https://designer.microsoft.com/",
        nota: "Generador de imágenes gratis con cuenta Microsoft; exporta en PNG.",
    },
    Recurso {
        nombre: "Canva",
        url: "https://www.canva.com/logos/",
        nota: "Plantillas y editor sencillo: elige una, cambia el nombre y descarga el PNG.",
    },
    Recurso {
        nombre: "Looka",
        url: "https://looka.com/",
        nota: "Genera varias propuestas y un kit de marca con colores y tipografías.",
    },
    Recurso {
        nombre: "Shopify Logo Maker",
        url: "https://www.shopify.com/tools/logo-maker",
        nota: "Gratis y sin cuenta; suficiente para empezar a facturar hoy.",
    },
    Recurso {
        nombre: "Ideogram",
        url: "https://ideogram.ai/",
        nota: "Va bien cuando el logo lleva texto o iniciales.",
    },
    Recurso {
        nombre: "remove.bg",
        url: "https://www.remove.bg/",
        nota: "Si el logo sale con fondo, quítalo aquí. La app lo pasa a blanco igualmente.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_recursos_son_enlaces_seguros() {
        for r in LOGO {
            assert!(r.url.starts_with("https://"), "{}", r.url);
            assert!(!r.nombre.trim().is_empty());
            assert!(!r.nota.trim().is_empty());
        }
    }

    #[test]
    fn el_prompt_se_entiende_y_tiene_huecos_que_rellenar() {
        assert!(PROMPT_LOGO.contains("[nombre de mi empresa]"));
        assert!(PROMPT_LOGO.contains("[sector o servicio]"));
        assert!(PROMPT_LOGO.contains("cuadrado"));
        assert!(PROMPT_LOGO.len() < 700, "un prompt corto se copia y se pega mejor");
    }
}
