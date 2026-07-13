#!/usr/bin/env node
// Maps your MX Bikes folder structure (read-only) so we can see exactly how
// tracks, bikes and liveries/paints are organized on disk.
//
// Usage (from anywhere, no install needed):
//   node map-mxbikes.mjs
//   node map-mxbikes.mjs "C:\Users\you\Documents\PiBoSo\MX Bikes"
//
// It prints a summary and writes the full map to `mxbikes-structure.txt` in the
// current folder. Send me that file (or paste it).

import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const DEFAULT = path.join(os.homedir(), "Documents", "PiBoSo", "MX Bikes");
const root = process.argv[2] ? path.resolve(process.argv[2]) : DEFAULT;

const MAX_DEPTH = 6; // deep enough to reach bike -> paints -> files
const MAX_DIRS = 12; // subfolders shown per folder, then summarized
const MAX_FILES = 6; // files shown per folder, then summarized
const MAX_LINES = 3000;

if (!fs.existsSync(root)) {
  console.error(`\nCouldn't find MX Bikes at:\n  ${root}\n`);
  console.error("Pass the path explicitly, e.g.:");
  console.error(
    `  node map-mxbikes.mjs "C:\\Users\\you\\Documents\\PiBoSo\\MX Bikes"\n`,
  );
  process.exit(1);
}

const lines = [];
let truncated = false;
const out = (s) => {
  if (lines.length >= MAX_LINES) truncated = true;
  else lines.push(s);
};

function extSummary(files) {
  const byExt = {};
  for (const f of files) {
    const e = (path.extname(f) || "(no ext)").toLowerCase();
    byExt[e] = (byExt[e] || 0) + 1;
  }
  return Object.entries(byExt)
    .sort((a, b) => b[1] - a[1])
    .map(([e, n]) => `${n} ${e}`)
    .join(", ");
}

function walk(dir, indent, depth) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch (e) {
    out(`${indent}<cannot read: ${e.code}>`);
    return;
  }

  const dirs = entries
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort((a, b) => a.localeCompare(b));
  const files = entries
    .filter((e) => e.isFile())
    .map((e) => e.name)
    .sort((a, b) => a.localeCompare(b));

  if (files.length) {
    out(`${indent}[${files.length} file(s): ${extSummary(files)}]`);
    for (const f of files.slice(0, MAX_FILES)) out(`${indent}  - ${f}`);
    if (files.length > MAX_FILES)
      out(`${indent}  - ... +${files.length - MAX_FILES} more`);
  }

  if (depth >= MAX_DEPTH) {
    if (dirs.length)
      out(`${indent}... (${dirs.length} subfolder(s), max depth reached)`);
    return;
  }

  for (const d of dirs.slice(0, MAX_DIRS)) {
    out(`${indent}${d}/`);
    walk(path.join(dir, d), indent + "    ", depth + 1);
  }
  if (dirs.length > MAX_DIRS)
    out(`${indent}... +${dirs.length - MAX_DIRS} more folder(s)`);
}

out(`MX Bikes structure map`);
out(`root: ${root}`);
out("");
walk(root, "", 0);

const report =
  lines.join("\n") +
  (truncated ? `\n\n... output truncated at ${MAX_LINES} lines` : "") +
  "\n";

const outFile = path.join(process.cwd(), "mxbikes-structure.txt");
fs.writeFileSync(outFile, report, "utf8");

console.log(report.length > 6000 ? report.slice(0, 6000) + "\n..." : report);
console.log("──────────");
console.log(`Full map written to: ${outFile}`);
console.log("Send me that file (or paste its contents) and I'll wire up bikes.");
