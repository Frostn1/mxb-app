-- What the game's own memory says about itself.
--
-- Everything diagnostics does so far identifies foreign code by what it *is*: a file hash, a
-- region fingerprint, a thread that started nowhere, a debug register. All of it describes
-- code that is present. None of it describes what that code *changed*, and a trainer that
-- writes one physics coefficient and unloads is a clean report — `worst_at` narrows the
-- window it has to hide in, but the write itself is never observed by any of it.
--
-- So: name a few runs of bytes the game loads once and never legitimately rewrites, have the
-- client hash them, and keep the answer a clean install gives. A report whose digest differs
-- is a report where something changed those bytes. It needs no idea what did — which is the
-- whole point, because it is the only signal here that fires on a trainer nobody has hashed.
--
-- Three things make this fit the pipeline that already exists rather than sit beside it:
--
--   * **The client still looks for nothing.** It fetches the region list, hashes what it is
--     told to, and reports digests. The baseline it is compared against is here. Nothing in
--     the shipped binary knows what any region is supposed to contain, for the same reason
--     nothing in it knows what `module_rules` says.
--
--   * **A deviation is a `client_module_seen` row like any other**, origin `state`, named for
--     its region and keyed by the digest that was seen. So `module_rules` reads it, the
--     prevalence column counts it, and the search finds it, exactly as migration 0020 did for
--     regions: a rule that names a digest reads like a rule that names a file hash.
--
--   * **A deviation is therefore `warn`, not `alert`.** Only a rule names a thing. This
--     matters more here than anywhere else: MX Bikes is a modding game, and a mod that
--     legitimately alters a physics table is indistinguishable at this layer from a trainer
--     that does. Prevalence is what tells them apart — a digest three hundred people share is
--     a popular mod and gets one `allow` row; a digest one person has is the interesting one.
--
-- The digest is deliberately **not** salted per report. A nonce would stop a trainer replaying
-- a captured clean digest, but it would also make every deviation unique per report, which
-- destroys both the prevalence read and the ability to write a rule about one. A client that
-- can intercept and rewrite its own report can lie about the module list too; that limit is
-- already stated and accepted. Keeping digests stable is worth more than defending a threat
-- the rest of the pipeline does not defend either.

-- Which bytes to hash, per build of the game, and what a clean install answers.
--
-- Baseline lives here as a column rather than in a table of its own: there is exactly one per
-- region, it is captured at the same time by the same person, and splitting it would buy a
-- history nobody reads at the cost of a join on the hot path.
CREATE TABLE state_regions (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  -- Which mxbikes.exe this describes, as the PE-header fingerprint the client already
  -- computes for mapped images. Reusing it rather than inventing a build id keeps the client
  -- from needing a second way to say which game it is looking at.
  --
  -- Baselines do not survive a game patch, and must not be allowed to: an unrecognised build
  -- has no rows here, so the manifest comes back empty, the client hashes nothing, and the
  -- report simply carries no digests. That is the correct answer to "we have not baselined
  -- this version yet" — not a silent pass, and emphatically not an alert on every player the
  -- day MX Bikes updates.
  build_fp   TEXT NOT NULL,
  -- 'physics-coefficients', 'unlock-flags'. Becomes the reported row's name as
  -- `state.<name>`, so it reads on the admin page as what it is.
  name       TEXT NOT NULL,
  rva        INTEGER NOT NULL,
  length     INTEGER NOT NULL,
  -- What a clean install answers. The fast path: equal means there is nothing to say, and no
  -- row is written. Only a difference becomes an observation.
  baseline   TEXT NOT NULL,
  -- Regions are retired rather than deleted, so a digest already in `client_module_seen` can
  -- still be explained by the row that produced it.
  active     INTEGER NOT NULL DEFAULT 1,
  -- How the baseline was taken, and by whom. A region whose baseline differs between two
  -- clean machines is not immutable and must be retired, not averaged — this is where that
  -- gets written down when it happens.
  note       TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);

-- One row per region per build, and the manifest read is "every active region for this
-- build", which is this index.
CREATE UNIQUE INDEX state_regions_unique ON state_regions (build_fp, name);
CREATE INDEX state_regions_build ON state_regions (build_fp, active);
