-- The mod catalogue: mods people can search and download from mxbsecure.com and the MXB App.
--
-- Two sources feed one set of tables:
--   * `mirror`: mxb-mods.com's catalogue, walked and copied into R2 by the sync (`mirror.ts`,
--     `mirrorfetch.ts`). The starting catalogue.
--   * `upload`: mods riders upload themselves from the MXB App (`uploads.ts`). The main event.
--
-- An asset is a mod. It has versions; a new upload of the same mod, or a changed download list
-- on the mirrored page, is a new version of the same asset rather than a new asset. A version
-- has files; a file is a blob in R2, keyed by SHA-256, so the same archive linked or uploaded
-- twice is stored once.

CREATE TABLE mod_assets (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  -- 'mirror' | 'upload'
  source        TEXT NOT NULL,
  -- For a mirrored asset, the source's post id. Null for an upload.
  source_ref    INTEGER UNIQUE,
  -- For an upload, the account that owns it. Null for a mirrored asset.
  owner_account TEXT REFERENCES accounts (id),
  -- 'public' (searchable) | 'unlisted' (reachable by id or link only)
  visibility    TEXT NOT NULL DEFAULT 'public',
  -- 'active' | 'hidden' (moderation, reversible) | 'removed' (moderation, files deleted)
  -- | 'deleted' (by its owner)
  state         TEXT NOT NULL DEFAULT 'active',
  slug          TEXT,
  title         TEXT NOT NULL,
  author        TEXT,
  -- paints | bikes | liveries | kits | tracks | other
  type          TEXT NOT NULL,
  -- Bike/manufacturer terms, '; ' separated.
  bike          TEXT NOT NULL DEFAULT '',
  -- Category names, '; ' separated. Searched, never shown raw.
  categories    TEXT NOT NULL DEFAULT '',
  description   TEXT NOT NULL DEFAULT '',
  thumb_src     TEXT,
  thumb_sha     TEXT,
  -- Where it came from: the mirrored post, or null for an upload.
  source_url    TEXT,
  -- The source's own `modified` for a mirrored post (site-local time); an ISO time for an upload.
  modified      TEXT NOT NULL,
  -- The version shown and downloaded by default.
  current_version INTEGER,
  first_seen    INTEGER NOT NULL,
  last_seen     INTEGER NOT NULL,
  -- Mirror only: whether the post's page has been read: due | ok | retry | robots | gone.
  page_status   TEXT NOT NULL DEFAULT 'ok',
  page_attempts INTEGER NOT NULL DEFAULT 0,
  page_due_at   INTEGER NOT NULL DEFAULT 0,
  page_error    TEXT,
  -- Open reports, so the moderation queue sorts without a join.
  reports_open  INTEGER NOT NULL DEFAULT 0,
  moderated_at  INTEGER,
  moderated_by  TEXT,
  moderation_note TEXT
);
CREATE INDEX mod_assets_page_due ON mod_assets (page_status, page_due_at);
CREATE INDEX mod_assets_list ON mod_assets (state, visibility, type, modified);
CREATE INDEX mod_assets_owner ON mod_assets (owner_account);
CREATE INDEX mod_assets_seen ON mod_assets (last_seen);

CREATE TABLE mod_versions (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  asset_id    INTEGER NOT NULL REFERENCES mod_assets (id) ON DELETE CASCADE,
  -- 1, 2, 3 … within the asset.
  seq         INTEGER NOT NULL,
  -- What the author calls it ("v2", "Beta 19").
  label       TEXT,
  notes       TEXT,
  -- 'quarantine' (uploaded, checks not passed yet) | 'live' | 'rejected'
  state       TEXT NOT NULL DEFAULT 'live',
  created_at  INTEGER NOT NULL,
  -- An account id, or 'mirror'.
  created_by  TEXT NOT NULL,
  UNIQUE (asset_id, seq)
);

-- A version's files. For a mirrored version, one row per download option on the page
-- (`part` 0), and when that option is a folder share, one per file in it (`part` 1..n, `rel`
-- its path inside the folder). An upload is one file, `idx` 0 `part` 0.
CREATE TABLE mod_files (
  version_id   INTEGER NOT NULL REFERENCES mod_versions (id) ON DELETE CASCADE,
  idx          INTEGER NOT NULL,
  part         INTEGER NOT NULL DEFAULT 0,
  rel          TEXT,
  -- The source link (mirror). Null for an upload.
  url          TEXT,
  host         TEXT NOT NULL DEFAULT '',
  label        TEXT,
  is_server    INTEGER NOT NULL DEFAULT 0,
  is_default   INTEGER NOT NULL DEFAULT 0,
  -- pending | queued | done | folder | retry | runner | failed
  --   folder: a folder share, expanded into its parts.
  --   runner: larger than a Worker may stream (`mirrorfetch.ts` MAX_WORKER_BYTES), for the runner.
  --   failed: retries exhausted, or a permanent refusal. A changed link starts over.
  status       TEXT NOT NULL DEFAULT 'pending',
  attempts     INTEGER NOT NULL DEFAULT 0,
  due_at       INTEGER NOT NULL DEFAULT 0,
  leased_until INTEGER NOT NULL DEFAULT 0,
  sha256       TEXT,
  filename     TEXT,
  error        TEXT,
  fetched_at   INTEGER,
  PRIMARY KEY (version_id, idx, part)
);
CREATE INDEX mod_files_due ON mod_files (status, due_at);
CREATE INDEX mod_files_sha ON mod_files (sha256);

