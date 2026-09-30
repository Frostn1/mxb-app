-- Native game hosts explicitly connected to the admin server manager.
-- Separate from `servers`: that table is the public/player-owned browser registry, while these
-- rows carry an operator credential and are never returned outside the Steam-admin surface.
CREATE TABLE managed_servers (
  id                  TEXT PRIMARY KEY,
  label               TEXT NOT NULL,
  provider            TEXT NOT NULL CHECK (provider IN ('aws-lightsail', 'ovh-vps')),
  region              TEXT NOT NULL,
  lifecycle           TEXT NOT NULL DEFAULT 'running' CHECK (lifecycle IN ('running', 'planned', 'retired', 'unknown')),
  game_endpoint       TEXT,
  agent_url           TEXT NOT NULL UNIQUE,
  agent_token         TEXT NOT NULL,
  deployment_revision TEXT NOT NULL,
  deployment_method   TEXT NOT NULL CHECK (deployment_method IN ('docker-compose', 'systemd')),
  game_port           INTEGER NOT NULL CHECK (game_port BETWEEN 1 AND 65535),
  created_at          INTEGER NOT NULL,
  updated_at          INTEGER NOT NULL
);
