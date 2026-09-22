import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const tracked = execFileSync("git", ["ls-files", "-z"], { encoding: "utf8" })
  .split("\0")
  .filter(Boolean);

const privateModules = new Set([
  "crates/core/src/mxbsecure.rs",
  "crates/core/src/sidecar.rs",
]);

const forbiddenRust = [
  ["desktop plaintext decrypt", /mxbsecure::open\b/, "crate::mxbsecure::open"],
  ["registered plaintext opener", /securesource::set_opener\b/, "securesource::set_opener"],
  ["core plaintext opener", /securesource::open\b/, "securesource::open"],
  ["plaintext opener registration", /register_secure_opener/, "register_secure_opener"],
];

for (const [label, pattern, probe] of forbiddenRust) {
  if (!pattern.test(probe)) throw new Error(`broken secure-boundary matcher: ${label}`);
}

const violations = [];
for (const path of tracked) {
  if (privateModules.has(path)) {
    violations.push(`${path}: private module is tracked`);
  }
  if (!path.endsWith(".rs")) continue;

  const source = readFileSync(path, "utf8");
  for (const [label, pattern] of forbiddenRust) {
    if (pattern.test(source)) violations.push(`${path}: ${label}`);
  }
  if (
    path === "crates/core/src/securesource.rs" &&
    /\b(?:type|struct)\s+Opener\b|\bfn\s+open\s*\(/.test(source)
  ) {
    violations.push(`${path}: plaintext opener capability`);
  }
}

if (violations.length) {
  console.error("Secure-content boundary violated:\n" + violations.map((v) => `- ${v}`).join("\n"));
  process.exit(1);
}

console.log("Secure-content boundary intact: desktop code cannot open .mxbsecure plaintext.");
