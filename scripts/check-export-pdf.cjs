/**
 * Verifies the PDF export's UI: the header button and `Ctrl+P`.
 *
 * Two things are checked here and neither can be checked from Rust:
 *
 * 1. `disabled` is real DOM state. The button is disabled with no file open
 *    (the compiler needs the note's folder) and outside Tauri (the web build
 *    has no picker and no compiler), so the test reads `button.disabled` from
 *    two different builds rather than from the app's internals.
 * 2. **How many times one Ctrl+P reaches the backend.** This is the bug this
 *    check exists for: `Mod-s` is bound twice — the global shortcut listener
 *    *and* the CodeMirror keymap — so one Ctrl+S opens two save dialogs, and
 *    `save_now` needs a re-entrancy guard to survive it. A command behind a
 *    native dialog and a subprocess must not inherit that, so the stub counts
 *    the invocations.
 *
 * The stub is backed by a real fixture folder on disk, and the note it hands
 * back is a real file, because the export's first step is resolving the note
 * against the workspace.
 *
 * Requires `npm run build`.
 */
const { chromium } = require("playwright");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const REPO = path.resolve(__dirname, "..");
const ROOT = path.join(REPO, "dist");
const PORT = Number(process.env.EXPORT_CHECK_PORT || 8093);

if (!fs.existsSync(path.join(ROOT, "index.html"))) {
  console.error(`FAIL: ${ROOT}/index.html not found. Run \`npm run build\` first.`);
  process.exit(1);
}

// Mirrors the shape Tauri 2 exposes on `window.__TAURI__`, plus the knobs this
// check needs: whether the next export is cancelled, and whether it fails.
const STUB = `
(function () {
  var calls = [];
  window.__ipcLog = calls;
  window.__fixtureRoot = __FIXTURE_ROOT__;
  window.__pdfRoute = __PDF_ROUTE__;
  window.__pdfCancels = false;
  window.__pdfFails = false;
  var nextId = 1;
  var handlers = new Map();

  window.__TAURI__ = {
    core: {
      invoke: function (cmd, args) {
        calls.push({ cmd: cmd, args: args });
        switch (cmd) {
          case "open_project_folder":
            return Promise.resolve(window.__fixtureRoot);
          case "list_markdown_files":
            return fetch(
              "/fixture-tree?folder=" + encodeURIComponent(args.folderPath)
            ).then(function (r) { return r.json(); });
          case "read_file":
            return fetch("/fixture-read?path=" + encodeURIComponent(args.pathStr))
              .then(function (r) { return r.text(); });
          case "export_pdf":
            if (window.__pdfFails) {
              window.__pdfFails = false;
              return Promise.reject(
                "El compilador de Typst falló:\\nerror: unknown variable: notfound"
              );
            }
            if (window.__pdfCancels) {
              window.__pdfCancels = false;
              return Promise.reject("Usuario cancelo la accion");
            }
            // Writes a file for real, so "did it export" can be answered by
            // looking at the disk instead of at the stub's own memory.
            return fetch("/fixture-pdf", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                path: window.__pdfRoute,
                content: args.content,
                note: args.pathStr,
              }),
            }).then(function (r) {
              return r.ok ? window.__pdfRoute : Promise.reject("no se pudo escribir");
            });
          default: return Promise.resolve(null);
        }
      },
    },
    event: {
      listen: function (name, handler) {
        var id = nextId++;
        handlers.set(id, { name: name, handler: handler });
        return Promise.resolve(id);
      },
      unlisten: function (name, id) {
        handlers.delete(id);
        return Promise.resolve();
      },
    },
  };

  window.__fireEvent = function (name, payload) {
    handlers.forEach(function (entry) {
      if (entry.name === name) entry.handler({ event: name, id: 0, payload: payload });
    });
  };
})();
`;

const conf = JSON.parse(
  fs.readFileSync(path.join(REPO, "src-tauri", "tauri.conf.json"), "utf8"),
);
const indexHtml = fs.readFileSync(path.join(ROOT, "index.html"), "utf8");
const hashes = [...indexHtml.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
  (m) => `'sha256-${crypto.createHash("sha256").update(m[1], "utf8").digest("base64")}'`,
);
const csp = conf.app.security.csp.replace("script-src 'self'", `script-src 'self' ${hashes.join(" ")}`);

const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "codedocs-export-"));
fs.writeFileSync(path.join(fixture, "nota.md"), "# Nota\n\nCon **negrita**.\n");
const pdfRoute = path.join(fixture, "salida.pdf");

