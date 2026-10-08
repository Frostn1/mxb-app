-- MSM (MXB Servers) signs in with Steam (src/hostedauth.ts) and lists the account's own hosted
-- servers, with no per-server claim link.
--
-- One row per sign-in: MSM sends a PKCE challenge and a state, the browser goes through Steam,
-- and the return stores the Steam ID with a one-time code that only the holder of the PKCE
-- verifier can swap for a token.
CREATE TABLE host_msm_logins (
  id               TEXT PRIMARY KEY,
  challenge        TEXT NOT NULL,
  state            TEXT NOT NULL,
  created_at       INTEGER NOT NULL,
  steam_id         TEXT,
  code_hash        TEXT UNIQUE,
  code_expires_at  INTEGER,
  used_at          INTEGER
);

-- A per-user MSM token: hosting only, stored as a SHA-256 digest, revoked on sign-out.
CREATE TABLE host_user_tokens (
  token_hash    TEXT PRIMARY KEY,
  steam_id      TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  last_used_at  INTEGER,
  revoked_at    INTEGER
);
CREATE INDEX host_user_tokens_steam ON host_user_tokens (steam_id);
