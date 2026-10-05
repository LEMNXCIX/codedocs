# Spike: exportación a PDF con Typst (CodeDocs)

**Fecha:** 2026-10-04 · **Worktree:** `/tmp/opencode/spike-pdf` (detached HEAD en `bfa7762`, nada commiteado)
**Pregunta:** ¿vale la pena agregar `typst-library` 0.15.1 + `typst-pdf` 0.15.1 a `src-tauri`?
**Respuesta corta:** el PDF sale **muy bien** (la matemática se compone de verdad), pero el costo es
**+59 MiB de binario y +4m42s de build release**. El veredicto depende de si CodeDocs puede
justificar 5× el tamaño del binario.

---

## 0. Hallazgo previo que invalida la premisa del spike

**`typst-library` + `typst-pdf` no compila nada.** En Typst 0.15 el compilador se partió en varios
crates y `typst-library` quedó sólo con *tipos + biblioteca estándar*:

> "most of the compilation *behaviour* is split out into separate crates
> (`typst-eval`, `typst-realize`, `typst-layout`, etc.)" — docs de `typst-library` 0.15.1

`typst-pdf::pdf()` consume un `&PagedDocument` pero no sabe producirlo, y `typst-library` no expone
ningún `compile()`. El punto de entrada real es el crate paraguas **`typst`**, que además arrastra
`typst-eval`, `typst-realize` y `typst-html` (obligatorios, sin feature para desactivarlos).

Conjunto mínimo que **sí** funciona:

```toml
typst        = "0.15.1"   # compilador (reexporta typst-library, syntax, utils)
typst-pdf    = "0.15.1"   # exportador PDF
typst-layout = "0.15.1"   # NECESARIO como dep directa: PagedDocument no se reexporta
typst-assets = { version = "0.15.1", features = ["fonts"] }  # fuentes embebidas
```

Dos trampas más, ambas encontradas compilando y no leyendo:

- **`typst-layout` hace falta como dependencia directa.** `PagedDocument` vive en `typst_layout` y ni
  `typst` ni `typst_library::layout` lo reexportan, aunque `pdf()` lo exija como argumento.
- **Sin `typst-assets`, feature `fonts`, no hay ninguna fuente.** El feature está apagado por
  defecto en toda la cadena. Un alta que sólo agregue `typst` + `typst-pdf` compila, no falla, y
  produce un PDF en el que **no se compone absolutamente nada**. Es un fallo silencioso.

`Cargo.lock`: 561 paquetes base → **740** con el conjunto final (+179).

---

## 1. Tiempos de build limpio

Método: `CARGO_TARGET_DIR` separado por medición, **`rm -rf` del target antes de cada build**,
reloj de pared alrededor de `cargo build -p codedocs`, registro completo en `timings/summary.txt`,
registry de crates pre-descargado (`cargo fetch`) para que el tiempo sea de compilación y no de red.

```bash
# script usado: timebuild.sh <label> <target_dir> <profile>
rm -rf "$TARGET_DIR"
START=$(date +%s.%N)
CARGO_TARGET_DIR="$TARGET_DIR" cargo build -p codedocs --profile "$PROFILE"
END=$(date +%s.%N)
```

### Todas las corridas (limpio, target borrado)

| corrida | perfil | segundos | set de deps |
|---|---|---|---|
| `base-dev` | dev | 163.6 | base |
| `base-release` | release | 234.8 | base |
| `typst-dev` | dev | 346.3 | `typst-library`+`typst-pdf`, **sin uso real** ⚠️ |
| `typst-release` | release | 494.1 | ídem, **sin uso real** ⚠️ |
| `base2-dev` | dev | **149.5** | base |
| `base2-release` | release | **281.8** | base |
| `typst2-dev` | dev | 244.1 | conjunto final |
| `typst3-release` | release | **564.2** | conjunto final |
| `typst4-dev` | dev | **260.9** | conjunto final |

⚠️ Las corridas `typst-dev`/`typst-release` **están descartadas**: además de usar el set de
deps equivocado, no llamaban a typst (ver §3). Sirven sólo como registro de por qué no sirve una
medición sin uso real.

### Deltas (parejas adyacentes en el tiempo)

