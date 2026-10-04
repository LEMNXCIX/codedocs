// Preview enhancement bridge.
//
// Turns the renderer's placeholders into rendered math / diagrams:
//   <span class="math-inline">TeX</span>    -> KaTeX HTML
//   <span class="math-display">TeX</span>  -> KaTeX HTML (display mode)
//   <pre class="mermaid-block">SOURCE</pre> -> mermaid <svg>
//
// The TeX and the mermaid source are always TEXT CONTENT of the element (the
// renderer escapes them; raw HTML from the document is stripped upstream), so
// they are read with `textContent` and written back with `textContent` or DOM
// construction -- never with `innerHTML` string concatenation, which would
// turn document text back into executable markup.
//
// KaTeX (268 KB) and mermaid (3.2 MB) are injected on demand, the first time a
// placeholder that needs them is found, so a document without math or diagrams
// never pays for them.
//
// Rust calls `__codedocs_render_enhancements()` via Reflect::get + call0 and
// does NOT await the result, so every entry point here is fire-and-forget safe:
// loading is kicked off and we return immediately, then the pass re-runs once
// the library lands. `__codedocs_render_enhancements_async()` is the awaitable
// variant.

var KATEX_CSS = "/public/vendor/katex/katex.min.css";
var KATEX_JS = "/public/vendor/katex/katex.min.js";
var MERMAID_JS = "/public/vendor/mermaid/mermaid.min.js";

var MATH_SELECTOR = ".math-inline, .math-display";
var MERMAID_SELECTOR = ".mermaid-block";

// Marks a node as already handled so repeated passes (re-renders, deferred
// retries after a library finishes loading) skip it instead of rendering twice.
var DONE_ATTR = "data-cd-done";

function noop() {}

/** Unique DOM id for mermaid's temporary render target. */
function uniqueId(prefix) {
  var c = window.crypto;
  // randomUUID needs a secure context; over plain http on a LAN address it is
  // undefined, so keep a fallback.
  if (c && typeof c.randomUUID === "function") {
    try {
      return prefix + "-" + c.randomUUID();
    } catch (e) {
      /* fall through to the random fallback */
    }
  }
  return prefix + "-" + Math.random().toString(36).slice(2, 11);
}

function head() {
  return document.head || document.documentElement;
}

function injectLink(href) {
  return new Promise(function (resolve) {
    var link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = href;
    // A missing stylesheet must not block math from rendering, so resolve on
    // error too.
    link.addEventListener("load", function () {
      resolve(true);
    });
    link.addEventListener("error", function () {
      resolve(false);
    });
    head().appendChild(link);
  });
}

function injectScript(src) {
  return new Promise(function (resolve, reject) {
    var script = document.createElement("script");
    script.src = src;
    script.async = false;
    script.addEventListener("load", function () {
      resolve(true);
    });
    script.addEventListener("error", function () {
      reject(new Error("codedocs: failed to load " + src));
    });
    head().appendChild(script);
  });
}

// Promise-cached loaders: concurrent callers share a single fetch, and a failed
// load is not retried on every keystroke (the promise stays cached).
var katexLoad = null;
var mermaidLoad = null;

function loadKatex() {
  if (katexLoad) return katexLoad;
  // The stylesheet is cosmetic (sizing + webfonts), so it is injected in
  // parallel and deliberately NOT awaited: a slow or blocked stylesheet must
  // never hold up math rendering.
  injectLink(KATEX_CSS).then(noop, noop);
  katexLoad = injectScript(KATEX_JS).then(function () {
    return window.katex;
  });
  return katexLoad;
}

function loadMermaid() {
  if (mermaidLoad) return mermaidLoad;
  mermaidLoad = injectScript(MERMAID_JS).then(function () {
    var mermaid = window.mermaid;
    if (mermaid && typeof mermaid.initialize === "function") {
      try {
        // We drive rendering ourselves; mermaid must not also scan the document.
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
      } catch (e) {
        /* keep the defaults */
      }
    }
    return mermaid;
  });
  return mermaidLoad;
}

function isDone(el) {
  return el.hasAttribute && el.hasAttribute(DONE_ATTR);
}

/**
 * Show diagram source that mermaid could not render.
 *
 * DOM construction, not `el.innerHTML = "<pre>" + code + "</pre>"`: the source
 * comes from the document and must never be reparsed as markup.
 */
function showSource(el, code) {
  try {
    var pre = document.createElement("pre");
    pre.textContent = code;
    el.replaceChildren(pre);
  } catch (e) {
    el.textContent = code;
  }
}

