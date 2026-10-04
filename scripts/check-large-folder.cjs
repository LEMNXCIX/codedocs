/**
 * Reproduces the reported failure against a real large directory.
 *
 * The bug: the walk had an 8 000-entry budget, counted *every* entry examined
 * rather than the markdown files kept, and returned an error on overflow —
 * discarding a perfectly good tree. A folder with 24 000 entries and 300 notes
 * came up empty, and the message only ever reached the console.
 *
 * The tree is produced by the real Rust walker (`codedocs-core/examples/walk.rs`)
 * and fed to the real frontend, so what is measured is the shipped logic rather
 * than a reimplementation of it.
 */
const { chromium } = require("playwright");
const { execFileSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");

const REPO = path.resolve(__dirname, "..");
const ROOT = path.join(REPO, "dist");
const PORT = Number(process.env.BIG_CHECK_PORT || 8089);

const JUNK_FILES = 24_000;
const NOTES = 300;

if (!fs.existsSync(path.join(ROOT, "index.html"))) {
  console.error(`FAIL: ${ROOT}/index.html not found. Run \`npm run build\` first.`);
  process.exit(1);
}

function buildFixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "codedocs-big-"));
  for (let i = 0; i < JUNK_FILES; i++) {
    fs.writeFileSync(path.join(root, `junk-${String(i).padStart(6, "0")}.bin`), "x");
  }
  for (let i = 0; i < NOTES; i++) {
    const depth = Math.min(i, 14);
    const dir = path.join(root, ...Array.from({ length: depth }, (_, d) => `n${d}`));
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, `nota-${String(i).padStart(4, "0")}.md`), "# nota\n");
  }
  return root;
}

console.log(`building a folder with ~${JUNK_FILES + NOTES} entries (${NOTES} markdown)...`);
const fixture = buildFixture();

// The real walker, with the shipped interactive limits.
const walkStarted = Date.now();
const treeJson = execFileSync(
  "cargo",
  ["run", "--quiet", "-p", "codedocs-core", "--example", "walk", "--", fixture],
  { cwd: REPO, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
);
const walkMs = Date.now() - walkStarted;
const tree = JSON.parse(treeJson);

console.log(`  walker: ${walkMs} ms, ${tree.fileCount} markdown files, truncated=${tree.truncated}`);
console.log("");

const conf = JSON.parse(
  fs.readFileSync(path.join(REPO, "src-tauri", "tauri.conf.json"), "utf8"),
);
let indexHtml = fs.readFileSync(path.join(ROOT, "index.html"), "utf8");
const hashes = [...indexHtml.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
  (m) => `'sha256-${crypto.createHash("sha256").update(m[1], "utf8").digest("base64")}'`,
);
const csp = conf.app.security.csp.replace("script-src 'self'", `script-src 'self' ${hashes.join(" ")}`);

indexHtml = indexHtml.replace(
  '<script src="/public/codemirror-bridge.js"></script>',
  '<script src="/tauri-stub.js"></script>\n<script src="/public/codemirror-bridge.js"></script>',
);

// The stub replays the real walker's output.
const STUB = `(function () {
  var calls = [];
  window.__ipcLog = calls;
  var TREE = ${treeJson};
  window.__TAURI__ = {
    core: {
      invoke: function (cmd, args) {
        calls.push({ cmd: cmd, args: args });
        if (cmd === "open_project_folder") return Promise.resolve(window.__fixtureRoot);
        if (cmd === "list_markdown_files") return Promise.resolve(TREE);
        if (cmd === "read_file") return Promise.resolve("# Hola\\n");
        return Promise.resolve(null);
      },
    },
    event: {
      listen: function (n) { calls.push({ listen: n }); return Promise.resolve(1); },
      unlisten: function () { return Promise.resolve(); },
    },
  };
})();`;

const TYPES = {
  ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".css": "text/css", ".svg": "image/svg+xml", ".ttf": "font/ttf",
  ".woff": "font/woff", ".woff2": "font/woff2",
};

const server = http.createServer((req, res) => {
  const url = decodeURIComponent(req.url.split("?")[0]);
  if (url === "/tauri-stub.js") {
    res.writeHead(200, { "Content-Type": "text/javascript", "Content-Security-Policy": csp });
    return res.end(STUB);
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

  await page.addInitScript((root) => {
    window.__fixtureRoot = root;
  }, fixture);

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);

  const started = Date.now();
  await page.getByRole("button", { name: /Abrir carpeta/i }).click();
  await page.waitForFunction(() => window.__ipcLog.some((c) => c.cmd === "list_markdown_files"), {
    timeout: 30000,
  });
  await page.waitForTimeout(3000);
  const elapsed = Date.now() - started;

  const rows = await page.evaluate(() =>
    [...document.querySelectorAll("aside li")].map((li) => li.textContent.trim()).filter(Boolean),
  );
  const toast = await page.evaluate(() => {
    const el = document.querySelector(".fixed.bottom-12");
    return el ? el.textContent.trim() : null;
  });

  await browser.close();
  server.close();
  fs.rmSync(fixture, { recursive: true, force: true });

  console.log(`UI: took ${elapsed} ms, ${rows.length} tree rows`);
  console.log(`  sample rows: ${JSON.stringify(rows.slice(0, 5))}`);
  console.log(`  toast: ${toast === null ? "(none)" : JSON.stringify(toast)}`);
  console.log("");

  const problems = [];
  for (const e of errors) {
    if (/demasiado grande|panicked|assertion|reading 'then'/i.test(e)) {
      problems.push("runtime error: " + e.split("\n")[0].slice(0, 160));
    }
  }
  // The original bug: the tree came back empty for a large folder.
  if (rows.length === 0) problems.push("the file tree is empty for a large folder");
  if (!rows.some((r) => r.includes("nota-"))) problems.push("no markdown files rendered");
  // Errors must reach the UI, not just the console.
  if (toast !== null && /demasiado grande/i.test(toast)) {
    problems.push("the old 'too large' refusal is back");
  }
  if (walkMs > 15000) problems.push(`the walker took ${walkMs} ms, which feels like a hang`);

  if (problems.length > 0) {
    console.error(`FAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("PASS: a large folder opens and lists its notes instead of erroring.");
})().catch((e) => {
  console.error("FAIL: " + e.message);
  process.exit(1);
});
