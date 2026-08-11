-- Who is on which server, so a roster can mean "this grid" rather than "everyone".
--
-- `roster` has always taken a `?server=` and ignored it, because nothing knew where anyone
-- was. That was survivable while the platform was one invite-only group; it is wrong as a
-- design. Every rider downloads the paints of every other enrolled rider, most of whom they
-- will never share a session with.
--
-- Reported by each player's own app, which already knows the server it launched into — the
-- dedicated server's log knows too, but the agent has no way to tell us yet, and waiting for
-- that would leave the scoping unbuilt.
--
-- Deliberately not a column on `accounts`: presence is short-lived and rewritten constantly,
-- identity is neither, and keeping them apart leaves room for the agent to report the same
-- thing later without touching the account row.
CREATE TABLE presence (
  account_id  TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  -- The registry id, or a normalized host:port for a server we do not run. Matches the key
  -- the app already computes with `server_key_for`.
  server_id   TEXT NOT NULL,
  -- Refreshed on every heartbeat. A row older than the cutoff is treated as gone: an app
  -- that crashed or a player who quit should not haunt a grid forever.
  updated_at  INTEGER NOT NULL
);

CREATE INDEX presence_server ON presence (server_id, updated_at);
