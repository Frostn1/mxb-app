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
| GET | `/v1/servers/browse` | — | The same list with each server's live detail folded in — track, who is on it, and the tracks and bikes the host has. Proxied from each agent's `/info` and cached briefly, so `agent_url` stays private. |
| GET | `/v1/servers/:id/info` | — | One server's live detail, for the panel behind a row. |
| GET | `/v1/players?q=` | bearer | Which server a rider is on, by name or GUID. Authenticated on purpose — see below. |
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

Enrollment by invite code stands in for Steam sign-in until there's an API key. `accounts`
already carries a nullable `steam_id`, so adding Steam is a backfill rather than a rewrite
of every account's identity.

### Why the player search needs a token when the server list does not

A server is a place, and its grid is on show to everyone standing on it — so listing servers,
and what is being ridden on them, is public. "Where is this person right now" is a question
about a person, and answering it for anyone who can reach the worker would make the registry
a tracker. Requiring an enrolled account is the smallest gate that keeps it a feature for
players rather than a public lookup.

The same line runs through what the agent reports. `/info` is unauthenticated and carries
player *names* — what you would see by joining — while GUIDs, which identify an install
across name changes, stay behind the agent's token on `/players`.

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
