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
(cd "$ROOT" && exec setsid nohup bin/mxbserver --config config/server.toml --report 10 \
  > logs/mxbserver.log 2>&1 < /dev/null) &
echo $! > "$ROOT/mxbserver.pid"
for _ in $(seq 40); do ready && break; sleep 0.25; done
ready || fail "stub did not start"

echo "=== detect and read"
out="$(remote read "$OBS")"
[[ "$(field mode <<<"$out")" == bare ]] || fail "mode: $out"
[[ "$(field config <<<"$out")" == "$ROOT/config/server.toml" ]] || fail "config path: $out"
[[ "$(field config_b64 <<<"$out" | base64 -d)" == "$(cat "$ROOT/config/server.toml")" ]] || fail "read"
sha="$(field sha <<<"$out")"

echo "=== validate"
good="$(sed 's/count = 4/count = 6/' "$ROOT/config/server.toml")"
[[ "$(remote validate "$OBS" "$(b64 <<<"$good")" | field valid)" == 1 ]] || fail "good candidate refused"
bad="$good
invalid = true"
[[ "$(remote validate "$OBS" "$(b64 <<<"$bad")" | field valid)" == 0 ]] || fail "bad candidate accepted"
ls "$ROOT"/config/.candidate-* 2>/dev/null && fail "candidate left behind"

echo "=== apply refuses a stale hash"
if remote apply "$OBS" "$(b64 <<<"$good")" "$(printf '0%.0s' {1..64})"; then fail "stale hash applied"; fi
grep -q "count = 4" "$ROOT/config/server.toml" || fail "stale apply changed the file"

echo "=== apply a good change: backup, replace, restart, ready"
old_pid="$(server_pid)"
out="$(remote apply "$OBS" "$(b64 <<<"$good")" "$sha")"
echo "$out"
[[ "$(field result <<<"$out")" == applied ]] || fail "not applied"
grep -q "count = 6" "$ROOT/config/server.toml" || fail "file not replaced"
grep -q "# the live config" "$ROOT/config/server.toml" || fail "comment lost"
backup="$(field backup <<<"$out")"
grep -q "count = 4" "$backup" || fail "backup is not the old config"
[[ "$(server_pid)" != "$old_pid" ]] || fail "server was not restarted"
ready || fail "not ready after apply"
tr '\0' ' ' < "/proc/$(server_pid)/cmdline" | grep -q -- "--config config/server.toml --report 10" || fail "arguments changed"
[[ "$(cat "$ROOT/mxbserver.pid")" == "$(server_pid)" ]] || fail "the pid file does not name the server"

echo "=== apply a change that never gets ready: the backup goes back"
sha="$(sha256sum "$ROOT/config/server.toml" | cut -d' ' -f1)"
broken="$(cat "$ROOT/config/server.toml")
# never_ready"
out="$(remote apply "$OBS" "$(b64 <<<"$broken")" "$sha" 2>&1)" || true
echo "$out"
[[ "$(field result <<<"$out")" == rolled-back ]] || fail "not rolled back"
grep -q never_ready "$ROOT/config/server.toml" && fail "broken config still live"
ready || fail "not ready after rollback"

echo "=== an invalid change is refused before anything happens"
sha="$(sha256sum "$ROOT/config/server.toml" | cut -d' ' -f1)"
pid="$(server_pid)"
if remote apply "$OBS" "$(b64 <<<"$bad")" "$sha"; then fail "invalid config applied"; fi
[[ "$(server_pid)" == "$pid" ]] || fail "invalid config restarted the server"

echo "=== admin-addr: the default, then the config's [admin] listen"
[[ "$(remote admin-addr "$OBS" | field admin)" == 127.0.0.1:9810 ]] || fail "default admin address"
printf '\n[admin]\nlisten = "127.0.0.1:9815" # the admin API\n' >> "$ROOT/config/server.toml"
[[ "$(remote admin-addr "$OBS" | field admin)" == 127.0.0.1:9815 ]] || fail "admin address from the config"

