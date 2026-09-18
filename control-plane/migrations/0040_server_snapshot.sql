-- The shared snapshot: what the servers were doing a minute ago, so a tab opens with a list in
-- it instead of a spinner.
--
-- `server_roster` (0035) answers "what servers exist" and carries addresses only, on purpose.
-- This answers "what were they doing", which needs the operator's own text — a name, a track, a
-- location — and that is the reason it is a separate thing with separate rules rather than more
-- columns on the roster.
--
-- ## Why this is safe to serve back
--
-- A row is stored only if its address is one the roster already serves, which means distinct
-- networks have independently seen it in the game's own master list. So nothing here can put a
-- *new* address in front of anybody: the join button goes to the address, and the address was
-- already corroborated. The text is stripped of control characters and length-capped on the way
-- in, and the app draws the snapshot's age beside it and replaces the whole thing with its own
-- sweep seconds later.
--
-- ## Why one row
--
-- A snapshot is a whole list, and the only one worth having is the latest. Keeping it as one
-- JSON payload is what makes the write a single statement and the read a single lookup, cached
-- at the edge — rather than several hundred rows rewritten every minute by every install that
-- has the tab open.
CREATE TABLE server_snapshot (
  -- Always 'live'. The column exists so the row has a key, not because there will be others.
  id         TEXT PRIMARY KEY,
  -- The rows as JSON, exactly as they are served.
  payload    TEXT NOT NULL,
  -- How many rows are in it, so the admin surfaces can read the size without parsing.
  servers    INTEGER NOT NULL,
  -- When this snapshot was true. Served alongside it, and what the once-a-minute write gate and
  -- the staleness cut-off both read.
  updated_at INTEGER NOT NULL
);