| perfil | base | typst | **delta** | % |
|---|---|---|---|---|
| dev (par 1) | 163.6 | 244.1 | **+80.5 s** | +49 % |
| dev (par 2) | 149.5 | 260.9 | **+111.4 s** | +75 % |
| dev (promedio) | 156.6 | 252.5 | **≈ +96 s** | +61 % |
| release (par adyacente) | 281.8 | 564.2 | **+282.4 s** | +100 % |

**El delta de build es +96 s en dev y +282 s en release.** El release **se duplica**.

Rango de confianza, dicho explícitamente: la máquina estaba compartida (load average 8–15 sobre
8 cores, hay otro agente compilando en `.worktrees/live-polish`). El delta de dev es el número
menos confiable: dos pares independientes dieron +80.5 s y +111.4 s. El de release se midió en una
pair única adyacente y es el que más confianza tiene.

No medido: efecto de `lto = true`, `codegen-units = 1`, `panic = "abort"` o `strip`. El perfil
release del proyecto no define ninguno de esos flags, así que estos números son con los defaults
que el proyecto ya tiene.

---

## 2. Tamaño del binario

`ls -la` del binario release en ambos casos (mismo `dist/` embebido en los dos builds).

| | bytes | MiB |
|---|---|---|
| base (`base2-release/release/codedocs`) | 16 045 936 | 15.30 |
| con typst (`typst3-release/release/codedocs`) | 78 028 352 | 74.41 |
| **delta** | **+61 982 416** | **+59.11** |

**El binario queda 4.86× más grande.**

Contexto importante: los 15.30 MiB de base ya incluyen los **12.4 MB del `.wasm` del frontend**
embebido por Tauri. El delta no se ve afectado, pero el número absoluto sin typst es artificial.

### Descomposición del delta por sección ELF

| sección | base | con typst | delta |
|---|---|---|---|
| `.text` (código) | 7 261 910 | 29 229 623 | **+20.95 MiB** |
| `.rodata` (datos) | 648 616 | 19 468 240 | **+17.95 MiB** |
| `.eh_frame` | 886 492 | 3 297 504 | +2.30 MiB |
| `.rela.dyn` | 312 144 | 2 504 160 | +2.09 MiB |
| `.gcc_except_table` | 991 224 | 2 036 800 | +1.00 MiB |

De los +17.95 MiB de `.rodata`, **9.50 MiB son las fuentes embebidas** de `typst-assets`
(`files/fonts/`, 9 956 001 bytes: Libertinus ×6, NewCMMath ×3, NewCM10 ×3, DejaVuSansMono ×4).

**El costo dominante es código (+21 MiB de `.text`), no fuentes (9.5 MiB, 16 % del delta).**
Apagar las fuentes embebidas y cargar fuentes del sistema no resuelve el problema de tamaño.
Aclarado: el binario es ELF sin comprimir; un `.deb`/AppImage comprimido crece menos. No medido.

Descartado como causa: `two-face` **no** trae fuentes de emoji embebidas (el feature `emoji` no está
activado y el crate no contiene ningún `.ttf`/`.otf`).

---

## 3. El PDF

### Veredicto: **PASS**

La PDF compila, se pagina, y **la matemática se compone de verdad** — que era el riesgo principal
del spike. No es texto plano ni bloques raros.

### El error que casi produce un falso FAIL

Mi primer intento de fórmula inline usó `$e^{i pi} + 1 = 0$`. Compiló sin error y **salió mal**:
`e^{i pi}` renderiza las llaves literales porque en Typst `{...}` es un bloque de código, no un
agrupador. El texto extraído daba `𝑒{𝑖𝜋} + 1 = 0`.

No era un FAIL de Typst: era sint mía. Corregido a `$e^(i pi) + 1 = 0$`. **Anotado porque cualquiera
que escriba el conversor Markdown→Typst va a pisar esto**: en Markdown `$e^{i\pi}$` usa llaves para
el exponente y en Typst hay que convertirlas a paréntesis.

### Cómo se verificó (comandos exactos, reproducibles)

Herramientas: poppler (`pdftotext`, `pdfinfo`, `pdffonts`, `pdftoppm`) — todas presentes en
`/usr/bin`. No hizo falta nada más. Sin python3, sin qpdf, sin mutool, sin gs.

