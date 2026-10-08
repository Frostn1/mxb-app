-- Indexes for the catalogue's sort orders (`SORTS` in src/modapi.ts). Each leads with the
-- columns every listing filters on, then the order, so a page reads its 24 rows off the index.

-- Newest first: the expression is the one the query orders by, `NEWEST` in src/modapi.ts.
CREATE INDEX mod_assets_newest ON mod_assets (
  state, visibility, (COALESCE(published, strftime('%Y-%m-%dT%H:%M:%SZ', first_seen / 1000, 'unixepoch'))) DESC, id DESC
);

-- Recently updated.
CREATE INDEX mod_assets_modified ON mod_assets (state, visibility, modified DESC, id DESC);

-- Name A-Z, ignoring case.
CREATE INDEX mod_assets_title ON mod_assets (state, visibility, title COLLATE NOCASE, id);

-- Largest: a size is summed per row from the current version's files, so there is nothing to
-- index. The listing sorts what it already computes for each card.
