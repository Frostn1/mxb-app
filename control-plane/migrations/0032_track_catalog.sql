-- Which catalogue product a server's track is, so the app's server tiles can show it.
--
-- A server names its track by an internal id and nothing else. The app sends us the ids it
-- doesn't have; an id we haven't seen is queued here and the cron looks it up on mxb-mods.com
-- (then the shop), keeping a copy of its picture in R2 under `trackart/<track_key>`. Players'
-- apps never ask the catalogue sites themselves.
--
-- Its own table: nothing else here is about tracks. `servers` is the registry of servers we
-- run, and `settings` is single values, not a row per track.
CREATE TABLE track_catalog (
  -- The id's letters and digits, lowercased: `Farm14`, `farm_14` and `Farm 14` are one row.
  track_key    TEXT PRIMARY KEY,
  -- The id as a server spelled it, which is what gets searched for.
  track_id     TEXT NOT NULL,
  -- 'mods' or 'shop' once found; '' while unknown or when nothing matched.
  source       TEXT NOT NULL DEFAULT '',
  -- 1 when the product's name is the track's; 0 when it only resembles it.
  exact        INTEGER NOT NULL DEFAULT 0,
  name         TEXT,
  url          TEXT,
  -- mxb-mods.com's post slug, which is what the app installs from.
  slug         TEXT,
  -- Where the picture came from, and whether R2 holds a copy.
  image_src    TEXT,
  has_image    INTEGER NOT NULL DEFAULT 0,
  -- JSON {currency, base, sale, free} for a shop product.
  price        TEXT,
  -- Last time an app asked about it. Rows nobody asks about for long are forgotten.
  requested_at INTEGER NOT NULL,
  checked_at   INTEGER NOT NULL DEFAULT 0,
  -- When to look it up (again). 0 means never looked up.
  due_at       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX track_catalog_due ON track_catalog (due_at);
CREATE INDEX track_catalog_requested ON track_catalog (requested_at);
