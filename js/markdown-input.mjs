/**
 * Ayudas de escritura markdown para el editor (Task 5).
 *
 * El markdown es el documento: todo lo que este módulo inserta son bytes de
 * markdown (espacios, `\n`, marcadores) que quedan guardados tal cual. No hay
 * cierre automático de delimitadores (`[`, `(`, `**`): descartado en el spec.
 *
 * Exporta `markdownInputExtension()`, que devuelve una extensión de
 * CodeMirror con:
 * - un keymap con `Prec.highest` (le gana al de `basicSetup`, que ya captura
 *   Enter y Tab) para continuar listas/citas con Enter e indentar con Tab,
 * - un `EditorView.inputHandler` que reconoce las reglas de tipeo (`- `,
 *   `1. `, `- [ ] ` al principio de una línea vacía).
 */
import { EditorView, keymap } from "@codemirror/view";
import { EditorSelection, Prec } from "@codemirror/state";
import { syntaxTree } from "@codemirror/language";

/**
 * `indent + bullet + [task] + espacios + resto`. El bullet preserva el
 * delimitador original (`.` o `)`), y la casilla acepta `[ ]`, `[x]` y `[X]`.
 */
export const LIST_RE = /^(\s*)([-*+]|\d+[.)])(\s+\[[ xX]\])?(\s+)(.*)$/;
export const QUOTE_RE = /^(\s*(?:>[ \t]*)+)(.*)$/;
/** La regla de tipeo sólo reconoce un marcador completo al final de la línea. */
export const TYPING_RE = /^(\s*)([-*+]|\d+[.)])(\s+\[[ xX]\])?\s$/;

const CODE_NODES = new Set(["FencedCode", "CodeBlock"]);

function clampPos(state, pos) {
  return Math.max(0, Math.min(state.doc.length, pos));
}

/** True si `pos` cae dentro de un bloque de código (cercado o indentado). */
function inCodeBlock(state, pos) {
  try {
    let node = syntaxTree(state).resolveInner(clampPos(state, pos), -1);
    while (node) {
      if (CODE_NODES.has(node.name)) return true;
      node = node.parent;
    }
  } catch {
    return false;
  }
  return false;
}

/** True si `pos` cae dentro de un ítem de lista (aunque la línea no lleve marcador). */
function inListItem(state, pos) {
  try {
    let node = syntaxTree(state).resolveInner(clampPos(state, pos), -1);
    while (node) {
      if (node.name === "ListItem") return true;
      if (CODE_NODES.has(node.name)) return false;
      node = node.parent;
    }
  } catch {
    return false;
  }
  return false;
}

/**
 * El marcador de continuación de `indent + bullet + task`: la numeración se
 * incrementa preservando el delimitador (`1.` → `2.`, `3)` → `4)`), y la
 * casilla de tarea se desmarca (`- [x]` → `- [ ] `).
 */
export function continuedMarker(indent, bullet, task) {
  const num = /^(\d+)([.)])$/.exec(bullet);
  const mark = num ? `${Number(num[1]) + 1}${num[2]}` : bullet;
  return `${indent}${mark}${task ? " [ ]" : ""} `;
}

/**
 * Acción de Enter para una línea de lista: `{ type: "continue", marker }`
 * para seguirla, `{ type: "exit" }` si el ítem está vacío (saca el marcador
 * en vez de crear un ítem en blanco), o `null` si no es lista.
 */
function listAction(lineText) {
  const m = LIST_RE.exec(lineText);
  if (!m) return null;
  const [, indent, bullet, task, , rest] = m;
  if (rest.trim() === "") return { type: "exit" };
  return { type: "continue", marker: continuedMarker(indent, bullet, task) };
}

/**
 * Acción de Enter para una cita. El resto tras el prefijo `> ` puede ser a
 * su vez una lista (`> - a` → `> - `, `> 1. a` → `> 2. `); un `> - ` vacío
 * suelta el marcador de lista pero sigue en la cita (`> `). Una cita vacía
 * sale de la cita. `null` si la línea no es cita.
 */
function quoteAction(lineText) {
  const q = QUOTE_RE.exec(lineText);
  if (!q) return null;
  const [, prefix, rest] = q;
  const inner = listAction(rest);
  if (inner) {
    if (inner.type === "exit") return { type: "continue", marker: prefix };
    return { type: "continue", marker: prefix + inner.marker };
  }
  if (rest.trim() === "") return { type: "exit" };
  return { type: "continue", marker: prefix };
}

/**
 * Enter: continúa listas y citas, sale del ítem/cita vacíos. Dentro de un
 * bloque de código no intercepta: devuelve `false` para que `basicSetup`
 * inserte el salto literal. También devuelve `false` si alguna selección
 * abarca varias líneas o ninguna línea es lista/cita (comportamiento por
 * defecto en vez de adivinar).
 */
