-- Where MX Bikes died, across everyone it died on.
--
-- The game closes to desktop on its own: landing an overjump, hitting an object, clicking go
-- to track, and more often on a full server. None of it is ours and all of it is ours to
-- answer for, because the people it happens to are running our app when it does.
--
-- The whole of the evidence used to be one line in one player's log. That was enough to take
-- apart exactly one crash (a null deref in the game's GHS handle pool, fixed in FrostMod
-- v0.28.0) and it took a disassembler to do it, because one address was all there was. The
-- reason there was only one is that nothing collected the others.
--
-- FrostMod now writes every crash out as JSON beside its log, and the app posts it here. One
-- row per crash. The column that matters is `site` — "mxbikes.exe+0x11D753", the module and
-- offset of the faulting instruction — because that is the key the same bug shares across
-- every machine it happens on. Ranked by how many distinct accounts hit it, it says which
-- crash to spend a week on, which is the question nobody could answer before.
--
-- What is deliberately NOT here: the minidump. It is megabytes, it contains process memory,
-- and it stays on the player's machine until someone asks for it by name. This table holds
-- what is small and safe to send.
--
-- Everything in a row came off a client and is treated as such. `frames` and `trail` are
-- stored as JSON text and never interpreted here; the app validates their shape before they
-- arrive and the admin read renders them as strings.
CREATE TABLE client_crashes (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,

  -- Snapshots, so a row still reads as something after the account is renamed or gone. Not a
  -- join: who they were at the time is the fact.
  rider_name    TEXT NOT NULL DEFAULT '',
  guid          TEXT NOT NULL DEFAULT '',

  -- What faulted. `site` is module+RVA and is the grouping key for everything below.
  site          TEXT NOT NULL,
  kind          TEXT NOT NULL DEFAULT '',   -- 'access violation', and the rest
  code          TEXT NOT NULL DEFAULT '',   -- '0xC0000005'
  access        TEXT NOT NULL DEFAULT '',   -- 'reading' | 'writing' | 'executing' | ''
  target        TEXT NOT NULL DEFAULT '',   -- the address it was refused at, or ''

  -- Which binaries. A crash site only means anything against the build it is an offset into,
  -- and a fault that only ever appears on one app or FrostMod version is ours, not the game's.
  game          TEXT NOT NULL DEFAULT '',   -- 'mxbikes.exe'
  build         TEXT NOT NULL DEFAULT '',   -- the game build, as the app reads it
  frostmod      TEXT NOT NULL DEFAULT '',
  app_version   TEXT NOT NULL DEFAULT '',

  -- Where the player was. `place` rather than `where`, which SQLite would read as a keyword.
  -- `riders` is nullable on purpose: not in a race is not a grid of zero.
  place         TEXT NOT NULL DEFAULT '',
  in_session    INTEGER NOT NULL DEFAULT 0,
  track         TEXT NOT NULL DEFAULT '',
  server        TEXT NOT NULL DEFAULT '',
  riders        INTEGER,
  reloads       INTEGER NOT NULL DEFAULT 0,
  since_frame_ms  INTEGER,
  since_reload_ms INTEGER,
  uptime_ms     INTEGER NOT NULL DEFAULT 0,

  -- The evidence. JSON arrays as the client sent them, capped before they got here.
  frames        TEXT NOT NULL DEFAULT '[]',
  trail         TEXT NOT NULL DEFAULT '[]',
  -- Whether a minidump exists on that machine. The answer to "can we ask for more".
  has_dump      INTEGER NOT NULL DEFAULT 0,

  crashed_at    INTEGER NOT NULL,           -- the report's own UTC stamp
  received_at   INTEGER NOT NULL
);

-- The dashboard's first question: which crash is hitting the most people, most recently.
CREATE INDEX client_crashes_site ON client_crashes (site, received_at);
-- And its second: what happened to this rider.
CREATE INDEX client_crashes_account ON client_crashes (account_id, received_at);

-- The app sends a report file and renames it on success. A send that succeeded and whose
-- answer was lost on the way back is retried on the next look, and must not become a second
-- row — a crash counted twice is a crash that looks twice as common as it is.
CREATE UNIQUE INDEX client_crashes_once ON client_crashes (account_id, crashed_at, site);
