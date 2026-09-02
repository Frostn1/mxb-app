-- What the server a rider is on is actually called, so the app can show a list of them.
--
-- Presence has only ever been a key: `room_key(server_name)` for a rider in a session, a
-- registry id or a `host:port` for one who joined by address. That is enough to scope a
-- roster — everyone on one grid computes the same key — and nothing at all to put in front
-- of a player. "frost racing eu" is a fold, not a name, and it carries no track and no
-- rider count.
--
-- These three are what the app already reads out of the running game through FrostMod, so
-- they cost nothing to report and they are what makes a server browser possible: a live
-- list of where people actually are, built out of what the people there can see.
--
-- Nullable, and stay that way: presence is written by two paths — the standalone report and
-- the roster's `?here=1` — and only the ones that can see a game session have anything to
-- put here. A row without them is still a rider on a server.
ALTER TABLE presence ADD COLUMN server_name TEXT;
ALTER TABLE presence ADD COLUMN track TEXT;
ALTER TABLE presence ADD COLUMN riders INTEGER;
