/**
 * Feeds the real `codedocs_core` renderer output to a real browser under the
 * shipped CSP, and asserts both that hostile markdown is neutralised and that
 * KaTeX and Mermaid still render.
 *
 * Why a browser and not just the Rust unit tests: the unit tests assert that the
 * sanitiser produces a particular string. This asserts that a real HTML parser
 * and a real CSP engine agree, and — the part the unit tests cannot reach — that
 * the lazy-loading of KaTeX/Mermaid still works once a Content-Security-Policy
 * is in force. Lazy-loading by script injection is exactly the kind of feature a
 * CSP breaks silently.
 *
 * The HTML under test comes from `codedocs-core/examples/render.rs`, so it is
 * the actual production renderer rather than a fixture written here.
 *
 * Requires: `npm run build`, and Playwright's chromium.
 */
const { chromium } = require("playwright");
const crypto = require("node:crypto");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");

const REPO = path.resolve(__dirname, "..");
const ROOT = path.join(REPO, "dist");
const PORT = Number(process.env.PREVIEW_CHECK_PORT || 8094);

const HOSTILE_MD = `# Report

<script>window.__pwned = true;</script>
<img src=x onerror="window.__pwned = true">
<svg onload="window.__pwned = true"></svg>
<iframe src="javascript:window.__pwned=true"></iframe>
<a href="javascript:window.__pwned=true">click</a>
[md link](javascript:window.__pwned=true)

Inline math: $E = mc^2$ and display:

$$a^2 + b^2 = c^2$$

\`\`\`mermaid
graph TD;
  A-->B;
\`\`\`

| col | col |
|---|---|
| 1 | 2 |

- [x] done
> quoted
`;

if (!fs.existsSync(path.join(ROOT, "index.html"))) {
  console.error(`FAIL: ${ROOT}/index.html not found. Run \`npm run build\` first.`);
  process.exit(1);
}

// The real renderer, not a hand-written fixture.
const rendered = execFileSync(
  "cargo",
  ["run", "--quiet", "-p", "codedocs-core", "--example", "render"],
  { cwd: REPO, input: HOSTILE_MD, encoding: "utf8", maxBuffer: 8 * 1024 * 1024 },
);

const conf = JSON.parse(
  fs.readFileSync(path.join(REPO, "src-tauri", "tauri.conf.json"), "utf8"),
);
const indexHtml = fs.readFileSync(path.join(ROOT, "index.html"), "utf8");
const hashes = [...indexHtml.matchAll(/<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/g)].map(
  (m) => `'sha256-${crypto.createHash("sha256").update(m[1], "utf8").digest("base64")}'`,
);
const csp = conf.app.security.csp.replace(
  "script-src 'self'",
  `script-src 'self' ${hashes.join(" ")}`,
);

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
  await page.waitForTimeout(1200);

  const result = await page.evaluate(async (html) => {
    window.__pwned = false;
    const host = document.createElement("div");
    host.className = "prose";
    host.innerHTML = html;
    document.body.appendChild(host);
    await window.__codedocs_render_enhancements_async();
    await new Promise((r) => setTimeout(r, 5000));
    return {
      pwned: window.__pwned === true,
      katexLoaded: typeof window.katex === "object",
      mermaidLoaded: typeof window.mermaid === "object",
    };
  }, rendered);

  const dom = await page.evaluate(() => {
    const hosts = document.querySelectorAll(".prose");
    const host = hosts[hosts.length - 1];
    if (!host) return { found: false };
    return {
      found: true,
      scriptTags: host.querySelectorAll("script").length,
      iframes: host.querySelectorAll("iframe").length,
      imgs: host.querySelectorAll("img").length,
      inlineHandlers: [...host.querySelectorAll("*")].filter((el) =>
        [...el.attributes].some((a) => a.name.startsWith("on")),
      ).length,
      jsHrefs: [...host.querySelectorAll("a")].filter((a) =>
        (a.getAttribute("href") || "").toLowerCase().includes("javascript:"),
      ).length,
      headingId: host.querySelector("h1")?.getAttribute("id"),
      katexRendered: host.querySelectorAll(".katex").length,
      mermaidSvg: host.querySelectorAll(".mermaid-block svg").length,
      tableRows: host.querySelectorAll("table tbody tr").length,
      checkboxChecked: !!host.querySelector('input[type="checkbox"][checked]'),
      blockquote: !!host.querySelector("blockquote"),
    };
  });

  await browser.close();
  server.close();

  const violations = errors.filter((e) => /Content Security Policy|Refused to/i.test(e));

  console.log("=== SANITISER (HTML real de codedocs_core, en un navegador real) ===");
  console.log("attacker script executed:", result.pwned);
  console.log(JSON.stringify(dom, null, 2));
  console.log("");
  console.log("=== ENHANCEMENTS BAJO CSP ===");
  console.log("katex loaded:", result.katexLoaded, "| mermaid loaded:", result.mermaidLoaded);
  console.log("CSP violations:", violations.length);
  for (const v of violations) console.log("  " + v.slice(0, 200));

  const safe =
    !result.pwned &&
    dom.scriptTags === 0 &&
    dom.iframes === 0 &&
    dom.inlineHandlers === 0 &&
    dom.jsHrefs === 0;
  // Mermaid legitimately emits an <svg>; a document that also contains raw
  // <svg> in the payload would be indistinguishable, so the injection attempts
  // are asserted separately above.
  const enhanced = dom.katexRendered > 0 && dom.mermaidSvg > 0;
  const features =
    dom.tableRows > 0 && dom.checkboxChecked && dom.blockquote && dom.headingId === "report";

  console.log("");
  console.log("sanitiser holds:", safe);
  console.log("katex + mermaid render under CSP:", enhanced);
  console.log("GFM features + heading ids:", features);

  const ok = safe && enhanced && features && violations.length === 0 && !result.pwned;
  console.log(ok ? "\nPASS" : "\nFAIL");
  process.exit(ok ? 0 : 1);
})().catch((err) => {
  console.error("FAIL: " + err.message);
  process.exit(1);
});
