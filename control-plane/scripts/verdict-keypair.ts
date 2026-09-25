/**
 * Generate the Ed25519 pair that signs the startup gate's verdicts. Run once, locally.
 *
 *   bun scripts/verdict-keypair.ts
 *
 * The private half goes into the worker as the `MXB_VERDICT_SIGNING_KEY` secret; the public half
 * is compiled into every app, as `VERDICT_PUBLIC_KEY` in `crates/core/src/appgate.rs`. A pair of
 * its own, not the plugin one: a leak of either should not be a leak of both.
 *
 * Rotating means shipping app builds: an install verifies against the key it was built with, so
 * a verdict signed by a new pair is ignored by an old build (which then behaves as if the gate
 * sent no signature at all — it still acts on the verdict, it just cannot keep it offline).
 *
 * The private key is printed once and not stored. Never commit it, never paste it anywhere but
 * `wrangler secret put`.
 */

function b64url(bytes: Uint8Array): string {
  let binary = "";
  for (const b of bytes) binary += String.fromCharCode(b);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

const pair = (await crypto.subtle.generateKey({ name: "Ed25519" }, true, [
  "sign",
  "verify",
])) as CryptoKeyPair;

const privatePkcs8 = new Uint8Array(await crypto.subtle.exportKey("pkcs8", pair.privateKey));
const publicRaw = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));

console.log(`
Private key (PKCS#8 DER, base64url) — the worker's secret.
Set it and then close this terminal:

  bunx wrangler secret put MXB_VERDICT_SIGNING_KEY
  ${b64url(privatePkcs8)}

Public key (raw, 32 bytes, base64url) — compile into the apps.
Put it in crates/core/src/appgate.rs as VERDICT_PUBLIC_KEY:

  ${b64url(publicRaw)}
`);
