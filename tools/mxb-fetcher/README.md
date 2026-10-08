# mxb-fetcher

mxb-mods.com and MediaFire answer Cloudflare Workers 403 and a normal machine 200. mxb-mods.com
also answers 403 to datacenter addresses (an AWS box gets 403 even with a browser User-Agent),
while MediaFire doesn't. So the fetcher runs in two places, the same binary in both:

| Where | `MXB_FETCHER_HOSTS` | Takes |
|---|---|---|
| Our Linux box (systemd) | `* -mxb-mods.com` | MediaFire, and any other host that refused the Worker |
| A home connection (Windows or Linux) | `mxb-mods.com` | mxb-mods.com: discovery, post pages, its pictures and files |

It opens no ports: it pulls work from the control plane (`/v1/mirror/fetcher/*`,
`control-plane/src/mirrorfetcher.ts`), and the lease only hands out jobs on the hosts the
fetcher asked for. Until a home fetcher is running, mxb-mods.com's jobs simply wait in D1.
Nothing sends the site a request in the meantime, and nothing alerts.

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
bytes don't match the SHA-256 the fetcher sent. The fetcher needs no R2 credentials.

Requests to one site are at least 2 s apart. A 403 or 429 backs that site off (Retry-After,
else 2 min doubling to 2 h) and hands its jobs back without counting an attempt.

## Configuration

From the environment, or a `KEY=VALUE` file whose path is the first argument (or
`MXB_FETCHER_CONFIG`). Values are trimmed, so CRLF line endings, a BOM or a pasted token with
a trailing newline are fine.

| Setting | |
|---|---|
| `MXB_FETCHER_API` | The control plane, `https://api.mxbsecure.com` |
| `MXB_FETCHER_TOKEN` | The control plane's `MIRROR_FETCHER_TOKEN` secret |
| `MXB_FETCHER_HOSTS` | Hosts this fetcher serves: names, `*` for any, `-name` to leave one out. Unset: all |
| `MXB_FETCHER_TMP` | Spool directory (default: the system temp dir; the unit sets `/var/lib/mxb-fetcher/tmp`) |
| `MXB_FETCHER_IDLE` | Seconds between leases when there is no work (default 30) |

Leave room on the disk for the largest file in flight (up to 4.5 GiB). A job cut off by a
stop or restart is leased out again once its lease runs out.

## The Linux box (Debian or Ubuntu, as root)

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
MXB_FETCHER_HOSTS=* -mxb-mods.com
EOF

systemctl daemon-reload
systemctl enable --now mxb-fetcher
journalctl -u mxb-fetcher -f
```

One JSON line per job in the journal: `done`, `failed`, or `deferred` (a site is backing us off).

To update: `git -C /opt/mxb-app pull`, then the `cargo build`, `install` and
`systemctl restart mxb-fetcher` lines again.

## A home Windows machine (for mxb-mods.com)

Build it once, with Rust from https://rustup.rs (either the MSVC or the GNU toolchain), from a
checkout of this repo:

```powershell
cd tools\mxb-fetcher
cargo test --locked
cargo build --release --locked

New-Item -ItemType Directory -Force C:\mxb-fetcher | Out-Null
Copy-Item target\release\mxb-fetcher.exe C:\mxb-fetcher\
@"
MXB_FETCHER_API=https://api.mxbsecure.com
MXB_FETCHER_TOKEN=<the MIRROR_FETCHER_TOKEN value>
MXB_FETCHER_HOSTS=mxb-mods.com
MXB_FETCHER_TMP=C:\mxb-fetcher\tmp
"@ | Set-Content C:\mxb-fetcher\fetcher.env
# Only you (and SYSTEM and Administrators) can read the token.
icacls C:\mxb-fetcher\fetcher.env /inheritance:r /grant:r "${env:USERNAME}:(R,W)" "SYSTEM:(R)" "Administrators:(R)"
```

Try it in the foreground first (Ctrl+C stops it):

```powershell
C:\mxb-fetcher\mxb-fetcher.exe C:\mxb-fetcher\fetcher.env
```

Then run it as a scheduled task that starts with Windows, hidden, and restarts if it stops.
From an **elevated** PowerShell (`S4U` runs it as you without storing your password, whether
or not you are signed in, with no window):

```powershell
$action = New-ScheduledTaskAction -Execute "cmd.exe" `
  -Argument '/c C:\mxb-fetcher\mxb-fetcher.exe C:\mxb-fetcher\fetcher.env >> C:\mxb-fetcher\fetcher.log 2>&1'
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType S4U
$settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
  -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName "MXB fetcher" -Action $action -Trigger $trigger -Principal $principal -Settings $settings
Start-ScheduledTask -TaskName "MXB fetcher"
Get-Content C:\mxb-fetcher\fetcher.log -Wait -Tail 20
```

To stop it: `Stop-ScheduledTask -TaskName "MXB fetcher"`. To update: rebuild, stop the task,
copy the new `mxb-fetcher.exe` over, and start the task again. To remove it:
`Unregister-ScheduledTask -TaskName "MXB fetcher" -Confirm:$false`.

A home Linux machine works the same as the box: the systemd steps above, with
`MXB_FETCHER_HOSTS=mxb-mods.com`.

## Development

```sh
cargo test
```

The MediaFire link and folder parsing (`src/mediafire.rs`) carries the MXB App's tests for
every page shape MediaFire has shipped.
