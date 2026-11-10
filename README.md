# AutónomoFACIL

Aplicación de escritorio en Rust para hacer **presupuestos y facturas** en España,
con registro **Verifactu** encadenado y exportación **FacturaE 3.2.2**.

Sin Electron, sin WebView, sin Node y sin toolchain de C: una ventana nativa
(egui/eframe) y un ejecutable.

---

## Estado legal: qué hace y qué no

| Pieza | Estado en este programa |
|---|---|
| Registro de facturación con huella SHA-256 encadenada | **Implementado** (`src/verifactu.rs`) |
| QR obligatorio en la factura | **Implementado** (PDF y pantalla) |
| FacturaE 3.2.2 | **Implementado** (`src/facturae.rs`) |
| Envío de registros a la AEAT (modalidad VERI\*FACTU) | **No implementado** |
| Firma XAdES del FacturaE | **No implementado** |
| Validación contra el XSD oficial | **No hecha** (ver *Límites*) |

### Plazos vigentes (a 10 de noviembre de 2026)

Esto cambia, compruébalo antes de fiarte:

- **Verifactu** (RD 1007/2023, modificado por el RDL 15/2025):
  - **1 de enero de 2027** para contribuyentes del Impuesto sobre Sociedades.
  - **1 de julio de 2027** para el resto, autónomos incluidos.
  - Hacienda anunció el 5 de octubre de 2026 su intención de aplazar las
    obligaciones pendientes a octubre de 2028, pero **a día de hoy no hay norma
    publicada en el BOE**: las fechas de 2027 siguen siendo las vigentes.
- **Factura electrónica entre empresas** (RD 238/2026 + Orden HAC/1028/2026,
  en vigor desde el 6 de octubre de 2026):
  - **6 de octubre de 2027** para empresas con facturación superior a 8 M€.
  - **6 de octubre de 2028** para el resto.
  - Las especificaciones de la solución pública de Hacienda aún no se han
    publicado, así que aquí sólo se implementa el formato FacturaE, no el
    envío a ninguna plataforma.

**Requisito que este programa no puede cumplir por ti:** para remitir registros
hace falta un certificado electrónico cualificado y que el productor del software
haya presentado la declaración responsable. Este código genera el registro, no
actúa como productor certificado.

---

## Cómo funciona el encadenamiento

Cada factura emitida produce un registro cuya huella es:

```
SHA-256(
  "IDEmisorFactura=<NIF>&NumSerieFactura=<serie+número>&FechaExpedicionFactura=<dd-mm-yyyy>"
  "&TipoFactura=<F1|F2>&CuotaTotal=<x.xx>&ImporteTotal=<x.xx>"
  "&Huella=<huella del registro anterior>&FechaHoraHusoGenRegistro=<ISO-8601 con huso>"
)
```

En el primer registro `Huella` va vacía. Al arrancar, el programa **recalcula
toda la cadena** (`verifactu::verificar_cadena`) y avisa si algún registro no
cuadra con su factura o si el encadenamiento se rompe: si alguien edita el JSON
a mano, se ve en la pantalla de Inicio.

Una factura emitida **no se edita**: se anula, y la anulación genera su propio
registro encadenado con importes a cero. Igual que en la norma.

---

## Cómo se ejecuta

Requisitos: Rust con el target `x86_64-pc-windows-gnu` y MinGW (para enlazar).

```bash
export PATH="$PWD/.toolchain/mingw64/bin:$HOME/.cargo/bin:$PATH"
cargo run --release
```

