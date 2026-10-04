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
import { Decoration, EditorView, ViewPlugin, WidgetType } from "@codemirror/view";
import { RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";

// SpanTag discriminants, mirroring `codedocs-core/src/markdown/live.rs`.
// Explicit numbers: renumbering silently recolors every decoration.
const TAG_HIDE = 0;
const TAG_STRONG = 1;
const TAG_EMPHASIS = 2;
const TAG_STRIKETHROUGH = 3;
const TAG_INLINE_CODE = 4;
const TAG_LINK_TEXT = 5;
const TAG_IMAGE_ALT = 6;
const TAG_HEADING_1 = 7;
const TAG_HEADING_6 = 12;
const TAG_QUOTE = 13;
const TAG_LIST_MARKER = 14;
const TAG_FENCE = 15;
const TAG_TABLE_PIPE = 16;
const TAG_TABLE_DELIMITER = 17;
const TAG_MATH_INLINE = 18;
const TAG_MATH_DISPLAY = 19;
const TAG_MERMAID = 20;

/**
 * Block-level widget tags (math display, mermaid). A line covered by
 * a block replacement must NOT also carry a `line` decoration: the two
 * ranges collide and `RangeSetBuilder` rejects the entire set, so *no*
 * decoration in the document appears — a failure that reads as "the engine
 * is broken" instead of "two decorations clashed". `blockLines` reserves
 * those lines, but only when the widget is actually emitted: on a
 * cursor-revealed line (or with source-visible on) there is no replacement,
 * so there is nothing to collide with and the raw source shows.
 */
const BLOCK_TAGS = new Set([TAG_MATH_DISPLAY, TAG_MERMAID]);

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
const DECO_TABLE_ROW = Decoration.line({ class: "cm-lp-table-row" });

/**
 * Task 4 widgets. One `WidgetType` subclass each; all replace their span
 * (`Decoration.replace`), so the cursor-reveal rule in `buildDecorations`
 * skips them on the cursor line and the raw source stays editable.
 *
 * `eq` is what keeps Mermaid usable: without it CodeMirror destroys and
 * rebuilds the widget on every transaction, re-rendering the diagram per
 * keystroke. With it the DOM node is reused while the source is unchanged.
 */

function paintMath(el, source, isDisplay, fallback) {
  const raw = fallback === undefined ? source : fallback;
  // `preview-bridge.mjs` owns the lazy KaTeX loader, so the megabytes stay
  // out of the startup path; this only calls the per-node entry point. The
  // bridge may be absent (Node unit tests have no `window`): fall back to the
  // raw source, never throw.
  const render =
    typeof window !== "undefined" && window.__codedocs_renderMathInto;
  if (typeof render !== "function") {
    el.textContent = raw;
    return;
  }
  try {
    const pending = render.call(window, el, source, isDisplay, raw);
    // The bridge promises to never reject; this only guards a future
    // regression from becoming an unhandled rejection per keystroke.
    if (pending && typeof pending.catch === "function") {
      pending.catch(() => {
        el.textContent = raw;
      });
    }
  } catch {
    el.textContent = raw;
  }
}

function paintMermaid(el, source) {
  const render =
    typeof window !== "undefined" && window.__codedocs_renderMermaidInto;
  if (typeof render !== "function") {
    const pre = document.createElement("pre");
    pre.textContent = source;
    el.replaceChildren(pre);
    return;
  }
  try {
    const pending = render.call(window, el, source);
    if (pending && typeof pending.catch === "function") {
      pending.catch(() => {
        const pre = document.createElement("pre");
        pre.textContent = source;
        el.replaceChildren(pre);
      });
    }
  } catch {
    const pre = document.createElement("pre");
    pre.textContent = source;
    el.replaceChildren(pre);
  }
}

class MathWidget extends WidgetType {
  constructor(source, isDisplay, raw) {
    super();
    this.source = source;
    this.isDisplay = !!isDisplay;
    // Full span text (delimiters included): what the fallback shows when
    // KaTeX fails, so the user sees the document text, not bare TeX.
    this.raw = raw === undefined ? source : raw;
  }
  eq(other) {
    return (
      other instanceof MathWidget &&
      other.source === this.source &&
      other.isDisplay === this.isDisplay &&
      other.raw === this.raw
    );
  }
  toDOM() {
    const el = document.createElement(this.isDisplay ? "div" : "span");
    el.className = this.isDisplay ? "cm-lp-math-block" : "cm-lp-math-inline";
    paintMath(el, this.source, this.isDisplay, this.raw);
    return el;
  }
  // Mandatory: without it clicks and selection inside the rendered math
  // never reach CodeMirror and the caret is lost.
  ignoreEvent() {
    return false;
  }
}

class MermaidWidget extends WidgetType {
  constructor(source) {
    super();
    this.source = source;
  }
  eq(other) {
    return other instanceof MermaidWidget && other.source === this.source;
  }
  toDOM() {
    const el = document.createElement("div");
    el.className = "cm-lp-mermaid";
    paintMermaid(el, this.source);
    return el;
  }
  // Same as MathWidget: without it clicks and selection inside the diagram
  // never reach CodeMirror, the caret stays put, and click-to-reveal of the
  // block never fires.
  ignoreEvent() {
    return false;
  }
}

class ImageWidget extends WidgetType {
  constructor(alt, src) {
    super();
    this.alt = alt;
    this.src = src;
  }
  eq(other) {
    return (
      other instanceof ImageWidget &&
      other.alt === this.alt &&
      other.src === this.src
    );
  }
  toDOM() {
    // No <img>: the CSP (`img-src 'self' data: blob:`) cannot resolve
    // workspace-relative paths, so the image would be broken in the editor
    // exactly as it is in the preview. Alt text with the path as tooltip.
    const el = document.createElement("span");
    el.className = "cm-lp-image";
    el.textContent = this.alt;
    if (this.src) el.title = this.src;
    return el;
  }
  // Same as MathWidget: clicks on the placeholder must reach CodeMirror so
  // the caret moves and the line reveals its raw source.
  ignoreEvent() {
    return false;
  }
}

/** Inner TeX of a `$…$` / `$$…$$` span (the span includes the delimiters). */
function mathSource(doc, from, to, isDisplay) {
  const d = isDisplay ? 2 : 1;
  const start = Math.min(from + d, to);
  return doc.sliceString(start, Math.max(start, to - d));
}

/** Diagram source of a fenced mermaid block span (fences excluded). */
function mermaidSource(doc, from, to) {
  const text = doc.sliceString(from, to);
  const open = text.indexOf("\n");
  const close = text.lastIndexOf("```");
  if (open < 0 || close <= open) return text;
  return text.slice(open + 1, close);
}

/**
 * Alt text and link target of an image. The `ImageAlt` span covers only the
 * alt text; the `](src)` lives in the hidden gap right after it, so the
 * target is re-read from the document. Unparseable target degrades to an
 * empty tooltip, never to a throw.
 */
function imageTarget(doc, from, to) {
  const alt = doc.sliceString(from, to);
  let src = "";
  const after = doc.sliceString(to, Math.min(doc.length, to + 512));
  const m = /^\]\(([^)\s]*)\)/.exec(after);
  if (m) src = m[1];
  return { alt, src };
}

