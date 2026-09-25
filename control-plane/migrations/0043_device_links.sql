-- Which accounts have been used on the same machine, so a ban follows the PC and not only the
-- account, the Steam login or the GUID (`bans.ts` says how the resolution reads it).
--
-- `device_hash` is never the machine identifier. The app sends SHA-256 of a domain tag and the
-- OS's machine id (`crates/core/src/device.rs`); the worker keys that again with its own secret,
-- `MXB_DEVICE_SALT` (HMAC-SHA256), and stores only the result. So a row here is useless without
-- the worker's secret: it cannot be turned back into a machine id, and it cannot be matched
-- against anything outside this database. Without the secret nothing is written at all.
--
-- Personal data all the same, so it is not kept past the account: erasure (`erasure.ts`)
-- deletes an account's rows outright — a banned account's included — and the cascade covers
-- the day an account row is ever deleted rather than cleared in place.
--
-- Keyed on the pair, like `guid_claims`: the same account on the same machine moves a
-- timestamp, a second machine is a second row.
CREATE TABLE device_links (
  account_id    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  device_hash   TEXT NOT NULL,
  first_seen_at INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL,
  PRIMARY KEY (account_id, device_hash)
);

-- The two directions: what machines an account has been on, and who else has been on one.
CREATE INDEX device_links_account ON device_links (account_id, last_seen_at);
CREATE INDEX device_links_device ON device_links (device_hash, last_seen_at);
