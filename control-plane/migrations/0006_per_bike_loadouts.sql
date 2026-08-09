-- Give every bike its own loadout.
--
-- `loadouts` was keyed on the account alone and carried `bike_id` as an ordinary column, and
-- `loadout_paints` had no bike dimension at all — its primary key was (account_id, slot). One
-- account could therefore only ever hold one bike's paints, and publishing a second bike
-- deleted the first. A rider who set up two bikes in the app appeared correctly on whichever
-- they touched last and in default livery on the other.
--
-- SQLite cannot widen a primary key, so both tables are rebuilt. Existing rows keep whatever
-- bike they were stored under: the single `loadouts.bike_id` value, or '' where an older
-- client never sent one.

CREATE TABLE loadouts_new (
  account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  bike_id     TEXT NOT NULL,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (account_id, bike_id)
);

INSERT INTO loadouts_new (account_id, bike_id, updated_at)
  SELECT account_id, COALESCE(bike_id, ''), updated_at FROM loadouts;

-- `bike_id` defaults to '' so the migration can be applied while the *previous* Worker is
-- still serving: its INSERT names no bike column, and without a default every publish from
-- an already-running deployment would fail for the minute or two between the two commands.
-- Those rows land under '' and are replaced by the first real publish.
CREATE TABLE loadout_paints_new (
  account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  bike_id     TEXT NOT NULL DEFAULT '',
  slot        TEXT NOT NULL,
  file_name   TEXT NOT NULL,
  sha256      TEXT NOT NULL,       -- R2 object key
  size        INTEGER NOT NULL,
  rel_dest    TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (account_id, bike_id, slot)
);

INSERT INTO loadout_paints_new (account_id, bike_id, slot, file_name, sha256, size, rel_dest)
  SELECT p.account_id,
         COALESCE((SELECT l.bike_id FROM loadouts l WHERE l.account_id = p.account_id), ''),
         p.slot, p.file_name, p.sha256, p.size, p.rel_dest
    FROM loadout_paints p;

DROP TABLE loadout_paints;
DROP TABLE loadouts;
ALTER TABLE loadout_paints_new RENAME TO loadout_paints;
ALTER TABLE loadouts_new RENAME TO loadouts;

-- Dropped with the old table; the upload check reads it on every publish.
CREATE INDEX loadout_paints_sha ON loadout_paints (sha256);
-- The roster reads every rider's paints at once, which is a scan of this table by account.
CREATE INDEX loadout_paints_account ON loadout_paints (account_id);