/**
 * Map one span tag to its decoration. Returns `{ kind, deco }` with
 * `kind: "mark" | "line"`, or `null` for widget tags (math, mermaid, image —
 * built inline in pass 1, where the span range and document are at hand)
 * and unknown tags — skipping is safer than guessing.
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
/**
 * Shared preamble for the inline and block builders: fetch the spans,
 * compute the cursor-revealed lines, and hand out clamped positions plus a
 * forward-walking line lookup. Returns `null` when there is nothing to
 * decorate, so both builders degrade to `Decoration.none` instead of
 * guessing.
 */
function liveContext(state, getSpanProvider, isSourceVisible, text) {
  const doc = state.doc;
  const provider = getSpanProvider();
  if (typeof provider !== "function") return null;

  let pair = null;
  try {
    pair = provider(text === undefined ? doc.toString() : text);
  } catch {
    return null;
  }
  const flat = pair && pair[0];
  const tags = pair && pair[1];
  if (!flat || !tags || typeof flat.length !== "number" || typeof tags.length !== "number") {
    return null;
  }
  const count = Math.min(tags.length, Math.floor(flat.length / 2));
  if (count === 0) return null;

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

  return { doc, flat, tags, count, showAll, clamp, revealed, lineNoAt };
}

/**
 * The block widget for one span, or `null` when the block shows raw source
 * (cursor on it, or source-visible mode). Single funnel so the inline set
 * (which reserves these lines) and the block set (which emits them) agree
 * on exactly which lines are replaced.
 */
