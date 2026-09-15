-- One server check per line instead of one per rider.
--
-- The front waiting rider reads the server's rider count (GETINFO) and sends it with their
-- heartbeat; everyone behind reads it back from theirs, so a full server gets one probe a beat
-- however long the line is. Columns on the reporter's own row rather than a table of counts:
-- the count lives exactly as long as someone in line is fresh, and dies with the row.
ALTER TABLE server_queue ADD COLUMN players INTEGER;
ALTER TABLE server_queue ADD COLUMN max_players INTEGER;
-- When the count was read. Older than a couple of beats and the line probes for itself again.
ALTER TABLE server_queue ADD COLUMN probed_at INTEGER;
