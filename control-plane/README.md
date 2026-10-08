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
| POST | `/v1/enroll` | invite code | Trade an invite for an account and a bearer token |
| GET | `/v1/servers` | — | Server registry. Public: it is the app's join picker, and the people who most need it are the ones with no account yet. `agent_url` is not returned. |
| GET | `/v1/me` | bearer | Account, and a per-bike summary of what is stored for it. Looks ordinary to a banned install on purpose — see below. |
| GET | `/v1/app/gate` | bearer | The desktop apps' startup gate. `{status:"ok"}` to run; `{status:"signin"}` when `MXB_REQUIRE_STEAM` is on and the account has no confirmed Steam link (the app shows a sign-in wall); `{status:"unsupported"}` for a banned install (a mundane untruth, never the word "ban"). With `MXB_VERDICT_SIGNING_KEY` set, each answer also carries `signed: {payload, sig}` — see **Signed verdicts, and the offline policy**. Reads the optional `X-MXB-Device` header — see **Device links** under **Banning a rider**. |
| POST | `/v1/keys/lease` | bearer | A signed 30-day lease for the caller's Valve-confirmed Steam account, which the DLL needs beside a `.mxbkey` before it unseals it. 403 `code: "blocked"` for a banned account, 409 with no Steam link, 503 without `MXB_VERDICT_SIGNING_KEY` — see **Key leases, and the 30-day offline window** under **Banning a rider**. |
| PUT | `/v1/me/guid` | bearer | Claim a GUID. Derived from the linked Steam identity and pinned (the client's value is ignored) for a Steam account; first-come for a non-Steam one; refused if banned. |
| PUT | `/v1/loadout` | bearer | Replace **one bike's** loadout. Kept for clients older than per-bike storage. |
| PUT | `/v1/loadouts` | bearer | Replace the whole look, every bike at once. Returns `missing` — the blobs still to upload. |
| GET | `/v1/roster?server=<id>` | bearer | Riders and their paints, for the sync. De-duplicated by destination. |
| POST | `/v1/paintsync/join` | bearer | Paint sync v2: "I'm on this server (address and/or name) wearing this look." Returns the caller's hashes nobody holds (upload only those) and the other riders with theirs, and notifies the room. Also the heartbeat. |
| PUT | `/v1/paintsync/paints/<sha256>` | bearer | Upload one paint from the caller's own look. Kept 7 days under `live/`. |
| POST | `/v1/paintsync/leave` | bearer | Leave a server; the room is told. |
| GET | `/v1/paintsync/room?server=<key>` | bearer (WebSocket) | Pushes `joined`/`left` for the server; `{t:"ping"}` every 5 min keeps presence. |
| POST | `/v1/servers` | bearer | Publish a server you run. Five per account, one per address. |
| DELETE | `/v1/servers/:id` | bearer + owner | Remove it from the list |
| GET | `/v1/servers/mine` | bearer | The servers you registered |
| GET/POST/PUT/DELETE | `/v1/web/hosting/*` | Steam sign-in | servers.mxbsecure.com: claim an invite, deploy a server (type, region), its progress, settings, restart, delete, the MSM link — see **User server deploy** |
| GET/POST/DELETE | `/v1/web/admin/hosting*` | Steam sign-in + `ADMIN_STEAM_IDS` | Operators: spend, boxes, alerts, invites, tracks |
| POST, GET/PUT/POST | `/v1/hosted/claim`, `/v1/hosted/servers/:id[/settings\|/restart]` | one-time claim, then bearer | MSM driving one hosted server |
| GET/POST | `/v1/hosting/*` | `MXB_BOX_ENROLL_KEY` | The box install runner: stages, enroll, the track manifest |
| GET/POST | `/v1/web/hosting/billing[/portal\|/servers/:id/checkout]` | Steam sign-in | Paid hosting: prices, each server's billing status, the Stripe Customer Portal, a fresh Checkout — see **Paid hosting** |
| POST | `/v1/stripe/webhook` | Stripe-Signature HMAC | Stripe events for paid hosting |
| PUT/GET | `/v1/paints/:sha256` | bearer | Content-addressed paint blobs |
| POST | `/v1/bmac/webhook` | HMAC signature | Buy Me a Coffee announcing a supporter. Posted on to Discord. |
| POST | `/v1/usage` | — | Anonymous usage counters from an install. Unauthenticated because most people who run the app never claim an invite; bounded by body size, event count and a per-address daily cap. |
| GET | `/v1/usage/stats` | `ADMIN_KEY` | The same numbers as JSON, for anything that scripts them |
| GET | `/v1/survey/polls` | — | The questions the apps should be asking. Carries no install id and is the same for everybody, so it is cacheable. |
| POST | `/v1/survey` | — | One install's answer to one question. Unauthenticated for the same reason as `/v1/usage`; bounded by size, a closed answer vocabulary and a per-address daily cap. |
| GET | `/v1/survey/stats` | `ADMIN_KEY` | What people answered, as JSON |
| POST | `/v1/search-misses` | — | One Browse search that found nothing: the query text (trimmed, lowercased, 80 chars) and the game, counted per day. No install id, account, Steam id or IP is stored; same size and rate-limit posture as `/v1/survey`. Swept after 90 days. |
| GET | `/v1/search-misses/stats` | `ADMIN_KEY` | The most-missed queries over a window, as JSON |
| POST | `/v1/master-status` | — | One install saying whether it could reach MX Bikes' own master server. Unauthenticated for the same reason as `/v1/usage`; one row per install per minute. |
| GET | `/v1/status` | — | Is the master answering? Public, CORS-open and cacheable — it is what mxbsecure.com/status renders and what a Discord bot answering `!timeout` reads. |
| POST | `/v1/roster` | — | Addresses an app saw in the game's own master list. Held back until distinct networks agree — see below; without that this would be a reflection amplifier. |
| GET | `/v1/roster` | — | The shared server book. Public and cacheable; the app seeds its own address book from it. |
| POST | `/v1/roster/mine` | bearer (invited) | A server's own operator adding it, which needs no corroborating: the account is the corroboration. |
| GET | `/v1/web/me` | Steam sign-in | Who is signed in on mxbsecure.com, whether they are a creator, and what is left of today's lock ceiling. Never cached. |
| POST | `/v1/web/creator` | Steam sign-in | Signing up as a creator, which is what opens `/admin/assets*`. Shut unless `MXB_CREATOR_SIGNUP` is `"open"` — see **The front door, and why it is shut**. An existing creator still gets `already: true`, never a refusal. |
| GET | `/v1/web/lockweb/*` | Steam sign-in | The WebAssembly locker. It cannot live on the static site, which serves everything it holds to everybody. Any signed-in rider gets it: the GUID lock is for all of them. |
| GET/POST | `/v1/web/admin/*` | Steam sign-in + `ADMIN_STEAM_IDS` | The dashboards at mxbsecure.com/admin — usage, diagnostics, paint sync, creators, bans, series (racing.mxbsecure.com/admin) and servers (servers.mxbsecure.com) |
| GET | `/v1/plugins` | — | The plugin catalogue. Public. Every plugin is free. |
| GET | `/v1/me/plugins` | bearer | A freshly signed license for every plugin |
| GET | `/v1/plugins/:id/bundle` | bearer | The build itself, streamed rather than redirected to |
| GET | `/v1/rating/leaderboard?class=`, `/v1/rating/classes` | — | Rider rating by class, and the classes that have one. CORS-open for mxbsecure.com/leaderboard. |
| GET | `/v1/series`, `/v1/series/:slug` | — | Published series: standings (with each rider's rating, joined by GUID), rounds (done or dropped), the schedule, approved entries. Never a GUID. CORS-open, cached a minute. |
| PUT/DELETE | `/v1/series/:slug` | series token | MSM publishing or unpublishing a series. Rider rows carry the GUID (stored privately for the joins); any other GUID-, Steam-ID- or UUID-shaped string refuses the whole body. |
| POST | `/v1/series/:slug/register`, `/v1/web/series/:slug/register` | — / Steam sign-in | A rider asking to race. Anonymous entries are unverified until the operator links a GUID; signed-in ones take the GUID from the Steam ID. Per-address limiter, daily cap and honeypot on the anonymous one. |
| GET/POST | `/v1/series/:slug/registrations[/:id]` | series token | The operator's list and approve/reject, from MSM. |
| GET/POST | `/v1/web/admin/series` | Steam sign-in + admin | Reserve a slug and mint its publish token (shown once), rotate, unpublish, delete. |

Enrollment by invite code stands in for Steam sign-in until there's an API key. `accounts`
already carries a nullable `steam_id`, so adding Steam is a backfill rather than a rewrite
of every account's identity.

### Why loadouts are per bike

A `profile.ini` holds a column per bike the rider has ever sat on, and which one they take
out is decided in the game — nothing tells us in advance. Storing one loadout per account
meant publishing a second bike deleted the first, so a rider appeared correctly on whichever
bike the app last touched and in default livery on every other. `loadout_paints` is therefore
keyed `(account_id, bike_id, slot)`, and the app publishes all of them together.

### User server deploy

Invited Steam accounts deploy a server of their own on servers.mxbsecure.com: `mxbserver` or
Legacy, in US East, US West, EU West, EU East or Oceania. Each lands in a slot on an OVH VPS
(`src/hosting.ts`, `src/hostregions.ts`, `src/ovh.ts`). Boxes are ordered only when a deploy
finds no free slot in its pool and region, and only while `MXB_HOST_BOX_PRICE_USD` x (billed
boxes + 1) stays within `MXB_HOST_SPEND_CAP_USD`; otherwise the user sees "No capacity in
<region> right now." and operators get an alert. Nothing is ever cancelled here: an empty box
drains, then is flagged near its renewal for a person to cancel at OVH.

Secrets, all optional (without them no box is ordered or installed):

```sh
bunx wrangler secret put OVH_APPLICATION_KEY      # OVH US account application
bunx wrangler secret put OVH_APPLICATION_SECRET
bunx wrangler secret put OVH_CONSUMER_KEY         # limited to the routes in src/env.d.ts
bunx wrangler secret put MXB_GH_DISPATCH_TOKEN    # Actions: write on Frostn1/mxbserver-releases
bunx wrangler secret put MXB_BOX_ENROLL_KEY       # same value as that repo's MXB_BOX_ENROLL_KEY
bunx wrangler secret put MXB_HOST_ALERT_WEBHOOK_URL   # optional
```

#### Paid hosting

One Stripe subscription per hosted server, on Creste LLC's Stripe account (`src/billing.ts`):
$5/month for `mxbserver`, $8/month for Legacy. Invites still decide who may deploy. With billing
on, a deploy writes the server as pending and returns a Stripe Checkout URL; the server is placed
(slot, or a box ordered) only when the webhook says it is paid. A failed renewal or an ended
subscription gives 3 days' grace, then the server is deleted through the idle path (slot freed)
and the subscription cancelled. A paid server is not reclaimed for idling.

Billing is off, and deploy is free and invite-only as before, unless all four are set:

```sh
STRIPE_SECRET_KEY=sk_test_... bun scripts/stripe-prices.ts   # prints the two price ids
# put them in wrangler.jsonc as STRIPE_PRICE_MXBSERVER and STRIPE_PRICE_LEGACY
bunx wrangler secret put STRIPE_SECRET_KEY        # sk_live_... (or a restricted key)
bunx wrangler secret put STRIPE_WEBHOOK_SECRET    # whsec_... of the endpoint below
```

In the Stripe dashboard add the endpoint `https://api.mxbsecure.com/v1/stripe/webhook` with
`checkout.session.completed`, `checkout.session.async_payment_succeeded`,
`checkout.session.expired`, `invoice.paid`, `invoice.payment_failed`,
`customer.subscription.updated` and `customer.subscription.deleted`, and turn on the Customer
Portal (Settings, Billing, Customer portal).

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

Without them the route answers 503.

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

### Plugins

Every plugin is free, so there are no keys and no license rows (migration 0055 dropped
`plugin_keys` and `plugin_licenses`). Any signed-in account gets a signed license for each
plugin on every check-in, good for a year and to be refreshed within seven days. The app's
install and run checks (account, bundle hash, refresh) still run on that license. A banned
account is refused by the ban gate before it reaches these routes.

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
`ADMIN_STEAM_IDS` (a Worker secret). Scripts use `GET /v1/usage/stats` with `ADMIN_KEY`:

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

### Asking the player

The counters above say what people open. They have never said whether any of it is any good —
a feature with high reach is one people *find* — so the apps also ask, rarely, on a card in the
corner: **How's it going? Bad · Fine · Good**, and a follow-up ("what happened?") always after
the answers a question names, and otherwise on a weighted coin.

**The questions live in the database, not in the client.** `survey_polls` is a row per
question, written at mxbsecure.com/admin/survey and picked up by every install on its next
fetch. That is the whole point: "have you tried Race mode" is worth asking for three weeks, and
a question baked into a release needs one release to start asking, another to stop, and a month
in between for either to reach anybody. A poll carries its text as a locale map, so it can be
written in one language or six; the standing mood poll carries none at all, because the apps
draw and translate its three answers themselves.

An answer carries the same install id the counters use, the app, its version, the OS, the
title, a poll id and a choice id. Storage is `survey_answers`, one row per install per poll per
day, so a retry replaces rather than votes twice.

**The note is the one free-text field in this deployment**, and worth naming rather than
burying. The follow-up may offer a box the player types into; a poll that has no use for prose
sets `note = 0` and collects chips alone. What arrives is capped at 280 characters, scrubbed of
addresses, links and user-folder paths (`scrubNote` — best effort, not a promise about a
sentence somebody typed), cleared by the sweep after 120 days while the answer itself is kept
400, and deletable one at a time from the dashboard.

The app's side is `crates/core/src/survey.rs`, shared by all three. It is gated on the counters'
consent as well as its own switch — an answer carries the install id, so being asked cannot be
a way round having said no to being counted — leaves a fresh install alone for three days,
never asks inside the first five minutes of a run, shows at most one card a day across every
question, and stops asking for half a year after three dismissals in a row. `MXB_NO_SURVEY=1`
turns it off for a run.

### The GUID is the Steam identity, and cannot be spoofed

A rider's MX Bikes GUID is not a separate fact we collect and trust — for a Steam copy of the
game it *is* the Steam account, written differently: `FF` followed by the SteamID64 as sixteen
uppercase hex digits (`guidFromSteamId` in `steam.ts`). The game derives it that way, mxb-ranked
keys a rider page on it, and so do we.

That matters because the Steam half is the one we can prove. `accounts.steam_id` is only ever set
by the Valve OpenID round trip (`/v1/steam/return`, `verifyAssertion`) — no endpoint trusts a
client-posted Steam ID — so once an account is linked, its identity is Valve's word, not the
app's. From that we **derive** the GUID and **pin** it (`pinGuidFromSteam` in `steamlink.ts`):

- On every Steam link, the derived GUID is written to the account, and if any other row was
  holding it — a stale first-come claim, or a spoofer who grabbed the victim's GUID — that row is
  dispossessed in the same batch. Valve's word beats first-come.
- `PUT /v1/me/guid` from a Steam-linked account ignores whatever the app sent and stores the
  derived value. A Steam player's only valid GUID is the one their identity maps to, so this both
  auto-corrects an honest stale value and refuses a spoof, with the same answer.
- `0039_derive_guids.sql` backfills every already-linked account at deploy and clears the GUIDs
  that were only guesses or spoofs.

The app doesn't have to *observe* its own GUID any more either — it derives it from the signed-in
Steam account (`mxb_core::steamid::local_guid`) the moment it starts, rather than watching a
dedicated-server log the way it used to. Auto-found, and the same value the server will accept.

A non-Steam (Piboso) copy has no SteamID64 to derive from, so its GUID stays opaque and
first-come, corroborated by the sightings other installs report. That is the one identity a ban
still leans on the claim log for; a Steam identity is nailed down by Valve.

#### Requiring a Steam sign-in for the whole estate

`MXB_REQUIRE_STEAM` (a var in `wrangler.jsonc`, `"1"` to enable, off by default) turns the
startup gate into a hard wall: an account with no Valve-confirmed Steam link gets `signin`
instead of `ok`, and the app shows "Sign in with Steam" and will not run until the OpenID round
trip lands (`steam_link_start` → the browser → `/v1/steam/return`), which sets `steam_id` and
pins the derived GUID. A ban still wins over the requirement.

Turned on, every install becomes a proven identity — which is what makes the GUID and the ban
unspoofable for the whole estate rather than only for the accounts that happened to link. It is a
switch and not a build for one reason: it locks out anyone without a **Steam** copy of the game.
A Piboso owner has no Steam identity to confirm and cannot pass the wall, so enabling this is a
deliberate "Steam players only" decision the deployment makes and can reverse — not something
baked into a release.

### The front door, and why it is shut

Becoming a creator was one click for anyone with a Steam account: sign in, `POST
/v1/web/creator`, start locking. The argument for that was sound as far as it went — the invite
list gatekept a tool whose real protection is elsewhere, since every asset is tied to the
account that made it and `MXB_ASSETS_PER_DAY` caps what that account mints in a day.

It answers the wrong question for the people the ban list is about. A ban follows an install,
the Steam login behind it and every account either has held; what it cannot follow is a brand
new Steam account, which is a new identity by every measure we have. That is a deliberate
limit — but while the front door was a button, it was also the whole of the work required to
come back. A ceiling on what a fresh account can mint is not an answer to an account that
should not be minting anything.

So `MXB_CREATOR_SIGNUP` (`wrangler.jsonc`) decides, `"open"` and nothing else opens it, and
unset is closed — a deployment that was never told should not be holding the door open. Closed:

- `POST /v1/web/creator` answers 403 with `SIGNUP_CLOSED`, which is honest and names the way
  in, because mxbsecure.com is where somebody with real work to sell turns up;
- `GET /v1/web/me` carries `creatorSignup`, so the site draws the closed door rather than a
  button that fails;
- **existing creators are untouched.** They keep `creator_at`, keep locking, and a second POST
  still answers `already: true` rather than telling them the door is shut. Closing is not
  removing; removing is the creators page's own button.

New creators arrive through the creators page instead (`addCreator`, `creator_source = 'admin'`),
which is the invite list again, on purpose: it puts a person between a fresh Steam account and
the right to mint keys. Reopening is a one-line diff when the ban list stops being the reason.

### Banning a rider

Every other revocation here is about *content*: a creator withdraws an asset, we take one down,
a removal takes the buyers' keys back. A ban is the other direction — somebody who unlocked
protected content and passed it around, refused across everything we run. MXB App, Studio,
Coach, FrostMod and mxbsecure are one brand, so a ban is a ban from all of it, not from the
locking system alone. `src/bans.ts` is the whole of it, and `0038_guid_bans.sql` says why it is
keyed the way it is.

**Keyed on the MX Bikes GUID.** It is the identity the game issues per install, it is what a
report about cracked content carries, and it is the one of the three we hold that is neither
free to mint (our account ids) nor replaceable for the price of a second purchase (a Steam ID).

**Resolved through every identity we can tie to it**, which is what makes it worth more than a
reinstall. `banFor` asks "is any identity this caller can be tied to a banned one", following
the GUID in front of it, the GUID a Steam identity *derives to* (`guidFromSteamId` — for a Steam
copy the two are one value, so a banned Steam login needs no row in the database to be refused,
which is the website's caller: signed in with Steam and possibly with no MXB App account at
all), every GUID the calling account holds *or has ever claimed* (`guid_claims`), every account
on the same Steam identity now or in the link log (`steam_links`), every account seen on the same
machine (`device_links`, when device linking is on — see **Device links** below), and every GUID those
accounts have used. So a second account on the same Steam
login, a fresh GUID claimed by a banned account, a fresh Steam account on a banned install, and a
fresh token on a banned PC all resolve back to the ban. `guid_claims` exists for exactly the reason `steam_links` does:
`accounts.guid` is a single mutable cell, and a ban that only read it would end at a rename.

**Asked at three doors, never per feature**, so a product added later inherits it:

| Door | What it covers |
|---|---|
| `route`, straight after `authenticate` | Every bearer-token endpoint in the estate: voice, paint sync, presence, the queue, the server registry, provisioning, the paid plugins, the key grants. They are all below that line, so a route added below it is covered without anybody remembering to ask. |
| `authorize` in `assets.ts` | The creator surface, which arrives on a sign-in cookie or a creator API key: no locking, no selling, no new asset ids, no API keys. |
| `web.ts` | mxbsecure.com — the signed-in identity, the creator signup, and the locker download. |

A closed list (`bannedMayUse`) names the few endpoints that stay open to a banned account, and
each is there because refusing it outright would work against the ban:

- `GET /v1/app/gate` is the one that stops the apps opening. A banned install reaches it and is
  told `{status:"unsupported"}` with a plausible, false reason ("this copy couldn't be verified,
  reinstall"); the app then refuses to run. It is disguised on purpose — see **The app is lied
  to** below.
- `GET /v1/me` still answers, and still looks ordinary. The app is never told here that it is
  banned; the gate above turns it away instead, so `/v1/me` staying unremarkable is part of the
  disguise.
- `PUT /v1/diagnostics` still observes, and still answers `{ ok: true }` whatever it made of the
  report. Refusing it would blind us to the install we most want to watch.
- `POST /v1/steam/login` still links an identity, which is the plumbing an appeal is decided on.
- `POST /v1/assets/status` still answers, with `revoked: true` for every secured file on the
  machine — this is what makes the app delete the keys it already holds, and a blanket 403 there
  would read as "we don't know", which keeps them. It carries no ban flag: the per-asset
  `revoked` reads exactly like the creator having removed the buyer, which is the disguise. And
  a `.mxbkey` opens offline, so without this a ban would leave the banned install playing
  everything it had already unlocked until its key lease ran out (below).
- `POST /v1/keys/grant` and `POST /v1/entitlements/check` still answer, and still write the
  denial to `entitlement_grants` as `banned` — a banned install walking the catalogue is only
  visible if the "no"s are recorded. What the app *sees* is the disguised failure (the grant) or
  a plain `"unavailable"` (the check), never the word.

#### The app is lied to; the website is not

The website (`/v1/web/me`, the lock pages, the dashboard) tells a banned creator plainly that
they are banned and why, because mxbsecure.com is where an appeal starts. The desktop apps are
told the opposite — a verification/integrity failure — and it is deliberate. The app is not a
place to argue; it is a place a content thief is trying to keep using. "Banned" only tells them
to make another account, the honest message is the exact next-step coaching they would act on,
and the reinstall the disguise names cannot help them, because a ban follows the GUID, the Steam
login and the install, never the files. We always know it is a ban — the ledger, the admin page
and the internal `reason` all say so. The machine in front of the person does not. The app side
of the gate — the signed verdict that keeps a blocked install blocked even offline — lives in
`crates/core/src/appgate.rs`.

#### Signed verdicts, and the offline policy

The gate's plain answer is enough to act on in the moment and worth nothing afterwards: a block
written to disk on the strength of an unsigned reply is a file anybody can delete, and a stored
"ok" is a file anybody can write. So `GET /v1/app/gate` also returns the verdict as a signed
statement (`src/verdict.ts`):

```json
"signed": {
  "payload": "{\"v\":1,\"status\":\"unsupported\",\"account\":\"acc_…\",\"token\":\"<sha-256 of the bearer token>\",\"steamId\":null,\"guid\":null,\"issuedAt\":1800000000000}",
  "sig": "<Ed25519 over the payload's UTF-8 bytes, base64url>"
}
```

`payload` is the exact string signed. It names the status, the account it is about, a SHA-256 of
the token it was fetched with (`hashToken` — how a launch, which knows its token but not its
account id, tells that a kept verdict is about itself), the Steam ID and GUID the server tied to
that account, and when it was issued — never a reason. The binding is inside the signature, so a
kept block cannot be moved onto another account or off its own by editing the file. The apps
hold only the public half of the pair (`VERDICT_PUBLIC_KEY` in `crates/core/src/appgate.rs`) and
keep the last verdict they could verify in the folder all three share, so one app's block holds
in the others. The policy:

- **An install never told it is banned keeps working offline.** No network, a timeout or an
  unreadable answer is never a reason to refuse anybody — exactly as before.
- **An install given a signed block stays blocked offline.** A kept `unsupported` for the account
  the install is signed in as refuses the launch without the network. Only a *newer* signed `ok`
  or `signin` for the same account lifts it, so a replayed old "ok" cannot, and a block cannot be
  carried to another account. A blocked launch asks the gate once, briefly, before refusing —
  that is how a lifted ban gets back in.

A signed block is kept only in that shared file; the older per-app `gate.lock` is still read, and
still written for a block that arrives unsigned. The apps also re-ask every half hour while open,
and at once when any call comes back 403 with `code: "blocked"`. Every app-facing refusal for a
ban carries that code beside the unchanged disguised message (and `POST /v1/entitlements/check`
beside its plain `"unavailable"`): the message is for the person, the code is for the app, so it can tell a
block from "not entitled" without matching on prose.

Signing is optional. Without `MXB_VERDICT_SIGNING_KEY` the gate answers exactly as it did, with
no `signed` field, and a signing failure is logged and answered unsigned — never an error. To
turn it on:

```sh
bun scripts/verdict-keypair.ts                   # prints both halves; stores neither
bunx wrangler secret put MXB_VERDICT_SIGNING_KEY  # paste the private half (PKCS#8, base64url)
```

and put the printed public half in `VERDICT_PUBLIC_KEY` in `crates/core/src/appgate.rs` for the
next app release. Never commit the private half. It is a pair of its own, not the plugin one.
Rotating it is a release: a build treats a verdict signed by any other key as unsigned — it still
acts on it, it just cannot keep it — and a block kept under the old key stays until a build with
the old key sees a newer lift, or the app is updated.

#### Key leases, and the 30-day offline window

A `.mxbkey` is sealed to the buyer's Steam ID and PC and opens with no server, so the status poll
above only reaches it while the app is online and left alone. An install kept offline used to keep
everything it had unlocked, forever. So the DLL now unseals a key only beside a **lease**: a small
statement signed with the verdict key (`src/lease.ts`), from `POST /v1/keys/lease`:

```json
{
  "lease": {
    "payload": "{\"v\":1,\"purpose\":\"mxbsecure-lease\",\"steamId\":\"7656…\",\"issuedAt\":1800000000000,\"expiresAt\":1802592000000}",
    "sig": "<Ed25519 over the payload's UTF-8 bytes, base64url>"
  },
  "expiresAt": 1802592000000
}
```

It names the caller's Valve-confirmed Steam ID and runs 30 days. One lease covers every key that
Steam account holds on the install. The app keeps it beside the DLL's manifest and renews it
silently whenever it is online and fewer than 25 days are left; the DLL checks the signature, the
purpose, that the Steam ID is the one it reads live, and that it has not run out. What that means
for a buyer: **play offline for up to 30 days between check-ins; the app renews silently whenever
it is online.**

The route is behind the ban gate and not in `bannedMayUse`, so a banned account is refused with
`code: "blocked"` — and the app deletes its lease (and runs the revocation sweep) when it hears
that. A ban therefore reaches keys already on disk within 30 days even on a PC that never comes
back online, and at once on one that does. Without a Steam link the answer is 409
`no Steam account linked`, the grant's own words.

Signed with the same key as the gate verdicts, so there is one secret and one public half. The two
cannot be swapped: a lease carries `purpose: "mxbsecure-lease"` and no `status` or `account`, and
each verifier refuses the other's shape.

Without `MXB_VERDICT_SIGNING_KEY` the route answers 503 `leases not configured`. That is safe
because the DLL only asks for a lease when it was built with the public half
(`LEASE_PUBLIC_KEY` in the private repo's `secure/src/lease.rs`); a DLL built without it unseals
exactly as before. Roll it out in that order: set the secret, ship an app that fetches leases, and
only then ship a DLL with the public key in it.

#### Device links

A fresh token, or a fresh Steam account, on the same banned PC used to be a new identity. With
`MXB_DEVICE_SALT` set it is not: the apps report the machine they run on in an `X-MXB-Device`
header on `GET /v1/app/gate` and on `POST /v1/account`, and the worker ties the account to it in
`device_links` (`src/devices.ts`, `0043_device_links.sql`). `banFor` then widens through it one
hop, beside the Steam hop: every account seen on a device the caller has been seen on.

The machine identifier itself never leaves the PC. The app sends SHA-256 of a domain tag and the
OS's machine id (`crates/core/src/device.rs`: `MachineGuid` on Windows, `IOPlatformUUID` on
macOS, `/etc/machine-id` on Linux); the worker keys that again with `HMAC-SHA256(MXB_DEVICE_SALT,
…)` and stores only the result, so a copy of the table is useless without the secret. A report
that is not 64 hex characters is ignored. Erasure (`DELETE /v1/me`) deletes an account's device
links outright, a banned account's included — `docs/privacy.md` says so to the people it is about.

Off unless configured: without the secret nothing is recorded, the resolution does not read the
table, and no request fails either way. To turn it on:

```sh
bunx wrangler d1 migrations apply mxb-control-plane --remote   # 0043_device_links.sql first
bunx wrangler secret put MXB_DEVICE_SALT                        # any long random string
```

Rotating the secret stops new reports matching the stored hashes, so a machine is linked afresh
the next time each install opens. It does not unlink accounts already linked to each other: those
rows still share a hash. To forget every link, delete them (`DELETE FROM device_links`) with the
rotation. Apply `0043` before deploying this worker at all — erasure deletes from the table
whether or not the secret is set, so that a link written while it was on is never left behind.

What a ban cannot reach is what carries no identity: the anonymous usage counters, the
master-server probe, the shared server book, a live share code, and track generation (capped by
its own shape rather than by who is asking). There is nothing there to match a ban against.

**Reversible, and reviewable.** A ban carries a reason (shown to the rider), the evidence, and
the admin who applied it; lifting one is a timestamp, never a delete, so an upheld appeal stays
readable and the same stale report cannot re-ban off it. Every ban goes through
mxbsecure.com/admin/bans (`GET`/`POST /v1/web/admin/bans`), which records who pressed it.

**No ban GUID in this repository, ever.** It is public, and a GUID beside a ban reason is an
accusation against a person that no later commit can take back. Migrations `0038` and `0040` once
seeded twelve bans; their data was removed on 2026-09-25 (the rows they inserted stay in the
database), and the ban records, with who each install was reported to be, are kept privately.
`bun run check:no-ban-guids` fails CI on any MX Bikes GUID not on `scripts/guid-allowlist.txt`,
which holds only synthetic test values and fixed samples.
with the rest of the evidence.

### The mod catalogue

Mods people can search and download from mxbsecure.com/mods and the MXB App. Two sources feed
one set of tables (`migrations/0056_mod_catalog.sql`):

- **Uploads** (`uploads.ts`): a signed-in, Steam-confirmed account opens a session, the app
  writes the file straight to R2 through presigned multipart part URLs (resumable, up to 2 GiB),
  and completes it. The bytes sit in `mxb-private/quarantine/` until the queue has checked the
  declared size and SHA-256 and the archive (`modscan.ts`: no programs, scripts or libraries by
  name or by content, no traversal, nested `.pkz`/`.zip` looked into one level), then move to
  `mxb-assets/<type>/<sha256>`. A new upload of the same mod is a new version of it. Quotas per
  account: 3 open sessions, 20 uploads and 10 GiB a day, 25 GiB stored.
- **The mirror** (`mirror.ts`, `mirrorfetch.ts`, `mirrorhosts.ts`): a cron every 10 minutes
  walks mxb-mods.com's REST listing from a cursor, reads each new or changed post's page for its
  download links (the app's `mods/mxb.rs` parsing), and queues each link. The queue consumer
  resolves it the way the app's `install.rs` does (MediaFire, Google Drive including the
  virus-scan form, MEGA with in-Worker AES-CTR decryption, Dropbox, OneDrive, Pixeldrain), lists
  folder shares into one row per file, and streams each file into R2 by SHA-256. Polite by
  construction: named user agent, robots.txt honoured, 3 s between requests, a two-hour
  cooldown the moment the site refuses one. `MXB_MIRROR` off stops it.
- **Only what is used** (`mirrorpolicy.ts`): the whole catalogue is indexed, but a file is copied
  into R2 only when someone downloads it (the first download is redirected to the original and
  queues the copy) or when it is a track on a live server (`track_catalog.requested_at`, stamped
  by the Servers tab's lookups, within `MXB_LIVE_TRACK_DAYS`). Liveries and bikes for bike models
  no rider has published a loadout for within `MXB_BIKE_ACTIVE_DAYS` are never copied;
  `MXB_MIRROR_BIKES_ALLOW` / `_DENY` override. A copy nobody downloads for
  `MXB_MIRROR_RETAIN_DAYS` (90) is evicted, unless a live server uses it.

Two Workers share this code. `mxb-control-plane` answers people (search, mod pages, download
redirects, upload sessions and completion, reports, moderation) and only *queues* slow work.
`mxb-mirror` (`mirror/index.ts`, `mirror/wrangler.jsonc`) does it: the mxb-mods cron, the
`mxb-mirror` queue (downloads, MEGA decryption, Drive's confirm page, upload scanning and
promotion), eviction and the dead-letter queue. It has no route and holds no secrets: it reads
R2 through its bindings, and the S3 credentials for presigning stay on the control plane.
Its tests are in `mirror/test/`; `bunx vitest run` runs both suites.

Search is FTS5 over title, author, bike, categories and description, bm25-ranked. Public files
are served by `cdn.mxbsecure.com`, the custom domain on `mxb-assets`; `.mxbsecure` locked
content stays in `mxb-private` behind five-minute signed links. Reports come in on
`POST /v1/assets/<id>/report`; admins hide, unhide or remove from mxbsecure.com/mods/moderate
(`/v1/web/mods/*`); an owner can edit or delete their own.

Provisioning, once, before the first deploy that carries the bindings (the deploy fails
without them):

```sh
bunx wrangler r2 bucket create mxb-assets
bunx wrangler r2 bucket create mxb-private
bunx wrangler r2 bucket domain add mxb-assets --domain cdn.mxbsecure.com --zone-id <mxbsecure.com zone id>
bunx wrangler queues create mxb-mirror
bunx wrangler queues create mxb-mirror-dlq
# CORS on the private bucket, so the app's PUTs to presigned URLs return their ETag:
bunx wrangler r2 bucket cors set mxb-private --file r2-cors.json
# An R2 API token with Object Read & Write on mxb-private only, then:
bunx wrangler secret put R2_ACCESS_KEY_ID
bunx wrangler secret put R2_SECRET_ACCESS_KEY
bunx wrangler secret put R2_S3_ENDPOINT        # https://<account id>.r2.cloudflarestorage.com
bunx wrangler secret put MXB_ASSET_URL_KEY     # any long random string
```

The CI deploy token also needs **Queues Edit** and **Workers R2 Storage Edit** next to Workers
Scripts and D1. Files over 2 GiB, SharePoint folders and personal OneDrive links are marked
`runner` rather than failed, for a separate runner to pick up.

#### The fetcher

mxb-mods.com and MediaFire answer Cloudflare Workers 403 and a normal machine 200. The hosts
in `MIRROR_FETCHER_HOSTS` (a var in both wrangler files) are fetched by `tools/mxb-fetcher` on
our own Linux box instead, as is any host that refuses the Worker a file. The box opens no
ports: it leases jobs from `/v1/mirror/fetcher/*` (`src/mirrorfetcher.ts`), hands page HTML
back to be parsed exactly as the Worker parses it, and PUTs files and pictures straight into
`mxb-assets` through presigned URLs. With mxb-mods.com routed there, discovery goes too: the
category tree, listing walk and id sweep become one `list` job at a time (a round every ten
minutes, as the cron ran them), whose JSON is applied by the same steps, and the Worker sends
the site nothing.

```sh
bunx wrangler secret put MIRROR_FETCHER_TOKEN  # 32+ random characters, the same on the box
```

The upload URLs are presigned with the `R2_*` key above, so that token needs Object Read &
Write on `mxb-assets` as well as `mxb-private`. Empty `MIRROR_FETCHER_HOSTS` turns it all off.

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
