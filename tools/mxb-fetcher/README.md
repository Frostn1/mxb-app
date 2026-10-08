# mxb-fetcher

mxb-mods.com and MediaFire answer Cloudflare Workers 403 and a normal machine 200. This small
service runs on our own Linux box and does those fetches for the mod mirror. It opens no ports:
it pulls work from the control plane (`/v1/mirror/fetcher/*`, `control-plane/src/mirrorfetcher.ts`).

Each round it leases up to two jobs and runs them side by side:

- **List**: GET one WordPress REST request (category tree, listing page, id sweep) and hand
  its status and body back. The control plane walks the catalogue with these, one at a time.
- **Page**: GET the mxb-mods.com post with a browser User-Agent and hand the HTML back. The
  control plane parses it exactly as the Worker does and answers with the pictures it still
  needs; each is fetched, checked to be a picture by its first bytes, and uploaded.
- **File**: MediaFire file links are resolved the way the MXB App does it (page first, API
  second); MediaFire folders are listed through the API and handed back as parts. The body is
  streamed to a temp file while it is hashed (never held in memory), then uploaded from disk.
  A file the mirror already holds (same SHA-256) is not uploaded.

Uploads go to the control plane (`upload/start`, `upload/part`, `upload/complete`), which
writes them into R2 as a multipart upload through its own bucket binding: 32 MiB parts, one in
memory at a time, in order. It hashes the parts as they arrive and aborts the upload if the
bytes don't match the SHA-256 the box sent. The box needs no R2 credentials.

Requests to one site are at least 2 s apart. A 403 or 429 backs that site off (Retry-After,
else 2 min doubling to 2 h) and hands its jobs back without counting an attempt.

Which hosts come here is the control plane's `MIRROR_FETCHER_HOSTS` var, plus any host that
refuses the Worker a file.

## Configuration

| Variable | |
|---|---|
| `MXB_FETCHER_API` | The control plane, `https://api.mxbsecure.com` |
| `MXB_FETCHER_TOKEN` | The control plane's `MIRROR_FETCHER_TOKEN` secret |
| `MXB_FETCHER_TMP` | Spool directory (the unit sets `/var/lib/mxb-fetcher/tmp`) |
| `MXB_FETCHER_IDLE` | Seconds between leases when there is no work (default 30) |

## Install (Debian or Ubuntu, as root)

```sh
apt-get install -y build-essential curl git
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"

git clone --depth 1 https://github.com/Frostn1/mxb-app.git /opt/mxb-app
cd /opt/mxb-app/tools/mxb-fetcher
cargo test --locked
cargo build --release --locked
install -m 755 target/release/mxb-fetcher /usr/local/bin/mxb-fetcher
install -m 644 mxb-fetcher.service /etc/systemd/system/mxb-fetcher.service

install -m 600 /dev/null /etc/mxb-fetcher.env
cat > /etc/mxb-fetcher.env <<'EOF'
MXB_FETCHER_API=https://api.mxbsecure.com
MXB_FETCHER_TOKEN=<the MIRROR_FETCHER_TOKEN value>
EOF

systemctl daemon-reload
systemctl enable --now mxb-fetcher
journalctl -u mxb-fetcher -f
```

One JSON line per job in the journal: `done`, `failed`, or `deferred` (a site is backing us off).

To update: `git -C /opt/mxb-app pull`, then the `cargo build`, `install` and
`systemctl restart mxb-fetcher` lines again.

Leave room on the disk for the largest file in flight (up to 4.5 GiB). A job cut off by a
restart is leased out again once its lease runs out.

## Development

```sh
cargo test
```

The MediaFire link and folder parsing (`src/mediafire.rs`) carries the MXB App's tests for
every page shape MediaFire has shipped.