function handleEnter(view) {
  const { state } = view;
  const ops = [];
  for (const range of state.selection.ranges) {
    // El sesgo -1 resuelve el borde final de la línea hacia adentro del
    // bloque, no hacia el `CodeMark` de cierre.
    if (inCodeBlock(state, range.head)) return false;
    if (range.from !== range.to) {
      const first = state.doc.lineAt(range.from);
      const last = state.doc.lineAt(range.to);
      if (first.number !== last.number) return false;
    }
    const line = state.doc.lineAt(range.from);
    const action = listAction(line.text) || quoteAction(line.text);
    if (!action) return false;
    ops.push({ range, line, action });
  }
  // Un solo `dispatch`: los cambios van en coordenadas previas y `delta`
  // acumula el corrimiento para los rangos siguientes (ordenados).
  let delta = 0;
  const changes = [];
  const sel = [];
  for (const { range, line, action } of ops) {
    if (action.type === "exit") {
      changes.push({ from: line.from, to: line.to, insert: "" });
      sel.push(EditorSelection.cursor(line.from + delta));
      delta -= line.to - line.from;
    } else {
      const insert = "\n" + action.marker;
      changes.push({ from: range.from, to: range.to, insert });
      sel.push(EditorSelection.cursor(range.from + delta + insert.length));
      delta += insert.length - (range.to - range.from);
    }
  }
  view.dispatch({
    changes,
    selection: EditorSelection.create(sel),
    scrollIntoView: true,
  });
  return true;
}

/**
 * Tab / Shift-Tab. En una línea de lista indenta (dos espacios al inicio de
 * la línea) o desindenta (saca hasta dos); fuera de una lista inserta dos
 * espacios en el cursor. Con una selección no colapsada devuelve `false`
 * para no romper el indentado por bloque de `basicSetup`.
 */
function handleIndent(view, dedent) {
  const { state } = view;
  for (const range of state.selection.ranges) {
    if (range.from !== range.to) return false;
  }
  let delta = 0;
  const changes = [];
  const sel = [];
  for (const range of state.selection.ranges) {
    const line = state.doc.lineAt(range.from);
    if (dedent) {
      const m = /^( {1,2})/.exec(line.text);
      if (!m) {
        sel.push(EditorSelection.cursor(range.from + delta));
        continue;
      }
      const n = m[1].length;
      changes.push({ from: line.from, to: line.from + n, insert: "" });
      const rel = Math.max(0, range.from - line.from - n);
      sel.push(EditorSelection.cursor(line.from + delta + rel));
      delta -= n;
    } else if (LIST_RE.test(line.text) || inListItem(state, range.from)) {
      changes.push({ from: line.from, insert: "  " });
      sel.push(EditorSelection.cursor(range.from + delta + 2));
      delta += 2;
    } else {
      changes.push({ from: range.from, insert: "  " });
      sel.push(EditorSelection.cursor(range.from + delta + 2));
      delta += 2;
    }
  }
  // Shift-Tab sin nada que sacar: reclamar la tecla igual, para que el foco
  // no salte fuera del editor.
  if (changes.length === 0) return true;
  view.dispatch({
    changes,
    selection: EditorSelection.create(sel),
    scrollIntoView: true,
  });
  return true;
}

/**
 * Reglas de tipeo (`- `, `* `, `+ `, `1. ` / `1) `, `- [ ] ` al principio de
 * una línea vacía, sólo cuando el cursor queda al final de lo insertado).
 *
 * El handler reconoce el patrón y devuelve `false` a propósito: la inserción
 * por defecto ya deja los bytes exactos, y reescribir (renumerar el `1.` o
 * desmarcar el `[x]`) corrompería el documento, que tiene que ser byte a
 * byte lo que el usuario escribió. Existe como punto de reconocimiento de la
 * extensión, no como transformación.
 */
const typingRule = EditorView.inputHandler.of((view, from, to, text) => {
  if (!/[ \t]/.test(text)) return false;
  const line = view.state.doc.lineAt(clampPos(view.state, from));
  // Sólo al final de la línea: en el medio no secuestra la escritura.
  if (to !== line.to) return false;
  const candidate = line.text.slice(0, from - line.from) + text;
  if (!TYPING_RE.test(candidate)) return false;
  return false;
});

/**
 * Extensión de ayudas de escritura. El keymap va con `Prec.highest`, igual
 * que el de formato en `codemirror-bridge.mjs`: sin eso empata con el de
 * `basicSetup` (que viene primero en el arreglo) y su Enter/Tab ganan.
 */
export function markdownInputExtension() {
  return [
    Prec.highest(
      keymap.of([
        { key: "Enter", run: handleEnter },
        { key: "Tab", run: (view) => handleIndent(view, false) },
        { key: "Shift-Tab", run: (view) => handleIndent(view, true) },
      ]),
    ),
    typingRule,
  ];
}
