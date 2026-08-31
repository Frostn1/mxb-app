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
    /** Keys the daily digest of a signup's IP address. Without it the digest is a plain
     *  hash, which is reversible for IPv4 — set it before open signup carries real load. */
    IP_HASH_SECRET?: string;
  }
}

export {};
