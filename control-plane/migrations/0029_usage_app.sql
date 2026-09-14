-- Which app a counter came from.
--
-- Frost's Studio and the MXB App share one config file (see `config::DATA_ID`), so they
-- share one install id — which is right: the id names a machine, and two apps on one machine
-- are not two machines. But the rollups were keyed on (install_id, day) alone, and the
-- upsert sets `version = excluded.version`. Two apps reporting on the same day would
-- overwrite each other's version on every flush, and their sessions and minutes would be
-- added together as if one app had been open twice as long. "Version now" — the panel that
-- answers "can I stop supporting 0.8.x" — would show whichever app flushed last.
--
-- So the app joins the key. Every existing row is the manager's: it is the only thing that
-- has ever reported. SQLite cannot widen a primary key in place, hence the copy.

CREATE TABLE usage_daily_new (
  install_id  TEXT NOT NULL,
  -- 'manager' (the MXB App) or 'studio' (Frost's Studio). Defaulted so a build that has not
  -- been updated to name itself still lands where it always did.
  app         TEXT NOT NULL DEFAULT 'manager',
  day         TEXT NOT NULL,
  version     TEXT NOT NULL,
  os          TEXT NOT NULL,
  game        TEXT NOT NULL,
  sessions    INTEGER NOT NULL DEFAULT 0,
  minutes     INTEGER NOT NULL DEFAULT 0,
  first_seen  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (install_id, app, day)
);
INSERT INTO usage_daily_new
    (install_id, app, day, version, os, game, sessions, minutes, first_seen, updated_at)
  SELECT install_id, 'manager', day, version, os, game, sessions, minutes, first_seen, updated_at
    FROM usage_daily;
DROP TABLE usage_daily;
ALTER TABLE usage_daily_new RENAME TO usage_daily;
CREATE INDEX usage_daily_day ON usage_daily (day);
-- The dashboard filters by app over a day range, which is the other way round from the
-- index above: the day alone would scan every app's rows to answer one app's question.
CREATE INDEX usage_daily_app_day ON usage_daily (app, day);

CREATE TABLE usage_events_new (
  day         TEXT NOT NULL,
  app         TEXT NOT NULL DEFAULT 'manager',
  name        TEXT NOT NULL,
  install_id  TEXT NOT NULL,
  count       INTEGER NOT NULL DEFAULT 0,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (day, app, name, install_id)
);
INSERT INTO usage_events_new (day, app, name, install_id, count, updated_at)
  SELECT day, 'manager', name, install_id, count, updated_at FROM usage_events;
DROP TABLE usage_events;
ALTER TABLE usage_events_new RENAME TO usage_events;
CREATE INDEX usage_events_day_name ON usage_events (day, name);
CREATE INDEX usage_events_app_day ON usage_events (app, day);
