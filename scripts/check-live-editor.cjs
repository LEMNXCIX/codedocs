/**
 * Verifies the live-format editor ("modo de edición en vivo") end to end.
 *
 * The markdown stays the document: CodeMirror holds the source and a
 * ViewPlugin turns `live_spans` ranges into decorations. Syntax characters
 * (`**`, `# `, `|`, fences) are hidden with `font-size: 0` — never with
 * `Decoration.replace` (select-all + copy would drop the opening `**`) and
 * never with `display: none` (the caret gets stuck at the offset).
 *
 * Structure mirrors `scripts/check-ipc-flow.cjs`: serve `dist/` over http
 * with the shipping CSP (inline-script hashes appended, as Tauri does) and
 * drive the real WASM app with Playwright. No Tauri stub: none of these
 * cases touch IPC.
 *
 * Requires `npm run build`.
 */
const { chromium } = require("playwright");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");

const REPO = path.resolve(__dirname, "..");
const ROOT = path.join(REPO, "dist");
const PORT = Number(process.env.LIVE_CHECK_PORT || 8091);

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
const csp = conf.app.security.csp.replace("script-src 'self'", `script-src 'self' ${hashes.join(" ")}`);

const TYPES = {
  ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".css": "text/css", ".svg": "image/svg+xml", ".ttf": "font/ttf",
  ".woff": "font/woff", ".woff2": "font/woff2",
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

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

(async () => {
  await new Promise((r) => server.listen(PORT, r));
  const browser = await chromium.launch();
  // Clipboard access for the copy/paste cases.
  const context = await browser.newContext({ permissions: ["clipboard-read", "clipboard-write"] });
  const page = await context.newPage();
  const problems = [];

  const errors = [];
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push("pageerror: " + e.message));

  // Always print `extra`, pass or fail. Without it a gate measuring 5 ms looks
  // identical to one measuring 49 ms, and the only time the number appears is
  // the run it starts failing — which is exactly the run nobody looks at.
  const check = (name, cond, extra) => {
    if (cond) console.log(`  ok: ${name}${extra ? ` — ${extra}` : ""}`);
    else problems.push(extra ? `${name} — ${extra}` : name);
  };

  const getContent = () => page.evaluate(() => window.__codedocs_getContent());
  const setContent = (text) => page.evaluate((t) => window.__codedocs_setContent(t), text);
  // Summed width of a line's client rects. A Range over the whole line yields
  // a single rect whether markers are hidden or not, so rect *count* cannot
  // tell hidden from visible — width can.
  const lineWidth = (n) => page.evaluate((idx) => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[idx - 1];
    if (!el) return -1;
    const range = document.createRange();
    range.selectNodeContents(el);
    let w = 0;
    for (const r of range.getClientRects()) w += r.width;
    return w;
  }, n);
  const hiddenInLine = (n) => page.evaluate((idx) => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[idx - 1];
    return el ? el.querySelectorAll(".cm-lp-hidden").length : -1;
  }, n);

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForTimeout(2000);

  // The span provider is the Task 2 / Task 3 seam: without it there is no
  // live formatting at all.
  check(
    "el bundle expone __codedocs_setSpanProvider",
    await page.evaluate(() => typeof window.__codedocs_setSpanProvider === "function"),
  );
  check(
    "el bundle expone __codedocs_setSourceVisible",
    await page.evaluate(() => typeof window.__codedocs_setSourceVisible === "function"),
  );

  // The app boots in Formatted mode (preview only). Ctrl+1 switches to Raw
  // so the CodeMirror instance mounts.
  await page.keyboard.press("Control+1");
  try {
    await page.waitForSelector(".cm-editor .cm-content", { timeout: 8000 });
  } catch {
    problems.push("el editor no montó tras Ctrl+1");
    console.error(`FAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    await browser.close();
    server.close();
    process.exit(1);
  }

  // Case: escribir `**hola**` — el contenido guardado es exacto.
  await setContent("");
  await page.click(".cm-editor .cm-content");
  await page.waitForTimeout(300);
  await page.keyboard.type("**hola**", { delay: 20 });
  await page.waitForTimeout(500);
  check("escribir `**hola**` deja el contenido exacto", (await getContent()) === "**hola**", JSON.stringify(await getContent()));

  // Case: seleccionar todo y copiar — el portapapeles conserva los `**`.
  await page.keyboard.press("Control+a");
  await page.waitForTimeout(300);
  await page.keyboard.press("Control+c");
  await page.waitForTimeout(300);
  let clipboard = null;
  try {
    clipboard = await page.evaluate(() => navigator.clipboard.readText());
  } catch (e) {
    problems.push("no se pudo leer el portapapeles: " + e.message);
  }
  check("copiar conserva `**hola**`", clipboard === "**hola**", JSON.stringify(clipboard));

  // From here the cursor must leave line 1: a line holding the cursor is
  // revealed (markers shown), so hiding is only observable elsewhere.
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("cola", { delay: 20 });
  await page.waitForTimeout(500);

  check(
    "existe un `.cm-lp-strong` en el DOM",
    (await page.evaluate(() => document.querySelectorAll(".cm-editor .cm-lp-strong").length)) >= 1,
  );
  const hiddenTotal = await page.evaluate(() => document.querySelectorAll(".cm-editor .cm-lp-hidden").length);
  check("la línea 1 oculta sus marcadores", hiddenTotal >= 2, `hidden=${hiddenTotal}`);
  check("el contenido sigue exacto tras ocultar", (await getContent()) === "**hola**\ncola", JSON.stringify(await getContent()));

  // Case: el `**` no se ve — el ancho oculto es menor que la línea base
  // revelada por varios caracteres de marcador.
  const wHidden = await lineWidth(1);
  await page.keyboard.press("ArrowUp"); // cursor a la línea 1: se revela
  await page.waitForTimeout(400);
  const wRevealed = await lineWidth(1);
  check("revelada la línea 1 no quedan ocultos en ella", (await hiddenInLine(1)) === 0, `hidden=${await hiddenInLine(1)}`);
  check(
    "el `**` oculto es más angosto que la línea base revelada",
    wHidden >= 0 && wRevealed - wHidden > 8,
    `hidden=${wHidden} revealed=${wRevealed}`,
  );

  // Case: cursor dentro de la línea — el texto visible vuelve a ser `**hola**`.
  const lineText = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    return el ? el.textContent : null;
  });
  check("revelada, la línea muestra `**hola**`", lineText === "**hola**", JSON.stringify(lineText));
  await page.keyboard.press("ArrowDown"); // cursor a la línea 2: se vuelve a ocultar
  await page.waitForTimeout(400);

  // Case: `Ctrl+/` — no queda ninguna decoración de ocultado en el DOM.
  await page.keyboard.press("Control+/");
  await page.waitForTimeout(500);
  const hiddenAfterToggle = await page.evaluate(() => document.querySelectorAll(".cm-editor .cm-lp-hidden").length);
  check("Ctrl+/ muestra el fuente sin ocultos", hiddenAfterToggle === 0, `hidden=${hiddenAfterToggle}`);
  check("Ctrl+/ no toca el contenido", (await getContent()) === "**hola**\ncola", JSON.stringify(await getContent()));
  await page.keyboard.press("Control+/"); // volver al modo vivo
  await page.waitForTimeout(500);

  // Case: deshacer hasta borrar los `**` y rehacer.
  const beforeUndo = await getContent();
  let undone = false;
  for (let i = 0; i < 30; i++) {
    await page.keyboard.press("Control+z");
    await page.waitForTimeout(80);
    if (!(await getContent()).includes("*")) {
      undone = true;
      break;
    }
  }
  check("deshacer borra los `**`", undone, JSON.stringify(await getContent()));
  let redone = false;
  for (let i = 0; i < 30; i++) {
    await page.keyboard.press("Control+Shift+z");
    await page.waitForTimeout(80);
    if ((await getContent()) === beforeUndo) {
      redone = true;
      break;
    }
  }
  if (!redone) {
    for (let i = 0; i < 30; i++) {
      await page.keyboard.press("Control+y");
      await page.waitForTimeout(80);
      if ((await getContent()) === beforeUndo) {
        redone = true;
        break;
      }
    }
  }
  check("rehacer devuelve el contenido", redone, JSON.stringify(await getContent()));
  check(
    "tras rehacer `.cm-lp-strong` reaparece",
    (await page.evaluate(() => document.querySelectorAll(".cm-editor .cm-lp-strong").length)) >= 1,
  );

  // Case: composición IME simulada con `Input.insertText`. El caret debe
  // quedar donde se insertó: escribir justo después deja la marca detrás
  // del texto insertado, lo que solo pasa si el caret avanzó con él.
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(200);
  const imeBefore = await getContent();
  await page.evaluate(() => {
    document.querySelector(".cm-editor .cm-content").focus();
    document.execCommand("insertText", false, "áé");
  });
  await page.waitForTimeout(200);
  await page.keyboard.type("Z", { delay: 20 });
  await page.waitForTimeout(400);
  const imeAfter = await getContent();
  check("IME inserta el texto exacto y el caret lo sigue", imeAfter === imeBefore + "áéZ", JSON.stringify(imeAfter));

  // Case: pegar `**x**` — el contenido es el pegado exacto.
  try {
    await page.evaluate(() => navigator.clipboard.writeText("**x**"));
  } catch (e) {
    problems.push("no se pudo escribir el portapapeles: " + e.message);
  }
  const pasteBefore = await getContent();
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(200);
  await page.keyboard.press("Control+v");
  await page.waitForTimeout(600);
  check("pegar deja el contenido exacto", (await getContent()) === pasteBefore + "**x**", JSON.stringify(await getContent()));

  // Case: marcadores desbalanceados — `**negrita` sin cerrar.
  await setContent("");
  await page.waitForTimeout(300);
  await page.click(".cm-editor .cm-content");
  await page.waitForTimeout(200);
  await page.keyboard.type("**negrita", { delay: 20 });
  await page.waitForTimeout(500);
  check("desbalanceado: el contenido queda intacto", (await getContent()) === "**negrita", JSON.stringify(await getContent()));
  await page.keyboard.type("!", { delay: 20 });
  await page.waitForTimeout(400);
  check("desbalanceado: el editor sigue respondiendo", (await getContent()) === "**negrita!", JSON.stringify(await getContent()));
  await page.keyboard.press("Enter");
  await page.keyboard.type("z", { delay: 20 });
  await page.waitForTimeout(400);
  await page.keyboard.press("ArrowUp");
  await page.waitForTimeout(400);
  const unbalancedStrong = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    return el ? el.querySelectorAll(".cm-lp-strong").length : -1;
  });
  check("desbalanceado: no hay strong a medio abrir", unbalancedStrong === 0, `strong=${unbalancedStrong}`);

  // Case: documento de 5 000 líneas — mediana de 20 pulsaciones < 50 ms,
  // del `dispatch` al `requestAnimationFrame` siguiente. La cuenta de líneas
  // sale del contenido: CodeMirror solo renderiza el viewport, así que
  // `.cm-line` cuenta lo visible, no el documento.
  const big = Array.from({ length: 5000 }, (_, i) => `línea ${i} con **negrita** y texto`).join("\n");
  await setContent(big);
  await page.waitForTimeout(800);
  const bigLines = (await getContent()).split("\n").length;
  check("el documento grande cargó", bigLines === 5000, `líneas=${bigLines}`);
  const times = await page.evaluate(async () => {
    const ed = document.querySelector(".cm-editor .cm-content");
    ed.focus();
    // Ctrl+End dentro del editor: el caret al final del documento.
    document.execCommand("selectAll", false, null);
    window.getSelection().collapseToEnd();
    const samples = [];
    for (let i = 0; i < 20; i++) {
      const t0 = performance.now();
      document.execCommand("insertText", false, "x");
      await new Promise((r) => requestAnimationFrame(() => r()));
      samples.push(performance.now() - t0);
    }
    samples.sort((a, b) => a - b);
    return samples;
  });
  const median = times.length ? times[Math.floor(times.length / 2)] : Infinity;
  check("mediana de 20 pulsaciones < 50 ms", median < 50, `mediana=${median.toFixed(2)}ms [${times.map((t) => t.toFixed(1)).join(",")}]`);

  for (const e of errors) {
    if (/reading 'then'|\.then.*undefined|panicked|assertion/i.test(e)) {
      problems.push("runtime error: " + e.split("\n")[0].slice(0, 160));
    }
  }

  await browser.close();
  server.close();

  if (problems.length > 0) {
    console.error(`FAIL: ${problems.length} problem(s):`);
    for (const p of problems) console.error("  - " + p);
    process.exit(1);
  }
  console.log("PASS: el editor en vivo oculta, estiliza y revela.");
})().catch((e) => {
  console.error("FAIL: " + e.message);
  process.exit(1);
});
