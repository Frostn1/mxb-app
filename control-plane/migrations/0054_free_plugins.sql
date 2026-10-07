-- Free plugins.
--
-- A free plugin still goes through the signed license, so the app's install and run checks
-- (bundle hash, account, refresh) are the same for free and paid. The difference is only
-- who gets one: every signed-in account, with no key, renewed on each check-in.

ALTER TABLE plugins ADD COLUMN free INTEGER NOT NULL DEFAULT 0;

UPDATE plugins
   SET free = 1,
       name = 'MXB Replay',
       summary = 'Camera paths for your replays: keys, cuts and a timeline, recorded as a video by Frost''s Studio.'
 WHERE id = 'replaycam';
