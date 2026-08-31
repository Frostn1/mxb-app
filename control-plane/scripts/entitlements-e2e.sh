#!/usr/bin/env bash
#
# The entitlement decision, end to end against a local control plane.
#
# Proves the thing step 2 exists to prove: the server, and only the server, decides whether
# a player may use a secured asset — and it decides on the Steam identity, not on anything
# the caller says about itself. No crypto is involved; there is nothing to decrypt yet.
#
# The Steam round trip itself cannot run here (it needs a browser and a real Steam account),
# so the link is seeded directly into D1. What Valve's half must do is covered by the
# adversarial cases in `test/steam.test.ts`, including an assertion minted for another site
# and a Steam that cannot be reached.
#
#   ./scripts/entitlements-e2e.sh
#
set -euo pipefail

PORT="${MXB_E2E_PORT:-8798}"
BASE="http://127.0.0.1:${PORT}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$(mktemp -t mxb-ent-e2e)"
STEAM_ID="76561198000000042"
# A fixed master key and content key so the grant can be checked byte-for-byte.
MASTER="AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="   # base64 of 0x00..0x1f
CEK_HEX="1111111111111111111111111111111111111111111111111111111111111111"
FAIL=0

cd "$HERE"

say()  { printf '\n== %s\n' "$1"; }
# want <description> <expected> <actual>
want() {
  if [ "$2" = "$3" ]; then
    printf '   ok   %s\n' "$1"
  else
    printf '   FAIL %s\n        expected: %s\n        actual:   %s\n' "$1" "$2" "$3"
    FAIL=1
  fi
}
d1() { npx wrangler d1 execute mxb-control-plane --local --command "$1" >/dev/null; }

say "clean local state"
rm -rf .wrangler/state
for m in migrations/*.sql; do
  npx wrangler d1 execute mxb-control-plane --local --file "$m" >/dev/null 2>&1 ||
    echo "   (${m##*/} already applied)"
done

say "start the control plane on :${PORT}"
npx wrangler dev --port "$PORT" --local --var "MXB_ASSET_MASTER_KEY:${MASTER}" >"$LOG" 2>&1 &
trap 'kill %1 2>/dev/null || true' EXIT
for _ in $(seq 1 60); do curl -fsS "${BASE}/v1/servers" >/dev/null 2>&1 && break; sleep 1; done
curl -fsS "${BASE}/v1/servers" >/dev/null || { echo "worker never came up:"; cat "$LOG"; exit 1; }

say "a player, and a creator's asset"
TOKEN=$(curl -fsS -X POST "${BASE}/v1/account" -H 'content-type: application/json' \
  -d '{"riderName":"Frost"}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')
ACCOUNT=$(npx wrangler d1 execute mxb-control-plane --local --json \
  --command "SELECT id FROM accounts LIMIT 1" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["results"][0]["id"])')
d1 "INSERT INTO assets (id, creator_id, title, created_at) VALUES ('trk_pinehill', '${ACCOUNT}', 'Pine Hill', 1)"
d1 "INSERT INTO assets (id, creator_id, title, created_at, withdrawn_at) VALUES ('trk_gone', '${ACCOUNT}', 'Withdrawn', 1, 2)"

check() { curl -s -o /dev/null -w '%{http_code}' -X POST "${BASE}/v1/entitlements/check" \
  -H "authorization: Bearer ${TOKEN}" -H 'content-type: application/json' \
  -d "{\"assetId\":\"$1\",\"sessionId\":\"s1\"}"; }

say "before anything is linked or bought"
want "an unlinked account owns nothing"      "403" "$(check trk_pinehill)"
want "and its entitlement list is empty"     "0" \
  "$(curl -fsS "${BASE}/v1/entitlements" -H "authorization: Bearer ${TOKEN}" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["assets"]))')"

say "link the Steam identity (the browser half, seeded)"
d1 "UPDATE accounts SET steam_id = '${STEAM_ID}' WHERE id = '${ACCOUNT}'"
want "linked but unentitled is still refused" "403" "$(check trk_pinehill)"

say "buy it"
d1 "INSERT INTO entitlements (steam_id, asset_id, source, granted_at) VALUES ('${STEAM_ID}', 'trk_pinehill', 'purchase', 1)"
want "an entitled player is allowed"          "200" "$(check trk_pinehill)"
want "and it shows in their list"             "trk_pinehill" \
  "$(curl -fsS "${BASE}/v1/entitlements" -H "authorization: Bearer ${TOKEN}" | python3 -c 'import json,sys; print(json.load(sys.stdin)["assets"][0]["assetId"])')"

say "the refusals that matter"
want "an asset that does not exist"           "403" "$(check trk_nosuch)"
want "an asset withdrawn from sale"           "403" "$(check trk_gone)"