// The stub is served as its own file: the CSP in effect allows inline scripts
// only by hash, and the hash list comes from the untouched `dist/index.html`.
const withStub = indexHtml.replace(
  '<script src="/public/codemirror-bridge.js"></script>',
  '<script src="/tauri-stub.js"></script>\n<script src="/public/codemirror-bridge.js"></script>',
);
// The same build with no `window.__TAURI__` at all: the browser demo, where the
// export button must be disabled rather than silently doing nothing.
const withoutStub = indexHtml;

const TYPES = {
  ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".css": "text/css", ".svg": "image/svg+xml", ".ttf": "font/ttf",
  ".woff": "font/woff", ".woff2": "font/woff2",
};

/** A folder's markdown tree, in the shape the backend returns. */
function listTree(dir) {
  const entries = [];
  for (const entry of fs
    .readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      entries.push({ name: entry.name, path: full, is_dir: true, children: listTree(full) });
    } else if (/\.md$/i.test(entry.name)) {
      entries.push({ name: entry.name, path: full, is_dir: false, children: [] });
    }
  }
  return entries;
}

const readBody = (req) =>
  new Promise((resolve) => {
    let raw = "";
    req.on("data", (chunk) => (raw += chunk));
    req.on("end", () => resolve(raw));
  });

const server = http.createServer((req, res) => {
  const [rawUrl, query = ""] = req.url.split("?");
  const url = decodeURIComponent(rawUrl);
  if (url === "/tauri-stub.js") {
    const body = STUB.replace("__FIXTURE_ROOT__", JSON.stringify(fixture))
      .replace("__PDF_ROUTE__", JSON.stringify(pdfRoute));
    res.writeHead(200, { "Content-Type": "text/javascript", "Content-Security-Policy": csp });
    return res.end(body);
  }
  if (url === "/fixture-tree") {
    const folder = new URLSearchParams(query).get("folder") || "";
    const body = { entries: fs.existsSync(folder) ? listTree(folder) : [], truncated: null };
    res.writeHead(200, { "Content-Type": "application/json", "Content-Security-Policy": csp });
    return res.end(JSON.stringify(body));
  }
  if (url === "/fixture-read") {
    const file = new URLSearchParams(query).get("path") || "";
    const body = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
    res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8", "Content-Security-Policy": csp });
    return res.end(body);
  }
  if (url === "/fixture-pdf") {
    readBody(req).then((raw) => {
      const { path: target } = JSON.parse(raw);
      fs.writeFileSync(target, "%PDF-1.7\n(exportado por el stub)");
      res.writeHead(200, { "Content-Type": "application/json", "Content-Security-Policy": csp });
      res.end("null");
    });
    return;
  }
  if (url === "/" || url === "/demo") {
    const body = url === "/demo" ? withoutStub : withStub;
    res.writeHead(200, { "Content-Type": "text/html", "Content-Security-Policy": csp });
    return res.end(body);
  }
  const file = path.join(ROOT, url);
  if (!file.startsWith(ROOT) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
    res.writeHead(404).end("not found");
    return;
  }
  res.writeHead(200, {
    "Content-Type": TYPES[path.extname(file)] || "application/octet-stream",
    "Content-Security-Policy": csp,
  });
  fs.createReadStream(file).pipe(res);
});

