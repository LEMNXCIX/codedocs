/**
 * Boots the built app in a real browser under the CSP that ships in
 * `src-tauri/tauri.conf.json`.
 *
 * Why this exists: the CSP is a string in a config file that the Rust compiler
 * never validates against the app's real requirements. Getting it wrong does not
 * produce a build error — it produces a window that silently renders nothing,
 * because trunk's WASM bootstrap is an inline `<script>` and the WASM module
 * needs its own script-src source. Both of those were real bugs here, found by
 * running this, not by reading the config.
 *
 * What it does:
 *   1. Replicates Tauri's runtime CSP handling — it hashes every inline
 *      `<script>` and appends those hashes to `script-src` (see
 *      `tauri::manager::set_csp`), which is what Tauri does when it serves the
 *      embedded assets.
 *   2. Serves `dist/` with that CSP as a response header, the same way Tauri
 *      does.
 *   3. Loads the page and asserts the app actually mounted, with zero CSP
 *      violations.
 *
 * Requires `trunk build` to have produced `dist/`, and Playwright's chromium.
 * Run via `npm run check:csp`.
 */
const { chromium } = require("playwright");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..", "dist");
const CONF = path.resolve(__dirname, "..", "src-tauri", "tauri.conf.json");
const PORT = Number(process.env.CSP_CHECK_PORT || 8095);

if (!fs.existsSync(path.join(ROOT, "index.html"))) {
  console.error(`FAIL: ${ROOT}/index.html not found. Run \`npm run build\` first.`);
  process.exit(1);
}

const conf = JSON.parse(fs.readFileSync(CONF, "utf8"));
const indexHtml = fs.readFileSync(path.join(ROOT, "index.html"), "utf8");

// Tauri's rule: hash the text of every inline script and add it to script-src.
const hashes = [...indexHtml.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
  (m) => `'sha256-${crypto.createHash("sha256").update(m[1], "utf8").digest("base64")}'`,
);

const configured = conf.app.security.csp;
if (!configured) {
  console.error("FAIL: app.security.csp is null; the preview renders untrusted HTML.");
  process.exit(1);
}
const csp = configured.replace("script-src 'self'", `script-src 'self' ${hashes.join(" ")}`);

const TYPES = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".wasm": "application/wasm",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".ttf": "font/ttf",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
};

const server = http.createServer((req, res) => {
  const url = decodeURIComponent(req.url.split("?")[0]);
  const file = path.join(ROOT, url === "/" ? "index.html" : url);
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
  await page.waitForTimeout(4000);

  const violations = errors.filter((e) => /Content Security Policy|Refused to/i.test(e));

  const report = await page.evaluate(() => ({
    mounted: document.body.innerHTML.trim().length > 0,
    hasSidebar: !!document.querySelector("aside"),
    hasStatusBar: !!document.querySelector("footer"),
    hasPreview: !!document.querySelector(".prose"),
    bridges: [
      "__codedocs_createEditor",
      "__codedocs_render_enhancements_async",
      "__codedocs_toggle_wrap",
      "__codedocs_insert_link",
    ].map((n) => `${n}=${typeof window[n]}`),
  }));

  await browser.close();
  server.close();

  console.log(`inline-script hashes added by Tauri: ${hashes.length}`);
  console.log(JSON.stringify(report, null, 2));
  if (violations.length > 0) {
    console.log("\nCSP violations:");
    for (const v of violations) console.log("  " + v.slice(0, 200));
  }

  const ok = report.mounted && report.hasSidebar && report.hasStatusBar && violations.length === 0;
  console.log(ok ? "\nPASS: the app boots under the shipped CSP." : "\nFAIL");
  process.exit(ok ? 0 : 1);
})().catch((err) => {
  console.error("FAIL: " + err.message);
  process.exit(1);
});