d1 "UPDATE entitlements SET revoked_at = 99 WHERE steam_id = '${STEAM_ID}' AND asset_id = 'trk_pinehill'"
want "a revoked entitlement stops immediately" "403" "$(check trk_pinehill)"
want "and drops out of their list"             "0" \
  "$(curl -fsS "${BASE}/v1/entitlements" -H "authorization: Bearer ${TOKEN}" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["assets"]))')"

say "somebody else's purchase is not yours"
d1 "UPDATE entitlements SET revoked_at = NULL WHERE steam_id = '${STEAM_ID}'"
d1 "UPDATE accounts SET steam_id = '76561198000000999' WHERE id = '${ACCOUNT}'"
want "same account, different Steam identity"  "403" "$(check trk_pinehill)"

say "the audit log"
GRANTS=$(npx wrangler d1 execute mxb-control-plane --local --json \
  --command "SELECT COUNT(*) AS n FROM entitlement_grants" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["results"][0]["n"])')
DENIES=$(npx wrangler d1 execute mxb-control-plane --local --json \
  --command "SELECT COUNT(*) AS n FROM entitlement_grants WHERE decision = 'deny'" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["results"][0]["n"])')
# Seven checks above: one allow and six refusals. Listing entitlements is not a decision
# and deliberately writes nothing — the log is what was *asked and answered*, not what was
# browsed.
want "every decision is recorded"              "7" "$GRANTS"
want "including the refusals"                  "6" "$DENIES"

say "releasing the content key to an entitled session"
# Relink the entitled identity (the earlier tests moved it away), and give trk_pinehill a
# real wrapped key. A second asset is entitled but never packed, to prove the keyless case.
d1 "UPDATE accounts SET steam_id = '${STEAM_ID}' WHERE id = '${ACCOUNT}'"
d1 "UPDATE entitlements SET revoked_at = NULL WHERE steam_id = '${STEAM_ID}' AND asset_id = 'trk_pinehill'"
d1 "INSERT INTO assets (id, creator_id, title, created_at) VALUES ('trk_nokey', '${ACCOUNT}', 'Never packed', 1)"
d1 "INSERT INTO entitlements (steam_id, asset_id, source, granted_at) VALUES ('${STEAM_ID}', 'trk_nokey', 'purchase', 1)"

WRAP="$(mktemp -t mxb-wrap).js"
cat > "$WRAP" <<'JS'
const { webcrypto } = require("crypto");
const { subtle } = webcrypto;
(async () => {
  const master = Buffer.from(process.argv[2], "base64");
  const cek = Buffer.from(process.argv[3], "hex");
  const key = await subtle.importKey("raw", master, { name: "AES-GCM" }, false, ["encrypt"]);
  const iv = webcrypto.getRandomValues(new Uint8Array(12));
  const ct = Buffer.from(await subtle.encrypt({ name: "AES-GCM", iv }, key, cek));
  process.stdout.write(Buffer.concat([Buffer.from(iv), ct]).toString("base64"));
})();
JS
WRAPPED="$(node "$WRAP" "$MASTER" "$CEK_HEX")"
d1 "UPDATE assets SET wrapped_key = '${WRAPPED}', key_id = 'k1' WHERE id = 'trk_pinehill'"

grant() { curl -s -X POST "${BASE}/v1/keys/grant" -H "authorization: Bearer ${TOKEN}"   -H 'content-type: application/json' -d "{\"assetId\":\"$1\",\"sessionId\":\"s2\"}"; }
grant_code() { curl -s -o /dev/null -w '%{http_code}' -X POST "${BASE}/v1/keys/grant"   -H "authorization: Bearer ${TOKEN}" -H 'content-type: application/json'   -d "{\"assetId\":\"$1\",\"sessionId\":\"s2\"}"; }

want "an entitled session is granted the key" "200" "$(grant_code trk_pinehill)"
GOT_HEX="$(grant trk_pinehill | python3 -c 'import json,sys,base64; print(base64.b64decode(json.load(sys.stdin)["contentKey"]).hex())')"
want "and it is the real content key"          "$CEK_HEX" "$GOT_HEX"

want "an entitled asset with no key is a 409"  "409" "$(grant_code trk_nokey)"

d1 "UPDATE entitlements SET revoked_at = 99 WHERE steam_id = '${STEAM_ID}' AND asset_id = 'trk_pinehill'"
want "a revoked entitlement is denied the key" "403" "$(grant_code trk_pinehill)"

rm -f "$WRAP"

printf '\n'
[ "$FAIL" = 0 ] && echo "PASS" || { echo "FAILURES ABOVE"; exit 1; }
