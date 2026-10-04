/**
 * Exercises the "open folder" path against a stubbed Tauri IPC.
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
  var nextId = 1;
  var handlers = new Map();

  window.__TAURI__ = {
    core: {
      invoke: function (cmd, args) {
        calls.push({ cmd: cmd, args: args });
        switch (cmd) {
          case "open_project_folder": return Promise.resolve(window.__fixtureRoot);
          case "list_markdown_files":
            return Promise.resolve([{ name: "Bienvenido.md", path: window.__fixtureRoot + "/Bienvenido.md", is_dir: false, children: [] }]);
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
indexHtml = indexHtml.replace(
  '<script src="/public/codemirror-bridge.js"></script>',
  '<script src="/tauri-stub.js"></script>\n<script src="/public/codemirror-bridge.js"></script>',
);

const TYPES = {
  ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".css": "text/css", ".svg": "image/svg+xml", ".ttf": "font/ttf",
  ".woff": "font/woff", ".woff2": "font/woff2",
};

const server = http.createServer((req, res) => {
  const url = decodeURIComponent(req.url.split("?")[0]);
  if (url === "/tauri-stub.js") {
    // The fixture root is baked in at serve time so no inline script is needed.
    const body = STUB.replace("__FIXTURE_ROOT__", JSON.stringify(fixture));
    res.writeHead(200, { "Content-Type": "text/javascript", "Content-Security-Policy": csp });
    return res.end(body);
  }
  if (url === "/") {
    res.writeHead(200, { "Content-Type": "text/html", "Content-Security-Policy": csp });
    return res.end(indexHtml);
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

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);

  await page.evaluate(() => {
    window.__ipcLog.length = 0;
  });
  await page.getByRole("button", { name: /Abrir carpeta/i }).click();
  await page.waitForTimeout(2500);

  const ipc = await page.evaluate(() => window.__ipcLog);
  const tree = await page.evaluate(() =>
    [...document.querySelectorAll("aside li")]
      .map((li) => li.textContent.trim())
      .filter(Boolean)
      .slice(0, 6),
  );

  await browser.close();
  server.close();
  fs.rmSync(fixture, { recursive: true, force: true });

  console.log("IPC calls after clicking 'Abrir carpeta':");
  for (const call of ipc) console.log("  " + JSON.stringify(call));
  console.log("file tree rows:", JSON.stringify(tree));
  console.log("");

  const problems = [];
  // The exact failure this guards against.
  for (const e of errors) {
    if (/reading 'then'|\.then.*undefined|panicked|assertion/i.test(e)) {
      problems.push("runtime error: " + e.split("\n")[0].slice(0, 160));
    }
  }
  if (!ipc.some((c) => c.cmd === "open_project_folder")) {
    problems.push("open_project_folder was never invoked");
  }
  if (!ipc.some((c) => c.cmd === "list_markdown_files")) {
    problems.push("list_markdown_files was never invoked");
  }
  if (!ipc.some((c) => c.cmd === "watch_folder")) {
    problems.push("watch_folder was never invoked");
  }
  if (!ipc.some((c) => c.listen === "fs-change")) {
    problems.push("never subscribed to fs-change");
  }
  if (!ipc.some((c) => c.listen === "fs-new-dir")) {
    problems.push("never subscribed to fs-new-dir");
  }
  if (!tree.some((row) => row.includes("Bienvenido"))) {
    problems.push("file tree did not render the returned file");
  }

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
