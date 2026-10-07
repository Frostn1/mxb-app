-- User server deploy (servers.mxbsecure.com): invited Steam accounts deploy a server of their
-- own into a slot on an OVH VPS the control plane orders on demand. See src/hosting.ts.
--
-- Keyed by SteamID64 rather than `accounts.id`: the site's session is a Steam sign-in and has
-- no app account behind it.

-- Single-use invites. Only the SHA-256 of the code is kept; the code itself is shown once.
CREATE TABLE host_invites (
  id          TEXT PRIMARY KEY,
  code_hash   TEXT NOT NULL UNIQUE,
  created_by  TEXT NOT NULL,           -- operator SteamID64
  steam_id    TEXT,                    -- bound to one account, or NULL for anyone
  quota       INTEGER NOT NULL DEFAULT 1,
  expires_at  INTEGER NOT NULL,
  claimed_by  TEXT,
  claimed_at  INTEGER,
  revoked_at  INTEGER,
  created_at  INTEGER NOT NULL
);

-- Who may deploy, and how many servers at once.
CREATE TABLE host_users (
  steam_id    TEXT PRIMARY KEY,
  quota       INTEGER NOT NULL DEFAULT 1,
  invite_id   TEXT,
  suspended   INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL
);

-- One OVH VPS. `state`: ordering -> delivering -> rebuilding -> installing -> ready, then
-- draining -> flagged -> cancelled; failed on the way. Every state but cancelled and failed
-- is billed and counts against the spend cap.
CREATE TABLE host_boxes (
  id               TEXT PRIMARY KEY,
  pool             TEXT NOT NULL CHECK (pool IN ('native', 'legacy')),
  region           TEXT NOT NULL,
  datacenter       TEXT NOT NULL,
  plan_code        TEXT NOT NULL,
  state            TEXT NOT NULL,
  price_eur        REAL NOT NULL,       -- what the spend cap counted it at
  quoted_price     REAL,                -- OVH's checkout preview, in its own currency
  quoted_currency  TEXT,
  ovh_order_id     INTEGER,
  ovh_service      TEXT,
  ip               TEXT,
  slots_total      INTEGER NOT NULL,
  agent_token      TEXT,                -- legacy only: the box's one mxb-agent token
  renews_at        INTEGER,
  stage_at         INTEGER NOT NULL,    -- when `state` last changed
  empty_since      INTEGER,
  flagged_at       INTEGER,
  cancelled_at     INTEGER,
  last_error       TEXT,
  install_log      TEXT,
  created_at       INTEGER NOT NULL
);
CREATE INDEX host_boxes_place ON host_boxes (pool, region, state);

-- One game server on a box. Native: an `mxbserver@sN` unit with its own control token.
-- Legacy: an mxb-agent instance `sN`.
CREATE TABLE host_slots (
  id          TEXT PRIMARY KEY,
  box_id      TEXT NOT NULL REFERENCES host_boxes(id),
  idx         INTEGER NOT NULL,
  game_port   INTEGER NOT NULL,
  token       TEXT,                     -- native only
  server_id   TEXT,                     -- the hosted server in it, or NULL when free
  UNIQUE (box_id, idx)
);

-- A user's server. `box_id` is set as soon as a box is chosen or ordered for it; `slot_id`
-- once it has one.
CREATE TABLE host_servers (
  id             TEXT PRIMARY KEY,
  steam_id       TEXT NOT NULL,
  name           TEXT NOT NULL,
  type           TEXT NOT NULL CHECK (type IN ('mxbserver', 'legacy')),
  region         TEXT NOT NULL,
  box_id         TEXT,
  slot_id        TEXT,
  state          TEXT NOT NULL,         -- waiting | ready | failed | deleted
  track          TEXT,
  bike_set       TEXT,
  max_riders     INTEGER NOT NULL DEFAULT 20,
  applied        INTEGER NOT NULL DEFAULT 0,
  riders         INTEGER,
  polled_at      INTEGER,
  last_active_at INTEGER NOT NULL,
  error          TEXT,
  created_at     INTEGER NOT NULL,
  ready_at       INTEGER,
  deleted_at     INTEGER
);
CREATE INDEX host_servers_owner ON host_servers (steam_id, state);
CREATE INDEX host_servers_box ON host_servers (box_id, state);

-- MSM: a one-time claim from the site, swapped for a bearer that drives one server.
CREATE TABLE host_claims (
  code_hash   TEXT PRIMARY KEY,
  server_id   TEXT NOT NULL,
  steam_id    TEXT NOT NULL,
  expires_at  INTEGER NOT NULL,
  used_at     INTEGER
);
CREATE TABLE host_tokens (
  token_hash  TEXT PRIMARY KEY,
  server_id   TEXT NOT NULL,
  steam_id    TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  revoked_at  INTEGER
);

-- The shared track set each pool's boxes install.
CREATE TABLE host_tracks (
  id          TEXT PRIMARY KEY,
  pool        TEXT NOT NULL CHECK (pool IN ('native', 'legacy')),
  name        TEXT NOT NULL,
  url         TEXT NOT NULL,
  sha256      TEXT NOT NULL,
  created_at  INTEGER NOT NULL
);

-- What an operator needs to see: refused capacity, flagged boxes, failed installs.
CREATE TABLE host_alerts (
  id          TEXT PRIMARY KEY,
  kind        TEXT NOT NULL,
  region      TEXT,
  box_id      TEXT,
  message     TEXT NOT NULL,
  created_at  INTEGER NOT NULL,
  acked_at    INTEGER
);
