#!/usr/bin/env node
// Pack (and sign) an MXB App extension.
//
//   node scripts/extensions/pack.mjs <folder> [--key author.pem] [--out name.mxbext]
//   node scripts/extensions/pack.mjs --keygen author.pem
//
// A `.mxbext` is a zip of the folder plus `signature.json`. The signature covers every
// file's path and SHA-256 (see `digest_input` in crates/core/src/extensions.rs), so a single
// byte changed after signing makes the app refuse the package.
//
// Without --key the package is unsigned: it installs only with developer mode on. With your
// own key it installs as "signed" and the app pins that key, so keep the .pem safe: an update
// signed by any other key is refused. Packages signed by mxbsecure's key show as verified.
//
// No dependencies: Node's own zlib and crypto. Node 18+.

import { createHash, generateKeyPairSync, createPrivateKey, createPublicKey, sign } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync, existsSync } from "node:fs";
import { basename, join, relative, sep } from "node:path";
import { deflateRawSync } from "node:zlib";

const ALLOWED = new Set(["json", "html", "htm", "js", "mjs", "css", "png", "jpg", "jpeg", "webp", "gif", "svg", "woff", "woff2", "ttf", "otf", "txt", "md"]);

function die(msg) {
  console.error(`pack: ${msg}`);
  process.exit(1);
}

const args = process.argv.slice(2);
const opt = (name) => {
  const i = args.indexOf(name);
  if (i < 0) return undefined;
  const v = args[i + 1];
  args.splice(i, 2);
  return v;
};

const keygen = opt("--keygen");
if (keygen) {
  if (existsSync(keygen)) die(`${keygen} exists; refusing to overwrite a key`);
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  writeFileSync(keygen, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
  const raw = publicKey.export({ type: "spki", format: "der" }).subarray(-32);
  console.log(`wrote ${keygen}`);
  console.log(`public key: ${raw.toString("base64")}`);
  process.exit(0);
}

const keyPath = opt("--key");
const outArg = opt("--out");
const folder = args[0];
if (!folder || !existsSync(folder) || !statSync(folder).isDirectory()) {
  die("usage: pack.mjs <folder> [--key author.pem] [--out name.mxbext] | --keygen author.pem");
}

// Same rules as the app's developer-mode folder loader: skip dotfiles and node_modules,
// skip types a package may not carry, and say what was skipped.
function walk(dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.name.startsWith(".") || e.name === "node_modules") continue;
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.isFile()) out.push(p);
  }
  return out;
}

const files = new Map();
for (const p of walk(folder)) {
  const rel = relative(folder, p).split(sep).join("/");
  const ext = rel.split(".").pop().toLowerCase();
  if (rel === "signature.json") continue;
  if (!ALLOWED.has(ext)) {
    console.warn(`skipped ${rel} (extensions can't carry .${ext} files)`);
    continue;
  }
  files.set(rel, readFileSync(p));
}
if (!files.has("extension.json")) die(`${folder} has no extension.json`);

let manifest;
try {
  manifest = JSON.parse(files.get("extension.json").toString("utf8"));
} catch (e) {
  die(`extension.json: ${e.message}`);
}

// Sorted by UTF-8 bytes, the same order Rust's BTreeMap<String, _> gives.
const sorted = [...files.keys()].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));

if (keyPath) {
  const key = createPrivateKey(readFileSync(keyPath));
  const pub = createPublicKey(key).export({ type: "spki", format: "der" }).subarray(-32);
  let input = "mxbext-v1\n";
  for (const rel of sorted) {
    input += `${rel}\n${createHash("sha256").update(files.get(rel)).digest("hex")}\n`;
  }
  const signature = sign(null, Buffer.from(input, "utf8"), key);
  files.set(
    "signature.json",
    Buffer.from(JSON.stringify({ publicKey: pub.toString("base64"), signature: signature.toString("base64") }, null, 2)),
  );
  sorted.push("signature.json");
  console.log(`signed with ${basename(keyPath)} (key ${createHash("sha256").update(pub).digest("hex").slice(0, 16)})`);
} else {
  console.warn("unsigned: this installs only with developer mode on");
}

// --- a minimal zip writer (deflate, no zip64; packages are capped at 25 MB anyway) --------
const crcTable = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

const locals = [];
const centrals = [];
let offset = 0;
for (const rel of sorted) {
  const data = files.get(rel);
  const name = Buffer.from(rel, "utf8");
  const comp = deflateRawSync(data);
  const crc = crc32(data);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4);
  local.writeUInt16LE(0x0800, 6); // UTF-8 names
  local.writeUInt16LE(8, 8); // deflate
  local.writeUInt32LE(0, 10); // time/date
  local.writeUInt32LE(crc, 14);
  local.writeUInt32LE(comp.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(name.length, 26);
  local.writeUInt16LE(0, 28);
  locals.push(local, name, comp);
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4);
  central.writeUInt16LE(20, 6);
  central.writeUInt16LE(0x0800, 8);
  central.writeUInt16LE(8, 10);
  central.writeUInt32LE(0, 12);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(comp.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(name.length, 28);
  central.writeUInt32LE(offset, 42);
  centrals.push(central, name);
  offset += local.length + name.length + comp.length;
}
const cd = Buffer.concat(centrals);
const end = Buffer.alloc(22);
end.writeUInt32LE(0x06054b50, 0);
end.writeUInt16LE(sorted.length, 8);
end.writeUInt16LE(sorted.length, 10);
end.writeUInt32LE(cd.length, 12);
end.writeUInt32LE(offset, 16);
const zip = Buffer.concat([...locals, cd, end]);
if (zip.length > 25 * 1024 * 1024) die("the package is over 25 MB");

const out = outArg ?? `${manifest.id ?? "extension"}-${manifest.version ?? "0.0.0"}.mxbext`;
writeFileSync(out, zip);
console.log(`wrote ${out} (${sorted.length} files, ${(zip.length / 1024).toFixed(1)} KB)`);