function renderMathNode(el, katex, displayMode) {
  if (isDone(el)) return;
  // Mark before rendering: katex.render rewrites this element's children, and
  // a retry loop over a broken formula would never settle.
  el.setAttribute(DONE_ATTR, "1");
  var tex = el.textContent;
  try {
    katex.render(tex, el, { throwOnError: false, displayMode: displayMode });
  } catch (e) {
    // katex may have replaced the children before throwing; put the source back.
    el.textContent = tex;
  }
}

function renderMath(katex) {
  if (!katex || typeof katex.render !== "function") return;
  var inline = document.querySelectorAll(".math-inline");
  var display = document.querySelectorAll(".math-display");
  var i;
  for (i = 0; i < inline.length; i++) {
    renderMathNode(inline[i], katex, false);
  }
  for (i = 0; i < display.length; i++) {
    // textContent keeps the TeX verbatim, newlines included.
    renderMathNode(display[i], katex, true);
  }
}

function renderMermaidNode(el, mermaid, code) {
  var id = uniqueId("mermaid");
  var result;
  try {
    result = mermaid.render(id, code);
  } catch (e) {
    showSource(el, code);
    return;
  }
  if (!result || typeof result.then !== "function") {
    showSource(el, code);
    return;
  }
  result
    .then(function (out) {
      if (out && out.svg) {
        // Trusted: mermaid generated this SVG itself.
        el.innerHTML = out.svg;
      } else {
        showSource(el, code);
      }
    })
    .catch(function () {
      showSource(el, code);
    });
}

function renderMermaid(mermaid) {
  if (!mermaid || typeof mermaid.render !== "function") return;
  var blocks = document.querySelectorAll(MERMAID_SELECTOR);
  for (var i = 0; i < blocks.length; i++) {
    var el = blocks[i];
    if (isDone(el)) continue;
    // Mark before the async render so concurrent passes do not start it twice.
    el.setAttribute(DONE_ATTR, "1");
    // Source is text content -- the renderer emits no data-* attribute.
    var code = el.textContent;
    if (!code || !code.trim()) continue;
    renderMermaidNode(el, mermaid, code);
  }
}

/** Kick off KaTeX rendering; returns immediately. */
window.__codedocs_render_math = function () {
  try {
    if (!document.querySelector(MATH_SELECTOR)) return;
    if (window.katex) {
      renderMath(window.katex);
      return;
    }
    loadKatex().then(function (katex) {
      // Re-run the pass: the placeholders found before loading are still there,
      // and renderMath skips anything already marked done.
      renderMath(katex);
    }, noop);
  } catch (e) {
    noop();
  }
};

/** Kick off mermaid rendering; returns immediately. */
window.__codedocs_render_mermaid = function () {
  try {
    if (!document.querySelector(MERMAID_SELECTOR)) return;
    if (window.mermaid) {
      renderMermaid(window.mermaid);
      return;
    }
    loadMermaid().then(function (mermaid) {
      renderMermaid(mermaid);
    }, noop);
  } catch (e) {
    noop();
  }
};

/**
 * Entry point called from Rust (Reflect::get + call0, not awaited).
 * Fire-and-forget: returns immediately, the deferred passes above do the work.
 */
window.__codedocs_render_enhancements = function () {
  // One malformed diagram must not take the whole preview down with it.
  try {
    window.__codedocs_render_math();
  } catch (e) {
    noop();
  }
  try {
    window.__codedocs_render_mermaid();
  } catch (e) {
    noop();
  }
};

/**
 * Awaitable variant: resolves after the libraries this document needs are
 * loaded and the render pass has run. Never rejects.
 */
window.__codedocs_render_enhancements_async = function () {
  // Render with the handles the loaders resolve with, not a re-read of the
  // globals: the promise cache can outlive a global that is later cleared.
  var mathLib = window.katex;
  var mermaidLib = window.mermaid;
  var waits = [];
  try {
    if (!mathLib && document.querySelector(MATH_SELECTOR)) {
      waits.push(loadKatex().then(function (lib) { mathLib = lib; }));
    }
    if (!mermaidLib && document.querySelector(MERMAID_SELECTOR)) {
      waits.push(loadMermaid().then(function (lib) { mermaidLib = lib; }));
    }
  } catch (e) {
    /* ignore: the sync entry point still ran */
  }
  return Promise.all(waits).then(function () {
    try {
      if (mathLib) renderMath(mathLib);
      if (mermaidLib) renderMermaid(mermaidLib);
    } catch (e) {
      noop();
    }
  });
};