function blockWidgetFor(ctx, tag, from, to) {
  if (tag !== TAG_MATH_DISPLAY && tag !== TAG_MERMAID) return null;
  const first = ctx.lineNoAt(from);
  const last = ctx.lineNoAt(to);
  if (ctx.showAll) return null;
  for (let line = first; line <= last; line++) {
    if (ctx.revealed.has(line)) return null;
  }
  const widget =
    tag === TAG_MERMAID
      ? new MermaidWidget(mermaidSource(ctx.doc, from, to))
      : new MathWidget(mathSource(ctx.doc, from, to, true), true, ctx.doc.sliceString(from, to));
  return { widget, first, last };
}

/** True when the transaction carries a `livePreviewRefresh` effect. */
function hasRefreshEffect(tr) {
  for (const effect of tr.effects) {
    if (effect.is(livePreviewRefresh)) return true;
  }
  return false;
}

export function buildDecorations(state, getSpanProvider, isSourceVisible) {
  const ctx = liveContext(state, getSpanProvider, isSourceVisible);
  if (!ctx) return Decoration.none;
  const { doc, flat, tags, count, showAll, clamp, revealed, lineNoAt } = ctx;

  // Pass 1 (range pass): marks and inline widgets in span order. Also
  // reserves the lines covered by block widgets so the line pass can skip
  // them. Block replacements themselves live in `buildBlockDecorations`:
  // CodeMirror rejects block decorations from a `ViewPlugin`
  // (`RangeError: Block decorations may not be specified via plugins`), so
  // they are provided through `EditorView.decorations` instead.
  const marks = [];
  const blockLines = new Set();
  for (let i = 0; i < count; i++) {
    const tag = tags[i];
    const from = clamp(flat[2 * i]);
    const to = clamp(flat[2 * i + 1]);
    if (!(to > from)) continue;
    // Reserve only: the block set emits the widget (or nothing, on a
    // revealed line) through the same `blockWidgetFor` funnel.
    if (tag === TAG_MATH_DISPLAY || tag === TAG_MERMAID) {
      const b = blockWidgetFor(ctx, tag, from, to);
      if (b) {
        for (let line = b.first; line <= b.last; line++) blockLines.add(line);
      }
      continue;
    }
    if (tag === TAG_MATH_INLINE || tag === TAG_IMAGE_ALT) {
      if (!showAll && !revealed.has(lineNoAt(from))) {
        let widget;
        if (tag === TAG_MATH_INLINE) {
          widget = new MathWidget(mathSource(doc, from, to, false), false, doc.sliceString(from, to));
        } else {
          const target = imageTarget(doc, from, to);
          widget = new ImageWidget(target.alt, target.src);
        }
        // Fresh instance per span: widgets carry their source, so unlike
        // marks they cannot share one `Decoration`. They are rare (one per
        // formula or image), not one per keystroke per line.
        marks.push({ from, to, deco: Decoration.replace({ widget }) });
      }
      continue;
    }
    // Fallback for a future block tag with no widget yet: reserve the lines
    // so a `line` decoration can never collide with it.
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
  // a block widget (see BLOCK_TAGS). Table rows join here too: a row is not
  // a styled span but a line carrying hidden pipes, so each pipe or
  // delimiter span promotes its line to the flex-row style.
  const seenLines = new Set();
  const lines = [];
  for (let i = 0; i < count; i++) {
    const tag = tags[i];
    const from = clamp(flat[2 * i]);
    const to = clamp(flat[2 * i + 1]);
    if (!(to > from)) continue;
    const isTableRow = tag === TAG_TABLE_PIPE || tag === TAG_TABLE_DELIMITER;
    const mapped = isTableRow
      ? { kind: "line", deco: DECO_TABLE_ROW }
      : decorationFor(tag);
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
  // order makes the builder throw and drops every decoration. Inline widget
  // replacements carry their own side (just below marks), so the mark sort
  // keys on startSide too — sorting by (from, to) alone throws as soon as a
  // widget shares its `from` with a mark. The provider promises `from`
  // order, but a defensive sort costs nothing next to a throw that would
  // blank the whole document.
  marks.sort(
    (x, y) =>
      x.from - y.from ||
      sideOf(x.deco, "start") - sideOf(y.deco, "start") ||
      x.to - y.to,
  );
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
 * Build the block-widget set for an editor state: display math and mermaid
 * diagrams as `Decoration.replace({ block: true })`.
 *
 * Separate set because CodeMirror only accepts block decorations through
 * `EditorView.decorations`, never from a `ViewPlugin` (it throws
 * `RangeError`). Same purity contract as `buildDecorations`: no DOM here,
 * only widget descriptions — `toDOM` runs later, in the browser.
 */
export function buildBlockDecorations(state, getSpanProvider, isSourceVisible) {
  const text = state.doc.toString();
  // Fast path for the 50 ms per-keystroke gate: block spans only exist
  // inside fenced blocks (mermaid needs the ``` fence) or `$$…$$` (display
  // math), so a document with neither marker cannot hold a block widget and
  // the provider walk is pure overhead. Skipping it keeps the common case
  // at one `live_spans` pass per rebuild, exactly as before widgets.
  if (!text.includes("```") && !text.includes("$$")) return Decoration.none;
  const ctx = liveContext(state, getSpanProvider, isSourceVisible, text);
  if (!ctx) return Decoration.none;
  const { flat, tags, count, clamp } = ctx;
  const ranges = [];
  for (let i = 0; i < count; i++) {
    const tag = tags[i];
    const from = clamp(flat[2 * i]);
    const to = clamp(flat[2 * i + 1]);
    if (!(to > from)) continue;
    const b = blockWidgetFor(ctx, tag, from, to);
    if (!b) continue;
    // Fresh instance per span, as in the inline pass.
    ranges.push({ from, to, deco: Decoration.replace({ widget: b.widget, block: true }) });
  }
  ranges.sort(
    (x, y) =>
      x.from - y.from ||
      sideOf(x.deco, "start") - sideOf(y.deco, "start") ||
      x.to - y.to,
  );
  const builder = new RangeSetBuilder();
  for (const r of ranges) builder.add(r.from, r.to, r.deco);
  return builder.finish();
}

/**
 * CodeMirror extensions rendering the live-format view: a `ViewPlugin` for
 * the inline set (marks, lines, inline widgets) plus a `StateField`
 * provided through `EditorView.decorations` for the block widgets.
 *
 * `options: { isSourceVisible: () => boolean, getSpanProvider: () => fn }`.
 * The caller owns both pieces of state; this module keeps none, so bundling
 * it twice (bridge bundle + standalone file) cannot split the truth.
 * Decorations are cached and rebuilt only when the document, the selection,
 * or the source-visible flag changes.
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
          if (hasRefreshEffect(tr)) {
            refresh = true;
            break;
          }
        }
      }
      if (refresh) {
        this.decorations = buildDecorations(update.view.state, getSpanProvider, isSourceVisible);
      }
    }
  }

  // Block widgets cannot ride the plugin (see `buildBlockDecorations`), so
  // they get their own field with the same rebuild rule.
  const blockField = StateField.define({
    create(state) {
      return buildBlockDecorations(state, getSpanProvider, isSourceVisible);
    },
    update(value, tr) {
      const selChanged = !tr.newSelection.eq(tr.startState.selection);
      if (tr.docChanged || selChanged || hasRefreshEffect(tr)) {
        return buildBlockDecorations(tr.state, getSpanProvider, isSourceVisible);
      }
      return value;
    },
    provide: (f) => EditorView.decorations.from(f),
  });

  return [
    ViewPlugin.fromClass(LivePreview, {
      decorations: (v) => v.decorations,
    }),
    blockField,
  ];
}
