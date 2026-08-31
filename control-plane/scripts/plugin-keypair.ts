/**
 * Generate the Ed25519 pair that signs plugin entitlements. Run once, ever.
 *
 *   bun scripts/plugin-keypair.ts
 *
 * The private half goes into the worker as a secret; the public half is compiled into the
 * app. Rotating means shipping a new app build, so this is not a thing to do casually —
 * every installed copy verifies against the key it was built with, and one that has not
 * updated will reject every entitlement signed by the new pair.
 *
 * The private key is printed once and not stored. If it is lost, mint a new pair and ship
 * an app update; if it leaks, do the same immediately, because anyone holding it can grant
 * themselves and everyone else a permanent licence.
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

  wrangler secret put PLUGIN_SIGNING_KEY
  ${b64url(privatePkcs8)}

Public key (raw, 32 bytes, base64url) — compile into the app.
Put it in src-tauri/src/plugins.rs as ENTITLEMENT_PUBLIC_KEY:

  ${b64url(publicRaw)}
`);
