# mxb-agent

Supervises an MX Bikes dedicated server and exposes it over an authenticated HTTP API, so
MXB App can manage a server without shipping cloud-provider credentials to end users.

The agent **owns** the `mxbikes.exe` process — it spawns it, watches it, and brings it back
if it exits on its own. That ownership is the point: exit detection comes from the child
handle rather than from polling the process table, "restart" isn't a race between a kill and
someone else's respawn, and a reboot that starts the agent starts the server with it.

## Configuration

`agent.json`, beside the binary (or pass a path as the first argument):

```json
{
  "token": "<a long random string>",
  "listen": "0.0.0.0:8787",
  "game_dir": "C:\\mxb\\game",
  "ini": "dedicated.ini",
  "game_port": 54210,
  "public_url": "http://203.0.113.10:8787"
}
```

`token` is required and must not be blank — an agent listening without one hands process
control to anyone who portscans the box.

`public_url` is optional. It only affects the pairing line below: `listen` is a *bind*
address, so `0.0.0.0` tells nobody how to reach the box, and the agent falls back to the
address of the interface it would use to reach the internet. Behind NAT or a reverse proxy
that guess is wrong and only you know the right answer — set it here.

## Pairing

On every start the agent prints one line to stdout:

```
mxb-agent:eyJ1cmwiOiJodHRwOi8vMjAzLjAuMTEzLjEwOjg3ODciLCJ0b2tlbiI6Ii4uLiJ9
```

Paste it into the app's **Add a server** field and the address and token fill themselves in;
the app then asks the agent for the server's name. Lost it? Restart the agent — it prints
again. It carries the token, so treat it like the token.

## API

Every endpoint except `/health` and `/info` requires `Authorization: Bearer <token>`.

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Liveness. Unauthenticated, reveals nothing. |
| GET | `/info` | The server-browser view — name, track, player **names**, and the tracks and bikes this host has. Unauthenticated: see below. |
| GET | `/status` | Game process state plus name/track/maxClients from the `.ini`. |
| GET | `/players` | Who is connected, with the GUID that identifies them, read from the server's log. |
| GET | `/tracks` | Track names this host has installed — the only values `PUT /config` will usefully accept. |
| POST | `/start` | Start the game if it isn't up. Idempotent. |
| POST | `/stop` | Stop it. Idempotent, and suppresses the crash watcher. |
| POST | `/restart` | Stop then start. |
| GET | `/config` | The raw `.ini` text. |
| PUT | `/config` | `{ track?, name?, maxClients? }` — patches the `.ini` **and restarts**, because the game only reads it at startup. |

## Building

```sh
cargo test
cargo build --release --target x86_64-pc-windows-gnu   # from macOS/Linux
```

## Security

- The token is compared in constant time; a short-circuiting compare leaks the matching
  prefix length through timing, which recovers the token a byte at a time.
- `PUT /config` accepts only three keys. It is an API for running a server, not for
  rewriting arbitrary game config, and values containing newlines are rejected so a caller
  can't inject unrelated `.ini` keys.
- `/info` is deliberately unauthenticated. It answers what a player asks *before* they have
  anything to do with this box — who is on it, what is being ridden, what it will accept —
  so a credential only its operator could hand out is the wrong gate, and the game already
  advertises the same server in its own in-game list. It reports player *names* and never
  GUIDs: a GUID is a stable per-install identity and stays behind the token on `/players`.
  The password is reported only as `locked: true`, never as its value.
- Nothing here is transport-encrypted. Terminate TLS in front of it, or keep the listener
  on a private network, before pointing anything at it over the open internet.