-- One distinct file body. `bucket` is 'public' (mxb-assets, served by cdn.mxbsecure.com) or
-- 'private' (mxb-private: `.mxbsecure` locked content, signed links only).
CREATE TABLE mod_blobs (
  sha256       TEXT PRIMARY KEY,
  bucket       TEXT NOT NULL,
  r2_key       TEXT NOT NULL,
  size         INTEGER NOT NULL,
  content_type TEXT,
  filename     TEXT,
  first_seen   INTEGER NOT NULL
);

-- An upload in flight: an R2 multipart upload the app writes to directly, through presigned
-- part URLs, into the private bucket's `quarantine/` prefix. Nothing there is ever served.
CREATE TABLE mod_uploads (
  id           TEXT PRIMARY KEY,
  account_id   TEXT NOT NULL REFERENCES accounts (id),
  -- A new version of this asset; null for a new asset.
  asset_id     INTEGER REFERENCES mod_assets (id),
  r2_key       TEXT NOT NULL,
  r2_upload_id TEXT NOT NULL,
  filename     TEXT NOT NULL,
  -- 'pkz' | 'zip' | 'pnt'
  kind         TEXT NOT NULL,
  size         INTEGER NOT NULL,
  sha256       TEXT NOT NULL,
  part_size    INTEGER NOT NULL,
  -- JSON: title, type, bike, description, visibility, version label, notes.
  meta         TEXT NOT NULL,
  -- open | verifying (queued for its check) | checking | live | rejected | aborted | expired
  state        TEXT NOT NULL DEFAULT 'open',
  error        TEXT,
  version_id   INTEGER,
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL,
  finished_at  INTEGER
);
CREATE INDEX mod_uploads_account ON mod_uploads (account_id, created_at);
CREATE INDEX mod_uploads_state ON mod_uploads (state, expires_at);

CREATE TABLE mod_reports (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  asset_id      INTEGER NOT NULL REFERENCES mod_assets (id) ON DELETE CASCADE,
  -- The reporting account, when the report came with one.
  reporter      TEXT,
  -- A keyed daily digest of the reporter's address (`voice.ts` `ipDigest`), for the cap.
  reporter_ip   TEXT,
  -- broken | stolen | malware | offensive | other
  reason        TEXT NOT NULL,
  details       TEXT,
  created_at    INTEGER NOT NULL,
  resolved_at   INTEGER,
  -- dismissed | hidden | removed
  resolution    TEXT,
  resolved_by   TEXT
);
CREATE INDEX mod_reports_open ON mod_reports (resolved_at, created_at);
CREATE INDEX mod_reports_asset ON mod_reports (asset_id);

-- The sync's cursors and cached source facts (robots.txt, the category tree), as JSON.
CREATE TABLE mirror_state (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- Full-text search. External content: the text lives in `mod_assets` once; the triggers keep
-- the index in step.
CREATE VIRTUAL TABLE mod_fts USING fts5 (
  title, author, bike, categories, description,
  content = 'mod_assets',
  content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER mod_assets_ai AFTER INSERT ON mod_assets BEGIN
  INSERT INTO mod_fts (rowid, title, author, bike, categories, description)
  VALUES (new.id, new.title, new.author, new.bike, new.categories, new.description);
END;

CREATE TRIGGER mod_assets_ad AFTER DELETE ON mod_assets BEGIN
  INSERT INTO mod_fts (mod_fts, rowid, title, author, bike, categories, description)
  VALUES ('delete', old.id, old.title, old.author, old.bike, old.categories, old.description);
END;

CREATE TRIGGER mod_assets_au AFTER UPDATE OF title, author, bike, categories, description ON mod_assets BEGIN
  INSERT INTO mod_fts (mod_fts, rowid, title, author, bike, categories, description)
  VALUES ('delete', old.id, old.title, old.author, old.bike, old.categories, old.description);
  INSERT INTO mod_fts (rowid, title, author, bike, categories, description)
  VALUES (new.id, new.title, new.author, new.bike, new.categories, new.description);
END;
