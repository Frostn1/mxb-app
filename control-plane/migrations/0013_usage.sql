-- Anonymous usage counters, so feature decisions stop being guesses.
--
-- Nothing here joins to `accounts`, deliberately. Most people who run the app never claim an
-- invite, and an analytics table keyed on accounts would answer "how many enrolled riders
-- are there" — a number we already have and nobody asked for. The key is an install id the
-- app mints for itself: a random UUID in its `config.json`, tied to no person and to no
-- other row in this database.
--
-- Rollups rather than an event log. A row per event per install per *day* is enough to
-- answer every question this exists for, and it is bounded by how many people run the app
-- rather than by how long they leave it open.

-- One row per install per day. `sessions` and `minutes` are what separate "installed it"
-- from "uses it", which is the distinction the download count could never make.
CREATE TABLE usage_daily (
  install_id  TEXT NOT NULL,
  -- UTC, YYYY-MM-DD. Days are UTC everywhere here: a local-time day would make the
  -- boundary depend on where the player lives and no two rows would agree on when today
  -- started.
  day         TEXT NOT NULL,
  version     TEXT NOT NULL,
  os          TEXT NOT NULL,
  -- Which title the app was driving. Cheap to record now, and the only way to know whether
  -- the GP Bikes side is worth carrying.
  game        TEXT NOT NULL,
  sessions    INTEGER NOT NULL DEFAULT 0,
  minutes     INTEGER NOT NULL DEFAULT 0,
  first_seen  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (install_id, day)
);
CREATE INDEX usage_daily_day ON usage_daily (day);

-- One row per install per event per day, holding a count rather than a row per occurrence.
--
-- Keeping `install_id` in the key is the whole point: a feature's *reach* is
-- COUNT(DISTINCT install_id) and its *volume* is SUM(count), and only having both tells
-- "nobody opens the track studio" apart from "one person lives in it".
CREATE TABLE usage_events (
  day         TEXT NOT NULL,
  -- `area.thing` — see `isEventName`. A closed vocabulary in practice, but not enforced
  -- here: a shipped build that starts sending a name this schema has never heard of should
  -- have its data land, not be dropped.
  name        TEXT NOT NULL,
  install_id  TEXT NOT NULL,
  count       INTEGER NOT NULL DEFAULT 0,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (day, name, install_id)
);
CREATE INDEX usage_events_day_name ON usage_events (day, name);

-- The per-IP daily counter gains a `kind`, so usage posts are rate limited by the same
-- table that already limits signups instead of a second table meaning the same thing.
-- SQLite cannot widen a primary key in place, hence the copy.
CREATE TABLE device_claims_new (
  ip_digest   TEXT NOT NULL,
  day         TEXT NOT NULL,
  -- 'signup' (an account was minted) or 'usage' (a stats post). Defaulted so the existing
  -- rows, all of which are signups, carry their meaning across.
  kind        TEXT NOT NULL DEFAULT 'signup',
  claims      INTEGER NOT NULL DEFAULT 1,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (ip_digest, day, kind)
);
INSERT INTO device_claims_new (ip_digest, day, kind, claims, updated_at)
  SELECT ip_digest, day, 'signup', claims, updated_at FROM device_claims;
DROP TABLE device_claims;
ALTER TABLE device_claims_new RENAME TO device_claims;
