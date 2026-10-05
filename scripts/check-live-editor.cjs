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

  // The editor boots live (formatted in place); the CodeMirror instance
  // mounts with the app, so no mode switch is needed before driving it.
  try {
    await page.waitForSelector(".cm-editor .cm-content", { timeout: 8000 });
  } catch {
    problems.push("el editor no montó al arrancar");
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

  // === Task 4: math, Mermaid, tabla e imagen dentro del editor ===
  //
  // Cada caso mira el DOM de verdad (nodos `.katex`, `svg`, clases
  // `.cm-lp-*`), no solo la ausencia de excepciones: con el widget roto o
  // ausente el selector no aparece y el caso falla.

  // Case: `$x^2$` sin cursor en la línea — KaTeX renderiza dentro del editor.
  await setContent("$x^2$\ncola");
  await page.keyboard.press("Control+End"); // cursor a "cola": la línea 1 se oculta
  await page.waitForTimeout(800);
  let katexCount = 0;
  try {
    await page.waitForSelector(".cm-editor .katex", { timeout: 8000 });
    katexCount = await page.evaluate(() => document.querySelectorAll(".cm-editor .katex").length);
  } catch {
    katexCount = 0;
  }
  const mathWidget = await page.evaluate(() => !!document.querySelector(".cm-editor .cm-lp-math-inline"));
  check("math inline renderiza KaTeX", katexCount >= 1 && mathWidget, `katex=${katexCount} widget=${mathWidget}`);

  // Case: cursor en la línea de `$x^2$` — se ve el fuente crudo.
  await page.keyboard.press("ArrowUp"); // cursor a la línea 1: se revela
  await page.waitForTimeout(500);
  const katexRevealed = await page.evaluate(() => document.querySelectorAll(".cm-editor .katex").length);
  const mathRaw = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    return el ? el.textContent : null;
  });
  check("cursor en math muestra `$x^2$` crudo", katexRevealed === 0 && mathRaw === "$x^2$", `katex=${katexRevealed} texto=${JSON.stringify(mathRaw)}`);

  // Case: `$$…$$` en bloque sin cursor — KaTeX en modo display.
  await setContent("$$a^2 + b^2 = c^2$$\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(800);
  let katexDisplay = 0;
  try {
    await page.waitForSelector(".cm-editor .katex", { timeout: 8000 });
    katexDisplay = await page.evaluate(() => document.querySelectorAll(".cm-editor .katex").length);
  } catch {
    katexDisplay = 0;
  }
  const mathBlockWidget = await page.evaluate(() => !!document.querySelector(".cm-editor .cm-lp-math-block"));
  check("math display renderiza KaTeX", katexDisplay >= 1 && mathBlockWidget, `katex=${katexDisplay} widget=${mathBlockWidget}`);

  // Case: bloque ```mermaid sin cursor — hay un svg dentro del editor.
  const mermaidDoc = "```mermaid\ngraph TD;\n  A-->B;\n```\ncola";
  await setContent(mermaidDoc);
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(800);
  let svgCount = 0;
  try {
    await page.waitForSelector(".cm-editor svg", { timeout: 15000 });
    svgCount = await page.evaluate(() => document.querySelectorAll(".cm-editor svg").length);
  } catch {
    svgCount = 0;
  }
  const mermaidWidget = await page.evaluate(() => !!document.querySelector(".cm-editor .cm-lp-mermaid"));
  check("mermaid renderiza un svg", svgCount >= 1 && mermaidWidget, `svg=${svgCount} widget=${mermaidWidget}`);

  // Case: cursor en el bloque mermaid — se ve el código fuente.
  await page.keyboard.press("ArrowUp"); // fin de "cola" -> línea del ``` de cierre: revela
  await page.waitForTimeout(500);
  const svgRevealed = await page.evaluate(() => document.querySelectorAll(".cm-editor svg").length);
  const mermaidRaw = await page.evaluate(() =>
    [...document.querySelectorAll(".cm-editor .cm-line")].map((el) => el.textContent).join("\n"),
  );
  check("cursor en mermaid muestra el fuente", svgRevealed === 0 && mermaidRaw.includes("graph TD;"), `svg=${svgRevealed} fuente=${mermaidRaw.includes("graph TD;")}`);

  // Case: tabla de 2 columnas — pipes ocultos, celdas en la misma línea visual.
  await setContent("| uno | dos |\n|---|---|\n| tres | cuatro |\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const tableRows = await page.evaluate(() => document.querySelectorAll(".cm-editor .cm-lp-table-row").length);
  check("tabla: las filas llevan `.cm-lp-table-row`", tableRows >= 3, `filas=${tableRows}`);
  check("tabla: las pipes no se ven", (await hiddenInLine(1)) >= 2, `hidden=${await hiddenInLine(1)}`);
  const cellTop = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    if (!el) return null;
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    let r1 = null;
    let r2 = null;
    let node;
    while ((node = walker.nextNode())) {
      if (!r1 && node.nodeValue.includes("uno")) {
        const r = document.createRange();
        r.selectNodeContents(node);
        r1 = [...r.getClientRects()][0];
      }
      if (!r2 && node.nodeValue.includes("dos")) {
        const r = document.createRange();
        r.selectNodeContents(node);
        r2 = [...r.getClientRects()][0];
      }
    }
    if (!r1 || !r2) return null;
    return Math.abs(r1.top - r2.top);
  });
  check("tabla: las dos celdas quedan en la misma línea visual", cellTop !== null && cellTop <= 2, `dTop=${cellTop}`);

  // Case: `![alt](foto.png)` — placeholder con el alt y la ruta en el title.
  // (La imagen no carga: el CSP no resuelve rutas del workspace. Ver spec.)
  await setContent("![alt](foto.png)\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const imgInfo = await page.evaluate(() => {
    const el = document.querySelector(".cm-editor .cm-lp-image");
    return el ? { text: el.textContent, title: el.getAttribute("title") } : null;
  });
  check("imagen muestra el alt con la ruta en el title", !!imgInfo && imgInfo.text === "alt" && imgInfo.title === "foto.png", JSON.stringify(imgInfo));

  // Case: Mermaid con sintaxis inválida — muestra el fuente, sin excepción.
  const errsBeforeBad = errors.length;
  await setContent("```mermaid\nnot a diagram %%% $$$\n```\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(6000); // mermaid ya cargó: el render falla -> fuente
  const badWidget = await page.evaluate(() => !!document.querySelector(".cm-editor .cm-lp-mermaid"));
  const badSvg = await page.evaluate(() => document.querySelectorAll(".cm-editor .cm-lp-mermaid svg").length);
  const badRaw = await page.evaluate(() => {
    const el = document.querySelector(".cm-editor .cm-lp-mermaid");
    return el ? el.textContent : "";
  });
  const badFatal = errors.slice(errsBeforeBad).filter((e) => /^pageerror|uncaught|unhandled|reading 'then'/i.test(e));
  check("mermaid inválido muestra el fuente", badWidget && badSvg === 0 && badRaw.includes("not a diagram"), `widget=${badWidget} svg=${badSvg} fuente=${badRaw.includes("not a diagram")}`);
  check("mermaid inválido no lanza", badFatal.length === 0, JSON.stringify(badFatal.map((s) => s.slice(0, 120))));

  // Case: documento de 5 000 líneas — mediana de 20 pulsaciones < 50 ms,
  // del `dispatch` al `requestAnimationFrame` siguiente. La cuenta de líneas
  // sale del contenido: CodeMirror solo renderiza el viewport, así que
  // `.cm-line` cuenta lo visible, no el documento.
  //
  // Se mide tres veces y se toma la mejor mediana, y el motivo es que una sola
  // corrida no mide el costo de esta feature sino el del scheduler de la
  // máquina. Medido: 40.1 / 41.1 / 47.8 / 50.1 ms sobre el mismo build, con
  // colas de hasta 168 ms cuando hay otra cosa corriendo — y un gate que
  // sobrevive a esas diferencias está midiendo ruido.
  //
  // Lo que NO cambia es el umbral ni lo que el gate busca: la regresión real
  // que ya se detectó con él (el `resolve()` en O(n²) de `live_spans`) daba
  // 128.8 ms, muy por encima de las tres corridas. El mejor de tres la sigue
  // detectando, porque esa regresión es sistemática y no ruido de scheduler.
  const big = Array.from({ length: 5000 }, (_, i) => `línea ${i} con **negrita** y texto`).join("\n");
  await setContent(big);
  await page.waitForTimeout(800);
  const bigLines = (await getContent()).split("\n").length;
  check("el documento grande cargó", bigLines === 5000, `líneas=${bigLines}`);

  const measureOnce = () =>
    page.evaluate(async () => {
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

  const runs = [];
  for (let attempt = 0; attempt < 3; attempt++) runs.push(await measureOnce());
  const medians = runs
    .map((t) => (t.length ? t[Math.floor(t.length / 2)] : Infinity))
    .sort((a, b) => a - b);
  const median = medians[0];
  check(
    "mediana de 20 pulsaciones < 50 ms",
    median < 50,
    `mediana=${median.toFixed(2)}ms de [${medians.map((m) => m.toFixed(2)).join(", ")}] ` +
      `[${runs[0].map((t) => t.toFixed(1)).join(",")}]`,
  );

  // Case: KaTeX que no carga — el widget muestra el fuente, sin excepción.
  // Página aparte con `katex.min.js` abortado: el promise-cache del loader
  // (`katexLoad`) retiene el fallo, así que no puede compartir la página
  // principal, donde KaTeX ya cargó para los casos anteriores.
  await context.route("**/katex.min.js", (r) => r.abort());
  const page2 = await context.newPage();
  const errors2 = [];
  page2.on("console", (m) => {
    if (m.type() === "error") errors2.push(m.text());
  });
  page2.on("pageerror", (e) => errors2.push("pageerror: " + e.message));
  await page2.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle" });
  await page2.waitForTimeout(2000);
  try {
    await page2.waitForSelector(".cm-editor .cm-content", { timeout: 8000 });
  } catch {
    problems.push("sin KaTeX: el editor no montó al arrancar");
  }
  await page2.evaluate((t) => window.__codedocs_setContent(t), "$x^2$\ncola");
  await page2.click(".cm-editor .cm-content"); // foco: sin esto el teclado no entra
  await page2.waitForTimeout(300);
  await page2.keyboard.press("Control+End");
  await page2.waitForTimeout(3000); // inyección fallida -> fallback al fuente
  const noKatexWidget = await page2.evaluate(() => {
    const el = document.querySelector(".cm-editor .cm-lp-math-inline");
    return el ? el.textContent : null;
  });
  const noKatexCount = await page2.evaluate(() => document.querySelectorAll(".cm-editor .katex").length);
  check("sin KaTeX el widget muestra el fuente", noKatexCount === 0 && noKatexWidget === "$x^2$", `katex=${noKatexCount} texto=${JSON.stringify(noKatexWidget)}`);
  const fatal2 = errors2.filter((e) => /^pageerror|uncaught|unhandled/i.test(e));
  check("sin KaTeX no hay excepción", fatal2.length === 0, JSON.stringify(fatal2.map((s) => s.slice(0, 120))));
  await page2.close();

  // === Task 5: ayudas de escritura (listas, citas, Tab) ===
  //
  // Cada caso compara el contenido EXACTO del documento. Los nueve casos de
  // Enter/Tab son discriminantes: sin la extensión de `markdown-input.mjs`
  // el Enter/Tab por defecto deja otro texto y el caso falla. Los dos de
  // tipeo (`- `, `- [ ] `) NO discriminan —`typingRule` es un no-op
  // intencional que preserva el texto byte a byte— y son guardas de
  // no-corrupción: si algún handler futuro reescribe lo tipeado, fallan.
  // El foco vuelve con click antes de cada caso porque `setContent`
  // reescribe el documento por fuera del teclado.
  const focusEditor = async () => {
    await page.click(".cm-editor .cm-content");
    await page.waitForTimeout(200);
  };

  // Case: `- a` + Enter continúa la lista.
  await setContent("- a");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  check("Enter continúa la lista con `-`", (await getContent()) === "- a\n- ", JSON.stringify(await getContent()));

  // Case: `1. a` + Enter continúa numerando.
  await setContent("1. a");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  check("Enter continúa la lista numerada", (await getContent()) === "1. a\n2. ", JSON.stringify(await getContent()));

  // Case: `- [x] a` + Enter continúa la tarea desmarcada.
  await setContent("- [x] a");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  check("Enter continúa la tarea desmarcada", (await getContent()) === "- [x] a\n- [ ] ", JSON.stringify(await getContent()));

  // Case: `- ` + Enter en el ítem vacío sale de la lista.
  await setContent("- ");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  check("Enter en ítem vacío sale de la lista", (await getContent()) === "", JSON.stringify(await getContent()));

  // Case: Tab en `- a` indenta la línea dos espacios.
  await setContent("- a");
  await focusEditor();
  await page.keyboard.press("Control+Home");
  await page.keyboard.press("Tab");
  await page.waitForTimeout(400);
  check("Tab indenta el ítem", (await getContent()) === "  - a", JSON.stringify(await getContent()));

  // Case: Shift-Tab en `  - a` lo desindenta.
  await setContent("  - a");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Shift+Tab");
  await page.waitForTimeout(400);
  check("Shift-Tab desindenta el ítem", (await getContent()) === "- a", JSON.stringify(await getContent()));

  // Case: Tab fuera de lista inserta dos espacios en el cursor.
  await setContent("texto");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Tab");
  await page.waitForTimeout(400);
  check("Tab fuera de lista inserta dos espacios", (await getContent()) === "texto  ", JSON.stringify(await getContent()));

  // Case: regla de tipeo `- ` al inicio de una línea vacía.
  await setContent("");
  await focusEditor();
  await page.keyboard.type("- ", { delay: 30 });
  await page.waitForTimeout(400);
  check("tipear `- ` arma el marcador", (await getContent()) === "- ", JSON.stringify(await getContent()));

  // Case: regla de tipeo `- [ ] ` al inicio de una línea vacía.
  await setContent("");
  await focusEditor();
  await page.keyboard.type("- [ ] ", { delay: 30 });
  await page.waitForTimeout(400);
  check("tipear `- [ ] ` arma la tarea", (await getContent()) === "- [ ] ", JSON.stringify(await getContent()));

  // Case: Enter dentro de un bloque cercado inserta un salto literal aunque
  // la línea parezca lista (sin la guarda de `CodeBlock` continuaría `- `).
  await setContent("```\n- a\n```");
  await focusEditor();
  await page.keyboard.press("Control+Home");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  check("Enter en bloque de código no continúa la lista", (await getContent()) === "```\n- a\n\n```", JSON.stringify(await getContent()));

  // Case: Enter en `> a` continúa la cita.
  await setContent("> a");
  await focusEditor();
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(400);
  check("Enter continúa la cita", (await getContent()) === "> a\n> ", JSON.stringify(await getContent()));

  // === Task 7: el outline mueve el cursor al encabezado ===
  //
  // Regresión de la Task 6: el click buscaba el heading por id en un panel de
  // preview que ya no existe, así que no hacía absolutamente nada. Ahora mueve
  // el cursor del editor al encabezado. Sin el fix el cursor se queda donde
  // estaba y estos casos fallan; con offsets en bytes en vez de UTF-16 el caso
  // acentuado falla.
  check(
    "el bundle expone __codedocs_setCursor",
    await page.evaluate(() => typeof window.__codedocs_setCursor === "function"),
  );
  check(
    "el bundle expone __codedocs_getCursor",
    await page.evaluate(() => typeof window.__codedocs_getCursor === "function"),
  );
  // El outline vive en la pestaña "Contenido" del sidebar.
  await page.evaluate(() => {
    const tab = [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Contenido");
    if (tab) tab.click();
  });
  await page.waitForTimeout(400);

  const clickOutlineEntry = (index) => page.evaluate((i) => {
    const nav = document.querySelector('nav[aria-label="Contenido del documento"]');
    if (!nav) return -1;
    const btns = nav.querySelectorAll("button");
    if (i >= btns.length) return -1;
    btns[i].click();
    return btns.length;
  }, index);
  const getCursor = () => page.evaluate(() => window.__codedocs_getCursor());

  // Case: click en el segundo encabezado — el cursor salta ahí.
  await setContent("# Primero\n\ntexto\n\n## Segundo\n\nmás\n\n### Tercero\n");
  await page.waitForTimeout(500);
  await focusEditor();
  await page.keyboard.press("Control+Home"); // cursor a 0: sin el fix se queda acá
  await page.waitForTimeout(200);
  const outlineCount = await clickOutlineEntry(1);
  await page.waitForTimeout(400);
  const cursorAfterClick = await getCursor();
  const expectedSecond = await page.evaluate(() => window.__codedocs_getContent().indexOf("## Segundo"));
  check("el outline lista los tres encabezados", outlineCount === 3, `entradas=${outlineCount}`);
  check("click en el outline mueve el cursor al encabezado", cursorAfterClick === expectedSecond, `cursor=${cursorAfterClick} esperado=${expectedSecond}`);

  // Case: con texto acentuado y emoji antes — el offset es UTF-16, no bytes.
  await setContent("# Cáfé 😀\n\n## Segundo\n");
  await page.waitForTimeout(500);
  await focusEditor();
  await page.keyboard.press("Control+Home");
  await page.waitForTimeout(200);
  await clickOutlineEntry(1);
  await page.waitForTimeout(400);
  const cursorAccented = await getCursor();
  const expectedAccented = await page.evaluate(() => window.__codedocs_getContent().indexOf("## Segundo"));
  check("con acentos el cursor cae en el encabezado", cursorAccented === expectedAccented, `cursor=${cursorAccented} esperado=${expectedAccented}`);

  // === Task 9: listas visibles, código delimitado, regla y footnotes ===
  //
  // Cada caso mira el DOM de verdad: con la viñeta oculta, el fondo ausente
  // o la clase sin aplicar, el selector no aparece y el caso falla.

  // Case: la viñeta se ve (no oculta) y el anidamiento se nota.
  const markerText = (n) => page.evaluate((idx) => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[idx - 1];
    const m = el ? el.querySelector(".cm-lp-listmarker") : null;
    return m ? m.textContent : null;
  }, n);
  const markerLeft = (n) => page.evaluate((idx) => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[idx - 1];
    const m = el ? el.querySelector(".cm-lp-listmarker") : null;
    if (!m) return -1;
    const r = document.createRange();
    r.selectNodeContents(m);
    const rect = [...r.getClientRects()][0];
    return rect ? rect.left : -1;
  }, n);
  await setContent("- uno\n  - anidada\ncola");
  await page.keyboard.press("Control+End"); // cursor a "cola": líneas 1-2 formateadas
  await page.waitForTimeout(500);
  check("lista: la viñeta lleva `.cm-lp-listmarker`", (await markerText(1)) === "- ", JSON.stringify(await markerText(1)));
  check("lista: sin ocultos en el ítem", (await hiddenInLine(1)) === 0, `hidden=${await hiddenInLine(1)}`);
  const ml1 = await markerLeft(1);
  const ml2 = await markerLeft(2);
  check("lista: la anidada se ve más adentro", ml1 >= 0 && ml2 > ml1, `l1=${ml1} l2=${ml2}`);

  // Case: cursor en el ítem — la línea muestra su fuente cruda (`- uno`),
  // distinguible de la viñeta estilada: sin cursor no hay `.cm-lp-listmarker`.
  await page.keyboard.press("Control+Home"); // cursor a la línea 1: se revela
  await page.waitForTimeout(400);
  const revealedMarker = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    return el ? el.querySelectorAll(".cm-lp-listmarker").length : -1;
  });
  const revealedItem = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    return el ? el.textContent : null;
  });
  check("lista: cursor en el ítem muestra `- uno` crudo", revealedMarker === 0 && revealedItem === "- uno", `markers=${revealedMarker} texto=${JSON.stringify(revealedItem)}`);

  // Case: `*` y `+` conservan su carácter del fuente, sin normalizar.
  await setContent("* uno\n+ dos\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const star = await markerText(1);
  const plus = await markerText(2);
  check("lista: `*` y `+` conservan su carácter", star === "* " && plus === "+ ", JSON.stringify([star, plus]));

  // Case: la lista numerada muestra el número del fuente, no un contador.
  await setContent("1. uno\n2. dos\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const num1 = await markerText(1);
  const num2 = await markerText(2);
  check("numerada: se ve el número del fuente", num1 === "1. " && num2 === "2. ", JSON.stringify([num1, num2]));

  // Case: `- [ ] tarea` — espacios alrededor del checkbox intactos.
  await setContent("- [ ] tarea\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  check("tarea: sin ocultos alrededor del checkbox", (await hiddenInLine(1)) === 0, `hidden=${await hiddenInLine(1)}`);
  const taskLine = await page.evaluate(() => {
    const el = document.querySelectorAll(".cm-editor .cm-line")[0];
    return el ? el.textContent : null;
  });
  check("tarea: el texto no queda pegado", taskLine === "- [ ] tarea", JSON.stringify(taskLine));

  // Case: bloque cercado — vallas ocultas, cuerpo sombreado.
  await setContent("```rust\ncode\n```\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const codeBody = await page.evaluate(() => {
    const el = document.querySelector(".cm-editor .cm-lp-codeblock");
    if (!el) return null;
    return { text: el.textContent, bg: getComputedStyle(el).backgroundColor };
  });
  check(
    "código cercado: el cuerpo lleva `.cm-lp-codeblock` con fondo",
    !!codeBody && codeBody.text === "code" && codeBody.bg !== "rgba(0, 0, 0, 0)",
    JSON.stringify(codeBody),
  );
  check("código cercado: las vallas siguen ocultas", (await hiddenInLine(1)) >= 1, `hidden=${await hiddenInLine(1)}`);

  // Case: bloque indentado con 4 espacios — cuerpo sombreado también.
  await setContent("para\n\n    code\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const indBody = await page.evaluate(() => {
    const el = document.querySelector(".cm-editor .cm-lp-codeblock");
    if (!el) return null;
    return { text: el.textContent, bg: getComputedStyle(el).backgroundColor };
  });
  check(
    "código indentado: el cuerpo lleva `.cm-lp-codeblock` con fondo",
    !!indBody && indBody.text.includes("code") && indBody.bg !== "rgba(0, 0, 0, 0)",
    JSON.stringify(indBody),
  );

  // Case: `---` se ve como una línea, no como tres guiones.
  await setContent("a\n\n---\n\nb");
  await page.keyboard.press("Control+End"); // cursor en "b": la regla formateada
  await page.waitForTimeout(500);
  check("regla: la línea lleva `.cm-lp-hr`", await page.evaluate(() => !!document.querySelector(".cm-editor .cm-lp-hr")));

  // Case: footnotes — referencia y definición se distinguen.
  await setContent("texto[^1]\n\n[^1]: pie\ncola");
  await page.keyboard.press("Control+End");
  await page.waitForTimeout(500);
  const fnRef = await page.evaluate(() => {
    const el = document.querySelector(".cm-editor .cm-lp-footnote-ref");
    return el ? el.textContent : null;
  });
  check("footnote: la referencia se distingue", fnRef === "[^1]", JSON.stringify(fnRef));
  check("footnote: la definición se lee como bloque", await page.evaluate(() => !!document.querySelector(".cm-editor .cm-lp-footnote-def")));

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
