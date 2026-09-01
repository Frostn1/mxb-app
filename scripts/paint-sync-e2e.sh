#!/usr/bin/env bash
#
# Two riders, one server, no invite codes and no agent.
#
# Stands up the control plane on local D1 and R2, then runs the ignored two-rider test
# against it. That test signs up two self-serve accounts, has both say they are on the same
# server — keyed by its name, the way the app derives it from what FrostMod reads out of the
# running game — publishes a paint for each, and checks the other rider ends up with it on
# disk. A third rider on a different server must end up with nothing, and two riders
# sharing a name must still be counted as two — which only holds if the GUID reaches the
# roster.
#
# What this does not cover: reading the server name out of a live MX Bikes. That half is
# Windows-only and has no substitute here.
#
#   ./scripts/paint-sync-e2e.sh
#
set -euo pipefail

PORT="${MXB_E2E_PORT:-8799}"
BASE="http://127.0.0.1:${PORT}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$(mktemp -t mxb-e2e-worker)"

cd "$ROOT/control-plane"

# A clean database every run. Two things in the control plane are deliberately one-shot and
# would otherwise fail the second run rather than the first: a GUID is UNIQUE per account, and
# self-serve sign-ups are capped at five per address per day. Both are correct — they are also
# why a test that signs up riders needs a fresh slate rather than a shared one.
echo "==> wiping local worker state"
rm -rf .wrangler/state

echo "==> applying migrations to local D1"
for m in migrations/*.sql; do
  npx wrangler d1 execute mxb-control-plane --local --file "$m" >/dev/null 2>&1 ||
    echo "    (${m##*/} already applied)"
done

echo "==> starting the control plane on :${PORT}"
npx wrangler dev --port "$PORT" --local >"$LOG" 2>&1 &
WORKER=$!
trap 'kill $WORKER 2>/dev/null || true' EXIT

for _ in $(seq 1 60); do
  # /v1/servers is public, so a 200 here means the worker is up and its D1 is bound.
  if curl -fsS "${BASE}/v1/servers" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
if ! curl -fsS "${BASE}/v1/servers" >/dev/null 2>&1; then
  echo "the control plane never came up. Worker log:" >&2
  cat "$LOG" >&2
  exit 1
fi
echo "    up"

echo "==> two riders, one server"
cd "$ROOT/src-tauri"
MXB_CONTROL_PLANE="$BASE" \
  cargo test --locked two_riders -- --ignored --nocapture --test-threads=1
