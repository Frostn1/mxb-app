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
    /** R2 S3 credentials for presigning upload part URLs (`uploads.ts`): an R2 API token with
     *  Object Read & Write on `mxb-private` only, and the account's S3 endpoint
     *  (`https://<account>.r2.cloudflarestorage.com`, a secret because this repo is public and
     *  the account id is kept out of it). Without all three, `POST /v1/uploads` answers 503. */
    R2_ACCESS_KEY_ID?: string;
    R2_SECRET_ACCESS_KEY?: string;
    R2_S3_ENDPOINT?: string;
    /** Mod mirror policy (`mirrorpolicy.ts`), all optional with the defaults shown: days a track
     *  counts as on a live server after an app last asked about it (14), days a mirrored file
     *  is kept without a download (90), days a bike counts as ridden after its last loadout
     *  (180), and comma-separated bike names always (ALLOW) or never (DENY) mirrored. */
    /** "on" lets the mirror Worker walk mxb-mods.com (mirror/wrangler.jsonc). */
    MXB_MIRROR?: string;
    MXB_LIVE_TRACK_DAYS?: string;
    MXB_MIRROR_RETAIN_DAYS?: string;
    MXB_BIKE_ACTIVE_DAYS?: string;
    MXB_MIRROR_BIKES_ALLOW?: string;
    MXB_MIRROR_BIKES_DENY?: string;
    /** HMAC key for the mod mirror's signed links to locked (`.mxbsecure`) files
     *  (`mirrorapi.ts`). Any long random string. Without it those downloads answer 503;
     *  public files are unaffected. */
    MXB_ASSET_URL_KEY?: string;
    /** View-only and locked paints: `on` delivers and accepts them; anything else (unset) pulls them. */
    VIEW_ONLY_MODE?: string;
    /** Paint download authorisation: unset = enforced, `log` = serve but log what would be refused, `off` = unchecked. A rollback lever, not a setting. */
    PAINT_AUTHZ_MODE?: string;
    /** OVHcloud API credentials for user server deploy (`ovh.ts`, `hosting.ts`), from an OVH US
     *  account. The consumer key is limited to GET/POST /order/cart, GET/POST /order/cart/*,
     *  GET /me/order/*, GET /vps, GET /vps/* and POST /vps/*\/rebuild — no termination: boxes are
     *  cancelled by hand. Without all three, no box is ordered and a deploy that needs one answers
     *  "No capacity". */
    OVH_APPLICATION_KEY?: string;
    OVH_APPLICATION_SECRET?: string;
    OVH_CONSUMER_KEY?: string;
    /** Overrides the OVH API base (default https://api.us.ovhcloud.com/1.0). */
    OVH_ENDPOINT?: string;
    /** OVH subsidiary the cart is opened for. Default `US`. */
    MXB_HOST_OVH_SUBSIDIARY?: string;
    /** Fine-grained GitHub token with Actions: write on the install repo only, to start
     *  `box-install.yml` for a freshly delivered box. Without it boxes stop before install. */
    MXB_GH_DISPATCH_TOKEN?: string;
    /** Where `box-install.yml` lives. Default `Frostn1/mxbserver-releases`. */
    MXB_HOST_INSTALL_REPO?: string;
    /** Shared with the install runner (a GitHub Actions secret of the same name): authorises
     *  `/v1/hosting/*`, where the runner reports stages and enrolls a box's slot tokens. At
     *  least 32 characters; without it those routes refuse everything. */
    MXB_BOX_ENROLL_KEY?: string;
    /** Optional Discord-style webhook that hosting alerts are also posted to. */
    MXB_HOST_ALERT_WEBHOOK_URL?: string;
    /** Paid hosting (`billing.ts`), on Creste LLC's Stripe account: the restricted or secret API
     *  key, and the signing secret of the `/v1/stripe/webhook` endpoint. Billing is on only when
     *  both are set and so are the `STRIPE_PRICE_*` vars; otherwise deploy is invite-only and free. */
    STRIPE_SECRET_KEY?: string;
    STRIPE_WEBHOOK_SECRET?: string;
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
    /** Ed25519 private key (PKCS#8 DER, base64url) that signs the startup gate's verdicts, so
     *  an app can keep a block it was given and enforce it offline. A different pair from the
     *  plugin one: the apps hold its public half in `crates/core/src/appgate.rs`. Without it
     *  `/v1/app/gate` answers exactly as before, unsigned — never an error. Generate with
     *  `bun scripts/verdict-keypair.ts`. Also signs the key leases `POST /v1/keys/lease` hands
     *  out (`lease.ts`); without it that route answers 503 and the DLL, built without the public
     *  half, does not ask for one. */
    MXB_VERDICT_SIGNING_KEY?: string;
    /** Ed25519 private key (PKCS#8 DER, base64 or base64url) that signs the paint lock list
     *  mxbserver fetches (`GET /v1/servers/paint-locks`, `paintpolicy.ts`). Its own pair: a server
     *  operator pins the public half (`GET /v1/paint-locks/pubkey`, minisign format) in
     *  `[paints] pubkey_file`. Without it that route answers 503 and servers keep their last list. */
    MXB_PAINTLOCK_SIGNING_KEY?: string;
    /** Keys the device hash the apps report (`X-MXB-Device`) before it is stored, so a row in
     *  `device_links` is useless without it (`devices.ts`). Any long random string. Without it
     *  device linking is off: nothing is recorded and bans do not follow the machine — never an
     *  error. Rotating it stops new reports matching stored links; accounts already linked to each
     *  other stay linked until their rows are deleted (`DELETE FROM device_links` forgets all). */
    MXB_DEVICE_SALT?: string;
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
     *  and operate servers.mxbsecure.com. A secret, so the public repo carries no Steam IDs;
     *  set with `wrangler secret put ADMIN_STEAM_IDS`. Unset means nobody is an admin;
     *  `ADMIN_KEY` still opens the rendered `/admin` pages on this host either way. */
    ADMIN_STEAM_IDS?: string;
    /** Who may use the FBX -> EDF converter besides the admins (SteamID64s, comma or space
     *  separated). A secret for the same reason; unset means the admins alone. */
    CONVERTER_STEAM_IDS?: string;
    /** Public HTTPS hostnames the admin server manager may call, comma or space separated.
     * Empty is fail-closed. Exact names only; the browser never receives their URLs or tokens. */
    MXB_SERVER_AGENT_HOSTS?: string;
    /** Signs mxbsecure.com's Steam sign-in state and session cookies. A secret; rotating it
     *  signs everyone out. Unset means the site has no sign-in. */
    MXB_WEB_SESSION_KEY?: string;
    /** Where sign-in sends the browser back to. Defaults to https://mxbsecure.com. */
    MXB_SITE_ORIGIN?: string;
    /** "1" lets a local build of the site (localhost:5173, 127.0.0.1:5173) call the site's routes
     *  and land sign-in there. For `.dev.vars` only — never set it in production. */
    MXB_ALLOW_DEV_ORIGINS?: string;
    /** New assets a creator may make a day — the whole of what keeps open signup from being a
     *  key-minting service. 10 when unset; the owner account has no ceiling. Not a secret: a
     *  var in `wrangler.jsonc`, so changing the ceiling is a reviewable diff. */
    MXB_ASSETS_PER_DAY?: string;
    /** Require a Valve-confirmed Steam sign-in before the desktop apps will run. `"1"` turns it
     *  on for every install; unset (the default) leaves the apps open to invite/self-serve
     *  accounts as before. A var in `wrangler.jsonc`, so switching it is a reviewable diff — and
     *  a switch, not a build, because it locks out anyone without a Steam copy (see README). */
    MXB_REQUIRE_STEAM?: string;
    /** Whether mxbsecure.com takes new creators. `"open"` leaves the front door open to anyone
     *  signed in with Steam; anything else, **including unset**, closes it. Closed is the
     *  default on purpose: this is the door an unlocker walks back through with a fresh Steam
     *  account, and a deployment that was never told either way should not be holding it open.
     *  The admin page still adds creators by hand, which is what "closed" means — invite-only,
     *  not shut. A var in `wrangler.jsonc`, so opening it is a reviewable diff. */
    MXB_CREATOR_SIGNUP?: string;
    /** Rate limit on `/v1/web/steam/login` and `/return`, per client address (`ratelimits` in
     *  `wrangler.jsonc`). Optional so tests and a bare `wrangler dev` run without it. */
    SIGNIN_LIMITER?: RateLimit;
    /** Rate limit on `/v1/keys/grant`, per account. Optional for the same reason. */
    KEY_GRANT_LIMITER?: RateLimit;
    /** Rate limit on `/v1/track/generate`, per client address: the call is unauthenticated and
     *  spends our Anthropic budget. Optional so tests and a bare `wrangler dev` run without it. */
    TRACK_LIMITER?: RateLimit;
    /** Rate limit on paint sync v2 (`/v1/paintsync/*` writes), per account. Optional so tests
     *  and a bare `wrangler dev` run without it. */
    PAINTSYNC_LIMITER?: RateLimit;
    /** Rate limit on `/v1/rating/ingest`, per managed server (after token auth). Optional so
     *  tests and a bare `wrangler dev` run without it. */
    INGEST_LIMITER?: RateLimit;
    /** Rate limit on `/v1/friends/*`, per account and bucket (read or write). Optional so tests
     *  and a bare `wrangler dev` run without it. */
    FRIENDS_LIMITER?: RateLimit;
    /** Rate limit on `/v1/series/{slug}/register`, per client address: the one anonymous write
     *  on the series surface (`series.ts`). Optional so tests and a bare `wrangler dev` run
     *  without it. */
    REGISTER_LIMITER?: RateLimit;
    /** One paint-sync room per server key (`paintroom.ts`). Optional so tests run without it. */
    PAINT_ROOMS?: DurableObjectNamespace;
    /** How the shop's catalogue dump is authenticated: `header:<name>`, `basic:<user>`,
     *  `bearer` or `query:<name>`. With `SHOP_CATALOG_KEY`, lets the track catalogue find sold
     *  tracks and their prices. Unset means mxb-mods.com only. */
    SHOP_CATALOG_AUTH?: string;
    /** The shop catalogue's key. A secret. */
    SHOP_CATALOG_KEY?: string;
  }
}

export {};