```bash
cd /tmp/opencode/spike-pdf
CARGO_TARGET_DIR=/home/leonardo/spike-pdf-target/typst4-dev \
  cargo run -q -p codedocs --example spike_pdf -- out/test.typ out/test.pdf

cd out
pdfinfo test.pdf                        # páginas, tamaño, metadatos
pdffonts test.pdf                       # fuentes embebidas
pdftotext -layout test.pdf -            # texto extraído
pdftotext -bbox-layout test.pdf -       # posición y tamaño de cada palabra
pdftoppm -png -r 130 test.pdf page      # rasterizado para inspección visual
```

**El documento de prueba se escribió a mano en Typst** (`out/test.typ`), sin usar el renderer de
markdown del proyecto, tal como se pidió. El markdown de referencia es `out/test.md` y existe sólo
como especificación de qué caso exercising; **nada de lo que se midió pasa por
`codedocs-core::markdown`**.

### Evidencia 1 — el texto extraído contiene todo lo esperado

`pdftotext -layout` sobre las 2 páginas contiene, verificado palabra por palabra:

`CodeDocs PDF Spike` · `Second level heading` · `negrita` · `cursiva` · `código inline` ·
`tachado` · `Fórmula inline` · las tres viñetas con su anidamiento · `Primer ítem numerado` …
`Tarea pendiente sin marcar` · `Tarea completada` · `Cita en bloque` · el bloque Rust completo
(`fn main() {`, `let x: i32 = 41;`, `println!("{}", x + 1); // 42`) · `Columna A/B/C` y las 9
`celda a1..c3` · `Fin del documento de prueba` · `Segunda página`.

`pdfinfo`: `Pages: 2`, `Page size: 595.276 x 841.89 pts (A4)`, `Tagged: yes`,
`Creator: Typst 0.15.1`, `File size: 46875 bytes`, `PDF version: 1.7`.

### Evidencia 2 — la matemática se compone de verdad

`pdffonts` muestra **`NewCMMath-Book` embebida y subconjuntada**. Es la fuente matemática de Typst:
sólo aparece si el motor de math corrió.

El texto extraído contiene code points del bloque *Mathematical Alphanumeric Symbols*, que **sólo**
produce un shaper matemático mapeando ASCII a esos code points. Una fórmula que saliera como texto
plano se habría extraído como `eip+1=0` en ASCII:

```
𝑒 -> U+1D452    𝑖 -> U+1D456    𝜋 -> U+1D70B
𝑥 -> U+1D465    𝑛 -> U+1D45B
```

`pdftotext -bbox-layout` sobre la fórmula inline, coordenadas del PDF:

```
x=397.228 y=112.050..123.050  𝑒     <- base, 11.0 pt de alto
x=402.354 y=110.717..118.417  𝑖𝜋    <- exponente, 7.7 pt de alto, BASE ELEVADA
```

El exponente es un **70 % del tamaño del cuerpo y está corrido hacia arriba** (`yMax` 118.4 contra
123.1 de la base). Eso es un superíndice real, no texto en la línea base.

Y la inspección visual del PNG rasterizado confirma el resto: el integral muestra los límites `0`
abajo y `1` arriba del signo `∫`, y `1/2` como fracción apilada; la suma muestra `i=1` debajo y `n`
encima del `∑`, con `n(n+1)/2` como fracción. Espaciado matemático alrededor de `+` y `=`.

### Evidencia 3 — inspección visual del rasterizado

`page-1.png` y `page-2.png`, mirados. Confirma además:

- **Resaltado de sintaxis Rust real** vía syntect: `fn` en rojo, keywords y tipos en color,
  string en rojo, comentario `// 42` en gris. No es texto plano en monoespaciado.
- Negrita, cursiva, código inline y tachado con estilos diferenciados.
- Listas con viñetas anidadas a 3 niveles, numeradas anidadas a 2 niveles.
- Tabla de 3×3 con bordes y fila de encabezado.
- Paginación: 2 páginas, texto justificado, encabezado de nivel 1 en la página 2.

### Lo que Typst NO hace solo (trabajo para el conversor futuro)

**No hay soporte nativo de task lists.** Compilé un probe con `- [ ]` / `- [x]` y el PDF salió con
los caracteres **literales**:

```
• [ ] Tarea pendiente
• [x] Tarea completada
```

