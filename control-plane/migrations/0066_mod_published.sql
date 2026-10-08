-- When a mod was published. A mirrored post: the source's own post date (`date_gmt`, UTC), not
-- when we first saw it. An upload: when it was uploaded. `modified` stays the last edit.
ALTER TABLE mod_assets ADD COLUMN published TEXT;

-- Uploads: their first_seen is the upload time.
UPDATE mod_assets SET published = strftime('%Y-%m-%dT%H:%M:%SZ', first_seen / 1000, 'unixepoch')
 WHERE source = 'upload';

CREATE INDEX mod_assets_published ON mod_assets (state, visibility, published);

-- Mirrored posts: the listing walk now carries `date_gmt`. Dropping its cursor makes the next
-- pass a full re-list (the same walk as the first one), which fills `published` for every
-- existing row without re-reading any post page.
DELETE FROM mirror_state WHERE key = 'listing';
