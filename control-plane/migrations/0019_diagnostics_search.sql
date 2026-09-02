-- Indexes for searching the diagnostics log rather than only reading the top of it.
--
-- The dashboard used to draw one query: the unaccounted files of the last N days, most
-- recent first. It now searches riders, searches files, and looks a file up backwards to
-- find who has loaded it, and those reads order by time across the whole table.
--
-- Nothing here changes what is stored. A substring search still scans — no index helps
-- `LIKE '%x%'` — but scanning is fine at this size and the ordering is not.

-- Files by recency, unfiltered by state. The existing (state, last_at) index only serves a
-- read that names a state, which the file search no longer does.
CREATE INDEX client_module_seen_last ON client_module_seen (last_at);

-- One rider's files, newest first. The primary key already finds the account; this is what
-- keeps the page from sorting the whole set to draw fifty rows.
CREATE INDEX client_module_seen_account ON client_module_seen (account_id, last_at);

-- The rider list is ordered by when each last reported, over every account that ever has.
CREATE INDEX client_modules_updated ON client_modules (updated_at);
