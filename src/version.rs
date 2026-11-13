//! Versión de la aplicación.
//!
//! Cargo exige semver de tres componentes, así que el paquete declara `0.7.3`.
//! La versión visible (título de la ventana, "Agregar o quitar programas" y el
//! instalador) es la de aquí, con cuatro campos como pide Windows.
//!
//! Es la única fuente de verdad: `empaquetar-nsis.sh` la lee de este fichero y
//! se la pasa a `makensis`, así que no hay números repetidos por el proyecto.

pub const VERSION: &str = "0.7.3.0";
