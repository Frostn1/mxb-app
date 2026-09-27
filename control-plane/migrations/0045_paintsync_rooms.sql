-- Paint sync v2: automatic, per-server, delta-only (`paintsync.ts`).
--
-- `joined_at` orders a server's riders by who arrived first, which is the tie-break when two
-- riders wear different paints under one file name. Kept when a heartbeat re-reports the same
-- server, reset when the rider moves. Null on rows the older flow wrote.
ALTER TABLE presence ADD COLUMN joined_at INTEGER;

-- The names a server address has been seen under, so a rider who joined from the in-game
-- browser (the app sees only the name) lands in the same room as one the app sent there by
-- address, and a server that changes address can be traced back by its name. Swept after a
-- day, like everything else paint sync keeps.
CREATE TABLE paint_servers (
  name_key   TEXT NOT NULL,
  address    TEXT NOT NULL,
  name       TEXT NOT NULL,
  last_seen  INTEGER NOT NULL,
  PRIMARY KEY (name_key, address)
);

CREATE INDEX paint_servers_seen ON paint_servers (last_seen);
