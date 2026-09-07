-- What the module list cannot describe.
--
-- The first version of this asked one question: what has the loader mapped into the game.
-- That describes a DLL which arrived by asking to be loaded, and nothing else — and asking
-- is optional. Code written straight into the process and started with a thread is in no
-- module list, has no path, and has no file to hash; a plugin sitting in the game's own
-- `plugins` folder is invisible until the game has actually loaded it.
--
-- The client now reports three more things, and none of them needs a new table:
--
--   * **Executable memory no loaded module covers.** Stored as a `client_module_seen` row
--     with origin `memory`, named by what the image says it is — its export name, or the
--     file name of the PDB the linker recorded — and keyed by a fingerprint of the PE
--     header fields rather than a hash of the bytes. The bytes are not the same twice: a
--     mapped image has had relocations applied against wherever it landed. The headers are.
--
--   * **Files in the game's `plugins` folder that are not loaded.** Origin `disk`. The
--     loaded ones are already a row, from the module list, so only the difference is stored
--     — and the difference is the point: a plugin the game refused, crashed on, or has not
--     started yet appears nowhere else.
--
--   * **Thread counts.** Three numbers on the live row: how many threads the game has, how
--     many started somewhere no module covers, and how many carry an armed hardware
--     breakpoint — the way to hook a function without altering a byte of it.
--
-- Everything reuses `module_rules`, the prevalence read, and the search, because everything
-- is the same shape: a name, a hash, and where it was seen. A rule that names a fingerprint
-- reads exactly like a rule that names a file hash.

-- The two counts worth a column of their own. A region is a row, so it needs no counter;
-- a thread is not, because a list of thread ids describes nothing anyone would read.
ALTER TABLE client_modules ADD COLUMN region_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE client_modules ADD COLUMN foreign_threads INTEGER NOT NULL DEFAULT 0;
ALTER TABLE client_modules ADD COLUMN breakpoints INTEGER NOT NULL DEFAULT 0;

-- One line about a row that is not a file: `rwx private image · thread` for a region,
-- `plugins folder, not loaded` for a file the game has not taken up. The columns from
-- migration 0017 all describe a file on disk and say nothing about a run of bytes in memory,
-- and filling them with something else would make every one of them mean two things.
ALTER TABLE client_module_seen ADD COLUMN detail TEXT NOT NULL DEFAULT '';
