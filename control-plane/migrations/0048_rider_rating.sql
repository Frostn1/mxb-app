-- Rider rating, Phase 1 (control-plane side).
--
-- Rates every one of Sean's managed servers (`managed_servers`, 0046/0047) — there is no
-- per-server opt-in here, unlike the community `servers` registry, because a managed row is
-- already Sean-operated by construction (0046's own comment: "never returned outside the
-- Steam-admin surface"). What a managed server needs beyond its `admin_token` is a *separate*
-- credential scoped to pushing race results — the admin token drives mxbserver's own admin
-- API (restart, config write, session), and a results push should not carry that power if the
-- token leaks from the box doing the pushing. So a second bearer, its own hash, its own issue
-- and rotate.
--
-- Everything below is keyed on the MX Bikes GUID (`0003_guid.sql`, `0038_guid_bans.sql`), not
-- on `accounts.id` — a rider races long before, and often without ever, creating an mxbsecure
-- account, and the rating has to exist independent of that link. `accounts.guid` is the join
-- when a signed-in rider wants to see their own numbers; nothing here requires it to be set.

-- A per-server credential for pushing results, beside (not replacing) the admin token that
-- already drives mxbserver's admin API. NULL until an admin issues one; issuing/rotating is
-- admin-only (`ratingtoken.ts`) and the plaintext is shown exactly once, the same rule
-- `auth.ts` already applies to account tokens.
ALTER TABLE managed_servers ADD COLUMN rating_token_hash TEXT;
ALTER TABLE managed_servers ADD COLUMN rating_token_issued_at INTEGER;

-- A digest is looked up directly, same as `accounts.token_hash` — one row can ever match.
CREATE UNIQUE INDEX managed_servers_rating_token ON managed_servers (rating_token_hash)
  WHERE rating_token_hash IS NOT NULL;

-- One race, ingested once. `id` is `server_id || ':' || event_id || ':' || race_id` so the
-- idempotency the ingest endpoint promises ("idempotent by server+event+race id") is a
-- primary-key conflict, not a second lookup that could race it.
CREATE TABLE ingested_races (
  id           TEXT PRIMARY KEY,
  server_id    TEXT NOT NULL REFERENCES managed_servers (id),
  event_id     TEXT NOT NULL,
  race_id      TEXT NOT NULL,
  track        TEXT,
  class        TEXT NOT NULL,
  session      TEXT NOT NULL,
  -- The server's own clock at the race, not our receipt time — what an admin reviewing
  -- history wants to sort by.
  occurred_at  INTEGER NOT NULL,
  -- How many human, non-bot riders were in this race, computed once at ingest so the 4+
  -- threshold (`rating.ts`) never has to be recomputed from `race_results` on every read.
  human_count  INTEGER NOT NULL,
  -- Whether this race actually fed a Glicko-2 update (the 4+ humans rule, §1/§3 of the
  -- design). A race below the threshold is still stored raw — "keep raw rows; never
  -- rating-only" — just never rated.
  rated        INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL,
  UNIQUE (server_id, event_id, race_id)
);

CREATE INDEX ingested_races_server ON ingested_races (server_id, occurred_at);
CREATE INDEX ingested_races_class ON ingested_races (class, occurred_at);

-- Every rider row out of the pushed result, raw — bots and spectators included, so the ban
-- on rating them can be audited later rather than only asserted.
CREATE TABLE race_results (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  race_row_id    TEXT NOT NULL REFERENCES ingested_races (id) ON DELETE CASCADE,
  guid           TEXT NOT NULL,
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
  -- Whether this individual rider's result fed the rating update: human, race rated overall,
  -- and >=50% of the race laps completed. A row can exist (raw) without ever counting.
  counted        INTEGER NOT NULL,
  -- Finish order used for the pairwise Glicko-2 update: classified riders by position, then
  -- DNF/DSQ riders ordered by laps completed, descending. NULL for a row that never counted.
  rank_order     INTEGER
);

CREATE INDEX race_results_race ON race_results (race_row_id);
CREATE INDEX race_results_guid ON race_results (guid, class);

-- One Glicko-2 triple per (rider, class) — never global (design §1).
CREATE TABLE rider_ratings (
  guid         TEXT NOT NULL,
  class        TEXT NOT NULL,
  rating       REAL NOT NULL,
  rd           REAL NOT NULL,
  volatility   REAL NOT NULL,
  races        INTEGER NOT NULL DEFAULT 0,
  last_race_at INTEGER,
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (guid, class)
);

CREATE INDEX rider_ratings_leaderboard ON rider_ratings (class, rating DESC);

-- The history chart on a profile page: one row per rider per rated race they were in.
CREATE TABLE rating_history (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  guid         TEXT NOT NULL,
  class        TEXT NOT NULL,
  race_row_id  TEXT NOT NULL REFERENCES ingested_races (id) ON DELETE CASCADE,
  rating_before REAL NOT NULL,
  rd_before     REAL NOT NULL,
  rating_after  REAL NOT NULL,
  rd_after      REAL NOT NULL,
  delta         REAL NOT NULL,
  created_at    INTEGER NOT NULL
);

CREATE INDEX rating_history_rider ON rating_history (guid, class, created_at);
