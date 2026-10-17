-- Link health for the mod catalogue's source links (`src/linkhealth.ts`).
--
-- The mirror holds a copy of only some files; for the rest, a download goes to the original
-- link, and those links die (Drive files removed or unshared, hosts gone). The cron checks each
-- link now and then, gently, and records what it found, so the site can order working links
-- first, mark dead ones and fall back past them.
--
--   link_state: 'ok' (the host answered with the file or its page), 'broken' (404/410, removed,
--               or not shared), or NULL (not checked yet, or a host that can't be checked
--               simply, such as MEGA, or one only the fetcher may reach).
--   link_checked_at: when it was last checked (ms); 0 never.
ALTER TABLE mod_files ADD COLUMN link_state TEXT;
ALTER TABLE mod_files ADD COLUMN link_checked_at INTEGER NOT NULL DEFAULT 0;
CREATE INDEX mod_files_link_due ON mod_files (link_checked_at) WHERE part = 0 AND url IS NOT NULL;
