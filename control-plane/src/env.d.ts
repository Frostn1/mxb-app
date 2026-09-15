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
    /**
     * Which model writes the track, overriding the default in `trackgen.ts`. A running-cost
     * decision rather than a code one — see the note there on what the cheap one cannot do.
     */
    TRACK_MODEL?: string;
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
    /** Keys the build signature on `POST /v1/usage`, matching the key compiled into the apps.
     *  Not authentication — the key ships inside a binary anyone can download — but it puts a
     *  reverse-engineering step between the endpoint and a script. Unset means reports are not
     *  checked. A secret. */
    USAGE_SIGNING_KEY?: string;
    /** `"1"` refuses an unsigned usage report. Not a secret — a var in `wrangler.jsonc`, so
     *  turning it on is a reviewable diff. Leave it off until signed builds are the ones in the
     *  field: switching early drops everybody's numbers and says nothing. */
    MXB_USAGE_REQUIRE_SIGNATURE?: string;
    /** Keys the daily digest of a signup's IP address. Without it the digest is a plain
     *  hash, which is reversible for IPv4 — set it before open signup carries real load. */
    IP_HASH_SECRET?: string;
    /** Base64 of 32 random bytes. Wraps every secured asset's content key, so a database
     *  leak yields wrapped keys and no way to unwrap them. Absent means secured content is
     *  off: `/v1/keys/grant` answers 503 rather than serving a key from nothing. Treated as
     *  master-key version "1" when the versioned map below is not set. */
    MXB_ASSET_MASTER_KEY?: string;
    /** The general form for rotation: a JSON map of master-key version to base64-of-32-bytes,
     *  e.g. `{"1":"…","2":"…"}`. Both an old and a new key live here during a rotation. */
    MXB_ASSET_MASTER_KEYS?: string;
    /** Which master-key version new wraps use (a key in `MXB_ASSET_MASTER_KEYS`). Defaults to
     *  "1". Point it at a new version, deploy, then POST `/admin/keys/rewrap` to rotate. */
    MXB_ASSET_MASTER_KEY_VERSION?: string;
    /** The account assets made through `/admin/assets` are created under. Not a secret — a
     *  var in `wrangler.jsonc`. Empty means `/admin/assets` answers 503. */
    MXB_OWNER_ACCOUNT_ID?: string;
    /** mxbsecure.com's own key: opens `/admin/assets*` and nothing else under `/admin`. A
     *  secret. `ADMIN_KEY` still works on those routes too. */
    MXB_ASSETS_KEY?: string;
    /** The Steam accounts (SteamID64, comma or space separated) that may read the dashboards
     *  at mxbsecure.com/admin. Not a secret — a var in `wrangler.jsonc`, so granting admin is
     *  a reviewable diff. Unset means nobody is an admin; `ADMIN_KEY` still opens the rendered
     *  `/admin` pages on this host either way. */
    MXB_ADMIN_STEAM_IDS?: string;
    /** Signs mxbsecure.com's Steam sign-in state and session cookies. A secret; rotating it
     *  signs everyone out. Unset means the site has no sign-in. */
    MXB_WEB_SESSION_KEY?: string;
    /** Where sign-in sends the browser back to. Defaults to https://mxbsecure.com. */
    MXB_SITE_ORIGIN?: string;
    /** "1" lets a local build of the site (localhost:5173, 127.0.0.1:5173) call the site's routes
     *  and land sign-in there. For `.dev.vars` only — never set it in production. */
    MXB_ALLOW_DEV_ORIGINS?: string;
    /** "open" lets any Steam account on mxbsecure.com start locking and selling. Unset means
     *  only accounts that are already creators can; nobody loses creator standing either way. */
    /** New assets a creator may make a day. 10 when unset; the owner account has no ceiling. */
    MXB_ASSETS_PER_DAY?: string;
    /** Rate limit on `/v1/web/steam/login` and `/return`, per client address (`ratelimits` in
     *  `wrangler.jsonc`). Optional so tests and a bare `wrangler dev` run without it. */
    SIGNIN_LIMITER?: RateLimit;
    /** Rate limit on `/v1/keys/grant`, per account. Optional for the same reason. */
    KEY_GRANT_LIMITER?: RateLimit;
    /** How the shop's catalogue dump is authenticated: `header:<name>`, `basic:<user>`,
     *  `bearer` or `query:<name>`. With `SHOP_CATALOG_KEY`, lets the track catalogue find sold
     *  tracks and their prices. Unset means mxb-mods.com only. */
    SHOP_CATALOG_AUTH?: string;
    /** The shop catalogue's key. A secret. */
    SHOP_CATALOG_KEY?: string;
  }
}

export {};
