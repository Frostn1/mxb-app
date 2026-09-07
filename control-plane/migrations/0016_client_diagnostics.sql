-- What is loaded inside a player's running game, so a session can be explained after it ends.
--
-- The app already reports presence and usage; this is the same idea applied to the game
-- process itself. The client walks the module list of the running game and sends what it
-- finds — file name, where it was loaded from, and a hash for anything that is not a Windows
-- system library. It sends observations and nothing else: every judgement about what an
-- observation *means* is made here, against `module_rules`, so the shipped binary carries no
-- opinion about any file and there is nothing in it to read.
--
-- Three tables, because they answer three different questions:
--
--   * `client_modules`   — what is true right now. One row per account, rewritten on every
--                          report, exactly like `presence`. Live state, not history.
--   * `client_module_seen` — what has ever been seen, per account and per file. Append-only
--                          in spirit (an upsert that bumps `last_at` and `hits`), because
--                          the live row forgets and the interesting observation is usually
--                          the one that was true ten minutes ago.
--   * `module_rules`     — how to read all of it. Editable at runtime from the admin page,
--                          so a new entry takes effect on the next report from every client
--                          without a release, a deploy, or a commit.
--
-- A client that is not running the app has no row. So a read answers "what have we been
-- told", never "what is true of everyone" — a missing row is the ordinary case.

-- Live state, one row per account.
CREATE TABLE client_modules (
  account_id    TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  -- 'ok' | 'warn' | 'alert' | 'unknown'. Text rather than an integer so a row read straight
  -- out of the database says what it means. 'unknown' is its own state and never means
  -- 'ok': it is what a client reports when it could not read the module list at all.
  state         TEXT NOT NULL,
  -- Highest `module_rules.id` in force when this was classified. A row read as 'ok' against
  -- an empty rule list means much less than one read against the current list, and the two
  -- are only distinguishable if the version travels with the verdict.
  rules_version INTEGER NOT NULL,
  -- How many modules were in the list, and how many of those came from outside the game,
  -- the system, and anything the app installed. The second number is the one worth reading.
  module_count  INTEGER NOT NULL,
  unknown_count INTEGER NOT NULL,
  -- JSON array of {name,sha256,label} for rule-matched modules only.
  matched       TEXT NOT NULL,
  -- The worst state reported inside the freshness window, and when.
  --
  -- Without it the whole thing is defeated by unloading: load something for one lap, unload
  -- it, and the next report overwrites 'alert' with 'ok' as if nothing happened. The live
  -- read only surfaces this while `worst_at` is inside the same window presence uses, so it
  -- describes the current session. `client_module_seen` is what remembers past that.
  worst_state   TEXT NOT NULL,
  worst_at      INTEGER NOT NULL,
  -- Which build reported. A state that only ever appears on one app version is a bug in the
  -- app before it is anything about the player.
  app_version   TEXT NOT NULL DEFAULT '',
  updated_at    INTEGER NOT NULL
);

-- Everything that has been seen loaded in anyone's game, per account and per file.
--
-- Keyed by account + name + hash, so a file that loads on every launch is one row with a
-- rising `hits` rather than a thousand rows. System libraries are deliberately not recorded
-- here: there are hundreds of them, they are the same on every machine, and they would bury
-- the handful of rows that are worth looking at.
CREATE TABLE client_module_seen (
  account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  -- Empty when the client could not read the file (deleted mid-session, or locked).
  sha256      TEXT NOT NULL,
  -- 'game' | 'app' | 'other' — where it was loaded from, as the client saw it.
  origin      TEXT NOT NULL,
  -- What the rules made of it the last time it was seen.
  state       TEXT NOT NULL,
  label       TEXT NOT NULL DEFAULT '',
  -- Snapshots, so a row still reads as something after the session and the presence row are
  -- both gone. Not a join: who they were and where they were at the time is the fact.
  rider_name  TEXT NOT NULL DEFAULT '',
  server_id   TEXT NOT NULL DEFAULT '',
  first_at    INTEGER NOT NULL,
  last_at     INTEGER NOT NULL,
  hits        INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (account_id, name, sha256)
);

-- The admin view opens on "what is not ok, most recent first", and the prevalence column
-- asks "how many other accounts have ever loaded this file" — a file seen on one machine out
-- of hundreds is the interesting one, whatever any rule says about it.
CREATE INDEX client_module_seen_state ON client_module_seen (state, last_at);
CREATE INDEX client_module_seen_name ON client_module_seen (name, sha256);

-- How to read an observation.
--
-- `deny` names something that should not be in a game process; `allow` says a file is fine
-- wherever it loads from, which is how a false positive is silenced. A rule matches by hash
-- or by lowercase name substring, never both — an entry needing each is two rules sharing a
-- label. `id` is monotone and its maximum is the rules version reported beside every state,
-- which is why this table is never rewritten in place.
CREATE TABLE module_rules (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  kind       TEXT NOT NULL,
  pattern    TEXT NOT NULL DEFAULT '',
  sha256     TEXT NOT NULL DEFAULT '',
  label      TEXT NOT NULL DEFAULT '',
  note       TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX module_rules_unique ON module_rules (kind, pattern, sha256);
