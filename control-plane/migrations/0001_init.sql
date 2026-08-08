-- MXB control plane, initial schema.
--
-- The shape here is driven by one constraint discovered the hard way: MX Bikes never
-- transmits paints. A remote rider renders using whatever local file matches the name they
-- picked, so paint sync has to be resolved by us — roster name -> account -> paint — and
-- every rider has to end up with the *same* file under the *same* name.

CREATE TABLE accounts (
  id           TEXT PRIMARY KEY,
  -- Must match the player's in-game rider name: it is the only identifier the game's
  -- roster gives us, so it is the join key between "who is on the server" and "what are
  -- they wearing". Unique case-insensitively, or two riders collide on the server.
  rider_name   TEXT NOT NULL,
  -- Null until Steam sign-in exists. Nullable rather than absent so adding Steam later is
  -- a backfill, not a migration of every row's identity.
  steam_id     TEXT UNIQUE,
  -- SHA-256 of the bearer token. The token itself is shown once at enrolment and never
  -- stored, so a database leak does not hand over live credentials.
  token_hash   TEXT NOT NULL UNIQUE,
  created_at   INTEGER NOT NULL
);
CREATE UNIQUE INDEX accounts_rider_name ON accounts (lower(rider_name));

-- Invite-only enrolment, standing in for Steam sign-in until there is an API key.
CREATE TABLE invites (
  code        TEXT PRIMARY KEY,
  note        TEXT,
  created_at  INTEGER NOT NULL,
  claimed_by  TEXT REFERENCES accounts (id),
  claimed_at  INTEGER
);

-- The server registry. `region` is here from the first migration deliberately: the fleet is
-- EU + US from day one, the app sorts by measured ping, and retrofitting a region column
-- across live rows and clients is far worse than carrying an unused one.
CREATE TABLE servers (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  region      TEXT NOT NULL,
  -- host:port, exactly the form the game's -directconnect flag takes.
  address     TEXT NOT NULL,
  -- Base URL of the mxb-agent on that host, if it has one.
  agent_url   TEXT,
  created_at  INTEGER NOT NULL
);

CREATE TABLE loadouts (
  account_id  TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  bike_id     TEXT,
  updated_at  INTEGER NOT NULL
);

-- One row per equipped slot. `file_name` is the canonical name every rider installs the
-- paint under — the sync is worthless if two people hold the same bytes under different
-- names, or different bytes under the same one.
CREATE TABLE loadout_paints (
  account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  slot        TEXT NOT NULL,
  file_name   TEXT NOT NULL,
  -- R2 object key. Content-addressed, so identical paints dedupe across every rider.
  sha256      TEXT NOT NULL,
  size        INTEGER NOT NULL,
  PRIMARY KEY (account_id, slot)
);
CREATE INDEX loadout_paints_sha ON loadout_paints (sha256);
