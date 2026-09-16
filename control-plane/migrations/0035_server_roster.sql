-- A shared server book: the addresses, pooled, so the app's outage fallback works on day one.
--
-- MXB App already survives a dead master server. Discovery is the only thing the master is the
-- sole source of — a server answers `GETINFO` to anyone, with no account, no ticket and no
-- challenge — so the app keeps every address it has ever been told about and, when the master
-- won't answer, rebuilds the whole list by asking the servers themselves. That already works.
--
-- It works for the wrong people. The book is per-install and starts empty, so it is useless to
-- exactly whoever is worst affected by an outage: a fresh install, or anyone who hadn't opened
-- the Servers tab before it started. Those are the people in the Discord asking what happened.
-- Pooling the book fixes that, and the pooled book is also, exactly, the roster a fallback
-- master would serve if one is ever stood up.
--
-- ## Addresses and nothing else
--
-- No names, no locations, no operator text. Partly because a `GETINFO` reply carries the name,
-- the riders, the seats and the whole event blob, so storing a staler copy buys nothing. Mostly
-- because a public endpoint that accepts free text from anonymous clients and serves it back to
-- every install is a content-injection channel, and this feature does not need one.
--
-- ## Why corroboration exists
--
-- This list tells thousands of apps where to send UDP. An open "add any address" endpoint is a
-- reflection amplifier: one POST, and every MXB App in the world probes whoever was named. So
-- an address is only served once distinct reporters have independently seen it in the game's own
-- master list on the same day, and `isPublicGameAddress` refuses loopback, private space,
-- carrier NAT, link-local (where cloud metadata lives) and multicast before any of that.
CREATE TABLE server_roster (
  -- `host:port`, exactly as the game's connect flag takes it.
  address         TEXT PRIMARY KEY,
  first_seen      INTEGER NOT NULL,
  -- Bumped by every report. What the sweep ages out on.
  last_seen       INTEGER NOT NULL,
  -- When enough independent reporters agreed this address is real. NULL means it is held back:
  -- stored, but never served. Sticky once set — corroboration is a fact about the past, and an
  -- address that quietly stopped qualifying would drop out of everyone's book at once.
  corroborated_at INTEGER,
  -- Set when a server's own operator registered it, signed in. A person taking responsibility
  -- with an account behind them is corroboration on its own, and unlike a sighting it is
  -- auditable — which is the point of recording who rather than just that.
  owner_account   TEXT REFERENCES accounts(id) ON DELETE SET NULL,
  owner_at        INTEGER
);

CREATE INDEX server_roster_served ON server_roster (corroborated_at, last_seen);

-- One reporter's sighting of one address on one day.
--
-- The reporter is the same day-salted digest of the caller's address that open signup and the
-- usage counters already use — never the address itself, and deliberately not the install id.
-- Less identifying than an install id (it is a different value every day, so nothing here
-- follows anyone between days), and it is also the *right* key: "two distinct reporters" should
-- mean two distinct networks, not two UUIDs one machine could mint at will.
--
-- The day salt is why corroboration is counted **within a single day** rather than across the
-- window. The same network hashes differently tomorrow, so counting across days would let one
-- person clear the threshold by reporting twice — which is precisely the injection this exists
-- to prevent.
CREATE TABLE server_sightings (
  address   TEXT NOT NULL,
  reporter  TEXT NOT NULL,
  day       TEXT NOT NULL,
  seen_at   INTEGER NOT NULL,
  PRIMARY KEY (address, reporter, day)
);

-- Sightings are evidence, not history: once an address is corroborated they have done their job
-- and the sweep drops them. This index serves both the same-day count and that sweep.
CREATE INDEX server_sightings_day ON server_sightings (day, address);
