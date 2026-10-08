-- A mirrored post's own words and pictures, and a faster page backfill.
--
-- `body` is the post's description as data (`src/modbody.ts`): text blocks, marks, links and
-- YouTube ids, never HTML. `mod_asset_images` are the content images the sync copied into the
-- public bucket as `img/<sha256>.<ext>`, in page order; `src` is where each came from, so a
-- re-read copies only what changed.
--
-- `page_rev` is the page parser a row was last read with. A row below the sync's PAGE_REV is
-- read again, at a lower priority than new pages: that is how the rows indexed before this
-- migration get their pictures. `page_read_at` is when, for the backfill's rate log.
--
-- Page reads go through the `mxb-mirror` queue now: a leased row is `page_status = 'queued'`
-- with `page_due_at` as the lease's end.

ALTER TABLE mod_assets ADD COLUMN body TEXT;
ALTER TABLE mod_assets ADD COLUMN page_rev INTEGER NOT NULL DEFAULT 0;
ALTER TABLE mod_assets ADD COLUMN page_read_at INTEGER;

CREATE INDEX mod_assets_page_read ON mod_assets (page_read_at);

CREATE TABLE mod_asset_images (
  asset_id   INTEGER NOT NULL REFERENCES mod_assets (id) ON DELETE CASCADE,
  idx        INTEGER NOT NULL,
  sha256     TEXT NOT NULL,
  src        TEXT NOT NULL,
  width      INTEGER,
  height     INTEGER,
  PRIMARY KEY (asset_id, idx)
);
CREATE INDEX mod_asset_images_sha ON mod_asset_images (sha256);
