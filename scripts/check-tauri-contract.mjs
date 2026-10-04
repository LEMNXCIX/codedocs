/**
 * Verifies that the Tauri command signatures and the arguments the frontend
 * sends actually agree.
 *
 * The Rust compiler cannot check this: `invoke("read_file", args)` is a string
 * plus a JS object, so renaming a command or an argument compiles cleanly and
 * fails at runtime with "invalid args". Same blind spot as
 * `check-bridge-contract.mjs`, one layer down.
 */
import { readFileSync } from "node:fs";

const BRIDGE = "src/utils/tauri_bridge.rs";
const COMMANDS = "src-tauri/src/commands.rs";

/** Split at top-level commas, ignoring those inside `<>` or `()`. */
function splitTopLevel(text, separator) {
  const parts = [];
  let depth = 0;
  let current = "";

  for (const ch of text) {
    if (ch === "<" || ch === "(" || ch === "[") depth++;
    else if (ch === ">" || ch === ")" || ch === "]") depth--;

    if (ch === separator && depth === 0) {
      parts.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  parts.push(current);
  return parts.map((p) => p.trim()).filter(Boolean);
}

/** Rust command name -> the argument names it declares, in camelCase. */
function rustCommands() {
  const source = readFileSync(COMMANDS, "utf8");
  const commands = new Map();

  const pattern =
    /#\[tauri::command(?:\(rename_all = "camelCase"\))?\]\s*\npub (?:async )?fn (\w+)\s*\(/g;

  let match;
  while ((match = pattern.exec(source)) !== null) {
    const name = match[1];

    // The parameter list runs to the paren that closes the fn signature.
    let depth = 0;
    let end = -1;
    for (let i = match.index + match[0].length - 1; i < source.length; i++) {
      if (source[i] === "(") depth++;
      else if (source[i] === ")") {
        depth--;
        if (depth === 0) {
          end = i;
          break;
        }
      }
    }
    if (end === -1) continue;

    const params = splitTopLevel(
      source.slice(match.index + match[0].length - 1 + 1, end),
      ",",
    )
      // `app: AppHandle` and `state: State<..>` are injected by Tauri, not sent
      // by the frontend.
      .filter((p) => !/^(app|state)\s*:/.test(p))
      // A parameter may already be snake_case; the default rename policy makes
      // it camelCase on the wire.
      .map((p) => p.split(":")[0].trim().replace(/_([a-z])/g, (_, c) => c.toUpperCase()))
      // Drop lifetime noise (`'_`) and anything that is not a plain ident.
      .filter((p) => /^[A-Za-z][A-Za-z0-9_]*$/.test(p));

    commands.set(name, new Set(params));
  }

  return commands;
}

/** Command name -> argument keys the frontend sends. */
function frontendCalls() {
  const source = readFileSync(BRIDGE, "utf8");
  const calls = [];

  // Match the command name, then read the balanced argument expression that
  // follows it so keys cannot bleed in from the next call.
  const pattern = /invoke(?:_with)?\(\s*"(\w+)"\s*(,)?/g;

  let match;
  while ((match = pattern.exec(source)) !== null) {
    const name = match[1];
    const hasArgs = match[2] === ",";
    const keys = [];

    if (hasArgs) {
      // Take the balanced expression after the comma.
      let depth = 0;
      let i = match.index + match[0].length;
      for (; i < source.length; i++) {
        const ch = source[i];
        if (ch === "(" || ch === "[") depth++;
        else if (ch === ")" || ch === "]") {
          if (depth === 0) break;
          depth--;
        }
      }
      const expression = source.slice(match.index + match[0].length, i);
      for (const pair of expression.matchAll(/\(\s*"([^"]+)"\s*,/g)) {
        keys.push(pair[1]);
      }
    }

    calls.push({ name, keys });
  }

  return calls;
}

const rust = rustCommands();
const calls = frontendCalls();

if (rust.size === 0) {
  console.error("FAIL: no commands parsed from", COMMANDS);
  process.exit(1);
}

const problems = [];

for (const { name, keys } of calls) {
  if (!rust.has(name)) {
    problems.push(`frontend invokes "${name}" but no such command exists`);
    continue;
  }
  const accepted = rust.get(name);
  for (const key of keys) {
    if (!accepted.has(key)) {
      problems.push(
        `"${name}": frontend sends "${key}" but Rust accepts [${[...accepted].join(", ")}]`,
      );
    }
  }
}

// Every command should be reachable; an orphan is dead code or a typo.
for (const name of rust.keys()) {
  if (!calls.some((c) => c.name === name)) {
    problems.push(`command "${name}" exists but the frontend never calls it`);
  }
}

if (problems.length > 0) {
  console.error(`FAIL: ${problems.length} contract mismatch(es):`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}

console.log(
  `PASS: ${calls.length} frontend call(s) match ${rust.size} Rust command(s).`,
);
