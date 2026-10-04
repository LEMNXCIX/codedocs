/**
 * Verifies the JS↔Rust contract that the Rust compiler cannot check:
 * every `js_name = __codedocs*` in src/ must be exported by the built bundles.
 *
 * A mismatch is silent at runtime — wasm-bindgen resolves the missing global to
 * `undefined` and the call throws only when the user presses the shortcut.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const RUST_DIR = "src";
const JS_FILES = ["js/codemirror-bridge.mjs", "js/live-preview.mjs", "js/preview-bridge.mjs", "js/markdown-input.mjs"];

/** Recursively collect Rust sources. */
function rustSources(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) rustSources(path, out);
    else if (path.endsWith(".rs")) out.push(path);
  }
  return out;
}

// Globals Rust asks the browser for.
const declared = new Set();
for (const file of rustSources(RUST_DIR)) {
  const source = readFileSync(file, "utf8");
  for (const match of source.matchAll(/js_name = (__codedocs[A-Za-z_]*)/g)) {
    declared.add(match[1]);
  }
}

// Globals the JS bridges actually assign, plus the ones Rust sets on `window`.
// Matches both `window.X =` and the shorthand `X =` at the start of a line,
// which is the style these bundles use.
const provided = new Set(["__codedocs_save"]);
for (const file of JS_FILES) {
  const source = readFileSync(file, "utf8");
  // The capture must start after the dot, otherwise it includes it.
  for (const match of source.matchAll(/window\.(__codedocs[A-Za-z_]*)\s*=/g)) {
    provided.add(match[1]);
  }
  for (const match of source.matchAll(/^(__codedocs[A-Za-z_]*)\s*=/gm)) {
    provided.add(match[1]);
  }
}

const missing = [...declared].filter((name) => !provided.has(name)).sort();

if (declared.size === 0) {
  console.error("FAIL: no js_name declarations found — the regex is wrong.");
  process.exit(1);
}

if (missing.length > 0) {
  console.error(`FAIL: ${missing.length} global(s) called from Rust but never exported by JS:`);
  for (const name of missing) console.error(`  ${name}`);
  process.exit(1);
}

console.log(`PASS: ${declared.size} Rust-declared global(s) all exported by the JS bridges.`);
