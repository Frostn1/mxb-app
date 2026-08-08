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
  "game_port": 54210
}
```

`token` is required and must not be blank — an agent listening without one hands process
control to anyone who portscans the box.

## API

Every endpoint except `/health` requires `Authorization: Bearer <token>`.

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Liveness. Unauthenticated, reveals nothing. |
| GET | `/status` | Game process state plus name/track/maxClients from the `.ini`. |
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
- Nothing here is transport-encrypted. Terminate TLS in front of it, or keep the listener
  on a private network, before pointing anything at it over the open internet.
