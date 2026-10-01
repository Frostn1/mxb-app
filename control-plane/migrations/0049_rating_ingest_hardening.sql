-- Rider rating ingest hardening.
--
-- Only riders whose GUID is bound to an account through the MXB App's claim_guid path
-- (`accounts.guid`) are rated, but a pushed race can carry a rider with no GUID at all, or one
-- nobody has claimed. Those rows are stored raw (the audit trail is the point) and simply never
-- rated, so `race_results.guid` has to be nullable. SQLite cannot drop NOT NULL in place, so the
-- table is rebuilt; nothing references it.
CREATE TABLE race_results_new (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  race_row_id    TEXT NOT NULL REFERENCES ingested_races (id) ON DELETE CASCADE,
  guid           TEXT,
  name           TEXT NOT NULL,
  race_num       INTEGER,
  class          TEXT NOT NULL,
  position       INTEGER,
  classified     INTEGER NOT NULL,
  dnf            INTEGER NOT NULL,
  dsq            INTEGER NOT NULL,
  laps_completed INTEGER NOT NULL,
  race_laps      INTEGER NOT NULL,
  is_bot         INTEGER NOT NULL,
  counted        INTEGER NOT NULL,
  rank_order     INTEGER
);

INSERT INTO race_results_new
  (id, race_row_id, guid, name, race_num, class, position, classified, dnf, dsq,
   laps_completed, race_laps, is_bot, counted, rank_order)
SELECT id, race_row_id, guid, name, race_num, class, position, classified, dnf, dsq,
       laps_completed, race_laps, is_bot, counted, rank_order
  FROM race_results;

DROP TABLE race_results;
ALTER TABLE race_results_new RENAME TO race_results;

CREATE INDEX race_results_race ON race_results (race_row_id);
CREATE INDEX race_results_guid ON race_results (guid, class);
