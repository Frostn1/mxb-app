# mxb-control-plane

Cloudflare Worker + D1 + R2 holding the accounts, the server registry, and — the point of
the whole thing — **what each rider is wearing**.

## Why this exists

MX Bikes transmits no custom content. A remote rider renders using whatever local file
matches the name they picked; miss it and you see the default bike and gear. The game can't
tell us which paint that is either — its plugin API exposes rider names, bikes and lap data,
and no paint field at all.

So the loop has to be closed outside the game: each player's app reports its loadout here,
and every other app on the server reads it back and fetches what it's missing. Two
consequences fall out of that, and they're baked into the schema:

- **Every rider needs the app**, not just the server owner.
- **Paints are content-addressed by SHA-256 and pinned to a canonical filename.** Matching
  is by name, so the sync is worthless if two riders hold the same bytes under different
  names — or different bytes under the same one.

## Endpoints

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/health` | — | Liveness |
| GET | `/v1/agent.exe` | — | The agent binary. Unauthenticated by necessity: a booting instance fetches it before it holds any credential. |
| POST | `/v1/enroll` | invite code | Trade an invite for an account and a bearer token |
| GET | `/v1/servers` | — | Server registry. Public: it is the app's join picker, and the people who most need it are the ones with no account yet. `agent_url` is not returned. |
| POST | `/v1/servers/:id/hello` | agent token | A provisioned box announcing that it is up. Its address is taken from `cf-connecting-ip`, never from the body, so a box cannot register somebody else's. |
| GET | `/v1/me` | bearer | Account, and a per-bike summary of what is stored for it |
| PUT | `/v1/me/guid` | bearer | Claim a GUID. First-come, and refused if that GUID is banned. |
| PUT | `/v1/loadout` | bearer | Replace **one bike's** loadout. Kept for clients older than per-bike storage. |
| PUT | `/v1/loadouts` | bearer | Replace the whole look, every bike at once. Returns `missing` — the blobs still to upload. |
| GET | `/v1/roster?server=<id>` | bearer | Riders and their paints, for the sync. De-duplicated by destination. |
| POST | `/v1/servers` | bearer | Publish a server you run. Five per account, one per address. |
| DELETE | `/v1/servers/:id` | bearer + owner | Remove it, terminating the instance if we launched it |
| GET | `/v1/servers/mine` | bearer | Your own servers, **with their agent tokens** — the only way to drive a box that has no console |
| GET | `/v1/fleet` | bearer | What is running. The count is everyone's (it is what the cap measures); the instance list is only yours. |
| POST | `/v1/provision` | bearer | Launch a server. Capped, and reaped when idle. |
| PUT/GET | `/v1/paints/:sha256` | bearer | Content-addressed paint blobs |
| POST | `/v1/bmac/webhook` | HMAC signature | Buy Me a Coffee announcing a supporter. Posted on to Discord. |
| POST | `/v1/usage` | — | Anonymous usage counters from an install. Unauthenticated because most people who run the app never claim an invite; bounded by body size, event count and a per-address daily cap. |
| GET | `/v1/usage/stats` | `ADMIN_KEY` | The same numbers as JSON, for anything that scripts them |
| POST | `/v1/master-status` | — | One install saying whether it could reach MX Bikes' own master server. Unauthenticated for the same reason as `/v1/usage`; one row per install per minute. |
| GET | `/v1/status` | — | Is the master answering? Public, CORS-open and cacheable — it is what mxbsecure.com/status renders and what a Discord bot answering `!timeout` reads. |
| POST | `/v1/roster` | — | Addresses an app saw in the game's own master list. Held back until distinct networks agree — see below; without that this would be a reflection amplifier. |
| GET | `/v1/roster` | — | The shared server book. Public and cacheable; the app seeds its own address book from it. |
| POST | `/v1/roster/mine` | bearer (invited) | A server's own operator adding it, which needs no corroborating: the account is the corroboration. |
| GET | `/v1/web/me` | Steam sign-in | Who is signed in on mxbsecure.com, whether they are a creator, and what is left of today's lock ceiling. Never cached. |
| POST | `/v1/web/creator` | Steam sign-in | Signing up as a creator, which is what opens `/admin/assets*`. Anyone signed in may; `MXB_ASSETS_PER_DAY` is what bounds them afterwards. |
| GET | `/v1/web/lockweb/*` | Steam sign-in | The WebAssembly locker. It cannot live on the static site, which serves everything it holds to everybody. Any signed-in rider gets it: the GUID lock is for all of them. |
| GET/POST | `/v1/web/admin/*` | Steam sign-in + `MXB_ADMIN_STEAM_IDS` | The dashboards at mxbsecure.com/admin — usage, diagnostics, paint sync, plugin keys, creators, bans |
| GET | `/v1/plugins` | — | The paid-plugin catalogue. Public: what is on offer is not a secret. |
| GET | `/v1/me/plugins` | bearer | What this account holds, each with a freshly signed license |
| POST | `/v1/plugins/redeem` | bearer | Trade a key for months on a license |
| GET | `/v1/plugins/:id/bundle` | bearer + license | The build itself, streamed rather than redirected to |

Enrollment by invite code stands in for Steam sign-in until there's an API key. `accounts`
already carries a nullable `steam_id`, so adding Steam is a backfill rather than a rewrite
of every account's identity.

### Why loadouts are per bike

A `profile.ini` holds a column per bike the rider has ever sat on, and which one they take
out is decided in the game — nothing tells us in advance. Storing one loadout per account
meant publishing a second bike deleted the first, so a rider appeared correctly on whichever
bike the app last touched and in default livery on every other. `loadout_paints` is therefore
keyed `(account_id, bike_id, slot)`, and the app publishes all of them together.

### How a provisioned server becomes joinable

Its public IP exists only in EC2's view, assigned while the instance boots — long after the
`servers` row was written — and its agent token exists only in that row and on the box. So the
box says so itself: the bootstrap reads its own address from IMDSv2, waits for the agent's
`/health`, and calls `POST /v1/servers/:id/hello`. That one call fills in `address` and
`agent_url` and flips `published`, which is what puts the server in everyone's join picker.
Its owner then gets the agent token from `/v1/servers/mine`, which is what makes Start, Stop
and Set track work on a machine nobody has a console for.

### Donations in Discord

`supporters.json` credits the people who bought a coffee, but nothing said when somebody had.
Buy Me a Coffee posts an event to `/v1/bmac/webhook`, which turns it into an embed in the
announcements channel — which is also the prompt to add them to `supporters.json`.

The route takes no bearer token, because BMAC has no account here. Its credential is the
`x-signature-sha256` header: HMAC-SHA256 over the **raw** body, so nothing may parse the body
before it is verified. Two secrets, neither in the repository:

```sh
bunx wrangler secret put BMAC_WEBHOOK_SECRET          # shown by BMAC when the webhook is made
bunx wrangler secret put DISCORD_DONATION_WEBHOOK_URL # the channel webhook — a credential itself
```

Without them the route answers 503, the same way provisioning does without its AWS key.

Only money-in events are announced (`donation.created`, `membership.started`,
`recurring_donation.started`, `extra_purchase.created`, `commission_order.created`,
`wishlist_payment.created`). Refunds, cancellations, pauses and anything unrecognised get a
200 and no post — a public channel is the wrong place to narrate a withdrawal, and a non-2xx
would only make BMAC retry an event we mean to drop.

The embed carries the supporter's name and their note, and nothing else. No amount, no coffee
count, and `supporter_email` is never read out of the payload at all. The note is escaped,
clipped to 400 characters and posted with `allowed_mentions: {parse: []}`, so somebody else's
words can't restyle the embed or ping the server. `bmac_events` records what has been
announced: BMAC retries a delivery up to four more times, and a reply it never received looks
exactly like a failure, so without that table one coffee arrives five times.

### Plugin keys and licenses

A paid plugin is bought in months. A **key** is a one-shot code that adds its months to an
account's **license**, and the app runs the plugin on a short-lived signed statement it can
check with no network — so a license is honoured for up to seven days with nobody to ask.

Both halves are revocable from mxbsecure.com/admin/plugins, which is also where keys are minted:

- **Revoking a key** withdraws a code that has not been spent. Redeeming it then answers
  "revoked" rather than "already used", because those are different next steps for whoever
  is holding it.
- **Revoking a license** ends access without waiting for the month to run out. It takes
  effect at the app's next check — within the grace window, not within the minute, which is
  the cost of the plugin working on a plane. `expires_at` is left alone, so lifting the
  revocation gives back the months that were paid for.

Months never land on a revoked license: redeeming a key against one is refused rather than
spending the code, and granting is refused rather than silently changing nothing.

`Grant months` hands an account a license with no key in between — for a tester, or for
putting someone right. `scripts/mint-plugin-key.ts` still prints SQL for a machine that
cannot reach the page.

### Is MX Bikes down, or is it you?

MX Bikes answers a dead master server with `connection timeout` and nothing else — the
identical string it prints for a firewall rule, a broken DNS server, a captive portal or a
router that wants power-cycling. So the commonest failure in the game is the one failure it
gives a player no way at all to place. It reaches the Discord as several people each debugging
a machine that is working perfectly, and by the time anyone works out the master was down for
ten minutes it is already back.

**This service cannot check for itself.** The master speaks its own protocol over UDP; a Worker
has no datagram socket, and the protocol is not in the public tree to put in one. A TCP probe
of the port would answer a different question, and answer it wrong.

So the check is the apps. Every MXB App already talks to the master whenever somebody opens the
Servers tab, and each one `POST`s whether its own fetch worked — an install id, worked-or-didn't,
and one word from `PROBE_REASONS` saying why not. That is the whole payload: no address, no
rider name, no server, no path. It turns out to be a *better* signal than a probe of our own
rather than a substitute for one, because "is it up from one Cloudflare colo" was never the
question anybody was asking. "Are the other twenty people who tried in the last ten minutes
also failing" is.

`GET /v1/status` folds the last ten minutes into one answer, each install counted once by its
**most recent** minute. Latest-wins matters: an "ever succeeded in the window" rule reads `up`
through the first ten minutes of every outage, because each affected install was working right
up until the master stopped — which is exactly the window people are in the channel asking
about. Below `MIN_INSTALLS` the answer is `unknown` rather than a guess; a status page that
guesses in the quiet hours is one nobody believes in the loud ones.

Rows live in `master_probes`, one per install per minute (so one person hammering Refresh counts
once, not twenty times), and are swept after an hour on the same cron as everything else.
Reporting rides on the app's anonymous-stats setting; reading does not, because the reason to
withhold a report is privacy and the reason to read the answer is that your game is broken.

### The shared server book

MXB App already survives a dead master server, and the mechanism matters because this is only
its missing half. The master is the sole source of **discovery** — the only thing that can tell
you a server exists — but it is the source of nothing else: a server answers `GETINFO` to
whoever asks, with no account, no ticket and no challenge, and that reply carries the name, the
riders, the seats and the whole event blob. So the app keeps a book of every address it has been
told about and, when the master won't answer, rebuilds the entire list by asking the servers
themselves.

That works, and it works for the wrong people. The book is per-install and starts empty, so it is
worth nothing to a fresh install and nothing to anyone who had not opened the Servers tab before
the outage began — which is the population an outage lands on hardest. `/v1/roster` pools it, so
the fallback is in place before the outage rather than after it.

**Addresses, and nothing else.** No names, no locations, no operator free text. `GETINFO` already
carries all of it, so a stored copy would only ever be staler — and an unauthenticated endpoint
that takes free text from anonymous clients and serves it to every install is a content-injection
channel this does not need.

**Why an address has to be corroborated.** This list tells thousands of apps where to send a
datagram, so an endpoint that served whatever it was handed would be a reflection amplifier with
a public API: one POST naming a victim's `host:port`, and every MXB App probes them on the next
outage. Two things stop that. `isPublicGameAddress` refuses loopback, private space, carrier NAT,
link-local (where cloud metadata lives) and multicast before anything is stored; and an address is
only *served* once `MIN_REPORTERS` distinct reporters have independently seen it in the game's own
master list **on the same day**. A reporter is the day-salted digest of the caller's address that
open signup already computes — never an install id, which one machine can mint at will — and the
same-day rule is forced by that salt: the same network hashes differently tomorrow, so counting
across days would read one persistent reporter as several and hand the injection straight back.

Storage is `server_roster` (one row per address, `corroborated_at` sticky once earned) and
`server_sightings` (evidence, swept the next day). A report takes the cheap path for every address
already corroborated — a `last_seen` bump and nothing else — which in the steady state is the
whole list, so contributing 300 servers every few minutes stays a couple of statements rather than
six hundred. Servers in our own registry are folded into the answer, so a caller does not have to
know we keep two lists.

### Usage counters

How many people run the app, and which parts they open — the question release downloads and
the accounts table both fail to answer.

The key is an **install id**: a random UUID the app mints for itself and keeps in its own
config, tied to no account, no rider and no machine. Reports carry that, a version, an OS, a
title, a session count, minutes open, and counters for names shaped `area.thing`. That shape
is the privacy property, not a style rule — a path, a rider name or an address cannot survive
`isEventName`, so a careless call site counts nothing instead of sending one. Addresses are
hashed for the day into the same `device_claims` counter open signup uses, and never stored.

Storage is two rollup tables (`usage_daily`, `usage_events`), a row per install per day and a
row per install per event per day. `install_id` stays in the events key so a feature's *reach*
(`COUNT(DISTINCT install_id)`) can be read apart from its *volume* (`SUM(count)`) — "nobody
opens the track studio" and "one person lives in it" are different answers. Rows older than
400 days are swept on the same cron as the idle servers.

Read them at mxbsecure.com/admin, signed in with a Steam account listed in
`MXB_ADMIN_STEAM_IDS` (`wrangler.jsonc`). Scripts use `GET /v1/usage/stats` with `ADMIN_KEY`:

```sh
bunx wrangler secret put ADMIN_KEY   # without it /v1/usage/stats answers 503, not 401
```

The app's side is `crates/core/src/usage.rs`, shared by all three apps. It is off in debug
builds unless `MXB_ANALYTICS_DEV=1`, off for a run with `MXB_NO_ANALYTICS=1`, and off for good
from the switch in Settings → General. The names it may send are a closed list there
(`KNOWN_EVENTS`), mirrored by the one here; `usage.test.ts` reads the Rust file and fails if
the two drift.

#### What holds the numbers up

The endpoint cannot authenticate — a token for every install would itself be an identifier —
so what keeps a figure worth deciding from is a stack of bounds rather than a credential:

- **`application/json` is required.** Without it a report is a CORS *simple request*, which
  means any web page can have its visitors post one from their own address — and the
  per-address cap buys nothing when every visitor brings a fresh address. Insisting on a type
  that needs a preflight, on a route that answers no CORS headers, is what closes that.
- **Row ceilings.** Reports accumulate onto a `(install, app, day)` row, and nothing used to
  bound the total: reports that each looked honest could put thousands of days of wall clock
  inside one day. A row now stops at `MAX_DAY_MINUTES` / `MAX_DAY_SESSIONS`.
- **A build signature**, where a deployment turns it on. Optional and **off by default**, and
  it must stay off until signed builds are the ones in the field — switching early throws
  everybody's numbers away silently.

  Rolling it out is three steps, **in this order**:

  ```sh
  # 1. The same value on both sides. Set the repo secret MXB_USAGE_KEY in mxb-app first —
  #    the release workflows already pass it to the builds.
  bunx wrangler secret put USAGE_SIGNING_KEY
  # 2. Tag a release of each app, and wait for signed builds to actually be out there.
  # 3. Only then: "MXB_USAGE_REQUIRE_SIGNATURE": "1" in wrangler.jsonc.
  ```

  The apps pick the key up at build time from `MXB_USAGE_KEY`, and `crates/core/build.rs`
  XOR-obfuscates it before baking it in so it is not a `strings` hit. A build without it —
  which is what a fork and the public repo produce — signs nothing and is accepted while the
  switch is off. The key still ships inside a downloadable binary, so this is **not**
  authentication: it raises the floor from "anyone with a terminal" to "someone willing to
  reverse a binary", which is the whole of the ambition.

None of that makes a field unforgeable — `version`, `os` and `game` are still whatever the
caller said, and they are what "can I stop shipping 0.8.x" and "is GP Bikes worth carrying"
are read off. Together the bounds make forging one cost more than the decision it would move.

### Banning a rider from mxbsecure

Every other revocation here is about *content*: a creator withdraws an asset, we take one down,
a removal takes the buyers' keys back. A ban is the other direction — somebody who unlocked
protected content and passed it around, refused across mxbsecure rather than asset by asset.
`src/bans.ts` is the whole of it, and `0038_guid_bans.sql` says why it is keyed the way it is.

**Keyed on the MX Bikes GUID.** It is the identity the game issues per install, it is what a
report about cracked content carries, and it is the one of the three we hold that is neither
free to mint (our account ids) nor replaceable for the price of a second purchase (a Steam ID).

**Resolved through every identity we can tie to it**, which is what makes it worth more than a
reinstall. `banFor` asks "is any identity this caller can be tied to a banned one", following
the GUID in front of it, every GUID the calling account holds *or has ever claimed*
(`guid_claims`), every account on the same Steam identity now or in the link log
(`steam_links`), and every GUID those accounts have used. So a second account on the same Steam
login, a fresh GUID claimed by a banned account, and a fresh Steam account on a banned install
all resolve back to the ban. `guid_claims` exists for exactly the reason `steam_links` does:
`accounts.guid` is a single mutable cell, and a ban that only read it would end at a rename.

**Where it lands.** At the gates, by position rather than per feature, so a product added later
inherits it:

| | What a ban does |
|---|---|
| `POST /v1/keys/grant`, `POST /v1/entitlements/check` | Refused for every asset, before entitlement is even looked up. Written to `entitlement_grants` as `deny`/`banned` like any other refusal. |
| `POST /v1/assets/status` | `revoked: true` for every secured file on the machine, so the app deletes the keys it already holds. This is the half that reaches content already unlocked — a `.mxbkey` opens offline forever, so a ban that only stopped the next grant would stop nothing. |
| `GET /v1/entitlements` | Empty, and says `banned` — a list nothing can open only misleads the app. |
| `/admin/assets*`, `POST /v1/web/creator`, `GET /v1/web/lockweb/*` | No locking, no selling, no signup, and no locker download: banned from making new protected content, not only from opening other people's. A creator API key belonging to a banned account is refused with it. |
| `GET /v1/me/plugins`, `GET /v1/plugins/:id/bundle`, `POST /v1/plugins/redeem` | The paid plugins are sold through mxbsecure too, so a ban reaches them: no signed license, no build, and a key is refused *before* it is read so it stays unspent and still worth something. |
| `PUT /v1/me/guid` | A banned GUID cannot be claimed. |

Voice, paint sync, presence and the server book are deliberately **untouched**. They are the
MXB App's, not mxbsecure's, and they are worthless unless the riders beside you can use them
too — banning somebody from the grid punishes the grid. A ban is about the locking system and
the content it protects.

**Reversible, and reviewable.** A ban carries a reason (shown to the rider), the evidence, and
the admin who applied it; lifting one is a timestamp, never a delete, so an upheld appeal stays
readable and the same stale report cannot re-ban off it. The six installs the deployment ships
banned arrived in the migration on purpose — this is the switch that refuses a paying customer,
so turning it on leaves a diff somebody can review and revert. Later ones go through
mxbsecure.com/admin/bans (`GET`/`POST /v1/web/admin/bans`), which records who pressed it.

## Security notes

- Tokens are shown **once** at enrollment and stored only as a SHA-256 digest. Lookup is by
  digest, so the comparison happens inside the index — there's no string compare of a secret
  to leak timing, and a database dump yields nothing presentable.
- An unknown invite code and an already-claimed one return the **same** 403. Distinguishing
  them turns the endpoint into an oracle for enumerating valid codes.
- Paint filenames are validated hard: this is the one input that becomes a *path on another
  player's disk*, so separators, `..`, control characters and Windows-illegal characters are
  rejected outright and the `.pnt` extension is required.

## Development

```sh
bun install
bunx wrangler types                                              # regenerate Env
bunx tsc --noEmit
bunx vitest run
for m in migrations/*.sql; do bunx wrangler d1 execute mxb-control-plane --local --file "$m"; done
bunx wrangler dev
```

### Pointing the app at it

`MXB_CONTROL_PLANE` redirects the desktop app to another control plane. **Debug builds only** —
a shipped binary always uses the baked-in URL, because responses from here become files written
into the game's mods folder and a redirectable target is a way to put content on a player's disk.

```sh
MXB_EXPERIMENTAL=1 MXB_CONTROL_PLANE=http://127.0.0.1:8799 bun run start-dev
```

The paint-sync round trip has a live test that needs both:

```sh
cd src-tauri
MXB_CONTROL_PLANE=http://127.0.0.1:8799 MXB_TEST_TOKEN=<token from /v1/enroll> \
  cargo test --locked live_sync -- --ignored --nocapture
```

Deploy with `bunx wrangler deploy`. Resources already provisioned in the personal account:
D1 `mxb-control-plane` (WEUR) and R2 `mxb-paints` (WEUR).
