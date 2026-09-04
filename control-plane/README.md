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
| PUT | `/v1/me/guid` | bearer | Claim a GUID. First-come. |
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
| GET | `/admin` | `ADMIN_KEY` | The dashboard. One URL, three tabs — usage, diagnostics, paint sync — and a search box on every page of it |
| GET | `/admin/search` | `ADMIN_KEY` | One query asked of all three at once: riders, mod files, paints |
| GET | `/admin/usage` | `ADMIN_KEY` | The usage tab on its own. What `/admin` opens on |

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
npx wrangler secret put BMAC_WEBHOOK_SECRET          # shown by BMAC when the webhook is made
npx wrangler secret put DISCORD_DONATION_WEBHOOK_URL # the channel webhook — a credential itself
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

```sh
npx wrangler secret put ADMIN_KEY   # without it both admin routes answer 503, not 401
```

Then `https://<worker>/admin?key=<ADMIN_KEY>` — the one URL worth bookmarking. It opens on
usage (`&days=7|30|90|365`), and the tabs across the top reach the diagnostics and paint-sync
dashboards without a second browser tab. The search box in the header answers across all
three: a rider name, a GUID, a Steam id, a file name, a signer or a digest.

The app's side is `src-tauri/src/usage.rs`. It is off in debug builds unless
`MXB_ANALYTICS_DEV=1`, off for a run with `MXB_NO_ANALYTICS=1`, and off for good from the
switch in Settings → General.

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
npm install
npx wrangler types                                              # regenerate Env
npx tsc --noEmit
npx vitest run
for m in migrations/*.sql; do npx wrangler d1 execute mxb-control-plane --local --file "$m"; done
npx wrangler dev
```

### Pointing the app at it

`MXB_CONTROL_PLANE` redirects the desktop app to another control plane. **Debug builds only** —
a shipped binary always uses the baked-in URL, because responses from here become files written
into the game's mods folder and a redirectable target is a way to put content on a player's disk.

```sh
MXB_EXPERIMENTAL=1 MXB_CONTROL_PLANE=http://127.0.0.1:8799 npm run start-dev
```

The paint-sync round trip has a live test that needs both:

```sh
cd src-tauri
MXB_CONTROL_PLANE=http://127.0.0.1:8799 MXB_TEST_TOKEN=<token from /v1/enroll> \
  cargo test --locked live_sync -- --ignored --nocapture
```

Deploy with `npx wrangler deploy`. Resources already provisioned in the personal account:
D1 `mxb-control-plane` (WEUR) and R2 `mxb-paints` (WEUR).
