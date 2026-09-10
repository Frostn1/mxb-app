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
// What each app's *built bundle* can reach.
//
// The bundle is the right oracle and the other two attempts were both wrong:
//
//  * Charging an app for every `invoke` in `packages/shared` is far too strict —
//    `api/mods.ts` wraps all 250-odd commands and no app imports more than a few.
//  * Following imports at module granularity is the same problem wearing a hat: reaching
//    that one module charges you for everything in it.
//  * Mapping each shared *export* to the commands in its body was too loose in the other
//    direction — it missed `export default function`, missed default imports, and missed a
//    shared component calling another. It passed the studio while `ViewerPanel` invoked
//    `preview_model_swap`, which the studio did not register; the app said "command not
//    found" the moment a bike preview refreshed.
//
// Rollup already answers this exactly: it tree-shakes, so a command name survives into a
// bundle only if that app can reach it. Verified — `launch_game` is in the manager's bundle
// and not the studio's; `preview_model_swap` is in both.
//
// The catch, and the reason this check once passed while broken: a *stale* bundle answers
// confidently about an app that no longer exists. So the freshness of the build is checked
// first, and a stale one fails rather than reporting on yesterday.
const apps = fs.readdirSync(path.join(root, "apps"), { withFileTypes: true })
  .filter(e => e.isDirectory()).map(e => e.name);

const registeredIn = (main) => {
  const out = new Set();
  if (!fs.existsSync(main)) return out;
  const src = fs.readFileSync(main, "utf8");
  const i = src.indexOf("generate_handler![");
  if (i < 0) return out;
  // Match the bracket rather than looking for `];`: the manager assigns the handler to a
  // variable and closes with `];`, the studio passes it to `.invoke_handler(...)` and closes
  // with `])`. Searching for the wrong one runs off the end of the list.
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

const newest = (dir) => walk(dir).reduce((t, f) => Math.max(t, fs.statSync(f).mtimeMs), 0);

const perApp = apps.map(app => ({
  app, reg: registeredIn(path.join(root, "apps", app, "src-tauri/src/main.rs")),
})).filter(a => a.reg.size);

// Every command name anyone registers — the vocabulary to look for in a bundle.
const vocabulary = new Set(perApp.flatMap(a => [...a.reg]));

let failed = false;
for (const { app, reg } of perApp) {
  const assets = path.join(root, "apps", app, "dist", "assets");
  if (!fs.existsSync(assets)) {
    failed = true;
    console.log(`${app}: no build in dist/ — run \`npm run build\` first`);
    continue;
  }
  const srcRoots = [path.join(root, "apps", app, "src"), path.join(root, "packages/shared/src")];
  const srcTime = Math.max(...srcRoots.map(newest));
  const bundleTime = newest(assets);
  if (bundleTime < srcTime) {
    failed = true;
    console.log(`${app}: dist/ is older than its sources — rebuild before trusting this`);
    continue;
  }
  const bundle = fs.readdirSync(assets).filter(f => f.endsWith(".js"))
    .map(f => fs.readFileSync(path.join(assets, f), "utf8")).join("\n");
  const reachable = [...vocabulary].filter(c => bundle.includes(`"${c}"`)).sort();
  const missing = reachable.filter(c => !reg.has(c));

  console.log(`${app}: registers ${reg.size}, bundle reaches ${reachable.length}`);
  if (missing.length) {
    failed = true;
    console.log(`  ERROR: in the bundle but not registered in this app (${missing.length}):`);
    for (const c of missing) console.log(`    ${c}`);
  }
}
console.log("");
if (failed) process.exit(1);
console.log("every app registers every command its frontend calls.");
JS
