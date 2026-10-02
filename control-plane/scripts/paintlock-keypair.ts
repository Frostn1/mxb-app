/**
 * Generate the Ed25519 pair that signs the paint lock list mxbserver fetches. Run once, locally.
 *
 *   bun scripts/paintlock-keypair.ts
 *
 * The private half goes into the worker as the `MXB_PAINTLOCK_SIGNING_KEY` secret
 * (`wrangler secret put MXB_PAINTLOCK_SIGNING_KEY`). The public half is printed in minisign
 * format: save it on each server that enforces locks as `[paints] pubkey_file` (the worker also
 * serves it at `GET /v1/paint-locks/pubkey`). A pair of its own: a leak of another key is not a
 * way to forge a lock list.
 *
 * The private key is printed once and not stored. Never commit it.
 */

import { lockKey, minisignPublicKey } from "../src/paintpolicy";
import { b64url } from "../src/verdict";

const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"])) as CryptoKeyPair;
const pkcs8 = b64url(new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer));
const key = await lockKey({ MXB_PAINTLOCK_SIGNING_KEY: pkcs8 } as Env);
if (!key) throw new Error("the generated key did not import");

console.log("MXB_PAINTLOCK_SIGNING_KEY (worker secret, keep private):");
console.log(pkcs8);
console.log("\npaintlock.pub (each enforcing server's [paints] pubkey_file):");
console.log(minisignPublicKey(key));
