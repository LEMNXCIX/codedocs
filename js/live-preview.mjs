/**
 * Live-format decorations for the CodeMirror editor.
 *
 * The markdown stays the document: this module only styles it. A span
 * provider (registered from wasm, see `__codedocs_setSpanProvider` in
 * `codemirror-bridge.mjs`) maps the full document text to an `Array` of two
 * elements — a `Uint32Array` with `ranges` (`[from0, to0, ...]`, UTF-16 code
 * units) and a `Uint8Array` with one `SpanTag` discriminant per span — and
 * `buildDecorations` turns those spans into a `DecorationSet`.
 *
 * Hiding technique (measured in the Task 0 spike, do not re-evaluate):
 * `Decoration.mark({ class: "cm-lp-hidden" })` with `font-size: 0`.
 * NOT `Decoration.replace`: select-all + copy would drop the opening `**`
 * and the user only finds out in another program. NOT `display: none`: the
 * caret gets stuck at the offset and cannot cross the marker.
 *
 * API notes, read off `node_modules/@codemirror/{view,state}/dist/index.d.ts`:
 * - `Decoration.mark/replace/line` are STATIC methods returning a
 *   `Decoration` — no `new`.
 * - `DecorationSet` is not exported as a value; the set is built with
 *   `RangeSetBuilder` (a class: `new`, `.add(from, to, value)`, `.finish()`),
 *   where `value` is the `Decoration` itself. (`.range(from, to)` builds a
 *   `Range<Decoration>` for `Decoration.set` arrays — passing it to
 *   `builder.add` stores a non-`RangeValue` that later crashes range
 *   iteration with `t.eq is not a function`. Do not do that.)
 * - `LineDecoration.range` throws unless the range is zero-length, and
 *   `MarkDecoration.range` throws on empty ranges, so neither is ever built
 *   for one. `Decoration.none` (a `RangeSet`, not a `Decoration`) is the
 *   empty set — it must never be passed to `builder.add`.
 * - `RangeSetBuilder.add` throws on ranges added out of order (going
 *   backwards in `from`), but overlapping ranges are fine: they spill into
 *   layered sets that `finish()` joins. Hence the two collection passes
 *   below are MERGED into one ordered fill — feeding the line pass into the
 *   same builder after the range pass would restart at low positions and
 *   reject the whole set, leaving the document undecorated.
 */
import { Decoration, ViewPlugin } from "@codemirror/view";
import { RangeSetBuilder, StateEffect } from "@codemirror/state";

// SpanTag discriminants, mirroring `codedocs-core/src/markdown/live.rs`.
// Explicit numbers: renumbering silently recolors every decoration.
const TAG_HIDE = 0;
const TAG_STRONG = 1;
const TAG_EMPHASIS = 2;
const TAG_STRIKETHROUGH = 3;
const TAG_INLINE_CODE = 4;
const TAG_LINK_TEXT = 5;
// 6 (ImageAlt) renders a widget in Task 4.
const TAG_HEADING_1 = 7;
const TAG_HEADING_6 = 12;
const TAG_QUOTE = 13;
const TAG_LIST_MARKER = 14;
const TAG_FENCE = 15;
const TAG_TABLE_PIPE = 16;
const TAG_TABLE_DELIMITER = 17;
// 18 (MathInline), 19 (MathDisplay), 20 (MermaidBlock) render widgets in
// Task 4.

/**
 * Block-level widget tags (Task 4: math display, mermaid). A line covered by
 * a block replacement must NOT also carry a `line` decoration: the two
 * ranges collide and `RangeSetBuilder` rejects the entire set, so *no*
 * decoration in the document appears — a failure that reads as "the engine
 * is broken" instead of "two decorations clashed". This task emits no block
 * widgets yet, but the exclusion is already wired: when Task 4 starts
 * emitting them, the lines are already reserved.
 */
const BLOCK_TAGS = new Set([19, 20]);

/** Dispatched by `__codedocs_setSourceVisible` to force a redecoration. */
export const livePreviewRefresh = StateEffect.define();

/** `line` decorations are point ranges at the line start. */
function lineClass(tag) {
  if (tag >= TAG_HEADING_1 && tag <= TAG_HEADING_6) return `cm-lp-h${tag - TAG_HEADING_1 + 1}`;
  if (tag === TAG_QUOTE) return "cm-lp-quote";
  return null;
}

