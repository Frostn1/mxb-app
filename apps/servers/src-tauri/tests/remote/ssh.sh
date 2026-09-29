#!/usr/bin/env bash
# remote.sh the way the app runs it on a real server: over SSH, script on stdin. Needs sshd
# on localhost (the CI runner has it) and test.sh's ~/mxbserver layout.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/../../src/remote.sh"
ROOT="$HOME/mxbserver"
OBS=19809

fail() { printf 'FAIL: %s
' "$*" >&2; exit 1; }
field() { sed -n "s/^@@$1 //p"; }

sudo systemctl start ssh 2>/dev/null || sudo service ssh start
mkdir -p "$HOME/.ssh" && chmod 700 "$HOME/.ssh"
[[ -f "$HOME/.ssh/id_ci" ]] || ssh-keygen -q -t ed25519 -N "" -f "$HOME/.ssh/id_ci"
cat "$HOME/.ssh/id_ci.pub" >> "$HOME/.ssh/authorized_keys"
chmod 600 "$HOME/.ssh/authorized_keys"
SSH=(ssh -i "$HOME/.ssh/id_ci" -o StrictHostKeyChecking=no -o BatchMode=yes localhost)

(cd "$ROOT" && exec setsid nohup bin/mxbserver --config config/server.toml --report 10   > logs/mxbserver.log 2>&1 < /dev/null) &
sleep 1

out="$(timeout 60 "${SSH[@]}" "bash -s -- read $OBS" < "$SCRIPT")" || fail "read over ssh"
sha="$(field sha <<<"$out")"
cand="$(sed 's/count = [0-9]*/count = 9/' "$ROOT/config/server.toml" | base64 -w0)"
start=$(date +%s)
# The SSH channel must close as soon as the script is done: nothing it starts may hold it.
out="$(timeout 60 "${SSH[@]}" "bash -s -- apply $OBS $cand $sha" < "$SCRIPT" 2>&1)" || fail "apply over ssh did not return: $out"
echo "$out"
[[ "$(field result <<<"$out")" == applied ]] || fail "not applied over ssh"
(( $(date +%s) - start < 30 )) || fail "apply over ssh took too long"
pkill -INT -u "$(id -un)" -x mxbserver || true
echo PASS
