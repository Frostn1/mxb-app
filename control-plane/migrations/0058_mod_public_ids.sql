-- Public ids, pictures and bylines for the mod catalogue.
--
-- `mod_assets.id` is an autoincrement: anyone could count through it, and through the unlisted
-- mods with it. Outside D1 a mod is known only by `public_id`, a random UUID (v4). The integer
-- stays the internal key every join uses.
--
-- `thumb_key` is the picture's key in the public bucket (`thumbs/<sha256>.<ext>`), served by
-- cdn.mxbsecure.com.

ALTER TABLE mod_assets ADD COLUMN public_id TEXT;
ALTER TABLE mod_assets ADD COLUMN thumb_key TEXT;

-- A v4 UUID out of SQLite's own randomness, for the rows that exist and for any insert that
-- doesn't name one (the code always does, with `crypto.randomUUID()`).
UPDATE mod_assets SET public_id =
  lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
  || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))
WHERE public_id IS NULL;

CREATE UNIQUE INDEX mod_assets_public_id ON mod_assets (public_id);

CREATE TRIGGER mod_assets_public_id AFTER INSERT ON mod_assets WHEN NEW.public_id IS NULL BEGIN
  UPDATE mod_assets SET public_id =
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))), 2) || '-'
    || substr('89ab', 1 + (abs(random()) % 4), 1) || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))
  WHERE id = NEW.id;
END;

-- Read every mirrored page again that is missing its picture, byline or files: the next cron
-- runs fill them in.
UPDATE mod_assets SET page_status = 'due', page_due_at = 0, page_attempts = 0
WHERE source = 'mirror' AND page_status IN ('ok', 'retry')
  AND (thumb_key IS NULL OR author IS NULL OR current_version IS NULL);