(async () => {
  await new Promise((r) => server.listen(PORT, r));
  const browser = await chromium.launch();
  const page = await browser.newPage();

  const errors = [];
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push("pageerror: " + e.message));

  const problems = [];
  const check = (name, cond, extra) => {
    if (cond) console.log(`  ok: ${name}${extra ? ` — ${extra}` : ""}`);
    else problems.push(extra ? `${name} — ${extra}` : name);
  };

  /** The export button, read from the DOM rather than from app internals. */
  const exportButton = () =>
    page.evaluate(() => {
      const button = document.querySelector('header button[aria-label="Exportar a PDF"]');
      return button ? { disabled: button.disabled, title: button.title } : null;
    });

  /** Open the folder and click the note, so a document is really open. */
  const openTheNote = async () => {
    await page.evaluate(() => window.__ipcLog.length = 0);
    await page.evaluate(() => {
      const button = [...document.querySelectorAll("button")].find(
        (b) => b.textContent.trim() === "Abrir carpeta del proyecto",
      );
      if (button) button.click();
    });
    await page.waitForTimeout(1200);
    await page.evaluate(() => {
      const row = [...document.querySelectorAll("li div")].find(
        (d) => d.textContent.trim() === "nota.md",
      );
      if (row) row.click();
    });
    await page.waitForTimeout(1200);
  };

  const ipcLog = () => page.evaluate(() => window.__ipcLog);
  const exportCalls = async () => (await ipcLog()).filter((c) => c.cmd === "export_pdf");
  const toastText = () =>
    page.evaluate(() => {
      const toast = document.querySelector(".fixed.bottom-12");
      return toast ? toast.textContent.trim() : null;
    });

  // === The browser demo: no Tauri, so the button must be disabled ===
  await page.goto(`http://localhost:${PORT}/demo`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);
  const inDemo = await exportButton();
  check("el botón existe en la demo web", inDemo !== null, JSON.stringify(inDemo));
  check(
    "el botón está deshabilitado fuera de Tauri",
    inDemo && inDemo.disabled === true,
    `disabled=${inDemo && inDemo.disabled}`,
  );

  // === The app: Tauri present, no document open yet ===
  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);
  const noFile = await exportButton();
  check("el botón está deshabilitado sin un archivo abierto", noFile && noFile.disabled === true, `disabled=${noFile && noFile.disabled}`);

  await openTheNote();
  const withFile = await exportButton();
  check("el botón se habilita con un archivo abierto", withFile && withFile.disabled === false, `disabled=${withFile && withFile.disabled}`);
  check("el botón anuncia su atajo", withFile && withFile.title.includes("Ctrl+P"), JSON.stringify(withFile && withFile.title));

  // === Ctrl+P reaches the backend exactly once ===
  //
  // This is the case `Mod-s` fails: bound in the global listener *and* in the
  // CodeMirror keymap. One keystroke must produce one export.
  await page.evaluate(() => window.__ipcLog.length = 0);
  await page.click(".cm-editor .cm-content");
  await page.waitForTimeout(200);
  await page.keyboard.press("Control+p");
  await page.waitForTimeout(1500);
  const fromKey = await exportCalls();
  check(
    "Ctrl+P dispara la exportación una sola vez",
    fromKey.length === 1,
    `invocaciones=${fromKey.length}`,
  );
  check(
    "Ctrl+P manda la nota abierta y el contenido",
    fromKey.length === 1 &&
      typeof fromKey[0].args.pathStr === "string" &&
      fromKey[0].args.pathStr.endsWith("nota.md") &&
      typeof fromKey[0].args.content === "string" &&
      fromKey[0].args.content.includes("negrita"),
    JSON.stringify(fromKey.map((c) => c.args)),
  );
  check("la exportación escribe el PDF", fs.existsSync(pdfRoute));
  check(
    "el usuario ve dónde quedó el PDF",
    ((await toastText()) || "").includes(pdfRoute),
    JSON.stringify(await toastText()),
  );

  // === The button fires exactly once too, and the same path ===
  await page.evaluate(() => window.__ipcLog.length = 0);
  const box = await page.evaluate(() => {
    const button = document.querySelector('header button[aria-label="Exportar a PDF"]');
    const r = button.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  });
  await page.mouse.click(box.x, box.y);
  await page.waitForTimeout(1500);
  const fromButton = await exportCalls();
  check("el botón dispara la exportación una sola vez", fromButton.length === 1, `invocaciones=${fromButton.length}`);

  // === A dismissed dialog is not an error ===
  //
  // The toast from the previous case is dismissed first: it is replaced by the
  // next message or by a click, never by a timer, so leaving it up would make
  // this case read a message the export never sent.
  await page.evaluate(() => {
    const toast = document.querySelector(".fixed.bottom-12");
    if (toast) toast.click();
  });
  await page.waitForTimeout(300);
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
    window.__pdfCancels = true;
  });
  await page.keyboard.press("Control+p");
  await page.waitForTimeout(1200);
  const cancelled = await exportCalls();
  check("cancelar igual intenta exportar", cancelled.length === 1, `invocaciones=${cancelled.length}`);
  check("cancelar no muestra un error", (await toastText()) === null, JSON.stringify(await toastText()));

  // === A failing compile shows what the compiler said ===
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
    window.__pdfFails = true;
  });
  await page.keyboard.press("Control+p");
  await page.waitForTimeout(1200);
  const failureToast = await toastText();
  check(
    "un fallo del compilador se muestra con su mensaje",
    !!failureToast && failureToast.includes("unknown variable"),
    JSON.stringify(failureToast),
  );

  for (const e of errors) {
    if (/reading 'then'|\.then.*undefined|panicked|assertion/i.test(e)) {
      problems.push("runtime error: " + e.split("\n")[0].slice(0, 160));
    }
  }

  await browser.close();
  server.close();
  fs.rmSync(fixture, { recursive: true, force: true });

  if (problems.length > 0) {
    console.error(`FAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("PASS: el botón y Ctrl+P exportan una sola vez.");
})().catch((e) => {
  console.error("FAIL: " + e.message);
  process.exit(1);
});
