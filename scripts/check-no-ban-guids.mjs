// No MX Bikes GUID of a real install belongs in this repository: it is public, and a GUID next to
// a ban reason is an accusation against a person that no later commit can take back. Bans live in
// the control plane's database, written through the admin API; their records are kept privately.
//
// Every tracked file is scanned for the GUID shape. The only ones allowed are listed in
// scripts/guid-allowlist.txt: synthetic test values and fixed samples, each added on purpose.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const GUID = /FF011000[0-9A-F]{10}/gi;
const ALLOWLIST = "scripts/guid-allowlist.txt";

if (!"id ff0110000111111111 ok".match(GUID)) throw new Error("broken GUID matcher");

const allowed = new Set(
  readFileSync(ALLOWLIST, "utf8")
    .split(/\r?\n/)
    .map((line) => line.replace(/#.*/, "").trim().toUpperCase())
    .filter(Boolean),
);

const tracked = execFileSync("git", ["ls-files", "-z"], { encoding: "utf8" })
  .split("\0")
  .filter((path) => path && path !== ALLOWLIST);

const violations = [];
for (const path of tracked) {
  let source;
  try {
    source = readFileSync(path, "utf8");
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
