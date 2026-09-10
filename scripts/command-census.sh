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

// --- what each binary registers -------------------------------------------------------
const registered = new Set();
for (const main of walk(path.join(root, "apps")).filter(p => p.endsWith("src-tauri/src/main.rs"))) {
  const src = fs.readFileSync(main, "utf8");
  const i = src.indexOf("generate_handler![");
  if (i < 0) continue;
  const j = src.indexOf("];", i);
  // An entry is a bare name or a path (`plugins::plugin_list`,
  // `mxb_core::trackview::read_track_info`); the command is the last segment either way.
  // The trailing comma is optional on the last entry, so it cannot be required here —
  // requiring it silently drops whichever command happens to be last.
  for (const m of src.slice(i + 18, j).matchAll(/^\s*([A-Za-z_][A-Za-z0-9_:]*)\s*,?\s*$/gm))
    registered.add(m[1].split("::").pop());
}

// --- what the frontends call ----------------------------------------------------------
// Two passes, deliberately different in strictness.
//
// `called` is the strict one — a name literally inside an `invoke(...)` — and it is what
// can fail this script, so a false positive there would be a broken build for no reason.
//
// `mentioned` is every quoted snake_case string anywhere in frontend source. Some commands
// are reached indirectly (`loadTrackSurfaces` takes the command as a defaulted parameter
// and passes it on), and those are invisible to the strict pass. It is only used to keep
// the informational "registered but not called" list from naming them.
const called = new Map();
const mentioned = new Set();
const srcDirs = [...walk(path.join(root, "apps")), ...walk(path.join(root, "packages"))]
  .filter(p => /\.(ts|tsx)$/.test(p) && !p.includes("src-tauri"));
for (const f of srcDirs) {
  const src = fs.readFileSync(f, "utf8");
  for (const m of src.matchAll(/\binvoke\s*(?:<[^>]*>)?\s*\(\s*"([a-z0-9_]+)"/g))
    if (!called.has(m[1])) called.set(m[1], path.relative(root, f));
  for (const m of src.matchAll(/"([a-z][a-z0-9_]*_[a-z0-9_]+)"/g)) mentioned.add(m[1]);
}

const missing = [...called.keys()].filter(c => !registered.has(c)).sort();
const unused = [...registered].filter(r => !called.has(r) && !mentioned.has(r)).sort();

console.log(`registered: ${registered.size}`);
console.log(`called:     ${called.size}\n`);
if (unused.length) {
  console.log(`registered but not called from any frontend (${unused.length}):`);
  for (const u of unused) console.log("  " + u);
  console.log("");
}
if (missing.length) {
  console.log(`ERROR: called by a frontend but registered nowhere (${missing.length}):`);
  for (const m of missing) console.log(`  ${m}   (${called.get(m)})`);
  process.exit(1);
}
console.log("every command the frontend calls is registered.");
JS