Datos en `%APPDATA%\AutonomoFACIL\datos.json` (escritura atómica + `.bak`).
Exportaciones en `%APPDATA%\AutonomoFACIL\export\`.

Pruebas:

```bash
cargo test
```

Generar un PDF y un FacturaE de ejemplo sin abrir la ventana:

```bash
cargo run -- --demo
```

---

## Distribuir: ejecutable e instalador

### Un solo comando

```bash
bash empaquetar-nsis.sh
```

Deja en `dist/`:

| Fichero | Qué es |
|---|---|
| `autonomofacil.exe` | La aplicación. Nativa, sin consola, sin runtime ni DLLs que instalar. |
| `AutonomoFACIL-NSIS.exe` | El instalador. Asistente en español, **por usuario**: no pide administrador. |

Hacen falta NSIS 3 y Python 3:

```bash
winget install --id NSIS.NSIS -e --accept-package-agreements --accept-source-agreements
```

### Qué hace el instalador

1. Copia la aplicación a `%LOCALAPPDATA%\Programs\AutonomoFACIL`.
2. Crea accesos directos en el menú Inicio y en el escritorio, con icono.
3. Se registra en **Agregar o quitar programas** de Windows (HKCU).
4. Deja un `Desinstalar.exe` que borra accesos, registro y carpeta.
5. Muestra la licencia antes de instalar.

**Tus datos no se borran al desinstalar**: quedan en `%APPDATA%\AutonomoFACIL`.

### El arte del asistente

Lo genera `instalador/imagenes.py` (sólo biblioteca estándar, sin PIL):
`icono.ico` de 16 a 256 px, `asistente.bmp` de 164x314 y `cabecera.bmp` de
150x57. Se redibuja en cada empaquetado, así que cambiar colores o textos es
editar ese script. El `.nsi` **debe llevar BOM UTF-8** o `makensis` lo lee como
ANSI y rompe los acentos.

> El instalador de NSIS en sí pide administrador para instalarse; el
> instalador que **genera** no, porque es por usuario.

> El instalador propio en Rust y el script de Inno Setup quedaron retirados en
> `archivo/`. No se usan ni se mantienen.

### Pendiente para distribuir de verdad

- **Firma de código.** Sin certificado, Windows SmartScreen avisa de que el
  editor es desconocido. Se arregla con un certificado EV/OV y `signtool`.
- **Icono propio.** Ahora mismo el ejecutable usa el icono genérico.
- **Actualizaciones.** No hay comprobación de versión ni autoactualización.

---

## Arquitectura

```
src/model.rs      Dominio: dinero en céntimos, líneas, totales, numeración
src/store.rs      Persistencia JSON atómica con copia de seguridad
src/verifactu.rs  Huella encadenada, QR, verificación de la cadena
src/facturae.rs   XML FacturaE 3.2.2
src/pdf.rs        Generador de PDF propio + QR dibujado en el papel
src/ui.rs         Interfaz egui: listas, editor, fichas, ajustes
```

Decisiones que conviene conocer:

- **Dinero en `i64` de céntimos.** Nunca `f64`. Las cantidades van en
  milésimas, así que 1,250 h y 0,5 ud son exactas. El redondeo es al céntimo
  más cercano y se hace **una vez por tipo de IVA**, que es como cuadra el
  desglose de FacturaE.
- **Un solo fichero JSON en vez de SQLite.** Evita una dependencia con
  compilador de C y hace los datos inspeccionables y copiables. Para miles de
  facturas es de sobra; el modelo está separado para migrar a SQL sin tocar la
  interfaz.
- **PDF a mano.** Catálogo, árbol de páginas, dos fuentes base-14 y WinAnsi
  (cubre á é í ó ú ñ ü ¿ ¡ €). Cero dependencias, cero binarios nativos.
- **El cliente se congela al emitir.** Editar después su ficha no cambia una
  factura ya emitida.

---

## Límites conocidos

1. **Sin firma XAdES.** Un FacturaE sin firmar no vale para FACe ni para
   intercambio B2B real. Falta firma con certificado.
2. **Sin validación XSD.** No hay validador incluido; el orden de elementos
   está calcado de un documento validado contra `Facturaev3_2_2.xsd`, pero
   conviene validar el XML con `xmllint` u otra herramienta antes de usarlo.
3. **Sin remisión a la AEAT.** No hay cliente SOAP, ni certificado, ni cola de
   reintentos, ni registro de eventos.
4. **Sin series por ejercicio automáticas** más allá del año natural, ni
   rectificativas (sólo anulación), ni recargo de equivalencia, ni IGIC/Canarias,
   ni intracomunitarias.
5. **Un solo usuario, una sola empresa.** No hay multi-tenant ni control de
   acceso.

---

## Qué haría falta para uso real

- Firma XAdES del XML con certificado del emisor.
- Validador XSD en los tests (con el esquema oficial versionado).
- Cliente SOAP de remisión + registro de eventos + política de reintentos.
- Rectificativas (R1) y facturas recapitulativas.
- Copia de seguridad programada y exportación/importación.


## Compatibilidad gráfica (parche local de eframe)

`vendor/eframe` es una copia de eframe 0.29.1 con un cambio en
`src/native/glow_integration.rs`: al crear el contexto OpenGL prueba 3.2, luego
3.0, luego 2.1 y por último GLES, en vez de pedir 3.2 core y rendirse.

Motivo: los drivers Intel antiguos (HD 2000/3000 de Sandy Bridge) llegan sólo a
OpenGL 3.1, así que pedir 3.2 hacía que glutin fallara con
`NotSupported("extension to create ES context with wgl is not present")` y la
ventana no llegaba a abrirse. Con el parche arranca con un contexto 3.0/3.1.

Al actualizar eframe, revisar si el parche sigue siendo necesario.


## Rendimiento y adaptación al equipo

El binario de release compila con `lto = "thin"`, `codegen-units = 1` y
`strip = true`. Medido en el equipo de desarrollo (i5-2400, Intel HD 2000,
Windows 10) con el mismo código:

| Métrica | Antes | Después |
|---|---|---|
| Tiempo hasta ver la ventana | 1919 ms | **385 ms** |
| CPU en reposo (1 núcleo) | 3,8 % | **2,2 %** |
| Memoria residente | 60,7 MB | **51,3 MB** |
| Tamaño del ejecutable | 14 MB | **4,7 MB** |

Además la app se adapta al equipo en tiempo de ejecución (`src/perf.rs`): lee
`GL_RENDERER` y `GL_VERSION` del driver, cuenta los núcleos y elige un perfil.

- **bajo** (OpenGL < 3.3 o gráfica por software): sin animaciones, sin sombras
  y sin suavizado de bordes por triangulación.
- **medio** (OpenGL 3.3-3.9): animaciones cortas, sin sombras.
- **alto** (OpenGL 4.0+): todo el adorno.

Se puede forzar con `AUTONOMOFACIL_PERFIL=bajo|medio|alto` o desde Ajustes, donde
también se muestra el equipo detectado.

### Medir en vez de suponer

```bash
AUTONOMOFACIL_PERF=1 AUTONOMOFACIL_PERF_SEGUNDOS=12 AUTONOMOFACIL_PERFIL=bajo   ./target/release/autonomofacil.exe
```

Repinta en bucle, escribe fps por stderr y se cierra solo. En el i5-2400 los
tres perfiles dan ~40 fps con 5-6 % de un núcleo: ahí el límite es el redibujado
completo de la ventana en la Intel HD 2000, no los efectos, así que el perfil no
cambia los fps. Se mantiene porque en máquinas con OpenGL por software
(llvmpipe, WARP) el suavizado de bordes y las sombras sí pesan.

## Proformas y contabilidad (0.2.0.0)

### Factura proforma

- Menú **Proformas**: el mismo editor que una factura, con serie propia
  (`PRO` + año) y numeración independiente.
- El PDF sale rotulado «FACTURA PROFORMA» y con la leyenda de que no tiene
  validez fiscal.
- No entra en la cadena Verifactu ni se exporta a FacturaE: la puerta única es
  `TipoDoc::es_fiscal()`, que sólo devuelve `true` para `Factura`.
- Se convierte en factura desde el panel de detalle con «Crear factura».

### Contabilidad

- Menú **Contabilidad**, con selector de año y de trimestre (o año entero).
- **Ingresos** e **IVA repercutido** salen de las facturas emitidas: ni
  presupuestos ni proformas ni borradores cuentan.
- Los **gastos** se registran a mano (proveedor, NIF, base, IVA, retención,
  categoría, pagado).
- Calcula **IVA a ingresar** (modelo 303: repercutido − soportado) e **IRPF 130
  estimado** (20 % del rendimiento neto menos las retenciones practicadas).
- **Libro de ingresos y gastos** exportable a CSV con `;` y BOM UTF-8 en
  `%APPDATA%\AutonomoFACIL\export\libro-<año>.csv`.
- Es una estimación informativa: no sustituye a la asesoría fiscal ni cubre
  compensaciones de trimestres anteriores, bienes de inversión o regímenes
  especiales.

## Cobros, copias y cierre de ejercicio (0.2.1)

### Cobros parciales

- En una factura emitida, **«Registrar cobro»** pide fecha e importe.
- Se pueden registrar varios cobros; el detalle muestra **cobrado** y
  **pendiente**, y «Pendiente de cobro» de Inicio ya descuenta lo cobrado.
- Al cubrir el total, la factura pasa sola a *Pagado*.

### Contabilidad por pestañas

| Pestaña | Contenido |
|---|---|
| Resumen | Cifras del periodo, gastos por categoría y libro CSV |
| IVA (303) | Bases y cuotas por tipo de IVA, repercutido y soportado, y resultado |
| IRPF (130) | Acumulado del año hasta el trimestre y cuota del 20 % |
| 347 | Clientes y proveedores por encima de 3.005,06 EUR |
| Calendario | 303/130 trimestrales, 390 y 347, con lo pasado y lo pendiente |
| Clientes | Facturado, cobrado y pendiente por cliente |

### Copias de seguridad

En **Ajustes → Copias de seguridad**:

- «Crear copia ahora» escribe `datos-AAAAMMDD-HHMMSS.json` en
  `%APPDATA%\AutonomoFACIL\copias`.
- Se listan las últimas copias con un botón **Restaurar**, que pide
  confirmación y antes guarda una copia automática del estado actual.

### Cierre de ejercicio

En **Contabilidad** hay un botón «Cerrar {año}». Mientras un ejercicio está
cerrado no se pueden **emitir** documentos con fecha de ese año (los borradores
se pueden seguir preparando). «Reabrir {año}» lo desbloquea.

### Facturas vencidas

Una factura emitida y no cobrada cuyo vencimiento ya pasó se marca con la
insignia **Vencida** en las listas y se cuenta en la tarjeta «Vencidas» de
Inicio.

## Autorrelleno y guía rápida (0.2.2)

### Lo que la aplicación rellena por ti

| Al crear… | Se rellena con |
|---|---|
| Un documento | Fecha de hoy, vencimiento según los días configurados, serie (la de la empresa o `PRO`+año), forma de pago por defecto y una primera línea con el IVA habitual |
| Una factura | IRPF por defecto de la empresa; presupuestos y proformas salen al 0 % |
| Nuevo documento | El **cliente habitual**: el último que usaste en cualquier documento |
| Un gasto | Fecha de hoy, IVA 21 % y las categorías/proveedores de siempre a un clic |
| Un cliente | Al guardar, un NIF que empieza por dígito se marca solo como persona física |
| El PDF | Nombre descriptivo (`Factura-2026-0001-cliente-s-l-2026-11-10.pdf`) y se abre el fichero y su carpeta |

En **Ajustes → Datos de tu empresa** se configuran los **días de vencimiento**
(30 por defecto) y la **forma de pago por defecto**: todo lo demás sale de ahí.

### Guía rápida

Menú **Guía rápida**, pensado para quien nunca ha facturado:

- Siete pasos (datos fiscales, clientes, catálogo, primera factura, gastos,
cobros y copia de seguridad) con una marca de **hecho / pendiente**, una barra
de progreso y un botón **Ir** que lleva a la pantalla correspondiente.
- El estado se deduce de tus datos: si ya tienes clientes o facturas, aparecen
como hechos sin configurar nada.
- **Qué hay que presentar y cuándo**: los 303 y 130 de cada trimestre, el 390 y
el 347, con la fecha límite.
- **«Descargar resumen del año para el gestor»** escribe
`%APPDATA%\AutonomoFACIL\export\resumen-<año>.txt` con las cifras anuales, las de
cada trimestre, el 347 y el 349, y lo abre junto a su carpeta.

## Actividad, IVA internacional y obligaciones (0.3.0)

### Tu actividad decide lo demás

En **Ajustes → Datos de tu empresa → Actividad** se elige entre servicios
profesionales, comercio, venta digital, formación, alquiler u otra. Al cambiarla
se proponen su IVA y su retención típicos (21 % y 15 % en servicios; 0 % en
formación y alquiler).

### Cuatro tipos de operación

Cada documento tiene ahora un campo **Operación**:

| Operación | IVA | Leyenda en el PDF |
|---|---|---|
| Nacional | El de la línea | — |
| Empresa de la UE | 0 | Inversión del sujeto pasivo |
| Fuera de la UE | 0 | Exportación de servicios (art. 21 LIVA) |
| Exenta | 0 | Operación exenta (art. 20 LIVA) |

Los totales, el registro Verifactu y el PDF cuadran a cero de IVA en las tres
últimas, y esas facturas **no entran en el 347** (sólo recoge operaciones
españolas). El 303 muestra aparte las **operaciones sin IVA** (intracomunitarias,
exportaciones y exentas) y el resumen del año incluye el **modelo 349** agrupado
por NIF-IVA del cliente.

### Obligaciones y seguimiento

En **Guía rápida** hay una lista de modelos (303, 130, 390, 100, 347, 349, 111,
190, 115, 180 y, en venta digital, el 368) con plazo y explicación. Cada uno se
marca por trimestres o una vez al año y el estado se guarda en `datos.json`.
Debajo, la guía cambia según la actividad.

### Vender juegos en Steam

- Valve es el **vendedor** frente al jugador: cobra, liquida el IVA o sales tax y
te paga tu parte. Tú facturas a **Valve Corporation (EE. UU.)**.
- Esa factura va como **«Fuera de la UE (exportación)»**: sin IVA, sin retención
y fuera del 349. En el 303 va como operación sin IVA; el bruto es ingreso de
actividad económica en el 130 y en la Renta.
- Valve clasifica el pago como *royalty* y retiene un 30 % en EE. UU. si no
tiene tu **W-8BEN** con NIF; con él aplica el convenio. Guarda el **1042-S**
para restar lo retenido en la Renta.
- El **368 (OSS)** sólo hace falta si vendes tú directamente a particulares de la
UE por encima de 10.000 € al año; con Steam no.
- Clientes empresa de la UE: factura «Empresa de la UE» y al 349.
