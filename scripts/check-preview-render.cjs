/**
 * Feeds hostile markdown plus math and a diagram to the real live-format
 * editor under the shipped CSP, and asserts both that hostile markdown is
 * neutralised and that KaTeX and Mermaid still render **inside `.cm-editor`**.
 *
 * Why a browser and not just the Rust unit tests: the unit tests assert that
 * the sanitiser produces a particular string. This asserts that a real HTML
 * parser and a real CSP engine agree, and — the part the unit tests cannot
 * reach — that the lazy-loading of KaTeX/Mermaid still works once a
 * Content-Security-Policy is in force. Lazy-loading by script injection is
 * exactly the kind of feature a CSP breaks silently.
 *
 * There is no read-only preview anymore: the markdown is the document and
 * CodeMirror holds the source, so the `.katex` nodes and the Mermaid `svg`
 * are asserted inside `.cm-editor`, next to the `.cm-lp-*` widgets that own
 * them. Every render assertion also requires its widget class: without the
 * widget the selector is gone and the case fails, so a broken or missing
 * widget cannot pass silently.
 *
 * Structure mirrors `scripts/check-live-editor.cjs`: serve `dist/` over http
 * with the shipping CSP (inline-script hashes appended, as Tauri does) and
 * drive the real WASM app with Playwright.
 *
 * Requires: `npm run build`, and Playwright's chromium.
 */
const { chromium } = require("playwright");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");

const REPO = path.resolve(__dirname, "..");
const ROOT = path.join(REPO, "dist");
const PORT = Number(process.env.PREVIEW_CHECK_PORT || 8094);

const DOC = `# Report

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

cola final
`;

if (!fs.existsSync(path.join(ROOT, "index.html"))) {
  console.error(`FAIL: ${ROOT}/index.html not found. Run \`npm run build\` first.`);
  process.exit(1);
}

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
  const problems = [];
  const errors = [];
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push("pageerror: " + e.message));

  // Always print `extra`, pass or fail (same convention as check-live-editor).
  const check = (name, cond, extra) => {
    if (cond) console.log(`  ok: ${name}${extra ? ` — ${extra}` : ""}`);
    else problems.push(extra ? `${name} — ${extra}` : name);
  };

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(2000);

  // Same boot as check-live-editor: the CodeMirror instance mounts here.
  await page.keyboard.press("Control+1");
  try {
    await page.waitForSelector(".cm-editor .cm-content", { timeout: 8000 });
  } catch {
    console.error("FAIL: el editor no montó tras Ctrl+1");
    await browser.close();
    server.close();
    process.exit(1);
  }

  await page.evaluate((t) => window.__codedocs_setContent(t), DOC);
  // Caret to the last line, off the math and diagram lines: those reveal
  // their source while the cursor is on them, so hiding/rendering is only
  // observable from elsewhere.
  await page.click(".cm-editor .cm-content");
  await page.waitForTimeout(300);
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(800);

  const libs = await page.evaluate(() => ({
    katexLoaded: typeof window.katex === "object",
    mermaidLoaded: typeof window.mermaid === "object",
  }));

  let katexInline = 0;
  try {
    await page.waitForSelector(".cm-editor .katex", { timeout: 8000 });
    katexInline = await page.evaluate(() => document.querySelectorAll(".cm-editor .katex").length);
  } catch {
    katexInline = 0;
  }
  const mathInlineWidget = await page.evaluate(
    () => !!document.querySelector(".cm-editor .cm-lp-math-inline"),
  );
  const mathBlockWidget = await page.evaluate(
    () => !!document.querySelector(".cm-editor .cm-lp-math-block"),
  );

  let mermaidSvg = 0;
  try {
    await page.waitForSelector(".cm-editor .cm-lp-mermaid svg", { timeout: 15000 });
    mermaidSvg = await page.evaluate(
      () => document.querySelectorAll(".cm-editor .cm-lp-mermaid svg").length,
    );
  } catch {
    mermaidSvg = 0;
  }
  const mermaidWidget = await page.evaluate(
    () => !!document.querySelector(".cm-editor .cm-lp-mermaid"),
  );

  const safety = await page.evaluate(() => {
    const editor = document.querySelector(".cm-editor");
    const els = editor ? [...editor.querySelectorAll("*")] : [];
    return {
      pwned: window.__pwned === true,
      scriptTags: editor ? editor.querySelectorAll("script").length : -1,
      iframes: editor ? editor.querySelectorAll("iframe").length : -1,
      inlineHandlers: els.filter((el) =>
        [...el.attributes].some((a) => a.name.startsWith("on")),
      ).length,
      jsHrefs: els.filter(
        (el) =>
          el.tagName === "A" &&
          (el.getAttribute("href") || "").toLowerCase().includes("javascript:"),
      ).length,
      hostileTextKept: (editor ? editor.textContent : "").includes("md link"),
    };
  });

  await browser.close();
  server.close();

  const violations = errors.filter((e) => /Content Security Policy|Refused to/i.test(e));

  console.log("=== SANITISER (markdown hostil dentro del editor en vivo) ===");
  console.log("attacker script executed:", safety.pwned);
  console.log(JSON.stringify(safety, null, 2));
  console.log("");
  console.log("=== ENHANCEMENTS BAJO CSP (dentro de .cm-editor) ===");
  console.log("katex loaded:", libs.katexLoaded, "| mermaid loaded:", libs.mermaidLoaded);
  console.log("CSP violations:", violations.length);
  for (const v of violations) console.log("  " + v.slice(0, 200));
  console.log("");

  check("el script del atacante no se ejecutó", !safety.pwned);
  check(
    "sin script/iframe/handlers/javascript: en el editor",
    safety.scriptTags === 0 &&
      safety.iframes === 0 &&
      safety.inlineHandlers === 0 &&
      safety.jsHrefs === 0,
    `script=${safety.scriptTags} iframe=${safety.iframes} on*=${safety.inlineHandlers} js:=${safety.jsHrefs}`,
  );
  check("el texto del enlace hostil se conserva", safety.hostileTextKept);
  check(
    "math inline renderiza KaTeX en su widget",
    katexInline >= 1 && mathInlineWidget,
    `katex=${katexInline} widget=${mathInlineWidget}`,
  );
  check(
    "math display renderiza KaTeX en su widget",
    mathBlockWidget,
    `widget=${mathBlockWidget}`,
  );
  check(
    "mermaid renderiza un svg en su widget",
    mermaidSvg >= 1 && mermaidWidget,
    `svg=${mermaidSvg} widget=${mermaidWidget}`,
  );
  check("sin violaciones de CSP", violations.length === 0, `violations=${violations.length}`);

  if (problems.length > 0) {
    console.error(`\nFAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("\nPASS: hostile markdown neutralised, KaTeX + Mermaid render in the live editor.");
})().catch((err) => {
  console.error("FAIL: " + err.message);
  process.exit(1);
});
