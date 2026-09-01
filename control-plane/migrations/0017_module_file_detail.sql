-- What a reported file says about itself.
--
-- The first version of client diagnostics carried a name, an origin and a hash, which
-- between them identify a file we already know about and say nothing at all about one we
-- don't. Every first sighting is a name nobody recognises, and the page filled up with rows
-- there was no way to read.
--
-- These seven columns are what the client can read off a file cheaply, and they split into
-- two kinds that must not be confused on the page:
--
--   * `trust` and `publisher` are Windows' answer, from `WinVerifyTrust` and the signing
--     certificate. Checked, and worth something: an overlay that belongs in a game process
--     is signed by a company whose name you know.
--   * `company`, `product` and `description` are the version resource — text the file
--     carries about itself, which anything can write and which nothing verifies. Useful for
--     recognising the ordinary, worthless as evidence about the unusual.
--
-- `size` and `mtime` are neither: they are facts about the file on that machine, and they
-- are what tells two builds of the same-named library apart when neither of them will hash.
--
-- Every column has a default, because the rows already in the table were written before any
-- of this existed and older clients keep reporting without it. An empty `trust` would be a
-- fourth state nobody has a name for, so absent reads as 'unchecked' — which is the same
-- thing the client says about a file it did not look at, and deliberately not 'unsigned'.
ALTER TABLE client_module_seen ADD COLUMN size INTEGER NOT NULL DEFAULT 0;
ALTER TABLE client_module_seen ADD COLUMN mtime INTEGER NOT NULL DEFAULT 0;
ALTER TABLE client_module_seen ADD COLUMN trust TEXT NOT NULL DEFAULT 'unchecked';
ALTER TABLE client_module_seen ADD COLUMN publisher TEXT NOT NULL DEFAULT '';
ALTER TABLE client_module_seen ADD COLUMN company TEXT NOT NULL DEFAULT '';
ALTER TABLE client_module_seen ADD COLUMN product TEXT NOT NULL DEFAULT '';
ALTER TABLE client_module_seen ADD COLUMN description TEXT NOT NULL DEFAULT '';

-- The unaccounted list is read grouped by file name now — one row per file, however many
-- people have loaded it — so the window scan is by name within the time window rather than
-- by recency alone. The existing `(state, last_at)` index still serves the window; this one
-- serves the grouping and the per-name signature summary that goes with it.
CREATE INDEX client_module_seen_name_state ON client_module_seen (name, state, last_at);
