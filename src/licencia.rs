//! Datos de la licencia del proyecto.
//!
//! El código y los contenidos propios de AutónomoFACIL se distribuyen bajo
//! CC BY-SA 4.0. Las dependencias de terceros conservan sus licencias.

pub const NOMBRE: &str = "Creative Commons Atribución-CompartirIgual 4.0 Internacional";
pub const CORTA: &str = "CC BY-SA 4.0";
pub const URL: &str = "https://creativecommons.org/licenses/by-sa/4.0/";
pub const URL_RESUMEN: &str = "https://creativecommons.org/licenses/by-sa/4.0/deed.es";
pub const URL_TEXTO: &str = "https://creativecommons.org/licenses/by-sa/4.0/legalcode.es";
pub const ATRIBUCION: &str =
    "© 2026 FranciscoNicolás · AutónomoFACIL. Algunos derechos reservados.";
pub const RESUMEN: &str = "Puedes copiar, redistribuir y adaptar el código y los contenidos propios, \
incluso con fines comerciales, siempre que cites la autoría, enlaces a la licencia e indiques si hay \
cambios. Las adaptaciones deben compartirse bajo la misma licencia.";
pub const ALCANCE: &str =
    "El código y los contenidos propios. Las dependencias de terceros conservan sus licencias.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_licencia_es_cc_by_sa_y_los_enlaces_son_seguros() {
        assert_eq!(CORTA, "CC BY-SA 4.0");
        assert!(NOMBRE.contains("Atribución-CompartirIgual"));
        for url in [URL, URL_RESUMEN, URL_TEXTO] {
            assert!(url.starts_with("https://creativecommons.org/licenses/by-sa/4.0/"));
        }
    }

    #[test]
    fn la_atribucion_nombra_al_titular_y_el_ano() {
        assert!(ATRIBUCION.contains("FranciscoNicolás"));
        assert!(ATRIBUCION.contains("2026"));
    }
}
