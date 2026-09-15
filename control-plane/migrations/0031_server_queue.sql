-- The line for a full server, so riders using the app stop spamming Join.
--
-- The control plane only keeps the order. Each waiting app reads the server's own rider
-- count (GETINFO) and launches the game when the slots free up cover everyone ahead of it.
--
-- Its own table rather than columns on `presence`: presence means "I am on this server" and
-- every roster, count and voice gate reads it that way. A rider in line is on no server yet,
-- and the line forgets people in seconds where presence waits minutes.
CREATE TABLE server_queue (
  account_id  TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  -- Normalized host:port, the key the app computes with `server_key_for`.
  server_id   TEXT NOT NULL,
  -- Place in line. Kept across heartbeats to the same server, reset on a new one.
  joined_at   INTEGER NOT NULL,
  -- Heartbeat. A row gone quiet past the TTL has left the line.
  updated_at  INTEGER NOT NULL,
  -- When the app launched into a slot. Still counts as ahead for a grace period, because
  -- the server doesn't count a rider who is still loading.
  launched_at INTEGER
);

CREATE INDEX server_queue_line ON server_queue (server_id, joined_at);