No hay elemento `task`, ni sintaxis de checkbox, ni nada en `typst-library` 0.15. En el documento de
prueba tuve que emitirlos a mano como `#box[#sym.square]` y `#box[#sym.ballot.check]`, y así
renderizan `□` y `☑︎`. El conversor Markdown→Typst va a tener que transformar `- [ ]` /
`- [x]` explícitamente. Es trabajo de implementación, no un bloqueo.

---

## 4. Conclusiones

1. **La calidad del PDF es el punto fuerte de Typst.** Sale listo para producción: math real,
   resaltado de sintaxis, tablas, paginación, A4, PDF etiquetado (accesible), fuentes embebidas y
   determinista. El riesgo técnico principal del spike **no se materializó**.

2. **El costo es el problema.** +59 MiB y 4.86× el binario; +282 s en cada build release limpio.
   El build limpio de release se duplica, lo que duele en CI.

3. **El costo no son las fuentes.** Son 9.5 MiB de 59. El resto es el compilador: +21 MiB de
   `.text`. Cambiar a fuentes del sistema no rescata el tamaño.

4. **Falta un crate y falta un feature.** `typst-library` + `typst-pdf` es un conjunto incompleto:
   hay que agregar `typst` y `typst-layout`, y encender `typst-assets/fonts` o el PDF sale vacío sin
   ningún error.

5. **El pico de riesgo restante es el conversor, no Typst.** El mapeo Markdown→Typst es trabajo
   real: llaves de exponente, task lists, y las decisiones de estilo que decidan que un documento
   Markdown se vea bien en A4. Eso no se midió.

### Alternativas que cuestan menos

No las medí — son candidatos a otro spike, no resultados.

| alternativa | qué se ahorra | a costa de |
|---|---|---|
| **`typst` CLI como subproceso** | los +59 MiB y los +282 s desaparecen del crate; se paga ~30–80 MB de RAM y latencia de arranque por exportación | depender de un binario externo: hay que empaquetarlo o el usuario tiene que tenerlo |
| **`typst` con fuentes del sistema** (fontdb, sin `typst-assets/fonts`) | −9.5 MiB | output no determinista entre máquinas |
| **Imprimir a PDF vía WebKitGTK** (`window.print()` / `WebKitPrintOperation`) | ~0 bytes, ~0 s: el backend ya está linkeado por Tauri | fidelidad de CSS, la que defina el preview, no la de Typst; peor control de paginación |
| **`markdown → HTML → headless Chromium** | ~0 bytes en el crate | enorme complejidad y runtime |

La opción que más me interesa explorar es la primera: **`typst` como subproceso**. Mantiene la
calidad de salida que acabamos de verificar —es el mismo compilador— y mueve todo el costo de build
y de tamaño fuera del binario de la app. Si el equipo está dispuesto a empaquetar el binario de
Typst, esa es la ruta.

---

## Reproducir

```bash
cd /tmp/opencode/spike-pdf

# tiempos (borra el target antes de cada uno; ver timebuild.sh)
./timebuild.sh base2-dev    /home/leonardo/spike-pdf-target/base2-dev    dev
# ... cambiar a src-tauri/Cargo.toml con el set final ...
./timebuild.sh typst4-dev   /home/leonardo/spike-pdf-target/typst4-dev   dev
./timebuild.sh base2-release /home/leonardo/spike-pdf-target/base2-release release
./timebuild.sh typst3-release /home/leonardo/spike-pdf-target/typst3-release release

# tamaños
ls -la /home/leonardo/spike-pdf-target/{base2,typst3}-release/release/codedocs
size -A /home/leonardo/spike-pdf-target/{base2,typst3}-release/release/codedocs

# PDF
cat timings/summary.txt
```

Archivos: `RESULT.md` (este informe), `timebuild.sh` (cronometraje), `timings/summary.txt` (cifras
crudas), `Cargo.toml.typst` / `lib.rs.typst` (config con typst, para poder alternar), `out/test.md`
(especificación markdown), `out/test.typ` (Typst a mano), `out/test.pdf`, `out/page-1.png`,
`out/page-2.png`, `src-tauri/src/pdf.rs` y `src-tauri/examples/spike_pdf.rs` (código de integración
mínimo, para forzar que el linker retenga typst).

Todo esto es desechable. No commiteé nada. El repo principal y `.worktrees/live-polish` no se
tocaron.