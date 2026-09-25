// No MX Bikes GUID of a real install belongs in this repository: it is public, and a GUID next to
// a ban reason is an accusation against a person that no later commit can take back. Bans live in
// the control plane's database, written through the admin API; their records are kept privately.
//
// Every tracked file is scanned for the GUID shape. The only ones allowed are the entries of
// scripts/guid-allowlist.txt: synthetic test values and fixed samples, each added on purpose. The
// allowlist itself is scanned too, so a GUID cannot hide in one of its comments.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const GUID = /FF011000[0-9A-F]{10}/gi;
const ENTRY = /^FF011000[0-9A-F]{10}$/i;
const ALLOWLIST = "scripts/guid-allowlist.txt";

if (!"id ff0110000111111111 ok".match(GUID)) throw new Error("broken GUID matcher");

// Any file as text a GUID can be read out of: bytes as Latin-1 with NULs dropped, so UTF-8,
// UTF-16 in either byte order, and ASCII embedded in a binary all match the same pattern.
const text = (path) => readFileSync(path).toString("latin1").replace(/\0/g, "");

const violations = [];
const allowed = new Set();
for (const [n, line] of text(ALLOWLIST).split(/\r?\n/).entries()) {
  const hash = line.indexOf("#");
  const entry = (hash < 0 ? line : line.slice(0, hash)).trim();
  const comment = hash < 0 ? "" : line.slice(hash);
  for (const found of comment.match(GUID) ?? []) {
    violations.push(`${ALLOWLIST}:${n + 1}: ${found} in a comment`);
  }
  if (!entry) continue;
  if (!ENTRY.test(entry)) {
    violations.push(`${ALLOWLIST}:${n + 1}: "${entry}" is not one GUID`);
    continue;
  }
  allowed.add(entry.toUpperCase());
}

const tracked = execFileSync("git", ["ls-files", "-z"], { encoding: "utf8" })
  .split("\0")
  .filter((path) => path && path !== ALLOWLIST);

for (const path of tracked) {
  let source;
  try {
    source = text(path);
  } catch {
    continue;
  }
  for (const found of new Set(source.match(GUID) ?? [])) {
    if (!allowed.has(found.toUpperCase())) violations.push(`${path}: ${found}`);
  }
}

if (violations.length > 0) {
  console.error("MX Bikes GUIDs that are not on the allowlist (a real install's GUID never goes in this public repo):");
  for (const v of violations) console.error(`  ${v}`);
  console.error(`Bans go through /v1/web/admin/bans. A synthetic test value goes in ${ALLOWLIST}.`);
  process.exit(1);
}
console.log(`no unlisted MX Bikes GUIDs in ${tracked.length} tracked files`);
