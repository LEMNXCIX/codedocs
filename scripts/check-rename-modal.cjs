/**
 * Checks the rename modal's confirm button against the real DOM.
 *
 * "Guardar cambios" shipped disabled: the signal named `can_confirm` was fed to
 * a `ModalShell` parameter whose meaning is *disabled*, so the button was greyed
 * out exactly when the name was fine and live when it was empty. Enter still
 * worked, because `on:keydown` read the same variable as a condition instead of
 * passing it through. Two paths, two copies of the same truth, one of them
 * inverted.
 *
 * A disabled button is DOM state, so nothing here can be asserted from Rust: a
 * host test would only see the expression that computes the flag, not the
 * attribute the browser refuses to click. So this drives the real WASM app with
 * Playwright and reads `button.disabled` off the element, then presses Enter at
 * the same state, because *those two agreeing* is the invariant that broke.
 *
 * The stub is backed by the real filesystem over `/fixture-tree` and
 * `/fixture-rename`, so "nothing was renamed" is proved by reading the directory
 * from this process rather than by trusting the stub's own log.
 *
 * Structure mirrors `scripts/check-ipc-flow.cjs`: serve `dist/` over http with
 * the shipping CSP (inline-script hashes appended, as Tauri does).
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
const PORT = Number(process.env.RENAME_CHECK_PORT || 8093);

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
            return Promise.resolve("# Hola\\n\\nContenido de prueba.\\n");
          // Renames for real, so "the button did nothing" can be checked against
          // the directory itself instead of against this log.
          case "rename_file":
            return fetch("/fixture-rename", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ oldPath: args.oldPath, newName: args.newName }),
            }).then(function (r) {
              return r.ok ? null : Promise.reject("No se pudo renombrar el archivo");
            });
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
})();
`;

// Module scope so the bottom handler can print what had already failed: a
// timeout on the next locator is a *consequence* of an earlier failed check
// (e.g. a confirm click that was not blocked closed the modal), and reporting
// only the timeout hides the check that actually broke.
const problems = [];

const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "codedocs-rename-"));
const ORIGINAL = "Bienvenido.md";
const VIA_CLICK = "RenombradoPorClic.md";
const VIA_ENTER = "RenombradoPorEnter.md";
fs.writeFileSync(path.join(fixture, ORIGINAL), "# Hola\n\nContenido de prueba.\n");

const conf = JSON.parse(
  fs.readFileSync(path.join(REPO, "src-tauri", "tauri.conf.json"), "utf8"),
);
const indexHtml = fs.readFileSync(path.join(ROOT, "index.html"), "utf8");
const hashes = [...indexHtml.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
  (m) => `'sha256-${crypto.createHash("sha256").update(m[1], "utf8").digest("base64")}'`,
);
const csp = conf.app.security.csp.replace("script-src 'self'", `script-src 'self' ${hashes.join(" ")}`);

// Served as a separate file, not inlined: the CSP in effect allows inline
// scripts only by hash, and the hash list comes from the untouched
// `dist/index.html`, so an extra inline script would be blocked for a reason
// that has nothing to do with the rename modal.
const withStub = indexHtml.replace(
  '<script src="/public/codemirror-bridge.js"></script>',
  '<script src="/tauri-stub.js"></script>\n<script src="/public/codemirror-bridge.js"></script>',
);

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
    const body = STUB.replace("__FIXTURE_ROOT__", JSON.stringify(fixture));
    res.writeHead(200, { "Content-Type": "text/javascript", "Content-Security-Policy": csp });
    return res.end(body);
  }
  if (url === "/fixture-tree") {
    const folder = new URLSearchParams(query).get("folder") || "";
    const body = { entries: fs.existsSync(folder) ? listTree(folder) : [], truncated: null };
    res.writeHead(200, { "Content-Type": "application/json", "Content-Security-Policy": csp });
    return res.end(JSON.stringify(body));
  }
  if (url === "/fixture-rename") {
    readBody(req).then((raw) => {
      const { oldPath, newName } = JSON.parse(raw);
      const target = path.join(path.dirname(oldPath), newName);
      try {
        fs.renameSync(oldPath, target);
        res.writeHead(200, { "Content-Type": "application/json", "Content-Security-Policy": csp });
        res.end("null");
      } catch (e) {
        res.writeHead(400, { "Content-Type": "application/json", "Content-Security-Policy": csp });
        res.end(JSON.stringify({ error: String(e) }));
      }
    });
    return;
  }
  if (url === "/") {
    res.writeHead(200, { "Content-Type": "text/html", "Content-Security-Policy": csp });
    return res.end(withStub);
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

  const check = (name, cond, extra) => {
    if (cond) console.log(`  ok: ${name}${extra ? ` — ${extra}` : ""}`);
    else problems.push(extra ? `${name} — ${extra}` : name);
  };

  const dialog = () => page.locator('[role="dialog"]');
  const nameInput = () => dialog().locator("input");
  const confirmButton = () => dialog().getByRole("button", { name: "Guardar cambios" });
  // The DOM property, not a copy of the app's state: this is what gates the
  // click, and reading anything else would only re-assert the fix's own
  // arithmetic.
  const confirmDisabled = () => confirmButton().evaluate((el) => el.disabled);
  const treeRows = () =>
    page.evaluate(() =>
      [...document.querySelectorAll("aside li")].map((li) => li.textContent.trim()),
    );
  const renameCalls = () =>
    page.evaluate(() => window.__ipcLog.filter((c) => c.cmd === "rename_file"));
  const clearIpcLog = () =>
    page.evaluate(() => {
      window.__ipcLog.length = 0;
    });
  const diskNames = () => fs.readdirSync(fixture).sort().join(",");

  // A real mouse click at the button's own coordinates, for both the enabled and
  // the disabled case. `locator.click()` would refuse the disabled one — the
  // browser refusing is the behaviour under test, so the gesture has to reach
  // the element and let the DOM decide.
  async function clickConfirm() {
    const box = await confirmButton().boundingBox();
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  }

  async function openRenameModal(fileName) {
    const row = page.locator("aside li", { hasText: fileName }).first();
    await row.hover();
    await row.locator('button[title="Renombrar"]').click();
    await dialog().waitFor({ state: "visible", timeout: 5000 });
    await page.waitForTimeout(150);
  }

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);

  await page.getByRole("button", { name: /Abrir carpeta/i }).click();
  await page.waitForTimeout(2000);
  check(
    "el árbol muestra el archivo a renombrar",
    (await treeRows()).some((row) => row.includes(ORIGINAL)),
    JSON.stringify(await treeRows()),
  );

  await openRenameModal(ORIGINAL);

  // --- 1. The modal opens with the current name already in the input --------
  check("el input arranca con el nombre actual", (await nameInput().inputValue()) === ORIGINAL,
    JSON.stringify(await nameInput().inputValue()));
  // Starting enabled would mean saving without touching anything sends the same
  // name, which the backend rejects — the button would look live and do nothing.
  check("al abrir, el botón arranca deshabilitado", (await confirmDisabled()) === true,
    `disabled=${await confirmDisabled()}`);

  // --- 5. A disabled button cannot fire the rename -------------------------
  await clearIpcLog();
  await clickConfirm();
  await page.waitForTimeout(600);
  check("clickear el botón deshabilitado no llama a rename_file", (await renameCalls()).length === 0,
    JSON.stringify(await renameCalls()));
  check("clickear el botón deshabilitado no renombra nada en disco",
    diskNames() === ORIGINAL, diskNames());
  check("el modal sigue abierto tras el click descartado",
    (await dialog().count()) === 1, `diálogos=${await dialog().count()}`);

  // --- 3, 4. States that must be disabled, and must not submit on Enter ----
  //
  // Each row asserts both halves of one row of the table: the button's DOM
  // state, and what Enter does at that same state. Reading only one of them is
  // how the two paths drifted apart in the first place.
  const inertStates = [
    ["input vacío", ""],
    ["solo espacios", "   "],
    ["el nombre original de nuevo", ORIGINAL],
  ];
  for (const [label, value] of inertStates) {
    await clearIpcLog();
    await nameInput().fill(value);
    await page.waitForTimeout(150);
    const disabled = await confirmDisabled();
    check(`${label}: el botón queda deshabilitado`, disabled === true, `disabled=${disabled}`);
    await nameInput().press("Enter");
    await page.waitForTimeout(600);
    const calls = await renameCalls();
    check(`${label}: Enter tampoco confirma`, calls.length === 0, JSON.stringify(calls));
    check(`${label}: nada se renombró en disco`, diskNames() === ORIGINAL, diskNames());
  }

  // --- 2. A valid, different name enables the button ------------------------
  await clearIpcLog();
  await nameInput().fill(VIA_CLICK);
  await page.waitForTimeout(150);
  const enabledDisabled = await confirmDisabled();
  check("un nombre válido y distinto habilita el botón", enabledDisabled === false,
    `disabled=${enabledDisabled}`);

  // --- 6a. The click renames, for real --------------------------------------
  await clickConfirm();
  await page.waitForTimeout(900);
  const clickCalls = await renameCalls();
  check("el clic manda rename_file con el nombre escrito", clickCalls.length === 1 &&
    clickCalls[0].args.newName === VIA_CLICK &&
    clickCalls[0].args.oldPath === path.join(fixture, ORIGINAL),
    JSON.stringify(clickCalls.map((c) => c.args)));
  check("el clic renombra el archivo en disco", diskNames() === VIA_CLICK, diskNames());
  check("el modal se cierra tras el clic", (await dialog().count()) === 0,
    `diálogos=${await dialog().count()}`);
  check("renombrar no muestra un error", (await page.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  })) === null, JSON.stringify(await page.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  })));
  check("el árbol muestra el nombre nuevo",
    (await treeRows()).some((row) => row.includes(VIA_CLICK)), JSON.stringify(await treeRows()));

  // --- 6b. Enter does exactly what the click did ----------------------------
  await openRenameModal(VIA_CLICK);
  check("el modal nuevo arranca con el nombre ya renombrado",
    (await nameInput().inputValue()) === VIA_CLICK, JSON.stringify(await nameInput().inputValue()));
  check("el modal nuevo arranca deshabilitado otra vez", (await confirmDisabled()) === true,
    `disabled=${await confirmDisabled()}`);

  await clearIpcLog();
  await nameInput().fill(VIA_ENTER);
  await page.waitForTimeout(150);
  const enterDisabled = await confirmDisabled();
  check("un nombre válido y distinto habilita el botón también para Enter",
    enterDisabled === false, `disabled=${enterDisabled}`);
  await nameInput().press("Enter");
  await page.waitForTimeout(900);
  const enterCalls = await renameCalls();
  check("Enter manda rename_file con el nombre escrito", enterCalls.length === 1 &&
    enterCalls[0].args.newName === VIA_ENTER &&
    enterCalls[0].args.oldPath === path.join(fixture, VIA_CLICK),
    JSON.stringify(enterCalls.map((c) => c.args)));
  check("Enter renombra el archivo en disco", diskNames() === VIA_ENTER, diskNames());
  check("el modal se cierra tras Enter", (await dialog().count()) === 0,
    `diálogos=${await dialog().count()}`);

  await browser.close();
  server.close();

  console.log("archivos al final:", JSON.stringify(diskNames()));
  console.log("");

  for (const e of errors) problems.push("runtime error: " + e.split("\n")[0].slice(0, 160));

  fs.rmSync(fixture, { recursive: true, force: true });

  if (problems.length > 0) {
    console.error(`FAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("PASS: el botón de renombrar y la tecla Enter comparten la misma condición.");
})().catch((e) => {
  console.error("FAIL: " + e.message.split("\n")[0]);
  for (const p of problems) console.error("  - " + p);
  process.exit(1);
});