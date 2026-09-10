#!/usr/bin/env bash
#
# Every Tauri command the frontend calls, against every command a binary registers.
#
#   scripts/command-census.sh
#
# The compiler cannot check this. `generate_handler!` takes a token list, and a name dropped
# from it is not a build error — it is a runtime "command not found" the first time someone
# opens the screen that needs it. That is the failure mode a command moving between crates
# invites, so it gets a check of its own.
#
# Exits non-zero when the frontend calls something nothing registers. Names registered but
# never called are reported without failing: a command can be reached from a plugin panel,
# or belong to the other app in the workspace.

set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Both halves are done in node: the handler list and an `invoke(...)` call both routinely
# span lines, and grep/sed are line-based. (BSD sed also has no \s, which is its own trap.)
node - "$root" <<'JS'
const fs = require("fs"), path = require("path");
const root = process.argv[2];

const walk = (d, out = []) => {
  if (!fs.existsSync(d)) return out;
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name);
    if (e.isDirectory()) { if (e.name !== "node_modules") walk(p, out); }
    else out.push(p);
  }
  return out;
};

// --- per app: what it registers, against what its own code can reach -------------------
//
// Three things this has to get right, each learned the hard way:
//
//  * Pooled across apps it is worthless — it was pooled when the studio shell was added,
//    and passed happily while the studio called `app_platform`, which only the manager
//    registered. Each app answers for itself.
//  * Counting every `invoke` in `packages/shared` against every app is far too strict:
//    `api/mods.ts` wraps every command in the suite, and an app importing two of those
//    wrappers does not call the other two hundred.
//  * Reading the built bundle is too *loose*: that module is not fully tree-shaken, so
//    every command's name is present as a string whether or not anything calls it.
//
// So: follow the imports. A shared module is read once to learn which command each exported
// function invokes; an app is then charged only for the exports it actually imports, plus
// whatever it invokes directly.
const apps = fs.readdirSync(path.join(root, "apps"), { withFileTypes: true })
  .filter(e => e.isDirectory()).map(e => e.name);

const registeredIn = (main) => {
  const out = new Set();
  if (!fs.existsSync(main)) return out;
  const src = fs.readFileSync(main, "utf8");
  const i = src.indexOf("generate_handler![");
  if (i < 0) return out;
  // Match the bracket rather than looking for `];`: the manager assigns the handler to a
  // variable and closes with `];`, the studio passes it to `.invoke_handler(...)` and
  // closes with `])`. Searching for the wrong one runs off the end of the list.
  let depth = 0, j = -1;
  for (let k = i + "generate_handler!".length; k < src.length; k++) {
    if (src[k] === "[") depth++;
    else if (src[k] === "]") { depth--; if (depth === 0) { j = k; break; } }
  }
  if (j < 0) return out;
  // The trailing comma is optional on the last entry, so it cannot be required here.
  for (const m of src.slice(i + "generate_handler![".length, j).matchAll(/^\s*([A-Za-z_][A-Za-z0-9_:]*)\s*,?\s*$/gm))
    out.add(m[1].split("::").pop());
  return out;
};

// exported function name -> the commands its body invokes, per shared module
const sharedExports = new Map();
for (const f of walk(path.join(root, "packages"))) {
  if (!/\.(ts|tsx)$/.test(f)) continue;
  const src = fs.readFileSync(f, "utf8");
  // Split on top-level `export function|const`, then look for invokes inside each.
  const parts = src.split(/\nexport (?:async )?(?:function|const) /);
  for (const part of parts.slice(1)) {
    const name = part.match(/^([A-Za-z_$][\w$]*)/)?.[1];
    if (!name) continue;
    const cmds = [...part.matchAll(/\binvoke\s*(?:<[^>]*>)?\s*\(\s*"([a-z0-9_]+)"/g)].map(m => m[1]);
    if (cmds.length) sharedExports.set(name, (sharedExports.get(name) ?? []).concat(cmds));
  }
}

let failed = false;
for (const app of apps) {
  const reg = registeredIn(path.join(root, "apps", app, "src-tauri/src/main.rs"));
  if (!reg.size) continue;
  const called = new Map();
  for (const f of walk(path.join(root, "apps", app, "src"))) {
    if (!/\.(ts|tsx)$/.test(f)) continue;
    const src = fs.readFileSync(f, "utf8");
    const rel = path.relative(root, f);
    for (const m of src.matchAll(/\binvoke\s*(?:<[^>]*>)?\s*\(\s*"([a-z0-9_]+)"/g))
      if (!called.has(m[1])) called.set(m[1], rel);
    // named imports from the shared package charge this app for what they invoke
    for (const m of src.matchAll(/import\s*(?:type\s*)?\{([^}]*)\}\s*from\s*"@frost\/shared[^"]*"/g))
      for (const raw of m[1].split(",")) {
        const nm = raw.trim().replace(/^type\s+/, "").split(/\s+as\s+/)[0].trim();
        for (const c of sharedExports.get(nm) ?? []) if (!called.has(c)) called.set(c, rel);
      }
  }
  const missing = [...called.keys()].filter(c => !reg.has(c)).sort();
  console.log(`${app}: registers ${reg.size}, reachable from its own code ${called.size}`);
  if (missing.length) {
    failed = true;
    console.log(`  ERROR: called but not registered in this app (${missing.length}):`);
    for (const m of missing) console.log(`    ${m}   (${called.get(m)})`);
  }
}
console.log("");
if (failed) process.exit(1);
console.log("every app registers every command its frontend calls.");
JS
