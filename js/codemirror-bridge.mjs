import { EditorView, basicSetup } from "codemirror";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { oneDark } from "@codemirror/theme-one-dark";
import { languages } from "@codemirror/language-data";
import { EditorState, Compartment } from "@codemirror/state";
import { keymap } from "@codemirror/view";

const themeCompartment = new Compartment();

let currentView = null;
let onChangeCallback = null;

function getExtensions(isDark) {
  return [
    basicSetup,
    markdown({ base: markdownLanguage, codeLanguages: languages }),
    themeCompartment.of(isDark ? oneDark : []),
    EditorView.lineWrapping,
    EditorView.updateListener.of((update) => {
      if (update.docChanged && onChangeCallback) {
        onChangeCallback(update.state.doc.toString());
      }
    }),
    keymap.of([
      {
        key: "Mod-s",
        run: () => {
          if (window.__codedocs_save) window.__codedocs_save();
          return true;
        },
      },
      {
        key: "Mod-b",
        run: () => {
          if (window.__codedocs_wrap_selection) window.__codedocs_wrap_selection("**");
          return true;
        },
      },
      {
        key: "Mod-i",
        run: () => {
          if (window.__codedocs_wrap_selection) window.__codedocs_wrap_selection("*");
          return true;
        },
      },
      {
        key: "Mod-k",
        run: () => {
          if (window.__codedocs_insert_link) window.__codedocs_insert_link();
          return true;
        },
      },
    ]),
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
};

window.__codedocs_wrap_selection = function (wrapper) {
  if (!currentView) return;
  const { from, to } = currentView.state.selection.main;
  const selected = currentView.state.sliceDoc(from, to);
  currentView.dispatch({
    changes: { from, to, insert: wrapper + selected + wrapper },
  });
  currentView.focus();
};

window.__codedocs_insert_link = function () {
  if (!currentView) return;
  const { from, to } = currentView.state.selection.main;
  const selected = currentView.state.sliceDoc(from, to);
  const linkText = selected || "text";
  currentView.dispatch({
    changes: { from, to, insert: `[${linkText}](url)` },
  });
  currentView.focus();
};
