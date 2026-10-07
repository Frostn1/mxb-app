-- Public series on mxbsecure.com (`series.ts`).
--
-- A series is run in MSM (the MXB Servers desktop app), which scores its rounds from mxbserver
-- weekends and official servers' live timing. What lands here is the publishable result of that:
-- display names, points and places, never a GUID or Steam ID (the publish endpoint rejects any).
--
-- The slug is reserved by an admin on mxbsecure.com/admin, which mints the series' publish token.
-- Only the token's SHA-256 digest is stored, the same rule as account and rating tokens: a dump of
-- this table holds nothing that can publish.
CREATE TABLE series (
  slug              TEXT PRIMARY KEY,
  name              TEXT NOT NULL,
  token_hash        TEXT,
  token_issued_at   INTEGER,
  -- 0 until the first publish, and again after an unpublish: public reads see published rows only.
  published         INTEGER NOT NULL DEFAULT 0,
  classes           TEXT NOT NULL DEFAULT '[]',   -- JSON array of class names
  points_table      TEXT NOT NULL DEFAULT '[]',   -- JSON array of numbers
  drop_worst        INTEGER NOT NULL DEFAULT 0,
  registration_open INTEGER NOT NULL DEFAULT 1,
  next_round        TEXT,                         -- JSON { label, track, startsAt } or NULL
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER
);

CREATE UNIQUE INDEX series_token ON series (token_hash) WHERE token_hash IS NOT NULL;

-- One row per round as last published. A publish replaces the series' rounds wholesale: MSM is the
-- source of truth and may renumber rounds when it adds or removes one.
CREATE TABLE series_rounds (
  series_slug  TEXT NOT NULL REFERENCES series (slug) ON DELETE CASCADE,
  round_no     INTEGER NOT NULL,
  label        TEXT NOT NULL,
  track        TEXT NOT NULL,
  started_at   INTEGER,                          -- unix seconds, as MSM has it
  results      TEXT NOT NULL,                    -- JSON array of result rows
  PRIMARY KEY (series_slug, round_no)
);

-- The standings as of each publish. The public page reads the newest; older ones are kept (a few)
-- so a bad push can be looked at afterwards.
CREATE TABLE series_standings (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  series_slug  TEXT NOT NULL REFERENCES series (slug) ON DELETE CASCADE,
  standings    TEXT NOT NULL,                    -- JSON array of standing rows
  created_at   INTEGER NOT NULL
);

CREATE INDEX series_standings_latest ON series_standings (series_slug, id DESC);

-- Riders asking to race. Pending until the operator approves or rejects them from MSM; only
-- approved ones are shown publicly, and Discord names never are.
CREATE TABLE series_registrations (
  id           TEXT PRIMARY KEY,
  series_slug  TEXT NOT NULL REFERENCES series (slug) ON DELETE CASCADE,
  rider_name   TEXT NOT NULL,
  name_key     TEXT NOT NULL,                    -- lower(trim(rider_name)), one entry per name
  race_number  INTEGER NOT NULL,
  class        TEXT NOT NULL,
  team         TEXT,
  discord      TEXT,
  status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
  -- A keyed hash of the address the form came from, for the per-address cap only. Never the address.
  ip_hash      TEXT,
  created_at   INTEGER NOT NULL,
  decided_at   INTEGER,
  UNIQUE (series_slug, name_key)
);

CREATE INDEX series_registrations_ip ON series_registrations (ip_hash, created_at);
