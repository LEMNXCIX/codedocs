/**
 * Exercises the filesystem-facing flows against a stubbed Tauri IPC.
 *
 * This is a regression test for a crash that shipped: `js_namespace` in a
 * `#[wasm_bindgen]` block only says *where* to look, and the property name
 * defaults to the Rust function name. Because the Rust-side bindings are
 * prefixed `raw_`, the generated glue called `__TAURI__.core.raw_invoke`,
 * which does not exist. Every IPC call resolved to `undefined`, the generated
 * promise glue threw "undefined is not an object (evaluating 'arg0.then')",
 * and opening a folder did nothing.
 *
 * Neither the Rust compiler nor a plain browser load can catch that: the glue
 * is emitted at build time and the failure only appears when a command is
 * actually invoked. Stubbing `window.__TAURI__` runs the real frontend WASM
 * through the real code path.
 *
 * The stub is backed by the *real* filesystem over two tiny endpoints
 * (`/fixture-tree`, `/fixture-write`), so the sidebar can only show a file that
 * was genuinely written and a save can be verified by reading the file back off
 * disk afterwards. Asserting against the stub's own memory would only prove the
 * stub agrees with itself.
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
const PORT = Number(process.env.IPC_CHECK_PORT || 8090);

if (!fs.existsSync(path.join(ROOT, "index.html"))) {
  console.error(`FAIL: ${ROOT}/index.html not found. Run \`npm run build\` first.`);
  process.exit(1);
}

// Mirrors the shape Tauri 2 exposes on `window.__TAURI__`.
const STUB = `
(function () {
  var calls = [];
  window.__ipcLog = calls;
  window.__fixtureRoot = __FIXTURE_ROOT__;
  window.__emptyRoot = __EMPTY_ROOT__;
  window.__saveRoute = __SAVE_ROUTE__;
  window.__saveCancels = false;
  // Makes the next save_file reject, so the failure path can be driven.
  window.__saveFails = false;
  // Makes the next list_markdown_files reject, so the "opening a folder failed"
  // path can be driven without breaking a real folder on disk.
  window.__listFails = false;
  var nextId = 1;
  var handlers = new Map();

  window.__TAURI__ = {
    core: {
      invoke: function (cmd, args) {
        calls.push({ cmd: cmd, args: args });
        switch (cmd) {
          case "open_project_folder":
            return Promise.resolve(window.__openRoot || window.__fixtureRoot);
          case "save_file_as":
            // The one place the stub decides the user cancelled, so the cancel
            // path can be driven without a native dialog.
            return window.__saveCancels
              ? Promise.reject("Usuario cancelo la accion")
              : Promise.resolve(window.__saveRoute);
          case "list_markdown_files":
            if (window.__listFails) {
              window.__listFails = false;
              return Promise.reject("No se pudo leer la carpeta: permiso denegado");
            }
            // The command returns a FileTree (entries + truncated), not a bare
            // array. Returning the array shape left the sidebar empty and this
            // check reporting "file tree did not render" for a failure that was
            // in the stub, not in the app.
            //
            // Read from disk on every call, so a file that only exists because
            // a save actually wrote it shows up in the tree.
            return fetch(
              "/fixture-tree?folder=" + encodeURIComponent(args.folderPath)
            ).then(function (r) { return r.json(); });
          case "save_file":
            if (window.__saveFails) {
              window.__saveFails = false;
              return Promise.reject("Error al guardar el archivo: no queda espacio");
            }
            return fetch("/fixture-write", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ path: args.pathStr, content: args.content }),
            }).then(function (r) {
              return r.ok ? null : Promise.reject("Error al guardar el archivo");
            });
          case "create_file":
            // Creates the file for real, so the tree and the disk agree about
            // what "Nuevo Archivo" produced.
            return fetch("/fixture-create", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ folder: args.folderPath, name: args.name }),
            }).then(function (r) { return r.text(); }).then(JSON.parse);
          case "read_file": return Promise.resolve("# Hola\\n\\nContenido de prueba.\\n");
          default: return Promise.resolve(null);
        }
      },
    },
    event: {
      listen: function (name, handler) {
        var id = nextId++;
        calls.push({ listen: name, id: id });
        handlers.set(id, { name: name, handler: handler });
        return Promise.resolve(id);
      },
      unlisten: function (name, id) {
        calls.push({ unlisten: name, id: id });
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

const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "codedocs-ipc-"));
fs.writeFileSync(path.join(fixture, "Bienvenido.md"), "# Hola\n\nContenido de prueba.\n");
// A folder *inside* the fixture, so "the parent of the chosen file" is a real
// directory that is not the workspace root. A route directly under the fixture
// root would let `parent_dir` returning the wrong thing still look plausible.
fs.mkdirSync(path.join(fixture, "Guardado"));
const saveRoute = path.join(fixture, "Guardado", "prueba.md");
const saveParent = path.dirname(saveRoute);
// A folder with no markdown at all, for the empty-state check. A *sibling* of
// the fixture, not a subdirectory of it: the sidebar lists nested folders as
// rows, and this one has to be invisible to the rest of the flow.
const emptyFolder = fs.mkdtempSync(path.join(os.tmpdir(), "codedocs-vacia-"));
fs.writeFileSync(path.join(emptyFolder, "no-es-markdown.txt"), "nada que ver");

const conf = JSON.parse(
  fs.readFileSync(path.join(REPO, "src-tauri", "tauri.conf.json"), "utf8"),
);
let indexHtml = fs.readFileSync(path.join(ROOT, "index.html"), "utf8");
const hashes = [...indexHtml.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
  (m) => `'sha256-${crypto.createHash("sha256").update(m[1], "utf8").digest("base64")}'`,
);
const csp = conf.app.security.csp.replace("script-src 'self'", `script-src 'self' ${hashes.join(" ")}`);

// The stub must exist before the WASM bootstrap, so `is_tauri()` is true and the
// IPC path is taken.
//
// It is served as a separate file rather than inlined: the CSP in effect here
// allows inline scripts only by hash, and the hash list is computed from the
// untouched `dist/index.html`, so any extra inline script would be blocked —
// which would make this test fail for a reason that has nothing to do with IPC.
const withStub = indexHtml.replace(
  '<script src="/public/codemirror-bridge.js"></script>',
  '<script src="/tauri-stub.js"></script>\n<script src="/public/codemirror-bridge.js"></script>',
);
// The same build with no `window.__TAURI__` at all: the browser demo. Used to
// check that saving without a file does not reach for a native dialog that the
// browser build does not have.
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
    // The fixture root and the save route are baked in at serve time so no
    // inline script is needed.
    const body = STUB.replace("__FIXTURE_ROOT__", JSON.stringify(fixture))
      .replace("__EMPTY_ROOT__", JSON.stringify(emptyFolder))
      .replace("__SAVE_ROUTE__", JSON.stringify(saveRoute));
    res.writeHead(200, { "Content-Type": "text/javascript", "Content-Security-Policy": csp });
    return res.end(body);
  }
  if (url === "/fixture-tree") {
    const folder = new URLSearchParams(query).get("folder") || "";
    const body = { entries: fs.existsSync(folder) ? listTree(folder) : [], truncated: null };
    res.writeHead(200, { "Content-Type": "application/json", "Content-Security-Policy": csp });
    return res.end(JSON.stringify(body));
  }
  if (url === "/fixture-create") {
    readBody(req).then((raw) => {
      const { folder, name } = JSON.parse(raw);
      const target = path.join(folder, name);
      fs.mkdirSync(folder, { recursive: true });
      fs.writeFileSync(target, "# Nuevo archivo\n");
      res.writeHead(200, { "Content-Type": "application/json", "Content-Security-Policy": csp });
      res.end(JSON.stringify(target));
    });
    return;
  }
  if (url === "/fixture-write") {
    readBody(req).then((raw) => {
      const { path: target, content } = JSON.parse(raw);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, content);
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

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);

  // The folder label is the only `<p>` the Files tab renders, and it is
  // `state.path` verbatim — read from the DOM rather than from app internals.
  const workspaceLabel = () =>
    page.evaluate(() => {
      const p = document.querySelector("aside p");
      return p ? p.textContent.trim() : null;
    });
  const headerLabel = () =>
    page.evaluate(() => {
      const span = document.querySelector("header span");
      return span ? span.textContent.trim() : null;
    });
  const treeRows = () =>
    page.evaluate(() =>
      [...document.querySelectorAll("aside li")].map((li) => li.textContent.trim()),
    );
  const toastText = () =>
    page.evaluate(() => {
      const el = document.querySelector(".fixed.bottom-12");
      return el ? el.textContent.trim() : null;
    });

  // --- Guard: the sidebar never advertised which build it is ---------------
  const asideText = await page.evaluate(() => {
    const aside = document.querySelector("aside");
    return aside ? aside.textContent : null;
  });
  // Asserting a negative against an empty selector proves nothing, so first
  // check the sidebar really rendered.
  check("el sidebar se renderizó", !!asideText && asideText.includes("CodeDocs"));
  check(
    "el sidebar no dice 'Escritorio (Nativo)' ni 'Web (Demo Mode)'",
    !!asideText && !/Escritorio \(Nativo\)|Web \(Demo Mode\)/.test(asideText),
    JSON.stringify(asideText ? asideText.slice(0, 60) : asideText),
  );

  // --- Save with no file: type first, and check nothing fires on its own ---
  const TYPED = "Hola mundo";
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
  });
  await page.click(".cm-editor .cm-content");
  await page.waitForTimeout(300);
  await page.keyboard.type(TYPED, { delay: 20 });
  // Longer than the 1500 ms autosave debounce: if typing could reach a native
  // dialog, it would have by now.
  await page.waitForTimeout(2200);
  const whileTyping = await page.evaluate(() => window.__ipcLog.map((c) => c.cmd));
  check(
    "escribir no dispara ningún guardado ni diálogo",
    !whileTyping.includes("save_file") && !whileTyping.includes("save_file_as"),
    JSON.stringify(whileTyping),
  );
  check("nada se escribió en disco mientras se escribe", !fs.existsSync(saveRoute));

  // --- Cancelling the save dialog is not an error the user has to see -----
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
    window.__saveCancels = true;
  });
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(1500);
  const cancelled = await page.evaluate(() => window.__ipcLog);
  check(
    "guardar sin archivo pregunta dónde guardarlo",
    cancelled.some((c) => c.cmd === "save_file_as"),
  );
  check(
    "cancelar no escribe nada",
    !cancelled.some((c) => c.cmd === "save_file"),
  );
  check("cancelar no muestra un error", (await toastText()) === null, JSON.stringify(await toastText()));
  // Cancel and the buffer is still pending: the header dot has to keep saying so,
  // or the document looks saved while it only exists in memory.
  const pendingDot = await page.evaluate(() => {
    const dot = document.querySelector("header span[title]");
    return dot ? dot.getAttribute("title") : null;
  });
  check(
    "cancelar deja el documento sin guardar",
    pendingDot === "Sin guardar",
    JSON.stringify(pendingDot),
  );

  // --- A write that genuinely fails is not a silent no-op -----------------
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
    window.__saveCancels = false;
    window.__saveFails = true;
  });
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(2000);
  const failedIpc = await page.evaluate(() => window.__ipcLog);
  const failedToast = await toastText();
  check(
    "un guardado fallido intenta escribir",
    failedIpc.some((c) => c.cmd === "save_file"),
    JSON.stringify(failedIpc.map((c) => c.cmd)),
  );
  check("un guardado fallido avisa el error", !!failedToast, JSON.stringify(failedToast));
  check("un guardado fallido no escribe nada", !fs.existsSync(saveRoute));
  // The document has no file: claiming one would leave the header naming a file
  // that is not there, and the autosave retrying it forever.
  check(
    "un guardado fallido deja el documento sin archivo",
    (await headerLabel()) === "Sin archivo seleccionado",
    JSON.stringify(await headerLabel()),
  );
  const failDot = await page.evaluate(() => {
    const dot = document.querySelector("header span[title]");
    return dot ? dot.getAttribute("title") : null;
  });
  check("un guardado fallido se marca como error", failDot === "Error al guardar", JSON.stringify(failDot));
  // Dismissed by clicking it, and only by clicking it: the toast is replaced by
  // the next message or by a click, never by a timer. Left up, it would be
  // mistaken for the next phase's own output below.
  await page.click(".fixed.bottom-12");
  await page.waitForTimeout(300);
  check("el aviso se puede descartar", (await toastText()) === null, JSON.stringify(await toastText()));

  // --- The real thing: pick a path, write it, adopt its folder ------------
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
    window.__saveCancels = false;
    window.__saveFails = false;
  });
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(2500);

  const ipc = await page.evaluate(() => window.__ipcLog);
  const saveCall = ipc.find((c) => c.cmd === "save_file");
  const listCall = ipc.find((c) => c.cmd === "list_markdown_files");
  const tree = await treeRows();

  console.log("IPC calls after 'Ctrl+S' with no file:");
  for (const call of ipc) console.log("  " + JSON.stringify(call));
  console.log("");

  // `Mod-s` is bound twice — the global shortcut listener and the CodeMirror
  // keymap — so one Ctrl+S reaches `save_now` twice. On the existing path that
  // wrote the same bytes twice; on this one it used to open the native dialog
  // twice, once per invocation.
  check(
    "un Ctrl+S abre el diálogo una sola vez",
    ipc.filter((c) => c.cmd === "save_file_as").length === 1,
    `save_file_as x${ipc.filter((c) => c.cmd === "save_file_as").length}`,
  );
  check(
    "un Ctrl+S escribe una sola vez",
    ipc.filter((c) => c.cmd === "save_file").length === 1,
    `save_file x${ipc.filter((c) => c.cmd === "save_file").length}`,
  );

  check(
    "save_file recibe la ruta que devolvió save_file_as",
    !!saveCall && saveCall.args && saveCall.args.pathStr === saveRoute,
    JSON.stringify(saveCall ? saveCall.args : null),
  );
  check(
    "save_file recibe el contenido del buffer",
    !!saveCall && saveCall.args && saveCall.args.content === TYPED,
    JSON.stringify(saveCall ? saveCall.args.content : null),
  );
  // The strongest form: the bytes on disk, read by this process, not a value
  // echoed back by the stub.
  const onDisk = fs.existsSync(saveRoute) ? fs.readFileSync(saveRoute, "utf8") : null;
  check(
    "el contenido quedó escrito en esa ruta",
    onDisk === TYPED,
    JSON.stringify(onDisk),
  );
  check(
    "la carpeta workspace es la PADRE de la ruta",
    !!listCall && listCall.args && listCall.args.folderPath === saveParent,
    JSON.stringify(listCall ? listCall.args : null),
  );
  check(
    "el sidebar muestra la carpeta padre",
    (await workspaceLabel()) === saveParent,
    JSON.stringify(await workspaceLabel()),
  );
  check("el árbol muestra el archivo guardado", tree.some((row) => row.includes("prueba.md")), JSON.stringify(tree));
  check(
    "la cabecera muestra el nombre del archivo",
    (await headerLabel()) === "prueba.md",
    JSON.stringify(await headerLabel()),
  );
  check("guardar no muestra un error", (await toastText()) === null, JSON.stringify(await toastText()));
  const savedDot = await page.evaluate(() => {
    const dot = document.querySelector("header span[title]");
    return dot ? dot.getAttribute("title") : null;
  });
  check("el documento queda marcado como guardado", savedDot === "Guardado", JSON.stringify(savedDot));

  // A second save now has a file, so it must go straight to `save_file` with no
  // dialog at all.
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
  });
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(1500);
  const secondSave = await page.evaluate(() => window.__ipcLog);
  check(
    "guardar de nuevo no vuelve a preguntar la ubicación",
    !secondSave.some((c) => c.cmd === "save_file_as"),
    JSON.stringify(secondSave.map((c) => c.cmd)),
  );
  check(
    "guardar de nuevo va directo a save_file",
    secondSave.filter((c) => c.cmd === "save_file").length === 1,
    JSON.stringify(secondSave.map((c) => c.cmd)),
  );

  // --- The browser demo has no native dialog to open ----------------------
  const demo = await browser.newPage();
  demo.on("pageerror", (e) => errors.push("demo pageerror: " + e.message));
  await demo.goto(`http://localhost:${PORT}/demo`, { waitUntil: "networkidle" });
  await demo.waitForTimeout(1500);
  await demo.click(".cm-editor .cm-content");
  await demo.keyboard.type("Hola mundo", { delay: 20 });
  await demo.keyboard.press("Control+s");
  await demo.keyboard.press("Control+n");
  await demo.waitForTimeout(1500);
  check(
    "en la build web, guardar sin archivo no invoca save_file_as",
    typeof (await demo.evaluate(() => window.__TAURI__)) === "undefined",
  );
  check(
    "en la build web queda 'Sin archivo seleccionado'",
    (await demo.evaluate(() => document.querySelector("header span").textContent.trim())) ===
      "Sin archivo seleccionado",
  );
  check(
    "en la build web la carpeta sigue en 'Sin carpeta'",
    (await demo.evaluate(() => document.querySelector("aside p").textContent.trim())) === "Sin carpeta",
  );
  // Without the `is_tauri()` guards the bridge rejects the call with "no está
  // disponible fuera de la app de escritorio", which is not a cancellation — so
  // this is what catches the browser build reaching for a dialog it has not got.
  check(
    "en la build web no aparece ningún error",
    (await demo.evaluate(() => {
      const el = document.querySelector(".fixed.bottom-12");
      return el ? el.textContent.trim() : null;
    })) === null,
  );
  await demo.close();

  // --- "Nuevo Archivo" with no workspace asks for a folder, same as save ---
  const create = await browser.newPage();
  create.on("pageerror", (e) => errors.push("create pageerror: " + e.message));
  await create.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await create.waitForTimeout(1500);
  await create.getByRole("button", { name: "Nuevo Archivo" }).click();
  await create.waitForTimeout(2500);
  const createIpc = await create.evaluate(() => window.__ipcLog);
  const createCall = createIpc.find((c) => c.cmd === "create_file");
  const createToast = await create.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  });
  const createRows = await create.evaluate(() =>
    [...document.querySelectorAll("aside li")].map((li) => li.textContent.trim()),
  );
  const createdPath = createCall ? path.join(createCall.args.folderPath, createCall.args.name) : null;

  check(
    "'Nuevo Archivo' abre el selector de carpeta",
    createIpc.some((c) => c.cmd === "open_project_folder"),
    JSON.stringify(createIpc.map((c) => c.cmd)),
  );
  check(
    "'Nuevo Archivo' no avisa que falte una carpeta",
    createToast === null && !/Abrí una carpeta/i.test(errors.join(" ")),
    JSON.stringify(createToast),
  );
  check(
    "el archivo se crea en la carpeta elegida",
    !!createCall && createCall.args.folderPath === fixture && /\.md$/.test(createCall.args.name),
    JSON.stringify(createCall ? createCall.args : null),
  );
  check("el archivo nuevo existe en disco", !!createdPath && fs.existsSync(createdPath), JSON.stringify(createdPath));
  check(
    "el archivo nuevo aparece en el árbol",
    createRows.some((row) => row.includes(path.basename(createdPath || "___"))),
    JSON.stringify(createRows),
  );
  await create.close();

  // --- The original case: opening a folder --------------------------------
  await page.evaluate(() => {
    window.__ipcLog.length = 0;
  });
  await page.getByRole("button", { name: /Abrir carpeta/i }).click();
  await page.waitForTimeout(2500);

  const openIpc = await page.evaluate(() => window.__ipcLog);
  const openTree = await treeRows();

  // --- A folder with no markdown is a state, not a failure -----------------
  //
  // Both halves of this one need saying out loud, because the check can only see
  // one of them. The *backend* half (an empty walk comes back as `Ok`, not as
  // "No se encontraron archivos Markdown") is asserted on the host by
  // `commands::tests::an_empty_folder_is_reported_as_an_empty_tree` — a stubbed
  // backend cannot prove anything about the real one. What this proves is the
  // *frontend* half: given an empty tree, no toast, and a note instead of an
  // error-looking screen.
  const empty = await browser.newPage();
  empty.on("pageerror", (e) => errors.push("empty pageerror: " + e.message));
  await empty.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await empty.waitForTimeout(1500);
  const emptyToastBefore = await empty.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  });
  await empty.evaluate(() => {
    window.__ipcLog.length = 0;
    window.__openRoot = window.__emptyRoot;
  });
  await empty.getByRole("button", { name: /Abrir carpeta/i }).click();
  await empty.waitForTimeout(2500);
  const emptyIpc = await empty.evaluate(() => window.__ipcLog);
  const emptyToast = await empty.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  });
  const emptyRows = await empty.evaluate(() =>
    [...document.querySelectorAll("aside li")].map((li) => li.textContent.trim()),
  );
  const emptyAside = await empty.evaluate(() => {
    const aside = document.querySelector("aside");
    return aside ? aside.textContent : "";
  });
  const emptyLabel = await empty.evaluate(() => {
    const p = document.querySelector("aside p");
    return p ? p.textContent.trim() : null;
  });

  check(
    "una carpeta vacía no avisa ningún error",
    emptyToast === null && !/No se pudo listar archivos/.test(errors.join(" ")),
    JSON.stringify([emptyToastBefore, emptyToast]),
  );
  check("una carpeta vacía no muestra filas", emptyRows.length === 0, JSON.stringify(emptyRows));
  check(
    "una carpeta vacía se explica en vez de fallar",
    /no tiene archivos Markdown todavía/.test(emptyAside),
    JSON.stringify(emptyAside.slice(-160)),
  );
  // The folder label is the first `<p>` in the sidebar and the empty-state note
  // lives after it: if the note ever took that slot, this would read the note.
  check("una carpeta vacía muestra su ruta en la etiqueta", emptyLabel === emptyFolder, JSON.stringify(emptyLabel));
  check(
    "una carpeta vacía se abre igual",
    emptyIpc.some((c) => c.cmd === "open_project_folder") &&
      emptyIpc.some((c) => c.cmd === "list_markdown_files"),
    JSON.stringify(emptyIpc.map((c) => c.cmd)),
  );

  // ...and the note goes away as soon as the folder has something in it, so it
  // cannot become a permanent lie.
  await empty.evaluate(() => {
    window.__openRoot = null;
  });
  await empty.getByRole("button", { name: /Abrir carpeta/i }).click();
  await empty.waitForTimeout(2500);
  const afterFillRows = await empty.evaluate(() =>
    [...document.querySelectorAll("aside li")].map((li) => li.textContent.trim()),
  );
  const afterFillAside = await empty.evaluate(() => {
    const aside = document.querySelector("aside");
    return aside ? aside.textContent : "";
  });
  check(
    "la nota de carpeta vacía desaparece cuando hay archivos",
    afterFillRows.some((row) => row.includes("Bienvenido")) &&
      !/no tiene archivos Markdown todavía/.test(afterFillAside),
    JSON.stringify(afterFillRows),
  );
  await empty.close();

  // --- A listing that genuinely fails still is an error --------------------
  //
  // The other side of the coin: "empty is not a failure" must not turn into
  // "nothing is a failure".
  const broken = await browser.newPage();
  broken.on("pageerror", (e) => errors.push("broken pageerror: " + e.message));
  await broken.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await broken.waitForTimeout(1500);
  await broken.evaluate(() => {
    window.__listFails = true;
  });
  await broken.getByRole("button", { name: /Abrir carpeta/i }).click();
  await broken.waitForTimeout(2500);
  const brokenToast = await broken.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  });
  check(
    "una carpeta que no se puede leer sigue avisando el error",
    !!brokenToast && /permiso denegado/.test(brokenToast),
    JSON.stringify(brokenToast),
  );
  await broken.close();

  await browser.close();
  server.close();

  console.log("IPC calls after clicking 'Abrir carpeta':");
  for (const call of openIpc) console.log("  " + JSON.stringify(call));
  console.log("file tree rows:", JSON.stringify(openTree));
  console.log("");

  // The exact failure this guards against.
  for (const e of errors) {
    if (/reading 'then'|\.then.*undefined|panicked|assertion/i.test(e)) {
      problems.push("runtime error: " + e.split("\n")[0].slice(0, 160));
    }
  }
  for (const e of errors) {
    // Cancellation is reported as a rejected promise on purpose; the frontend
    // has to swallow it, and a console error here means it did not.
    if (/Usuario cancelo/i.test(e)) {
      problems.push("cancelar el diálogo llegó a la consola como error: " + e.slice(0, 160));
    }
  }
  if (!openIpc.some((c) => c.cmd === "open_project_folder")) {
    problems.push("open_project_folder was never invoked");
  }
  if (!openIpc.some((c) => c.cmd === "list_markdown_files")) {
    problems.push("list_markdown_files was never invoked");
  }
  if (!openIpc.some((c) => c.cmd === "watch_folder")) {
    problems.push("watch_folder was never invoked");
  }
  if (!openIpc.some((c) => c.listen === "fs-change")) {
    problems.push("never subscribed to fs-change");
  }
  if (!openIpc.some((c) => c.listen === "fs-new-dir")) {
    problems.push("never subscribed to fs-new-dir");
  }
  if (!openTree.some((row) => row.includes("Bienvenido"))) {
    problems.push("file tree did not render the returned file");
  }

  fs.rmSync(fixture, { recursive: true, force: true });
  fs.rmSync(emptyFolder, { recursive: true, force: true });

  if (problems.length > 0) {
    console.error(`FAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("PASS: the IPC path works end to end.");
})().catch((e) => {
  console.error("FAIL: " + e.message);
  process.exit(1);
});