// Decoration instances are immutable, so one per kind is shared by every
// range. A 5 000-line document yields ~15 000 spans per keystroke; minting a
// fresh `Decoration` (plus its spec object) for each one costs milliseconds
// of allocation pressure on every keystroke.
const DECO_HIDDEN = Decoration.mark({ class: "cm-lp-hidden" });
const DECO_STRONG = Decoration.mark({ class: "cm-lp-strong" });
const DECO_EMPHASIS = Decoration.mark({ class: "cm-lp-emphasis" });
const DECO_STRIKETHROUGH = Decoration.mark({ class: "cm-lp-strikethrough" });
const DECO_INLINE_CODE = Decoration.mark({ class: "cm-lp-inlinecode" });
const DECO_LINK = Decoration.mark({ class: "cm-lp-link" });
const DECO_LINE = {
  "cm-lp-h1": Decoration.line({ class: "cm-lp-h1" }),
  "cm-lp-h2": Decoration.line({ class: "cm-lp-h2" }),
  "cm-lp-h3": Decoration.line({ class: "cm-lp-h3" }),
  "cm-lp-h4": Decoration.line({ class: "cm-lp-h4" }),
  "cm-lp-h5": Decoration.line({ class: "cm-lp-h5" }),
  "cm-lp-h6": Decoration.line({ class: "cm-lp-h6" }),
  "cm-lp-quote": Decoration.line({ class: "cm-lp-quote" }),
};

/**
 * Map one span tag to its decoration. Returns `{ kind, deco }` with
 * `kind: "mark" | "line"`, or `null` for widget tags (Task 4) and unknown
 * tags — skipping is safer than guessing.
 */
function decorationFor(tag) {
  switch (tag) {
    case TAG_HIDE:
    case TAG_LIST_MARKER:
    case TAG_FENCE:
    case TAG_TABLE_PIPE:
    case TAG_TABLE_DELIMITER:
      return { kind: "hide", deco: DECO_HIDDEN };
    case TAG_STRONG:
      return { kind: "mark", deco: DECO_STRONG };
    case TAG_EMPHASIS:
      return { kind: "mark", deco: DECO_EMPHASIS };
    case TAG_STRIKETHROUGH:
      return { kind: "mark", deco: DECO_STRIKETHROUGH };
    case TAG_INLINE_CODE:
      return { kind: "mark", deco: DECO_INLINE_CODE };
    case TAG_LINK_TEXT:
      return { kind: "mark", deco: DECO_LINK };
    default: {
      const cls = lineClass(tag);
      if (cls) return { kind: "line", deco: DECO_LINE[cls] };
      return null;
    }
  }
}

function sideOf(deco, which) {
  const v = which === "start" ? deco.startSide : deco.endSide;
  return typeof v === "number" ? v : 0;
}

/**
 * Build the decoration set for an editor state.
 *
 * Takes `state` (not the view) so it stays a pure function of
 * `(doc, selection, provider, flag)` — directly unit-testable in Node with
 * a real `EditorState`, no DOM needed.
 *
 * - Lines holding the cursor are revealed: every span on them is skipped, so
 *   the raw source (`**hola**`) is editable where the user is looking.
 * - `showAll` (source-visible mode, `Ctrl+/`) skips the hiding marks
 *   document-wide but keeps the styling, so the source reads raw yet
 *   legible.
 * - Spans arrive sorted by `from`. Marks and lines are collected in two
 *   passes, then merged into a single ordered builder fill.
 */
