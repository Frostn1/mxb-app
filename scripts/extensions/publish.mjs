#!/usr/bin/env node
// Publish signed .mxbext packages to the MXB App extension marketplace.
//
//   MXB_TOKEN=<your account token> node scripts/extensions/publish.mjs a.mxbext [b.mxbext ...]
//   MXB_API=http://127.0.0.1:8787 ...   (a local `wrangler dev`, for testing)
//
// The token is your MXB App account's (config.json `cpToken`). Packages must be signed
// (pack.mjs --key). The first upload of an id pins it to your account and your key; later
// versions must come from both. Packages not signed by mxbsecure wait for review before
// they show in the marketplace.

import { readFileSync } from "node:fs";

const api = (process.env.MXB_API ?? "https://api.mxbsecure.com").replace(/\/+$/, "");
const token = process.env.MXB_TOKEN;
const files = process.argv.slice(2);
if (!token || files.length === 0) {
  console.error("usage: MXB_TOKEN=... node scripts/extensions/publish.mjs <package.mxbext>...");
  process.exit(1);
}

let failed = 0;
for (const f of files) {
  const body = readFileSync(f);
  const res = await fetch(`${api}/v1/extensions`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/zip" },
    body,
  });
  const out = await res.json().catch(() => ({}));
  if (res.ok) console.log(`${f}: ${out.id} ${out.version} ${out.status}${out.verified ? " (verified)" : ""}`);
  else {
    failed++;
    console.error(`${f}: ${res.status} ${out.error ?? ""}`);
  }
}
process.exit(failed ? 1 : 0);
