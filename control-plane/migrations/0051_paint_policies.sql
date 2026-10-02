-- View-only and locked paints (`paintpolicy.ts`).
--
-- One row per paint a rider has set a policy on. Additive: no existing table changes shape,
-- and a paint with no row behaves exactly as before (shared, wearable by anyone).
--
-- `view_only`: other riders' apps install it for the session only and delete it when the game
--   exits. Only apps that say they understand the flag are sent it at all; older apps and the
--   roster route never see a view-only paint.
-- `locked`: only the owner and the team list (`paint_shares` rows for this paint, account or
--   GUID) may wear it. mxbserver servers with `[paints] enforce_locks` refuse it to anyone
--   else, from the signed list at `GET /v1/servers/paint-locks`.
-- `rel_dest` is copied from the owner's loadout when the policy is written, so a lock keeps
--   naming the file after the live copy of the paint has been swept.
CREATE TABLE paint_policies (
  owner_account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  sha256           TEXT NOT NULL,
  rel_dest         TEXT NOT NULL,
  bike_id          TEXT NOT NULL DEFAULT '',
  view_only        INTEGER NOT NULL DEFAULT 0,
  locked           INTEGER NOT NULL DEFAULT 0,
  updated_at       INTEGER NOT NULL,
  PRIMARY KEY (owner_account_id, sha256)
);

CREATE INDEX paint_policies_sha ON paint_policies (sha256);
CREATE INDEX paint_policies_locked ON paint_policies (locked) WHERE locked = 1;