export function buildDecorations(state, getSpanProvider, isSourceVisible) {
  const doc = state.doc;
  const provider = getSpanProvider();
  if (typeof provider !== "function") return Decoration.none;

  let pair = null;
  try {
    pair = provider(doc.toString());
  } catch {
    return Decoration.none;
  }
  const flat = pair && pair[0];
  const tags = pair && pair[1];
  if (!flat || !tags || typeof flat.length !== "number" || typeof tags.length !== "number") {
    return Decoration.none;
  }
  const count = Math.min(tags.length, Math.floor(flat.length / 2));
  if (count === 0) return Decoration.none;

  const showAll = !!isSourceVisible();
  const docLen = doc.length;
  const clamp = (x) => Math.max(0, Math.min(docLen, x | 0));

  const revealed = new Set();
  if (!showAll) {
    for (const range of state.selection.ranges) {
      revealed.add(doc.lineAt(clamp(range.from)).number);
      revealed.add(doc.lineAt(clamp(range.to)).number);
    }
  }

  // Pass 1 (range pass): marks in span order. Also reserves the lines
  // covered by block widgets so the line pass can skip them.
  //
  // Spans arrive in non-decreasing `from` order, so line lookups walk
  // forward: one cached line replaces a binary search (plus a `Line`
  // allocation) per span — tens of thousands of them on a big document.
  let curLine = null;
  const lineNoAt = (pos) => {
    // Both bounds: the block-widget branch below queries `to` ahead of the
    // next span's `from`, so positions are not always monotonic.
    if (!curLine || pos > curLine.to || pos < curLine.from) curLine = doc.lineAt(pos);
    return curLine.number;
  };
  const marks = [];
  const blockLines = new Set();
  for (let i = 0; i < count; i++) {
    const tag = tags[i];
    const from = clamp(flat[2 * i]);
    const to = clamp(flat[2 * i + 1]);
    if (!(to > from)) continue;
    if (BLOCK_TAGS.has(tag)) {
      const first = lineNoAt(from);
      const last = lineNoAt(to);
      for (let line = first; line <= last; line++) blockLines.add(line);
      continue;
    }
    const mapped = decorationFor(tag);
    if (!mapped || mapped.kind === "line") continue;
    if (!showAll && revealed.has(lineNoAt(from))) continue;
    if (showAll && mapped.kind === "hide") continue;
    marks.push({ from, to, deco: mapped.deco });
  }

  // Pass 2 (line pass): one point decoration per tagged line, in line
  // order. Never on a cursor-revealed line, and never on a line covered by
  // a block widget (see BLOCK_TAGS).
  const seenLines = new Set();
  const lines = [];
  curLine = null;
  for (let i = 0; i < count; i++) {
    const tag = tags[i];
    const from = clamp(flat[2 * i]);
    const to = clamp(flat[2 * i + 1]);
    if (!(to > from)) continue;
    const mapped = decorationFor(tag);
    if (!mapped || mapped.kind !== "line") continue;
    const lineNo = lineNoAt(from);
    if (!showAll && revealed.has(lineNo)) continue;
    if (blockLines.has(lineNo)) continue;
    const key = `${lineNo}:${mapped.deco.spec.class}`;
    if (seenLines.has(key)) continue;
    seenLines.add(key);
    lines.push({ from: doc.line(lineNo).from, to: doc.line(lineNo).from, deco: mapped.deco });
  }

  // Single ordered fill: merge by (from, startSide). Line decorations sort
  // before marks at the same position (Side.Line < NonIncStart); any other
  // order makes the builder throw and drops every decoration. The provider
  // promises `from` order, but a defensive sort costs nothing next to a
  // throw that would blank the whole document.
  marks.sort((x, y) => x.from - y.from || x.to - y.to);
  const builder = new RangeSetBuilder();
  let a = 0;
  let b = 0;
  const takeMark = () => {
    const m = marks[a++];
    builder.add(m.from, m.to, m.deco);
  };
  const takeLine = () => {
    const l = lines[b++];
    builder.add(l.from, l.to, l.deco);
  };
  while (a < marks.length && b < lines.length) {
    const m = marks[a];
    const l = lines[b];
    if (l.from < m.from || (l.from === m.from && sideOf(l.deco, "start") <= sideOf(m.deco, "start"))) {
      takeLine();
    } else {
      takeMark();
    }
  }
  while (a < marks.length) takeMark();
  while (b < lines.length) takeLine();
  return builder.finish();
}

/**
 * CodeMirror extension rendering the live-format view.
 *
 * `options: { isSourceVisible: () => boolean, getSpanProvider: () => fn }`.
 * The caller owns both pieces of state; this module keeps none, so bundling
 * it twice (bridge bundle + standalone file) cannot split the truth.
 * Decorations are cached on the plugin value and rebuilt only when the
 * document, the selection, or the source-visible flag changes.
 */
export function livePreviewExtension(options = {}) {
  const isSourceVisible = options.isSourceVisible || (() => false);
  const getSpanProvider = options.getSpanProvider || (() => null);

  class LivePreview {
    constructor(view) {
      this.decorations = buildDecorations(view.state, getSpanProvider, isSourceVisible);
    }
    update(update) {
      let refresh = update.docChanged || update.selectionSet;
      if (!refresh) {
        for (const tr of update.transactions) {
          for (const effect of tr.effects) {
            if (effect.is(livePreviewRefresh)) {
              refresh = true;
              break;
            }
          }
          if (refresh) break;
        }
      }
      if (refresh) {
        this.decorations = buildDecorations(update.view.state, getSpanProvider, isSourceVisible);
      }
    }
  }

  return ViewPlugin.fromClass(LivePreview, {
    decorations: (v) => v.decorations,
  });
}
