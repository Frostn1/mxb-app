-- The last two seconds before a physics NaN, attached to the crash report that carries it.
--
-- FrostMod's nantrap=1 diagnostic keeps a ring of the player's bike state (wheel spin, tyre
-- sample radius, a sample point, ground gap, chassis speed) for every physics step, and writes
-- it to a small CSV when the NaN shows up. The crash report names that file as `nanRing`; the
-- app reads it and sends it in the same PUT as `nanRingCsv`.
--
-- One row per crash at most, keyed on the crash. ON DELETE CASCADE means the ring goes with
-- its crash: account erasure (which deletes client_crashes by account) and the retention sweep
-- take it without knowing it exists. Text, capped by the endpoint below D1's 2 MB row limit;
-- a ring of 4,000 rows is about 400 KB. Never public: read only through the admin crash view.
CREATE TABLE client_crash_rings (
  crash_id    INTEGER PRIMARY KEY REFERENCES client_crashes (id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  bytes       INTEGER NOT NULL,
  csv         TEXT NOT NULL
);
