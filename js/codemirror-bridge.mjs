import { EditorView, basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { oneDark } from "@codemirror/theme-one-dark";
import { languages } from "@codemirror/language-data";
import { EditorState, Compartment, Prec } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { livePreviewExtension, livePreviewRefresh } from "./live-preview.mjs";

const themeCompartment = new Compartment();

let currentView = null;
let onChangeCallback = null;
// Live-format mode: the span provider labels document spans (markdown stays
// the document), and `sourceVisible` shows the raw source with styling kept.
let spanProvider = null;
let sourceVisible = false;

/** Re-run the live-preview plugin without touching the document. */
function refreshLivePreview() {
  if (currentView) {
    currentView.dispatch({ effects: livePreviewRefresh.of(null) });
  }
}

function setSourceVisible(isVisible) {
  sourceVisible = !!isVisible;
  refreshLivePreview();
}

/**
 * Formatting commands, keyed by the shortcut that triggers them.
 *
 * Wrapping is implemented with a transaction rather than a raw string insert so
 * it works with an empty selection: `**` around a collapsed cursor leaves the
 * caret between the delimiters, ready to type.
 */
const WRAP_COMMANDS = {
  bold: "**",
  italic: "*",
  inlineCode: "`",
  strikethrough: "~~",
};

/** Wrap the selection (or the empty selection) in `wrapper`. */
function wrapSelection(wrapper) {
  if (!currentView) return;
  const view = currentView;
  const changes = view.state.changeByRange((range) => ({
    from: range.from,
    to: range.to,
    insert: wrapper + view.state.sliceDoc(range.from, range.to) + wrapper,
  }));
  view.dispatch({ changes, scrollIntoView: true });
  // Put the caret between the delimiters instead of after the closing one.
  view.dispatch({
    selection: { anchor: view.state.selection.main.from + wrapper.length },
  });
  view.focus();
}

/** Toggle the wrapped form, so re-pressing the shortcut unwraps. */
function toggleWrap(wrapper) {
  if (!currentView) return;
  const view = currentView;
  const { from, to } = view.state.selection.main;
  const selected = view.state.sliceDoc(from, to);

  if (selected.startsWith(wrapper) && selected.endsWith(wrapper) && selected.length >= wrapper.length * 2) {
    const inner = selected.slice(wrapper.length, selected.length - wrapper.length);
    view.dispatch({
      changes: { from, to, insert: inner },
      selection: { anchor: from, head: from + inner.length },
    });
  } else {
    wrapSelection(wrapper);
    return;
  }
  view.focus();
}

function getExtensions(isDark) {
  const wrapKey = (key, wrapper) => ({
    key,
    preventDefault: true,
    run: () => {
      toggleWrap(wrapper);
      return true;
    },
  });

  return [
    basicSetup,
    markdown({ base: markdownLanguage, codeLanguages: languages }),
    themeCompartment.of(isDark ? oneDark : []),
    EditorView.lineWrapping,
    livePreviewExtension({
      isSourceVisible: () => sourceVisible,
      getSpanProvider: () => spanProvider,
    }),
    // `Prec.highest`, not a second `keymap.of` argument (which `Facet.of`
    // ignores): without it this keymap ties with `basicSetup`'s default
    // keymap, and `basicSetup` comes first in the array, so its `Mod-/`
    // (toggleComment, which rewrites the document) wins over the toggle.
    Prec.highest(
      keymap.of([
        {
          key: "Mod-s",
          preventDefault: true,
          run: () => {
            if (window.__codedocs_save) window.__codedocs_save();
            return true;
          },
        },
        wrapKey("Mod-b", WRAP_COMMANDS.bold),
        wrapKey("Mod-i", WRAP_COMMANDS.italic),
        wrapKey("Mod-e", WRAP_COMMANDS.inlineCode),
        wrapKey("Mod-Shift-s", WRAP_COMMANDS.strikethrough),
        {
          key: "Mod-k",
          preventDefault: true,
          run: () => {
            window.__codedocs_insert_link();
            return true;
          },
        },
        // Source-visible toggle. Highest precedence, like the wrap keys:
        // `basicSetup` binds Mod-/ to toggleComment, which would rewrite the
        // document (`<!-- -->`) instead of revealing it.
        {
          key: "Mod-/",
          preventDefault: true,
          run: () => {
            setSourceVisible(!sourceVisible);
            return true;
          },
        },
      ]),
    ),
    EditorView.updateListener.of((update) => {
      if (update.docChanged && onChangeCallback) {
        onChangeCallback(update.state.doc.toString());
      }
    }),
  ];
}

window.__codedocs_createEditor = function (parentEl, initialContent, isDark) {
  if (currentView) {
    currentView.destroy();
  }

  const state = EditorState.create({
    doc: initialContent || "",
    extensions: getExtensions(isDark),
  });

  currentView = new EditorView({
    state,
    parent: parentEl,
  });

  return currentView;
};

window.__codedocs_getContent = function () {
  if (!currentView) return "";
  return currentView.state.doc.toString();
};

window.__codedocs_setContent = function (content) {
  if (!currentView) return;
  currentView.dispatch({
    changes: {
      from: 0,
      to: currentView.state.doc.length,
      insert: content,
    },
  });
};

window.__codedocs_setTheme = function (isDark) {
  if (!currentView) return;
  currentView.dispatch({
    effects: themeCompartment.reconfigure(isDark ? oneDark : []),
  });
};

window.__codedocs_setOnChange = function (callback) {
  onChangeCallback = callback;
};

window.__codedocs_focus = function () {
  if (currentView) currentView.focus();
};

window.__codedocs_destroyEditor = function () {
  if (currentView) {
    currentView.destroy();
    currentView = null;
  }
  onChangeCallback = null;
  spanProvider = null;
  sourceVisible = false;
};

/**
 * Registers the span provider the live-preview plugin calls with the full
 * document text on every redecoration. The callback never modifies the
 * document; it returns an `Array` of two elements: a `Uint32Array` with
 * `ranges` (`[from0, to0, ...]`, UTF-16 units) and a `Uint8Array` with the
 * `SpanTag` discriminants of `tags`.
 */
window.__codedocs_setSpanProvider = function (callback) {
  spanProvider = typeof callback === "function" ? callback : null;
  refreshLivePreview();
};

/** Shows (`true`) or hides (`false`) the raw markdown source. */
window.__codedocs_setSourceVisible = function (isVisible) {
  setSourceVisible(isVisible);
};

window.__codedocs_wrap_selection = function (wrapper) {
  wrapSelection(wrapper);
};

window.__codedocs_toggle_wrap = function (wrapper) {
  toggleWrap(wrapper);
};

window.__codedocs_insert_link = function () {
  if (!currentView) return;
  const view = currentView;
  const { from, to } = view.state.selection.main;
  const selected = view.state.sliceDoc(from, to);

  // Already a link: rewrite only the URL, keeping the label.
  const asLink = /^\[([^\]]*)\]\(([^)]*)\)$/.exec(selected);
  if (asLink) {
    const label = asLink[1] || "text";
    view.dispatch({
      changes: { from, to, insert: `[${label}]()` },
      selection: { anchor: from + label.length + 3 },
    });
    view.focus();
    return;
  }

  const label = selected || "text";
  view.dispatch({
    changes: { from, to, insert: `[${label}]()` },
    selection: { anchor: from + label.length + 3 },
  });
  view.focus();
};
