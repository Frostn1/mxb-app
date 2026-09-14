-- API keys a creator makes on mxbsecure.com, so a shop's server can add and remove buyers.
--
-- A key acts as its creator, on their own assets only, and only reads the asset list and
-- changes grants: it can't make assets, withdraw them, or mint more keys. The secret is shown
-- once when it's made; only its SHA-256 is kept, the same as `accounts.token_hash`. Revoked
-- rather than deleted, so the dashboard can say which key did what.
CREATE TABLE creator_keys (
  id           TEXT PRIMARY KEY,
  account_id   TEXT NOT NULL REFERENCES accounts (id),
  label        TEXT NOT NULL,
  token_hash   TEXT NOT NULL UNIQUE,
  created_at   INTEGER NOT NULL,
  last_used_at INTEGER,
  revoked_at   INTEGER
);
CREATE INDEX creator_keys_account ON creator_keys (account_id);
