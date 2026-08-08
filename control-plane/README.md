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
| GET | `/v1/me` | bearer | Account and current loadout |
| PUT | `/v1/loadout` | bearer | Replace the loadout; returns `missing` — the blobs still to upload |
| GET | `/v1/servers` | — | Server registry. Public: it is the app's join picker, and the people who most need it are the ones with no account yet. `agent_url` is not returned. |
| GET | `/v1/roster?server=<id>` | bearer | Riders and their paints, for the sync |

Enrollment by invite code stands in for Steam sign-in until there's an API key. `accounts`
already carries a nullable `steam_id`, so adding Steam is a backfill rather than a rewrite
of every account's identity.

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
npx wrangler d1 execute mxb-control-plane --local --file migrations/0001_init.sql
npx wrangler dev
```

Deploy with `npx wrangler deploy`. Resources already provisioned in the personal account:
D1 `mxb-control-plane` (WEUR) and R2 `mxb-paints` (WEUR).