echo "=== tracks: list the .pkz files beside the configured package, install one"
printf '\n[track]\npackage = "tracks/a.pkz"\n' >> "$ROOT/config/server.toml"
mkdir -p "$ROOT/config/tracks"
touch "$ROOT/config/tracks/a.pkz" "$ROOT/config/tracks/b.pkz" "$ROOT/config/tracks/notes.txt"
out="$(remote tracks "$OBS")"
[[ "$(field dir <<<"$out")" == "$ROOT/config/tracks" ]] || fail "track dir: $out"
[[ "$(field tracks_b64 <<<"$out" | base64 -d | tr '\n' ' ')" == "a.pkz b.pkz " ]] || fail "track list: $out"
printf 'pkz' > /tmp/mxb-servers-test
out="$(remote install-track "$OBS" mxb-servers-test c.pkz "$(sha256sum /tmp/mxb-servers-test | cut -d' ' -f1)")"
[[ "$(field installed <<<"$out")" == c.pkz ]] || fail "install-track: $out"
[[ -f "$ROOT/config/tracks/c.pkz" ]] || fail "track not installed"
printf 'pkz' > /tmp/mxb-servers-test
if remote install-track "$OBS" mxb-servers-test d.pkz "$(printf '0%.0s' {1..64})"; then fail "track with a wrong hash installed"; fi
rm -f /tmp/mxb-servers-test

echo "=== tracks: a package whose folder does not exist; the files are in content/ and content/tracks/"
sed -i 's#package = "tracks/a.pkz"#package = "../tracks/755-Compound.pkz"#' "$ROOT/config/server.toml"
mkdir -p "$ROOT/content/tracks"
touch "$ROOT/content/tracks/755-Compound.pkz" "$ROOT/content/tracks/WDR.MX.26.R01.pkz"   "$ROOT/content/tracks/WDR.MX.26.R02.pkz" "$ROOT/content/tracks/ZD_-_BlackwoodMXPark.pkz"   "$ROOT/content/arlfinals-rd02-pro-server.pkz" "$ROOT/content/zd-blackwood-server.pkz"   "$ROOT/content/tracks/755-compound.txt"
[[ ! -d "$ROOT/tracks" ]] || fail "test layout has a tracks folder"
out="$(remote tracks "$OBS")"
[[ "$(field package <<<"$out")" == "$ROOT/tracks/755-Compound.pkz" ]] || fail "package: $out"
want="755-Compound.pkz WDR.MX.26.R01.pkz WDR.MX.26.R02.pkz ZD_-_BlackwoodMXPark.pkz arlfinals-rd02-pro-server.pkz zd-blackwood-server.pkz "
[[ "$(field tracks_b64 <<<"$out" | base64 -d | LC_ALL=C sort | tr '
' ' ')" == "$want" ]] || fail "content track list: $out"
[[ "$(field paths_b64 <<<"$out" | base64 -d | grep -c pkz)" == 6 ]] || fail "content track paths: $out"
printf 'pkz' > /tmp/mxb-servers-test
out="$(remote install-track "$OBS" mxb-servers-test e.pkz "$(sha256sum /tmp/mxb-servers-test | cut -d' ' -f1)")"
[[ -f "$ROOT/content/tracks/e.pkz" ]] || fail "track not installed into content/tracks: $out"
rm -f /tmp/mxb-servers-test

echo "=== logs: the log file of a bare server"
remote logs "$OBS" 5 | grep -q "stub server" || fail "logs"

echo "=== service restart: the server comes back with the same arguments"
old_pid="$(server_pid)"
[[ "$(remote service "$OBS" restart | field result)" == restarted ]] || fail "service restart"
[[ "$(server_pid)" != "$old_pid" ]] || fail "service restart kept the old process"
ready || fail "not ready after service restart"

echo "=== install-version: new binary in, restart, previous kept"
cp "$ROOT/bin/mxbserver" /tmp/mxb-servers-bin
out="$(remote install-version "$OBS" mxb-servers-bin mxbserver "$(sha256sum /tmp/mxb-servers-bin | cut -d' ' -f1)" v-test)"
[[ "$(field result <<<"$out")" == installed ]] || fail "install-version: $out"
[[ "$(field version <<<"$out")" == v-test ]] || fail "version label: $out"
[[ -f "$ROOT/bin/mxbserver.previous" ]] || fail "no previous binary kept"
ready || fail "not ready after install-version"
rm -f /tmp/mxb-servers-bin

echo "=== a second server of ours: the one on the observe port is the one edited"
mkdir -p "$HOME/other/config"
printf '[server]
observe = "127.0.0.1:%s"
' $((OBS + 1)) > "$HOME/other/config/server.toml"
(cd "$HOME/other" && exec setsid nohup "$ROOT/bin/mxbserver" --config config/server.toml > /dev/null 2>&1 < /dev/null) &
sleep 1
out="$(remote read "$OBS")"
[[ "$(field config <<<"$out")" == "$ROOT/config/server.toml" ]] || fail "picked the wrong server: $out"
out="$(remote read $((OBS + 1)))"
[[ "$(field config <<<"$out")" == "$HOME/other/config/server.toml" ]] || fail "second server: $out"

pkill -INT -u "$(id -un)" -x mxbserver || true
echo PASS
