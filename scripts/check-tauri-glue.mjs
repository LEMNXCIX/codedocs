/**
 * Static half of the IPC contract check.
 *
 * `js_namespace = ["window", "__TAURI__", "core"]` only says *where* to look;
 * the property that gets called defaults to the Rust function name. So a Rust
 * binding named `raw_invoke` makes the generated glue call
 * `__TAURI__.core.raw_invoke`, which does not exist — every IPC call silently
 * resolves to `undefined`. That shipped as a crash on "open folder".
 *
 * The Rust compiler cannot see it, and neither can a source read. What this
 * does is inspect the *built* glue for the exact paths Tauri v2 actually
 * exposes, which is the thing the browser will look up at runtime.
 *
 * Requires `npm run build`.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const DIST = join(process.cwd(), "dist");

// The surface Tauri v2 puts on `window.__TAURI__`, with `withGlobalTauri: true`.
const REQUIRED = [
  "__TAURI__.core.invoke",
  "__TAURI__.event.listen",
  "__TAURI__.event.unlisten",
];

const files = readdirSync(DIST).filter((f) => f.endsWith(".js"));
if (files.length === 0) {
  console.error(`FAIL: no JS found in ${DIST}. Run \`npm run build\` first.`);
  process.exit(1);
}

const glue = files.map((f) => readFileSync(join(DIST, f), "utf8")).join("\n");

const problems = [];

for (const required of REQUIRED) {
  if (!glue.includes(required)) {
    // Find what it actually called, which is the actionable part of the report.
    const calls = [...new Set(glue.match(/__TAURI__\.[a-z]+\.[A-Za-z_]+/g) || [])];
    problems.push(
      `the built glue never calls ${required}. ` +
        (calls.length ? `It calls: ${calls.join(", ")}` : "It calls no __TAURI__ path at all."),
    );
  }
}

// Catch the shape of the bug generically: a __TAURI__ path whose final segment
// starts with `raw_` is the Rust binding name leaking into the JS call.
const leaked = [...new Set(glue.match(/__TAURI__\.[a-z]+\.raw_[A-Za-z_]+/g) || [])];
for (const call of leaked) {
  problems.push(
    `the built glue calls ${call}, which is a Rust binding name, not a Tauri API. ` +
      `Add js_name to the #[wasm_bindgen] attribute.`,
  );
}

if (problems.length > 0) {
  console.error(`FAIL: ${problems.length} problem(s):`);
  for (const p of problems) console.error("  - " + p);
  process.exit(1);
}

console.log(
  `PASS: the built glue calls the ${REQUIRED.length} real Tauri API path(s) and no raw_* leaks.`,
);
