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

// --- per app: what it registers, against what its bundle can actually reach ------------
//
// Pooled across apps this check is worthless, and worse than worthless — it was pooled when
// the studio shell was added, and it passed happily while the studio called `app_platform`,
// which only the manager registered.
//
// Source scanning cannot answer it either: `packages/shared/api/mods.ts` wraps every command
// in the app, and an app that imports two of those wrappers does not ship the other two
// hundred. So the question is asked of the built bundle, which is tree-shaken and therefore
// contains exactly the command names that app can reach. That means this has to run after
// the build; if a bundle is missing it says so and skips, rather than passing quietly.
const apps = fs.readdirSync(path.join(root, "apps"), { withFileTypes: true })
  .filter(e => e.isDirectory()).map(e => e.name);

const registeredIn = (main) => {
  const out = new Set();
  if (!fs.existsSync(main)) return out;
  const src = fs.readFileSync(main, "utf8");
  const i = src.indexOf("generate_handler![");
  if (i < 0) return out;
  const j = src.indexOf("];", i);
  // An entry is a bare name or a path (`plugins::plugin_list`,
  // `mxb_core::viewer::load_bike_model`); the command is the last segment either way. The
  // trailing comma is optional on the last entry, so it cannot be required here.
  for (const m of src.slice(i + 18, j).matchAll(/^\s*([A-Za-z_][A-Za-z0-9_:]*)\s*,?\s*$/gm))
    out.add(m[1].split("::").pop());
  return out;
};

const perApp = apps.map(app => ({
  app,
  reg: registeredIn(path.join(root, "apps", app, "src-tauri/src/main.rs")),
})).filter(a => a.reg.size);

// Every command name anyone registers — the vocabulary to look for in a bundle.
const vocabulary = new Set(perApp.flatMap(a => [...a.reg]));

let failed = false, skipped = 0;
for (const { app, reg } of perApp) {
  const assets = path.join(root, "apps", app, "dist", "assets");
  if (!fs.existsSync(assets)) {
    console.log(`${app}: no build in dist/ — skipped (run \`npm run build\` first)`);
    skipped++;
    continue;
  }
  const bundle = fs.readdirSync(assets).filter(f => f.endsWith(".js"))
    .map(f => fs.readFileSync(path.join(assets, f), "utf8")).join("\n");
  const reachable = [...vocabulary].filter(c => bundle.includes(`"${c}"`)).sort();
  const missing = reachable.filter(c => !reg.has(c));

  console.log(`${app}: registers ${reg.size}, bundle reaches ${reachable.length}`);
  if (missing.length) {
    failed = true;
    console.log(`  ERROR: in the bundle but registered nowhere in this app (${missing.length}):`);
    for (const m of missing) console.log(`    ${m}`);
  }
}
console.log("");
if (skipped === perApp.length) {
  console.log("nothing checked — no bundles found.");
  process.exit(1);
}
if (failed) process.exit(1);
console.log("every app registers every command its frontend calls.");
JS
