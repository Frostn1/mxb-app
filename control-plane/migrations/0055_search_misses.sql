-- Searches in the app's Browse that found nothing, so missing catalog sources show up.
--
-- Anonymous by construction: one row per (day, query, game) holding a count. There is no
-- install id, no account, no Steam id, no IP and no per-request timestamp — the day is the
-- finest time there is. The sweep (`pruneSearchMisses`) drops rows after 90 days.
CREATE TABLE search_misses (
  day    TEXT NOT NULL,
  query  TEXT NOT NULL,
  game   TEXT NOT NULL DEFAULT '',
  misses INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (day, query, game)
);
CREATE INDEX search_misses_day ON search_misses (day);
