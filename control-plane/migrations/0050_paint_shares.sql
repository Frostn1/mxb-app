-- Paint download authorisation (`paintsync.ts`, `mayFetchPaint`).
--
-- A paint blob is served to its owner, to a rider currently on the same server as an owner
-- (presence), or to a grantee listed here. Additive: nothing existing changes shape.
--
-- `grantee_kind` is 'account' (grantee = accounts.id) or 'guid' (grantee = a rider GUID).
-- 'team' is reserved for the team lists in the view-only design: nothing reads or writes it
-- yet, and a row of an unknown kind never matches, so adding it later needs no migration.
-- `expires_at` null means until revoked.
CREATE TABLE paint_shares (
  sha256           TEXT NOT NULL,
  owner_account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  grantee_kind     TEXT NOT NULL CHECK (grantee_kind IN ('account', 'guid', 'team')),
  grantee          TEXT NOT NULL,
  created_at       INTEGER NOT NULL,
  expires_at       INTEGER,
  PRIMARY KEY (sha256, grantee_kind, grantee)
);

CREATE INDEX paint_shares_grantee ON paint_shares (grantee_kind, grantee);
