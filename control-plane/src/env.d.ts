/**
 * Secrets, declared for the type checker.
 *
 * `wrangler types` generates `Env` from `wrangler.jsonc`, and secrets deliberately do not
 * appear there — putting their *values* in config is the mistake the secret store exists to
 * prevent. So their names are declared here instead, by interface merging, and the values
 * come from `wrangler secret put`.
 *
 * They are optional on purpose. A deployment without them is a valid deployment — paint
 * sync, the registry and the roster all work — it simply cannot provision, and the
 * provisioning endpoints answer 503 rather than crashing on a missing key.
 */
declare global {
  interface Env {
    /** IAM key scoped to launching and managing `mxb:managed` instances in one region. */
    AWS_ACCESS_KEY_ID?: string;
    AWS_SECRET_ACCESS_KEY?: string;
    /** Buy Me a Coffee's webhook signing secret. Without it `/v1/bmac/webhook` answers 503. */
    BMAC_WEBHOOK_SECRET?: string;
    /** Discord webhook the supporter announcements are posted to. A credential in itself:
     *  anyone holding the URL can post to that channel. */
    DISCORD_DONATION_WEBHOOK_URL?: string;
    /** Anthropic API key, for writing track programs. Without it `/v1/track/generate`
     *  answers 503 — the app says track generation isn't available and everything else
     *  carries on. Spend is capped by the endpoint itself: one Opus call per request, at
     *  most 16k output tokens, and only ever a motocross track. */
    ANTHROPIC_API_KEY?: string;
    /** Ed25519 private key (PKCS#8 DER, base64url) that signs plugin entitlements. The app
     *  holds only the public half, so a leak of the app cannot mint licenses. Without it
     *  every licensing endpoint answers 503 rather than issuing something unsigned - an
     *  unsigned license is not a degraded one, it is a forgery with our name on it.
     *  Generate with `bun scripts/plugin-keypair.ts`. */
    PLUGIN_SIGNING_KEY?: string;
    /** Reads the usage dashboard and the stats JSON. Without it both answer 503, which is
     *  the right default: a deployment that was never given a key has no admin surface
     *  rather than an open one. */
    ADMIN_KEY?: string;
    /** Keys the daily digest of a signup's IP address. Without it the digest is a plain
     *  hash, which is reversible for IPv4 — set it before open signup carries real load. */
    IP_HASH_SECRET?: string;
    /** Base64 of 32 random bytes. Wraps every secured asset's content key, so a database
     *  leak yields wrapped keys and no way to unwrap them. Absent means secured content is
     *  off: `/v1/keys/grant` answers 503 rather than serving a key from nothing. */
    MXB_ASSET_MASTER_KEY?: string;
  }
}

export {};
