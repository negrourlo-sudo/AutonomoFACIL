# AutónomoFácil

Aplicación de escritorio en Rust para hacer **presupuestos y facturas** en España,
con registro **Verifactu** encadenado, exportación **FacturaE 3.2.2** y un
módulo de **empleados y nóminas** para autónomos con trabajadores y sociedades.

Sin Electron, sin WebView, sin Node y sin toolchain de C: una ventana nativa
(egui/eframe) y un ejecutable.

Versión actual: **0.7.10**. La corrección de una factura emitida se hace con una
**R1 por diferencias**; no se borra ni se anula la original.

[![Última release](https://img.shields.io/github/v/release/negrourlo-sudo/AutonomoFacil?label=release)](https://github.com/negrourlo-sudo/AutonomoFacil/releases/latest)
[![Licencia: CC BY-SA 4.0](https://img.shields.io/badge/licencia-CC%20BY--SA%204.0-lightgrey.svg)](LICENSE)

## Descargar

La versión lista para usar está en **[Releases](https://github.com/negrourlo-sudo/AutonomoFacil/releases/latest)**.
Es una aplicación nativa para **Windows de 64 bits** (Windows 10/11); no necesita
runtime, Node, WebView ni permisos de administrador.

| Fichero | Para qué |
|---|---|
| `AutonomoFacil-0.7.10-Setup.exe` | Instalador por usuario. Crea los accesos y se puede desinstalar desde «Agregar o quitar programas». |
| `AutonomoFacil-0.7.10.exe` | Ejecutable portable. No instala nada; guarda los datos en `%APPDATA%\AutonomoFacil`. |
| `SHA256SUMS.txt` | Huellas SHA-256 de los dos ficheros anteriores. |

### Novedades de la 0.7.10

- El IVA de las líneas del XML **FacturaE** se calcula con redondeo acumulado
  por tipo: la suma de cada grupo coincide exactamente con su cuota agrupada.
- El reparto ya no puede dejar una línea con **IVA negativo**, aunque la suma
  de los redondeos individuales supere el total del grupo.
- Funciona también cuando hay **varios tipos de IVA intercalados** (por
  ejemplo, líneas al 21 % y al 10 % alternadas).
- Se mantienen las cantidades con **tres decimales** de la 0.7.9: `1,255 h`
  no se convierte en `1,250 h`.

El instalador y el ejecutable se publican **sin firma Authenticode**. Windows
puede mostrar «editor desconocido» o SmartScreen; es esperado. Antes de ejecutar,
comprueba la descarga:

```powershell
Get-FileHash .\AutonomoFacil-0.7.10-Setup.exe -Algorithm SHA256
```

El resultado debe coincidir con la línea correspondiente de `SHA256SUMS.txt`.
Tus datos se guardan sólo en tu equipo, en `%APPDATA%\AutonomoFacil`; al
desinstalar se conservan salvo que marques la casilla para borrarlos.

Para compilar desde el código, consulta [Cómo se ejecuta](#cómo-se-ejecuta).

---

## Estado legal: qué hace y qué no

| Pieza | Estado en este programa |
|---|---|
| Registro de facturación con huella SHA-256 encadenada | **Implementado** (`src/verifactu.rs`) |
| QR tributario de cotejo en la factura | **Implementado** (PDF y pantalla); **no verificable** |
| FacturaE 3.2.2 | **Implementado** (`src/facturae.rs`) |
| Facturas rectificativas R1 encadenadas | **Implementado** (por diferencias) |
| Envío de registros a la AEAT (modalidad VERI\*FACTU) | **No implementado** |
| Firma XAdES del FacturaE | **No implementado** |
| Validación contra el XSD oficial | **No hecha** (ver *Límites*) |

### Plazos vigentes (a 18 de noviembre de 2026)

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

Cada factura emitida produce un registro de alta cuya huella es:

```
SHA-256(
  "IDEmisorFactura=<NIF>&NumSerieFactura=<serie+número>&FechaExpedicionFactura=<dd-mm-yyyy>"
  "&TipoFactura=<F1|F2|R1>&CuotaTotal=<x.xx>&ImporteTotal=<x.xx>"
  "&Huella=<huella del registro anterior>&FechaHoraHusoGenRegistro=<ISO-8601 con huso>"
)
```

La anulación **no reutiliza esos ocho campos**: usa cinco y otros nombres.

```
SHA-256(
  "IDEmisorFacturaAnulada=<NIF>&NumSerieFacturaAnulada=<serie+número>"
  "&FechaExpedicionFacturaAnulada=<dd-mm-yyyy>&Huella=<huella anterior>"
  "&FechaHoraHusoGenRegistro=<ISO-8601 con huso>"
)
```

En el primer registro `Huella` va vacía. `CuotaTotal` incluye IVA y recargo de
equivalencia; `ImporteTotal` es base + IVA + recargo y **no resta IRPF**. Los
importes se escriben con punto y dos decimales, la cadena va en UTF-8 y el
resultado en mayúsculas. Al arrancar, el programa **recalcula toda la cadena**
(`verifactu::verificar_cadena`) y avisa si una huella, un número, una fecha, un
tipo, una cuota o un importe no cuadra, si el encadenamiento se rompe, si una
anulación no tiene su factura anulada o si falta el registro de una factura
emitida. Los registros guardados con el algoritmo antiguo (sin campo `version`)
se siguen verificando con su formato para no romper cadenas ya existentes. El
JSON guarda además un campo local `orden` que desempata dos registros generados
en el mismo segundo; no forma parte de la huella ni del XML oficial.

Una factura emitida **no se edita**: se corrige con una **rectificativa R1** por
la diferencia (la original sigue contando y la R1 suma la corrección, positiva o
negativa), o se anula. La anulación genera su propio registro encadenado con
importes a cero. Igual que en la norma.

### Facturas rectificativas R1

En la ficha de una factura emitida, «Rectificar R1» abre una ventana donde se
indica la **diferencia de base** (por ejemplo, `-100,00` para bajar el importe),
el IVA de esa diferencia y el motivo. La app calcula IVA, recargo e IRPF, crea
una factura en la serie `R<aaaa>` con una sola línea y la emite con
`TipoFactura = R1` y `TipoRectificativa = I` (por diferencias). La original no se
anula ni se borra: queda marcada como rectificada y conserva su registro de
alta. El PDF de la R1 se titula «FACTURA RECTIFICATIVA» y cita la original y el
motivo.

El 303, el 347 y el 349 suman la R1 con su signo, así que no hay que excluir la
factura original. Una R1 no se puede volver a rectificar desde la app; si se
anula, la original deja de aparecer como rectificada. FacturaE queda desactivado
para R1 porque el XML correctivo no está implementado.

### Verifactu explicado para usarlo sin sustos

Dentro de la app, el botón **«Cómo funciona Verifactu»** (en Guía rápida,
Ajustes, el asistente y la ficha de cada factura) abre la explicación completa
con preguntas rápidas. En resumen:

- **Qué es.** Un registro de facturación que no se puede cambiar sin que se
  note: cada factura deja una huella SHA-256 encadenada con la anterior. El alta
  usa ocho campos (NIF, número, fecha, tipo, cuota, importe, huella anterior y
  fecha/hora); la anulación usa **cinco** (NIF, número y fecha de la factura
  anulada, huella anterior y fecha/hora), sin tipo, cuota ni importe. Si se
  altera o se borra una intermedia, la cadena se rompe y la app avisa en Inicio.
- **Qué hace la app.** Numera, genera el registro de alta, encadena, crea el QR
  tributario de 34 mm (nivel M, zona de silencio de 2 mm, etiqueta «QR
  tributario:»), lo imprime una sola vez, al principio de la primera página
  —antes del contenido, como pide la AEAT— y verifica toda la cadena al
  arrancar. Al
  anular, añade un registro N con importes a cero y marca la factura como
  anulada; no la borra. En el registro, `CuotaTotal` es IVA + recargo de
  equivalencia e `ImporteTotal` es base + IVA + recargo, **sin restar IRPF**;
  el TOTAL del recibo sí resta la retención.
- **Qué no hace.** **No es un sistema VERI\*FACTU ni un SIF certificado y no
  remite los registros a la AEAT**: la remisión exige certificado cualificado y
  declaración responsable del fabricante. El QR es de cotejo, **no verificable**
  (`ValidarQRNoVerifactu`), y no lleva la leyenda «Factura verificable en la sede
  electrónica de la AEAT». Como no se remite, la Sede puede contestar que no
  consta. Tampoco firma el FacturaE ni sustituye a tu asesor.
- **Cómo usarlo.** Actívalo antes de la primera factura; rellena bien el NIF;
  emite con «Guardar y emitir»; para corregir sin anular, usa «Rectificar R1»;
  para anular, «Anular». Si la cadena no cuadra, no sigas emitiendo: restaura
  una copia.
- **Errores típicos.** Editar una factura emitida, borrarla, activar Verifactu
  tarde o repetir un número. Ninguno se arregla reescribiendo: se corrige con
  una R1, se anula dejando el registro N o se restaura la copia anterior.
- **Plazos.** 1 de enero de 2027 para sociedades y 1 de julio de 2027 para
  autónomos y resto. El posible aplazamiento a 2028 anunciado en octubre de
  2026 no está en el BOE a 12 de noviembre de 2026.

---

## Empleados y nóminas

### 1. Prepara la ficha

En **Datos → Empleados** está la plantilla. La ficha guarda nombre, NIF, NAF,
puesto, contacto, fechas de alta y baja, tipo de contrato, grupo de cotización,
salario bruto anual e IRPF. Los ficheros de datos antiguos se abren sin tocar
nada: si no había empleados, los vectores empiezan vacíos.

- **Fechas:** el alta y la baja son días naturales y se cuentan los dos. Un
alta el día 1 o una baja el último día dan el mes completo; un alta el último
día da un solo día. No hace falta marcar la baja para dejar de generar nóminas:
la fecha ya lo hace.
- **Tipo de contrato:** sólo cambia la cotización por desempleo. Indefinido:
1,55 % del trabajador y 5,50 % de la empresa. Temporal, formación o prácticas:
1,60 % y 6,70 %. No modela bonificaciones ni peculiaridades del contrato.
- **Grupo de cotización:** decide los topes de la base. Los grupos 1 a 7 usan
bases mensuales; los grupos 8 a 11 usan bases diarias, con ajuste a 30 días en
meses completos. No cambia el salario.
- **Salario bruto anual:** es el bruto antes de Seguridad Social e IRPF,
repartido en **12 pagas**. La nómina ordinaria es el anual entre 12. Las pagas
extras, horas extra, variables y demás incidencias se añaden después con el
editor de **Conceptos** de cada borrador.
- **IRPF:** es el porcentaje de retención que se aplica al bruto. No es una
cotización: la Seguridad Social se calcula aparte, sobre la base de cotización.

### 2. Genera la nómina del mes

En la pestaña **Nóminas** elige el año y el mes y pulsa **«Generar nóminas del
mes»**. Se crea una nómina por cada empleado con algún día de alta en ese mes;
no se genera para quien todavía no había entrado ni para los meses posteriores
a su baja. Si ya existe la nómina de ese empleado y mes, no se duplica.

Cada nómina guarda una copia de los datos laborales (nombre, NIF, NAF, puesto y
contrato), los días trabajados y del mes, el bruto, la base de cotización
aplicada, la Seguridad Social del trabajador, el IRPF, el neto, la Seguridad
Social de la empresa y el coste total. También guarda un **expediente de
cálculo** con las entradas, los topes, las líneas de cotización, las fórmulas y
una huella SHA-256 para detectar cambios. La tabla muestra además el IRPF
retenido del trimestre (para el **111**) y del año (para el **190**).

### 3. Añade conceptos e incidencias del mes

Cada fila de la pestaña **Nóminas** tiene el botón **«Conceptos»** mientras la
nómina esté en **Borrador**. La ventana permite introducir:

- **Pagas extras:** importe devengado en el mes. Suma al bruto, a la base de
  cotización (con los topes del grupo) y al IRPF.
- **Horas extra:** importe. Se aplican los tipos generales del paquete; la app
  no calcula la cotización adicional específica de horas extra.
- **Variables y complementos:** bonus, incentivos u otros complementos
  variables. Suman al bruto y a la base.
- **Vacaciones:** días informativos e importe de compensación. Si las
  vacaciones ya están incluidas en el salario, deja el importe a cero; los días
  no reducen el salario por sí solos.
- **Bajas:** días informativos e importe de descuento. El descuento resta del
  bruto y de la base; la app no calcula automáticamente la prestación de IT.
- **Otras deducciones:** anticipos, cuotas sindicales u otras deducciones.
  Restan del neto, pero no del bruto, de la base ni del IRPF.

La ventana muestra en tiempo real el **bruto, la base, el neto y el coste de
empresa** actuales y con los conceptos, con la diferencia. **Guardar y
recalcular** sustituye el cálculo del borrador, actualiza su expediente y deja
registrada una copia de los conceptos en la nómina. En una nómina revisada,
aprobada, pagada o cerrada el botón queda bloqueado: se corrige con un recibo
de diferencias.

### 4. Revisa, aprueba y cierra el recibo

Cada nómina recorre estos estados, siempre hacia delante:

```text
Borrador → Revisada → Aprobada → Pagada → Cerrada
```

- En **Borrador** se puede borrar. A partir de **Revisada** el botón de borrado
  queda bloqueado para conservar el ciclo.
- **Nómina explicada** abre el expediente guardado: entradas, base mínima y
  máxima, cada cotización, IRPF, neto, coste y la huella del cálculo. No
  recalcula ni modifica nada.
- Al llegar a **Cerrada** la nómina y su expediente quedan inmutables. No se
  puede borrar ni cambiar de estado.

### Recibo de diferencias

Si hay que corregir una nómina ya cerrada, no se edita el original. Desde
**Nómina explicada** se pulsa **«Crear recibo de diferencias»** y se indican
sólo los importes de la corrección (bruto, Seguridad Social del trabajador,
IRPF y Seguridad Social de la empresa). Pueden ser positivos o negativos. El
recibo nuevo queda enlazado al original, empieza en **Borrador** y recorre los
mismos estados. El original no se toca.

La tabla muestra los días, el bruto, la Seguridad Social del trabajador, el
IRPF, el neto, el coste de empresa y el estado. Pasa el cursor por las
cabeceras para ver qué significa cada columna y por el bruto para consultar la
base de cotización aplicada. El botón **PDF** descarga un resumen de trabajo:
no es el recibo oficial de salarios ni el boletín de cotización.

### Prorrateo del salario por altas y bajas

El salario mensual se prorratea cuando el alta o la baja caen dentro del mes:

- se usan **días naturales** y se cuentan tanto el día de alta como el de baja;
- el divisor es el número real de días del mes: **28, 29, 30 o 31**;
- alta el día 1 o baja el último día dan el **mes completo**;
- alta el último día o baja el primer día dan **un día**;
- cada mes se prorratea por separado, también al cambiar de año;
- las cotizaciones se calculan sobre la base de cotización y el IRPF sobre el
  bruto prorrateado.

La fórmula del bruto es:

```text
bruto mensual = salario anual / 12
bruto del mes = bruto mensual × días naturales / días reales del mes
bruto con conceptos = bruto del mes + pagas extras + horas extra
                    + variables + compensación de vacaciones - descuento por baja
base de cotización = bruto con conceptos, limitado por el grupo
neto = bruto con conceptos - SS trabajador - IRPF - otras deducciones
coste de empresa = bruto con conceptos + SS empresa
```

El resultado se redondea al céntimo. Si el mes está completo se usa el bruto
mensual entero. El bruto con conceptos no puede quedar por debajo de cero. Las nóminas creadas antes de esta versión no guardan los días ni
el expediente y conservan su importe; el PDF muestra los días cuando están
disponibles.

### Bases de cotización

Cada empleado tiene un **grupo de cotización** del 1 al 11. La nómina toma el
bruto prorrateado y lo limita entre la base mínima y la máxima de ese grupo y
ejercicio; la base aplicada queda guardada en la propia nómina. Las
cotizaciones se calculan sobre esa base y el IRPF sobre el bruto. La base puede
ser **mayor que el bruto** si se aplica el mínimo, o **menor** si se aplica el
máximo.

La tabla incluye los ejercicios **2024, 2025 y 2026**. Para 2026:

| Grupo | Base mínima | Base máxima |
|---|---:|---:|
| 1 | 1.989,30 €/mes | 5.101,20 €/mes |
| 2 | 1.649,70 €/mes | 5.101,20 €/mes |
| 3 | 1.435,20 €/mes | 5.101,20 €/mes |
| 4 a 7 | 1.424,40 €/mes | 5.101,20 €/mes |
| 8 a 11 | 47,48 €/día | 170,04 €/día |

| Grupo | Mes completo | Mes incompleto |
|---|---|---|
| 1 a 7 | límite mensual | límite mensual × días naturales / 30 |
| 8 a 11 | límite diario × 30 (mín. 1.424,40 / máx. 5.101,20) | límite diario × días de alta |

La app calcula con salario mensual. En los grupos 8 a 11, el mes completo se
ajusta a 30 días para respetar el tope mensual: 170,04 × 30 = **5.101,20 €**,
no 170,04 × 31. En meses incompletos se usa la base diaria por cada día de
alta. La app no modela la retribución diaria pura.

**Ejemplo:** grupo 7, 1.500 €/mes, alta el 15/02/2026 (14 de 28 días): bruto
750 € y base mínima 1.424,40 × 14 / 30 = **664,72 €**, por lo que la base es
750 €. Grupo 8, mismos 14 días: mínimo 47,48 × 14 = **664,72 €** y máximo
170,04 × 14 = **2.380,56 €**; en mes completo, máximo **5.101,20 €**.

Para ejercicios sin tabla cargada no se aplican topes y se conserva el bruto
como base. Los datos antiguos sin grupo se interpretan como **grupo 7**
(auxiliares administrativos) hasta que se cambie en la ficha; las nóminas
antiguas que no guardaban base conservan una base 0 y no se recalculan.

### Paquetes anuales de parámetros

Los tipos y las bases no están escritos en el motor: cada ejercicio tiene un
paquete JSON versionado en `parametros/nomina/2024.json`,
`parametros/nomina/2025.json` y `parametros/nomina/2026.json`. Cada paquete
incluye su año, versión, fecha de vigencia, fuente y URL oficial, las bases de
los once grupos, los tipos generales y la estimación de AT/EP. El programa
embebe esos ficheros para seguir siendo un único ejecutable, pero el cálculo
sólo lee el paquete del ejercicio.

Cada nómina guarda una **copia del paquete usado** en su campo `paquete`; la
vista «Nómina explicada» y el PDF muestran su versión y fuente. Así una nómina
antigua no cambia si en el futuro se publica otra versión del paquete. Las
nóminas generadas antes de esta versión no tienen paquete y se muestran como
tales; no se recalculan.

`parametros/nomina/casos-validados.json` contiene casos contrastados con las
órdenes de cotización y las tablas oficiales de 2024, 2025 y 2026. La suite de
pruebas ejecuta esos casos contra el motor y comprueba bases, cotización del
trabajador, cotización de la empresa sin AT/EP y total con la estimación de
AT/EP. Las fuentes oficiales están enlazadas en cada caso.

### 111 y 190 desde las nóminas

Desde la propia pestaña **Nóminas** (y también desde **Modelos e impuestos**)
se pueden preparar los dos modelos:

- **111 del trimestre**: agrupa las nóminas del trimestre por trabajador.
  Casilla 01: número de perceptores; 02: percepciones íntegras (el **bruto** de
  las nóminas); 03: retenciones de IRPF. La **base de cotización no se usa** en
  el 111. El detalle por perceptor es informativo.
- **190 del año**: suma todas las nóminas del ejercicio. Casilla 01: número de
  percepciones (nóminas, no personas); 02: total de percepciones íntegras; 03:
  total de retenciones; P: personas distintas, sólo informativo. Incluye el
  detalle anual por perceptor con NIF, bruto y retención.

Cada botón escribe un **PDF borrador** y un **JSON** en
`export\<empresa>\Modelos\`, con el modelo, el año, el periodo y la empresa en el
nombre. El PDF lleva las casillas, el resumen y una tabla de detalle; el JSON
sirve para revisarlo o pasárselo al gestor. Los recibos de diferencias se
suman al periodo en el que se registran, con el signo de la corrección.

**Límites importantes:** el cálculo usa **sólo las nóminas generadas en
AutónomoFácil**. No incluye facturas de profesionales con retención (casillas
07–09 del 111), retribuciones en especie, regularizaciones, atrasos, dietas ni
otras rentas. Tampoco presenta nada en la AEAT: son borradores para revisar y
teclear o importar en la Sede. Antes de presentar, comprueba que cada perceptor
tiene un NIF válido y que el 190 cuadra con los cuatro 111 del año.

La estimación usa 12 pagas y el paquete anual del ejercicio. Tipos generales
del Régimen General:

| Ejercicio | Trabajador indefinido | Trabajador temporal | Empresa indefinido* | Empresa temporal* | MEI trabajador / empresa |
|---|---:|---:|---:|---:|---:|
| 2024 | 6,47 % | 6,52 % | 31,98 % | 33,18 % | 0,12 % / 0,58 % |
| 2025 | 6,48 % | 6,53 % | 32,07 % | 33,27 % | 0,13 % / 0,67 % |
| 2026 | 6,50 % | 6,55 % | 32,15 % | 33,35 % | 0,15 % / 0,75 % |

\* Incluye la estimación de AT/EP del 1,50 %, que no es un tipo general: la
tarifa oficial depende de la actividad. El paquete marca esta estimación y la
vista de la nómina lo advierte.

La Seguridad Social del trabajador y de la empresa se calcula sobre la **base
de cotización**, no sobre el bruto. El IRPF se calcula sobre el **bruto**. Las
fórmulas del recibo son:

```text
SS trabajador = base de cotización × tipo del paquete (6,50 % indefinido en 2026)
SS empresa = base de cotización × tipo del paquete (32,15 % indefinido en 2026)*
IRPF = bruto × porcentaje de la ficha
neto = bruto - SS trabajador - IRPF
coste de empresa = bruto + SS empresa
```

\* La empresa incluye la estimación de AT/EP del paquete. Los porcentajes no
incluyen bonificaciones, tramos personales, regularizaciones ni el boletín de
cotización. **No es una nómina oficial**: sirve para presupuestar el coste de
una persona y anticipar las retenciones. El recibo y la presentación se revisan
con el asesor o con el programa de nóminas.

El **coste de empresa** de cada nómina se suma por devengo a **Contabilidad** y
al cálculo del **130**. Si el pago bancario de la nómina ya está registrado como
gasto, marca la casilla **«Excluir: ya está en el coste de personal»** o usa la
categoría **«Nóminas»**: ese apunte no se vuelve a sumar en gastos, IVA ni 347.
Así el coste de personal no se cuenta dos veces.

---

## Cómo se ejecuta

Requisitos: Rust con el target `x86_64-pc-windows-gnu` y MinGW (para enlazar).

```bash
export PATH="$PWD/.toolchain/mingw64/bin:$HOME/.cargo/bin:$PATH"
cargo run --release
```

Datos en `%APPDATA%\AutonomoFacil\datos.json` (escritura atómica + `.bak`).
Exportaciones organizadas en `%APPDATA%\AutonomoFacil\export\<empresa>\`: `Facturas`, `Presupuestos` y `Proformas` guardan una subcarpeta por cliente (el PDF y el FacturaE del mismo documento van juntos); `Nominas`, una por empleado; y `Modelos` y `Contabilidad` guardan sus PDF, JSON, CSV o TXT directamente en su carpeta.

Pruebas:

```bash
cargo test
# La prueba de carga masiva está ignorada por defecto:
cargo test --release genera_carga_masiva -- --ignored --nocapture
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
| `AutonomoFacil-NSIS.exe` | El instalador. Asistente en español, **por usuario**: no pide administrador. |

Para publicar una release, renombra los artefactos a
`AutonomoFacil-<versión>-Setup.exe` y `AutonomoFacil-<versión>.exe`, genera
`SHA256SUMS.txt` y adjunta los tres a la release de GitHub. Las notas deben
repetir la versión, la licencia y la nota de firma.

Hacen falta NSIS 3 y Python 3:

```bash
winget install --id NSIS.NSIS -e --accept-package-agreements --accept-source-agreements
```

### Qué hace el instalador

1. Comprueba que **AutónomoFácil esté cerrado**. Si está abierto, pide cerrarlo
   y reintentar; en modo silencioso aborta en vez de tocar un ejecutable en uso.
2. Si ya hay una instalación, guarda una **copia previa** del programa en
   `%TEMP%\AutonomoFacil-backup-<versión>`. Si falla la copia de ficheros,
   cancela antes de modificar nada.
3. Copia la aplicación a `%LOCALAPPDATA%\Programs\AutonomoFacil`.
4. Crea accesos directos en el menú Inicio y en el escritorio, usando el
   **icono incrustado en el .exe**; también deja `icono.ico` como respaldo.
5. Se registra en **Agregar o quitar programas** de Windows (HKCU).
6. Deja un `Desinstalar.exe` que borra accesos, registro y carpeta del programa.
7. Si se cancela o falla la instalación después de empezar, **restaura la copia
   anterior** y, si la instalación era nueva, elimina sólo los ficheros que había
   creado. Los datos de `%APPDATA%` no se tocan.
8. Al desinstalar, muestra una casilla **sin marcar** para borrar también datos,
   exportaciones, nóminas, copias y la carpeta antigua `Facturacion` si existe.
9. Muestra el aviso legal y la licencia CC BY-SA 4.0 antes de instalar.

El instalador no borra la carpeta elegida con `RMDir /r`: en el rollback sólo se
tocan los tres ficheros de la aplicación (ejecutable, icono y desinstalador). Si
ya existían en una carpeta compartida, se guardan antes y se restauran; los
demás ficheros de la carpeta no se modifican.

**Tus datos no se borran por defecto al desinstalar**: quedan en
`%APPDATA%\AutonomoFacil`. El desinstalador permite borrarlos de forma opcional
marcando esa casilla; si se marca, también elimina la carpeta antigua
`%APPDATA%\Facturacion` y la eliminación es permanente. En una desinstalación
silenciosa (`/S`) la página no se muestra y los datos se conservan.

### Firma Authenticode (opcional)

El script firma `dist/autonomofacil.exe` **antes** de incrustarlo en el
instalador y vuelve a firmar `dist/AutonomoFacil-NSIS.exe` al final. Sin
certificado no falla: genera el paquete sin firmar y muestra un aviso. La firma
es opcional para compilar, pero necesaria para distribuir sin el aviso de
“editor desconocido”.

Necesitas un certificado de firma de código **OV o EV** de una CA reconocida y
`signtool.exe` del Windows SDK. Hay dos formas de decirle al script dónde está
la clave; **elige una**, no las dos.

#### A. Certificado del almacén de Windows (token, HSM o nube)

Es la ruta para los certificados actuales, cuya clave privada no se puede
exportar y vive en un token USB, un HSM o un servicio cloud. Funciona si el
fabricante instala el certificado en el almacén de Windows mediante su
middleware o un adaptador CNG/KSP; por ejemplo, un token SafeNet/YubiKey o un
adaptador cloud como eSigner CKA. Después `signtool` firma con `/sha1` y el
token/HSM/servicio pide el PIN, OTP o confirmación.

1. Instala el software del token/HSM/servicio, conecta el dispositivo y
   comprueba que el certificado tiene la EKU **Code Signing**.
2. Localiza la huella SHA-1 del certificado.

```powershell
Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert |
  Format-Table Subject, Thumbprint, NotAfter
```

Si está en el almacén del equipo:

```powershell
Get-ChildItem Cert:\LocalMachine\My -CodeSigningCert |
  Format-Table Subject, Thumbprint, NotAfter
```

3. Exporta la huella y, si está en el almacén de equipo, el almacén:

```bash
export AUTONOMOFACIL_CERT_SHA1="0123456789ABCDEF0123456789ABCDEF01234567"
export AUTONOMOFACIL_CERT_STORE="usuario"   # o "maquina"
bash empaquetar-nsis.sh
```

La huella puede copiarse con espacios o en minúsculas: el script la normaliza.
Debe ser una huella **SHA-1 de 40 caracteres hexadecimales**, que es la que
selecciona `/sha1`; una huella SHA-256 de 64 caracteres no vale.

#### B. PFX exportable (certificados antiguos)

Si tu CA te entrega un PFX con clave privada exportable:

```bash
export AUTONOMOFACIL_CERT_PFX="/ruta/segura/AutonomoFacil.pfx"
export AUTONOMOFACIL_CERT_PASSWORD="la-contraseña-del-pfx"
bash empaquetar-nsis.sh
```

El PFX y su contraseña **no se guardan en el repositorio** ni se imprimen por
pantalla; mantenlos fuera del proyecto. Muchas CA actuales ya no entregan PFX:
si el certificado está en token/HSM, usa la ruta A.

#### Variables

| Variable | Cuándo se usa | Contenido |
|---|---|---|
| `AUTONOMOFACIL_CERT_SHA1` | Ruta A | Huella SHA-1 de 40 caracteres; puede llevar espacios o minúsculas. |
| `AUTONOMOFACIL_CERT_STORE` | Ruta A | `usuario` (por defecto) o `maquina`; con `maquina` se añade `/sm`. |
| `AUTONOMOFACIL_CERT_PFX` | Ruta B | Ruta al PFX exportable. |
| `AUTONOMOFACIL_CERT_PASSWORD` | Ruta B | Contraseña del PFX. |
| `AUTONOMOFACIL_TIMESTAMP_URL` | Ambas | Sello de tiempo; por defecto `http://timestamp.digicert.com`. |
| `AUTONOMOFACIL_SIGNTOOL` | Ambas | Ruta completa a `signtool.exe` si no está en el PATH. |

#### Flujo completo

1. `empaquetar-nsis.sh` compila `dist/autonomofacil.exe`.
2. Lo firma con el certificado elegido y comprueba la firma con
   `signtool verify /pa`.
3. Compila el instalador NSIS con ese ejecutable ya firmado.
4. Firma `dist/AutonomoFacil-NSIS.exe` y vuelve a verificar.
5. Muestra en pantalla el modo usado (almacén o PFX), pero nunca la contraseña.

El orden importa: si firmas sólo el instalador, el `.exe` que lleva dentro
sigue sin firma. No ejecutes `empaquetar-nsis.sh` otra vez sin certificado
después de firmar a mano, porque reemplazaría la aplicación firmada por una
sin firmar.

#### Comprobar la firma

```bash
signtool verify /pa /v dist/autonomofacil.exe
signtool verify /pa /v dist/AutonomoFacil-NSIS.exe
```

O desde PowerShell:

```powershell
Get-AuthenticodeSignature .\dist\AutonomoFacil-NSIS.exe |
  Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

Debe aparecer `Status: Valid`, el certificado del firmante y el sello de
tiempo. El sello permite que la firma siga siendo válida cuando caduque el
certificado.

#### Límites y avisos

- Un servicio cloud que sólo firme mediante su API y no publique el certificado
  en el almacén de Windows no funciona con `/sha1`; usa la herramienta del
  proveedor. Es el caso, por ejemplo, de Azure Trusted Signing o de algunas
  modalidades de KeyLocker sin integración CNG.
- Un certificado OV puede seguir mostrando avisos de SmartScreen hasta que
  Windows acumule reputación; uno EV suele ayudar antes, pero no lo garantiza.
- La firma no cambia el contenido del instalador: sólo acredita quién lo firmó.
- Si no defines ninguna de las dos rutas, el paquete se genera sin firma y
  Windows puede mostrar “editor desconocido”.

### Icono del ejecutable

`build.rs` compila `recursos/icono.rc` con `windres` y enlaza el icono en el PE.
Busca `WINDRES`, el toolchain local `.toolchain/mingw64/bin` y las rutas
habituales de MinGW/MSYS2; si no lo encuentra, avisa y el build sigue sin icono.
Los accesos directos y `DisplayIcon` apuntan al `.exe` (índice 0), no al `.ico`
suelto; el `.ico` se conserva como respaldo y para el desinstalador.

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

- **Actualizaciones.** No hay comprobación de versión ni autoactualización.
- **Reputación de SmartScreen.** Aunque el paquete esté firmado, un certificado
  OV nuevo puede tardar en ganar reputación; SmartScreen puede seguir avisando.

---

## Arquitectura

```
src/model.rs      Dominio: dinero en céntimos, líneas, totales, numeración, R1
src/store.rs      Persistencia JSON atómica con copia de seguridad
src/verifactu.rs  Huella encadenada, QR, verificación de la cadena
src/facturae.rs   XML FacturaE 3.2.2
src/contab.rs     Libro, resumen y análisis por cliente
src/empleados.rs  Empleados, contratos, nóminas y retenciones 111/190
src/modelos.rs    Cálculos de 303, 130/131, 390, 347 y 349
src/fiscal.rs     Calendario de obligaciones y plazos
src/presentacion.rs Guías de presentación de cada modelo
src/asistente.rs  Asistente de puesta en marcha y consejos
src/pdf.rs        Generador de PDF propio + QR dibujado en el papel
src/ui.rs         Interfaz egui: listas, editor, fichas, ajustes
```

Decisiones que conviene conocer:

- **Dinero en `i64` de céntimos.** Nunca `f64`. Las cantidades van en
  milésimas, así que 1,255 h y 0,5 ud son exactas. El redondeo es al céntimo
  más cercano; en FacturaE se usa el redondeo acumulado por tipo de IVA, de
  modo que cada grupo de líneas suma exactamente su cuota agrupada.
- **Un solo fichero JSON en vez de SQLite.** Evita una dependencia con
  compilador de C y hace los datos inspeccionables y copiables. Para miles de
  facturas es de sobra; el modelo está separado para migrar a SQL sin tocar la
  interfaz.
- **PDF a mano.** Catálogo, árbol de páginas, dos fuentes base-14 y WinAnsi
  (cubre á é í ó ú ñ ü ¿ ¡ €). Cero dependencias, cero binarios nativos.
- **El cliente se congela al emitir.** Editar después su ficha no cambia una
  factura ya emitida.

---

## Accesibilidad

Esta aplicación es **nativa de escritorio** (egui/eframe): no incluye una interfaz web, HTML ni un servidor. Por eso **WCAG y WAI-ARIA no son estándares aplicables tal cual** y no se puede afirmar que el proyecto «pase WCAG»: WAI-ARIA necesita un árbol DOM y aquí no existe. El equivalente nativo es exponer la interfaz a la API de accesibilidad del sistema; en Windows, UI Automation mediante AccessKit.

Lo que sí incorpora esta versión:

- **AccessKit activado** en eframe (`Cargo.toml`). Windows puede exponer botones, campos, listas y la navegación a Narrador/NVDA a través de UI Automation.
- **Navegación lateral con nombre y estado** para lectores de pantalla; los iconos de ayuda anuncian su tema.
- **Etiquetas de formulario asociadas** a los campos que usan el componente común `campo_h`.
- **Zoom de interfaz** al 80–160 % desde la barra superior; el tamaño se guarda.
- **Perfil de rendimiento bajo** sin animaciones ni sombras, útil para reducir movimiento; se puede elegir en Ajustes o con `AUTONOMOFACIL_PERFIL=bajo`.
- Atajos existentes `Ctrl+S` (guardar), `Ctrl+Enter` (guardar y emitir) y `Esc` para cancelar diálogos.

**Pendiente de una auditoría real:** no se ha probado con Narrador, NVDA ni VoiceOver, no hay un recorrido de foco certificado y los PDF exportados **no están etiquetados como PDF/UA** (el generador PDF es propio y no incluye árbol de estructura). Para accesibilidad web habría que construir una interfaz HTML separada; no se ha hecho.

---

## Límites conocidos

1. **Sin firma XAdES.** Un FacturaE sin firmar no vale para FACe ni para
   intercambio B2B real. Falta firma con certificado.
2. **Sin validación XSD.** No hay validador incluido; el orden de elementos
   está calcado de un documento validado contra `Facturaev3_2_2.xsd`, pero
   conviene validar el XML con `xmllint` u otra herramienta antes de usarlo.
3. **Sin remisión a la AEAT.** No hay cliente SOAP, ni certificado, ni cola de
   reintentos, ni registro de eventos. Por eso el QR es de cotejo no verificable
   y no se usa la leyenda de factura verificable.
4. **Sin series por ejercicio automáticas** más allá del año natural, ni
   IGIC/Canarias. Las rectificativas R1 son por diferencias; no se generan
   sustitutivas (R1 con `TipoRectificativa = S`).
5. **Un solo usuario, una sola empresa.** No hay multi-tenant ni control de
   acceso.

---

## Qué haría falta para uso real

- Firma XAdES del XML con certificado del emisor.
- Validador XSD en los tests (con el esquema oficial versionado).
- Cliente SOAP de remisión + registro de eventos + política de reintentos.
- Facturas recapitulativas.
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

## Proformas y contabilidad

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
- El **coste de personal** de las nóminas se suma automáticamente a los gastos y
  al 130; los pagos bancarios marcados como personal no se duplican.
- Calcula **IVA a ingresar** (modelo 303: repercutido − soportado) e **IRPF 130
  estimado** (20 % del rendimiento neto menos las retenciones practicadas).
- **Libro de ingresos y gastos** exportable a CSV con `;` y BOM UTF-8 en
  `%APPDATA%\AutonomoFacil\export\<empresa>\Contabilidad\Libro-<año>-<empresa>.csv`.
- Es una estimación informativa: no sustituye a la asesoría fiscal ni cubre
  compensaciones de trimestres anteriores, bienes de inversión o regímenes
  especiales.

## Cobros, copias y cierre de ejercicio

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

- «Crear copia ahora» escribe `datos-<empresa>-AAAAMMDD-HHMMSS.json` en
  `%APPDATA%\AutonomoFacil\copias`.
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

## Autorrelleno y guía rápida

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
`%APPDATA%\AutonomoFacil\export\<empresa>\Contabilidad\Informe-<año>-<empresa>.txt`
con las cifras anuales, las de cada trimestre, el 347 y el 349, y lo abre junto
a su carpeta.

## Actividad, IVA internacional y obligaciones

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
190, 115, 180 y, en venta digital, el 369 y el 035) con plazo y explicación. Cada uno se
marca por trimestres o una vez al año y el estado se guarda en `datos.json`.
Debajo, la guía cambia según la actividad.

### Autónomo o sociedad: los modelos cambian

Marca en Ajustes si facturas como persona física o como sociedad. Si eres
sociedad, la lista quita la Renta (100) y los pagos a cuenta 130/131, y añade el
**Impuesto de Sociedades (200)** y su **pago fraccionado (202)**; el resto de
modelos (303, 390, 347, 349, 111, 190…) sigue igual. El asistente lo configura
solo al elegir «Sociedad» y el resumen del último paso lo explica.

### Perfil: qué falta y por qué

Inicio muestra una tarjeta «Tu perfil, listo para facturar» con los seis datos
que evitan problemas al emitir y presentar: nombre, NIF, dirección fiscal,
email, IBAN y régimen/actividad. Cada línea explica al pasar el ratón por qué
hace falta, y los botones llevan a Ajustes o repiten el asistente. Con los seis
completos, la tarjeta deja paso a un «Perfil completo».

### Vender juegos en Steam

- Valve es el **vendedor** frente al jugador: cobra, liquida el IVA o sales tax y
te paga tu parte. Tú facturas a **Valve Corporation (EE. UU.)**.
- Esa factura va como **«Fuera de la UE (exportación)»**: sin IVA, sin retención
y fuera del 349. En el 303 va como operación sin IVA; el bruto es ingreso de
actividad económica en el 130 y en la Renta.
- Valve clasifica el pago como *royalty* y retiene un 30 % en EE. UU. si no
tiene tu **W-8BEN** con NIF; con él aplica el convenio. Guarda el **1042-S**
para restar lo retenido en la Renta.- El **369 (OSS)** sólo hace falta si vendes tú directamente a particulares de la
  UE por encima de 10.000 € al año, con alta previa en el formulario **035**; con
  Steam no, porque Valve es el vendedor.
- Clientes empresa de la UE: factura «Empresa de la UE» y al 349.
## Venta digital: qué modelo toca según el cliente

La regla no depende del producto, sino de **quién compra y dónde está**:

- **Particular en España**: IVA español del 21 % y va al **303**. Sin 369.
- **Particular de la UE, hasta 10.000 € al año** (sumando todos los países, sin
  IVA): puedes seguir aplicando el 21 % español y declararlo en el **303**.
- **Particular de la UE, por encima de 10.000 €**: el IVA es el del país del
  cliente y se declara en el **369** (OSS), trimestral. Antes hay que darse de
  alta con el formulario **035**; no hace falta registrarse en cada país.
- **Empresa de la UE con NIF-IVA válido en VIES**: factura sin IVA con
  inversión del sujeto pasivo; esa base va al **349**, no al 369.
- **Cliente de fuera de la UE** (particular o empresa): exportación, sin IVA
  español; va al **303** como operación sin IVA y no entra en 349 ni 369.
- **Marketplace** (Steam, Google Play, Amazon, App Store): la plataforma es el
  vendedor frente al cliente y liquida el IVA; tú le facturas a ella y por esas
  ventas no presentas el 369.

El umbral de 10.000 € se cuenta por año natural y sin IVA. Si lo superas a mitad
de año, el 369 se aplica desde ese momento; si estás cerca, conviene revisarlo
económicamente antes de decidir. La app todavía no calcula el 369: te explica
cuándo te toca, te lleva a la página oficial y deja a mano el total de ventas
sin IVA; el desglose por país se teclea en la Sede, que aplica los tipos.

### Vender solo por plataformas

Si tus juegos se venden únicamente en Steam, Apple App Store, Google Play o
Epic Games, tú no vendes a los jugadores: le facturas a la plataforma. Ella
cobra y liquida el IVA del consumidor final, así que **no presentas el 369**.
Tus modelos son los de una venta B2B:

- **Plataforma en la UE** (Apple o Google, Irlanda): factura intracomunitaria a
  su NIF-IVA, al 0 % con la cláusula de inversión del sujeto pasivo. Va al
  **349** y la base se declara en el **303, casilla 59** (operaciones no sujetas
  por reglas de localización). Necesitas el alta en el **ROI** con el 036/037.
- **Plataforma fuera de la UE** (Valve, Epic, EE. UU.): exportación de
  servicios, sin IVA. Va al **303, casilla 59**, y no entra en el 349.
- En ambos casos el IVA de tus gastos (equipo, software, internet) se deduce en
  el 303, y la retención de EE. UU. se documenta con el W-8BEN y el 1042-S.

| Sede de la plataforma | 303 | 349 | IRPF |
|---|---|---|---|
| UE (Apple, Google — Irlanda) | Sí · casilla 59, informativo | **Sí**, obligatorio con su NIF-IVA | 130 si eres persona física; 200/202 si eres sociedad |
| Fuera de la UE (Valve, Epic, itch.io) | Sí · casilla 59, informativo | **No** | 130 si eres persona física; 200/202 si eres sociedad |

El 349 es exclusivo de operaciones con la UE: una plataforma de EE. UU. nunca
entra en él. El 303 se presenta igual, aunque esas ventas no lleven IVA, porque
ese mismo 303 es el que te deja deducir el IVA de tus gastos.

## Rutina del año

La app no sustituye a un asesor, pero sí deja claro qué toca y cuándo:

- **Cada trimestre** (hasta el día 20 del mes siguiente): cierra facturas y
gastos, presenta el **303** y, si eres persona física, el **130/131**. Con **SII**
el 303 pasa a ser **mensual**, pero el 130/131 sigue trimestral. En recargo de
equivalencia no hay 303 (sólo el 309 si compras a la UE o hay inversión del
sujeto pasivo), pero el **130/131 sigue** si eres persona física. Las
**sociedades** no presentan 130/131.
- **Enero**: 4T y resumen anual de IVA (**390**, si no estás en SII ni
exonerado); **190** y **180** antes del 31 si tienes nóminas o alquiler con
retención.
- **Febrero**: **347** si con un mismo cliente o proveedor superaste 3.005,06 €
con IVA en el año.
- **Abril–junio**: Renta (**100**); las retenciones y los 130 pagados se restan.
- **Sociedades**: **202** en abril, octubre y diciembre; **200** en julio.
- **Venta digital a particulares de la UE**: **369** trimestral por encima de
10.000 € al año (mensual en IOSS), con alta previa en el **035**.
- **Siempre**: numeración sin saltos, rectificativa para corregir (nunca borrar
una emitida) y facturas, justificantes y extractos guardados cuatro años.

Cada modelo listado en la guía tiene su ficha de presentación con enlace a la
Sede, pasos, si admite fichero y el aviso que más se atasca; hay un test que
falla si se añade un modelo sin ficha.

## Modelos rellenados

En la guía, cada modelo que la app sabe calcular (**303, 130, 390, 347 y
349**) tiene su botón **«Rellenar modelo»**. Los trimestrales (303, 130, 349)
llevan además selector de periodo; los anuales (390, 347) se calculan del año
entero. Genera dos ficheros en
`%APPDATA%\AutonomoFacil\export\<empresa>\Modelos\` y abre el PDF y su carpeta:

- `Modelo-303-2026-1T-<empresa>.pdf`: las casillas del modelo con los importes
  ya calculados (devengado por tipo, deducible, resultado). Es un **borrador**
  para revisar, no un documento oficial de la AEAT.
- `Modelo-303-2026-1T-<empresa>.json`: las mismas casillas en formato máquina
  (`{"numero":"69","valor":12345,"unidad":"cents"}`), para pegarlas en otro
  programa o pasárselas a un robot.
- Los anuales se llaman `Modelo-390-2026-anual-<empresa>.pdf` y
  `Modelo-347-2026-anual-<empresa>.pdf`.

Casillas contrastadas con las instrucciones de la AEAT (ejercicio 2026):

- **303**: 01/02/03 (4 %), 04/05/06 (10 %), 07/08/09 (21 %), 150/151/152
  (0 %), 27 total cuota devengada, 28/29 y 45 deducible, 46 resultado del
  régimen general, 64 suma de resultados y **69 resultado** (71 es la
  liquidación tras compensar y devolver).
- **130**: 01 ingresos acumulados, 02 gastos, 03 rendimiento neto, 04 el
  20 %, 05 pagos anteriores, 06 retenciones del trimestre, 07 resultado y
  **19 resultado final** (con las casillas 13 a 18 en blanco, 12 y 19
  coinciden con 07).
- **390**: numeración propia, no la del 303: 01 a 06 (base y cuota al 4,
  10 y 21 %), 33/34 totales, 48/49 y 64 deducible, 65 resultado del régimen
  general, 84 suma de resultados y 86 resultado de la liquidación.
- **347**: un renglón por cliente y otro por proveedor que supere 3.005,06 €
  con IVA, con el total del año.
- **349**: un renglón por NIF-IVA de cliente con la base intracomunitaria del
  trimestre y el total.

### Entenderlo de un vistazo

El botón dice **«Preparar el modelo»** y debajo aparece una frase en
castellano llano: «Sale a ingresar 1.234,56 EUR: 2.000,00 EUR de IVA
repercutido menos 765,44 EUR deducible.» Esa misma frase va destacada en la
cabecera del PDF, encima de las casillas, para que el documento se entienda
solo. El aviso al terminar dice qué se ha generado y para qué sirve cada
fichero.

### Vista «Modelos e impuestos»

En el menú lateral, dentro de DATOS, hay una vista propia para los modelos:

- Selector de **ejercicio** (◀ 2026 ▶) y trimestre activo.
- **Tus datos fiscales** tal y como saldrán en el PDF: nombre, NIF,
dirección completa, provincia, email, IBAN y actividad, con botón para
editarlos.
- **Lo que falta**, en lenguaje llano: nombre, NIF sin los 9 caracteres,
dirección incompleta, IBAN, email, años sin facturas o sin gastos, actividad
en «Otra»… Cuando está todo, lo dice en verde; si no, cada punto con su
motivo y un botón a Ajustes.
- La **lista de modelos del año** con su plazo (y aviso en rojo si venció sin
  presentar), casillas para marcar lo presentado, selector de trimestre en los
  trimestrales, botón **«Preparar el modelo»**, la frase de resumen («Sale a
  liquidar…») y atajos para abrir la Sede de la AEAT o ver el paso a paso.
- Cada tarjeta lleva una línea **«Por qué: …»** calculada con tu perfil, no un
  texto genérico: explica de dónde sale ese modelo en tu caso.

### Por qué te toca cada modelo

El «por qué» cambia con el tipo de titular y el régimen, y se ve en la tarjeta,
en el asistente y en Ajustes:

| Si eres… | Lo que verás y por qué |
|---|---|
| Autónomo persona física | 130 (o 131 si estás en módulos), Renta 100 y ningún 200/202: tributas por IRPF. |
| Sociedad | 200 y 202, sin 100 ni 130/131: tributas por el Impuesto de Sociedades. |
| Recargo de equivalencia | 309 y no 303/390: el IVA lo repercute tu proveedor; el 130/131 sigue si eres persona física. |
| SII | 303 mensual con casillas 01–12 y sin 390: el resumen anual va dentro del SII. |

El mismo «por qué» aparece al elegir titular y recargo en el asistente, y en
Ajustes al cambiar los datos: cambiar de autónomo a sociedad no es un detalle,
cambia la lista entera de modelos.

## El asistente de puesta en marcha

La primera vez que se abre el programa (o mientras no se haya completado) sale
un asistente de cuatro pasos:

1. **Tú** — nombre, NIF (se comprueba la letra de control) y si eres autónomo
   persona física o sociedad.
2. **Tu actividad** — a qué te dedicas, régimen del IRPF (directa
   simplificada, normal o módulos) y una previsión de facturación. Marca
   también si tienes empleados, pagas alquiler de local o estás en el SII.
3. **Tus clientes** — particulares, empresas, profesionales que retienen,
   comercios minoristas, empresas de la UE o clientes con inversión del sujeto
   pasivo. Y si tú eres minorista en recargo de equivalencia.
4. **Tu plan** — resumen en una frase de cómo tributas, IVA y retención que
   aplicas, lista de modelos con plazos y consejos para no equivocarte.

El asistente es una **ventana emergente** centrada, con el resto de la app
oscurecida detrás: no se puede tocar nada de fuera hasta terminar o cerrar. Los
campos se rellenan dentro del propio diálogo, con la barra de pasos arriba y los
botones Anterior / Siguiente / Guardar y empezar abajo, junto a la voz. Si el
contenido no cabe (el plan final es largo), el paso tiene su propio
desplazamiento.

Al pulsar **Guardar y empezar** se rellenan los datos de la empresa, el IVA y
el IRPF por defecto, el régimen y las casillas de recargo/SII, y el programa
abre el inicio. `Más tarde` vuelve al inicio sin guardar y el asistente vuelve
a salir en el siguiente arranque.

Se puede repetir cuando haga falta desde **Asistente** en la barra lateral, y
mientras no esté hecho el inicio muestra una tarjeta para abrirlo. El motor
(`src/asistente.rs`) reutiliza las mismas obligaciones que la vista de modelos,
así que el asistente y el resto del programa nunca dicen cosas distintas.

### Editor de facturas: comprobación y atajos

- **Validación en vivo**: bajo los totales aparece «Falta poco (n)» con la lista
  de lo que queda (serie, líneas, fechas, descripciones). Si todo está bien, se
  pone en verde: «Todo listo: puedes guardar el borrador o emitir».
- **Borrador guardable a medias**: basta con que fechas y números estén bien
  escritos; puedes guardarlo sin serie o sin líneas y terminarlo después. Si un
  número está mal, el botón se desactiva para no guardar ceros sin querer.
- **Atajos**: `Ctrl+S` guarda el borrador, `Ctrl+Enter` emite cuando no falta
  nada.
- **Autocompletado de artículos**: al escribir una descripción que empieza como
  un artículo guardado (3 letras o más), se rellenan solos el precio, el IVA y la
  cantidad.
- **IVA automático por operación**: al pasar a intracomunitaria, exportación o
  exenta las líneas van al 0 %; al volver a nacional recuperan tu IVA por
  defecto.
- **Duplicar**: en el panel de detalle, «Duplicar» crea un borrador nuevo con el
  mismo cliente y líneas, fecha de hoy y vencimiento según tus días por defecto.
  Es la vía rápida para una factura repetitiva.
- **Aviso de cambios sin guardar**: al pulsar Cancelar o al cambiar de pantalla
  con un documento abierto, un diálogo ofrece **Guardar borrador y salir**,
  **Descartar cambios** o **Seguir editando**. Solo aparece si de verdad has
  tocado algo: se compara una huella de lo que hay en pantalla (incluidos los
  textos a medio escribir) con la del momento de abrirlo.

### Primer arranque: sin datos de ejemplo

La aplicación arranca vacía: `Empresa::default()` nace sin nombre y
`Db::default()` con clientes, documentos y gastos a cero. No hay ninguna
empresa sembrada, así que el asistente te pregunta lo tuyo desde el principio.

Los datos viven en `%APPDATA%\AutonomoFacil\datos.json`; **reinstalar no los
borra** y desinstalar tampoco los elimina por defecto. Es lo que quieres para no
perder facturas al actualizar. Si alguna vez
quieres volver a empezar como el primer día, cierra la aplicación y aparta ese
fichero (mejor copiarlo antes). Ojo: en el primer arranque se migra una sola vez
la carpeta antigua `%APPDATA%\Facturacion`, y si allí sigue habiendo un
`datos.json` se copiará igual.

### Salir del tutorial

El diálogo de puesta en marcha es modal y no se puede arrastrar ni redimensionar,
así que no se pierde por la pantalla. Siempre tiene **Salir** a la vista y
responde a **Esc**: cierra sin guardar nada y volverá a salir al abrir el
programa. Un clic derecho o un descuido no pueden dejar el tutorial sin salida.

### Gastos y clientes: NIF y totales en vivo

- Al escribir el **NIF de un proveedor** o de un **cliente**, si la letra de
  control no cuadra sale un aviso ámbar en el momento, sin esperar a guardar:
  una factura con NIF erróneo no sirve como justificante y no entra en el 347.
  Un NIF vacío se sigue permitiendo (factura simplificada al cliente; gasto sin
  NIF en proveedores de fuera).
- En la ficha de gasto, debajo de los campos, un recuadro calcula **cuota de
  IVA, retención de IRPF y total** mientras se teclea, con el redondeo al
  céntimo del modelo. Recuerda que la cuota se deduce en el 303 y la base resta
  en el 130.

### Gráficas de contabilidad

La pestaña **Contabilidad → Resumen** abre con la evolución del año:

- **Ingresos y gastos por mes**: barras azules y ámbar con la base imponible
  de cada mes; al pasar el ratón por encima dice ingresos, gastos y resultado
  de ese mes.
- **Resultado acumulado**: área con línea mes a mes, para ver de un vistazo si
  el año va a favor o en contra, con el último valor destacado.
- **IVA por trimestre**: repercutido frente a soportado, con el resultado del
  303 en el tooltip.
- **Clientes que más facturan** y **gastos por categoría**: barras
  horizontales con importe y porcentaje del total.

La pestaña **IVA (303)** repite la gráfica trimestral para comparar con las
casillas. Todo se cuenta por fecha de factura o gasto (devengo), igual que los
modelos, y se filtra por el año elegido arriba.

Las gráficas se dibujan con el pintor de egui (`src/graficas.rs`), sin
bibliotecas nuevas: las series se calculan en funciones puras con tests y el
dibujo sólo pinta. Los ejes se redondean a 1, 2 o 5 por potencia de diez para
que las guías se lean de un vistazo.

### La voz del tutorial

El asistente se lee en voz alta con una **voz femenina** del sistema: en un
Windows en español usa Microsoft Helena (es-ES); si no está, cualquier voz
femenina instalada. Cada paso se narra al entrar en él, con frases pensadas
para el oído (sin listas ni abreviaturas).

Abajo hay una casilla **Voz** para silenciarla, un botón **Escuchar este paso**
para repetir, y un aviso «Leyendo en voz alta…» mientras habla. La preferencia
se guarda y también se puede cambiar en **Ajustes → Voz en el tutorial**.

Se implementa con SAPI a través de PowerShell, sin dependencias: cada frase es
un proceso oculto que se puede cortar al instante, y el texto viaja en base64
para que acentos y comillas no rompan el comando. Si no hay voz disponible, el
asistente funciona igual, en silencio.

### Tipo de cliente: IVA, retención y recargo automáticos

Cada cliente tiene un **régimen fiscal** que decide lo que se aplica en sus
facturas:

| Régimen | IVA | Retención | Operación |
|---|---|---|---|
| Empresa o particular | 21 % | 0 % | Nacional |
| Autónomo profesional | 21 % | **15 %** | Nacional |
| Comercio minorista | 21 % + **5,2 % de recargo** | 0 % | Nacional |
| Empresa de la UE | 0 % + leyenda | 0 % | Intracomunitaria (349) |
| Inversión del sujeto pasivo | 0 % + leyenda | 0 % | Nacional |

Al elegir el cliente en una factura, la app ajusta el IVA de las líneas, el
IRPF y el recargo, y cambia el tipo de operación. El recargo de equivalencia
se suma al total, sale en su propia línea en el PDF y en el XML de FacturaE
(como impuesto «Otros», código 05, porque FacturaE no tiene un código
propio).

### Logo en los documentos

En **Ajustes → Logo de la empresa** se elige una imagen con el diálogo de
Windows. Cualquier formato vale (PNG, JPG, BMP, GIF): se convierte a JPEG sobre
fondo blanco, con 900 px de lado como máximo, y se guarda en
`%APPDATA%\AutonomoFacil\logo.jpg`.

El logo sale arriba a la izquierda en la cabecera de facturas, presupuestos y
proformas, en todas las páginas, y el bloque de datos de la empresa se aparta a
su derecha. En el PDF se copia el JPEG tal cual (`/DCTDecode`), sin recodificar:
no hace falta ninguna biblioteca de imagen. Si la ruta no existe o el fichero no
es un JPEG válido, el documento se genera sin logo en vez de fallar.

Si aún no tienes logo, la misma tarjeta de Ajustes tiene un desplegable
**«¿No tienes logo? Créalo con IA o gratis en dos minutos»** con un prompt
listo para copiar (sólo hay que cambiar `[nombre de mi empresa]` y
`[sector o servicio]`) y botones que abren **Nano Banana · Gemini**, **ChatGPT**,
**Microsoft Designer**, **Canva**, **Looka**, **Shopify Logo Maker**,
**Ideogram** y **remove.bg**. El prompt pide estilo minimalista, dos colores,
cuadrado, fondo blanco y que se reconozca a 32 px; los consejos recuerdan
exportar a 800–1000 px y sin texto diminuto. Los enlaces están en
`src/recursos.rs`, con un test que exige que todos sean `https://`.

El XML de FacturaE no lleva logotipo (el estándar no lo tiene): sólo el PDF.

### Ayudas al pasar el ratón

Las etiquetas y los campos de Ajustes, de la ficha de cliente y del editor de
factura llevan un texto de ayuda: qué es el NIF y por qué se valida, cuándo se
aplica el 15 % de retención, qué significa cada operación, qué es el recargo
de equivalencia, para qué sirve el IBAN, qué formato admite la cantidad… Se
muestra dejando el ratón quieto encima.

### Regímenes: directa, módulos, recargo de equivalencia y SII

En Ajustes hay dos controles nuevos que cambian la lista de modelos y sus
avisos:

- **Régimen del IRPF**: estimación directa (normal o simplificada) o
  **estimación objetiva (módulos)**. En módulos el pago a cuenta pasa a ser el
  **131** y desaparece el 130, con su propia guía de presentación.
- **Recargo de equivalencia** (comercio minorista): desaparecen el **303** y
  el **390**, porque el IVA y el recargo los ingresa el proveedor; aparece el
  **309** para las operaciones no periódicas (adquisiciones
  intracomunitarias, inversión del sujeto pasivo, venta de inmuebles).
- **SII**: el 303 pasa a mensual, los libros van en tiempo real y el **390**
  deja de listarse.

La lista de obligaciones se calcula con `fiscal::obligaciones_con(actividad,
empresa)`; `fiscal::obligaciones(actividad)` sigue existiendo para los tests y
usa el régimen por defecto.

### Comercio electrónico

Para actividades de comercio o venta digital, la guía muestra la tarjeta
**«Comercio electrónico sin sustos»** con los ocho puntos que más problemas
dan, contrastados con la AEAT:

- Ventas a distancia a particulares de la UE: umbral único de **10.000 €** de
  IVA excluido; por encima, IVA del país del cliente y modelo **369 (OSS)**, con
  alta previa en el **035**.
- Importaciones de hasta **150 €** desde fuera de la UE: **IOSS**, alta con el
  **035** y declaración en el **369**, que en este régimen es mensual.
- **Marketplaces** (Amazon, Steam, Etsy): la plataforma actúa como vendedor
  presunto y liquida el IVA; tú le facturas a ella.
- **Factura simplificada** hasta 400 € (general) y hasta 3.000 € en ventas al
  por menor; no vale para entregas intracomunitarias ni ventas a distancia
  desde otro Estado miembro.
- Devoluciones con **factura rectificativa**, nunca borrando la original.
- Comisiones de **pasarelas de pago** (Stripe, PayPal, Redsys) como gasto
  deducible; se declara el bruto del cliente.
- Recordatorio de **recargo de equivalencia** y de **criterio de caja/SII**.

### Validaciones y justificantes

Antes de presentar, la vista avisa de los fallos que de verdad duelen:

- **NIF**: se comprueba la letra de control de DNI, NIE y CIF (con K, L y M
  incluidas). Si no cuadra, sale en «Lo que falta» y no se deja pasar como si
  nada.
- **303**: cada cuota contra su base y su tipo (si editas 07 sin tocar 09, te
  dice qué importe serían); resultado negativo («sale a compensar»);
  operaciones exentas, de exportación o intracomunitarias que no van en las
  casillas 01-09 y su importe.
- **390**: el total devengado contra la suma de los cuatro 303.
- **130**: la casilla 05 contra lo que salió en los trimestres anteriores; y
  retenciones en facturas con la casilla 06 a cero.
- **347**: gastos sin NIF que se quedan fuera del modelo.
- **349**: facturas intracomunitarias con un NIF-IVA sin prefijo de país.

Son avisos, no bloqueos: salen en ámbar bajo el resumen y el autónomo decide.

Al marcar un periodo como presentado aparece el campo **«Justificante»**
para guardar el nº de registro o el CSV que da la Sede (se guarda en
`datos.json` con el mismo criterio que la marca de presentado).

### Casillas a mano, con totales que se recalculan

En cada modelo de la vista (303, 130 y 390) hay un desplegable **«Editar
casillas a mano»**: una rejilla con casilla, concepto y un campo de texto
con el valor calculado como pista. Se escribe sólo lo que se quiera cambiar
(AIB, prorrata, compensaciones de trimestres anteriores, la casilla 13 del
130…) y el resto se respeta.

- Al cambiar una base o una cuota, **los totales se recalculan solos**: en el
  303 el 27 sale de 03+06+09+152, el 45 del 29, y 46, 64 y 69 del resultado;
  en el 390 el 33, 34, 64, 65, 84 y 86; en el 130 el 03, 04, 07 y 19.
- Lo escrito se guarda en `datos.json` (`ajustes_modelos`) por modelo y
  periodo (`303-2026-1T`, `390-2026-anual`), así que sigue ahí al volver y
  es lo que va al PDF y al JSON al pulsar «Preparar el modelo».
- El botón «Quitar los ajustes de este modelo» vuelve a los valores
  calculados.
- El resumen en una frase se actualiza al instante con los valores editados.

### Guía de entrega en la app

Cada modelo rellenable tiene una sección desplegable **«Cómo presentarlo en
la Sede»** con: con qué te puedes identificar (certificado, DNIe, Cl@ve,
MiDNI/eIDAS), el paso a paso hasta el justificante, qué documentos admite
cada uno (el 303 y el 130 sólo formulario web; el 390 el 347 y el 349 también
fichero con diseño de registro) y el aviso propio del modelo. Botones:
**Abrir la Sede de la AEAT** (página oficial de ese modelo), **Copiar
casillas (JSON)** para pegarlas en otro programa o mandárselas al gestor y
**Diseño de registro oficial** en los que lo admiten.

Al final de la guía hay una tarjeta **«Presentarlo automáticamente, estilo
TaxDown»** con las cuatro vías reales: guiado en la Sede, fichero con diseño
de registro, API de un colaborador social y robot sobre la Sede (RPA). La
AEAT no publica API de presentación, así que la app no promete enviar sola:
documenta y prepara los datos.

### Plazos automáticos

Cada obligación de la guía muestra ya su **próximo plazo pendiente**: el
primer trimestre del año que no esté marcado como presentado, con la fecha
límite habitual (20 de abril/julio/octubre, 30 de enero el 4T, 30 de enero
el 390, 28 de febrero el 347, 30 de junio el 100) y los días que faltan. Si
el plazo ya pasó y sigue sin marcar, sale en rojo con «⚠ Sin presentar». Al
marcar un trimestre, el aviso salta solo al siguiente.

### ¿Se puede presentar automáticamente por API?

Hoy no: la AEAT no publica una API para presentar declaraciones. Todo pasa
por su Sede con **certificado electrónico, DNIe o Cl@ve**. Las vías reales
de automatización, de menos a más esfuerzo, son:

1. **Teclear con el borrador al lado**: el PDF da los importes casilla a
   casilla y el formulario web los suma solo.
2. **Fichero con diseño de registro**: el 303, el 390, el 347 y el 349
   admiten subir un fichero con el formato publicado por la AEAT. Requiere
   implementar ese diseño por ejercicio (cambia cada año) y firmarlo en la
   Sede.
3. **Robot sobre la Sede (RPA)**: automatizar el navegador con un
   certificado instalado en el equipo. Frágil (la Sede cambia) y el
   certificado debe estar en poder del autónomo.
4. **Intermediario autorizado**: un servicio de facturación que presente por
   el autónomo; el autónomo le da de alta como colaborador. Implica sacar
   los datos de la app.

El JSON de las casillas es el formato pensado para las opciones 2, 3 y 4.
Los modelos que la app aún no rellena (390, 347, 349, 100, 111, 115) siguen
teniendo su calendario y su explicación en la guía.

### Pantallas pequeñas, desplazamiento y zoom

- La ventana puede bajar hasta 820 × 540 y **todo el contenido tiene barra de
  desplazamiento**: en Inicio y en la guía la barra es vertical y horizontal;
  las listas, el editor, clientes, artículos, contabilidad y ajustes ya
  llevaban la suya.
- La barra lateral se adapta al ancho de la ventana (24 %, entre 158 y 232
  puntos) y se puede **arrastrar** para hacerla más estrecha o más ancha. El
  panel de detalle de un documento también se arrastra (240-520 puntos) y
  arranca más estrecho en ventanas menores de 1180 puntos.
- **Zoom**: botones `−`, `+` y `100 %` bajo el título de la barra lateral, más
  `Ctrl` + rueda y `Ctrl` + `+` / `Ctrl` + `−` que ya interpreta egui. Se
  limita al 60-200 % y se guarda en `datos.json` (`"zoom": 1.0`), así que se
  recuerda al volver a abrir.

## Licencia

[![CC BY-SA 4.0](https://licensebuttons.net/l/by-sa/4.0/88x31.png)](https://creativecommons.org/licenses/by-sa/4.0/)

El código y los contenidos propios de **AutónomoFácil** están bajo la licencia
[Creative Commons Atribución-CompartirIgual 4.0 Internacional (CC BY-SA 4.0)](https://creativecommons.org/licenses/by-sa/4.0/).
AutónomoFácil © 2026 de Francisco Nicolás está bajo la licencia CC BY-SA 4.0. Para ver una copia de esta licencia, visite https://creativecommons.org/licenses/by-sa/4.0/

Puedes copiar, redistribuir y adaptar el material, incluso comercialmente,
siempre que cites la autoría, enlaces a la licencia e indiques si hay cambios;
las adaptaciones deben compartirse bajo la misma licencia.

Las dependencias de terceros conservan sus propias licencias. El texto legal
está en [creativecommons.org](https://creativecommons.org/licenses/by-sa/4.0/legalcode.es).
