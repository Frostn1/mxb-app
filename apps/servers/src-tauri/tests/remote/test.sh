#!/usr/bin/env bash
# remote.sh against a bare-process server laid out like Lightsail today (~/mxbserver, started
# by hand), with stub.c standing in for mxbserver. Run on a Linux CI runner:
#   bash apps/servers/src-tauri/tests/remote/test.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/../../src/remote.sh"
ROOT="$HOME/mxbserver"
OBS=19809

fail() { printf 'FAIL: %s\n' "$*" >&2; tail -n 20 "$ROOT/logs/mxbserver.log" >&2 || true; exit 1; }
remote() { bash -s -- "$@" < "$SCRIPT"; }
field() { sed -n "s/^@@$1 //p"; }
b64() { base64 -w0; }
ready() { curl -fsS --max-time 1 "http://127.0.0.1:$OBS/readyz" >/dev/null 2>&1; }
server_pid() { pgrep -u "$(id -un)" -x mxbserver | head -n1; }

rm -rf "$ROOT"
mkdir -p "$ROOT"/{bin,config,logs}
gcc -O1 -o "$ROOT/bin/mxbserver" "$HERE/stub.c"
cat > "$ROOT/config/server.toml" <<EOF
# the live config
[server]
observe = "127.0.0.1:$OBS"

[ghost]
count = 4
EOF
(cd "$ROOT" && setsid nohup bin/mxbserver --config config/server.toml --report 10 \
  > logs/mxbserver.log 2>&1 < /dev/null & echo $! > mxbserver.pid)
for _ in $(seq 40); do ready && break; sleep 0.25; done
ready || fail "stub did not start"

echo "=== detect and read"
out="$(remote detect)"
[[ "$(field mode <<<"$out")" == bare ]] || fail "mode: $out"
[[ "$(field config <<<"$out")" == "$ROOT/config/server.toml" ]] || fail "config path: $out"
out="$(remote read)"
[[ "$(field config_b64 <<<"$out" | base64 -d)" == "$(cat "$ROOT/config/server.toml")" ]] || fail "read"
sha="$(field sha <<<"$out")"

echo "=== validate"
good="$(sed 's/count = 4/count = 6/' "$ROOT/config/server.toml")"
[[ "$(remote validate "$(b64 <<<"$good")" | field valid)" == 1 ]] || fail "good candidate refused"
bad="$good
invalid = true"
[[ "$(remote validate "$(b64 <<<"$bad")" | field valid)" == 0 ]] || fail "bad candidate accepted"
ls "$ROOT"/config/.candidate-* 2>/dev/null && fail "candidate left behind"

echo "=== apply refuses a stale hash"
if remote apply "$(b64 <<<"$good")" "$(printf '0%.0s' {1..64})" "$OBS"; then fail "stale hash applied"; fi
grep -q "count = 4" "$ROOT/config/server.toml" || fail "stale apply changed the file"

echo "=== apply a good change: backup, replace, restart, ready"
old_pid="$(server_pid)"
out="$(remote apply "$(b64 <<<"$good")" "$sha" "$OBS")"
echo "$out"
[[ "$(field result <<<"$out")" == applied ]] || fail "not applied"
grep -q "count = 6" "$ROOT/config/server.toml" || fail "file not replaced"
grep -q "# the live config" "$ROOT/config/server.toml" || fail "comment lost"
backup="$(field backup <<<"$out")"
grep -q "count = 4" "$backup" || fail "backup is not the old config"
[[ "$(server_pid)" != "$old_pid" ]] || fail "server was not restarted"
ready || fail "not ready after apply"
tr '\0' ' ' < "/proc/$(server_pid)/cmdline" | grep -q -- "--config config/server.toml --report 10" || fail "arguments changed"

echo "=== apply a change that never gets ready: the backup goes back"
sha="$(sha256sum "$ROOT/config/server.toml" | cut -d' ' -f1)"
broken="$(cat "$ROOT/config/server.toml")
# never_ready"
out="$(remote apply "$(b64 <<<"$broken")" "$sha" "$OBS" 2>&1)" || true
echo "$out"
[[ "$(field result <<<"$out")" == rolled-back ]] || fail "not rolled back"
grep -q never_ready "$ROOT/config/server.toml" && fail "broken config still live"
ready || fail "not ready after rollback"

echo "=== an invalid change is refused before anything happens"
sha="$(sha256sum "$ROOT/config/server.toml" | cut -d' ' -f1)"
pid="$(server_pid)"
if remote apply "$(b64 <<<"$bad")" "$sha" "$OBS"; then fail "invalid config applied"; fi
[[ "$(server_pid)" == "$pid" ]] || fail "invalid config restarted the server"

kill -INT "$(server_pid)"
echo PASS
