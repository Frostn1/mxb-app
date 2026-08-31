-- Paid plugins, the keys that grant them, and who currently holds one.
--
-- A plugin is a bundle the app downloads and runs. The bundle is not the secret — it is
-- handed to anyone with a live licence, and a determined person keeps their copy. What the
-- licence actually buys is the right to keep receiving working, current ones, which is why
-- the interesting column here is an expiry rather than a boolean.

CREATE TABLE plugins (
  -- Stable, short, and part of every path and filename the plugin touches: 'replaycam'.
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  summary       TEXT,
  -- The bundle currently on offer. The hash is served in the entitlement as well, so the
  -- app can tell whether the copy it already has is the one being offered without
  -- downloading it to find out — and so a bundle swapped in R2 without going through here
  -- fails verification instead of running.
  version       TEXT,
  bundle_key    TEXT,
  bundle_sha256 TEXT,
  created_at    INTEGER NOT NULL
);

-- Redeemable keys. Minted by hand today. A billing webhook, when there is one, inserts the
-- same rows and nothing downstream has to know the difference.
CREATE TABLE plugin_keys (
  code        TEXT PRIMARY KEY,
  plugin_id   TEXT NOT NULL REFERENCES plugins(id),
  -- Whole months. A key is bought in months, and the licence it grants is measured the same
  -- way, so a renewal is arithmetic on this rather than a date someone typed.
  months      INTEGER NOT NULL,
  note        TEXT,
  created_at  INTEGER NOT NULL,
  -- Set on redemption, and the reason a key is one-shot: the renewable thing is the licence
  -- it created, not the code. Redeeming twice must extend nothing.
  redeemed_by TEXT REFERENCES accounts(id),
  redeemed_at INTEGER
);
CREATE INDEX plugin_keys_plugin ON plugin_keys (plugin_id);

-- One row per (account, plugin). A renewal moves `expires_at` rather than adding a row, so
-- "may they run it" is one lookup and never a max() over a history table.
CREATE TABLE plugin_licences (
  account_id  TEXT NOT NULL REFERENCES accounts(id),
  plugin_id   TEXT NOT NULL REFERENCES plugins(id),
  expires_at  INTEGER NOT NULL,
  granted_at  INTEGER NOT NULL,
  PRIMARY KEY (account_id, plugin_id)
);
CREATE INDEX plugin_licences_account ON plugin_licences (account_id);

-- The one plugin there is. Bundle columns stay null until a build is published: a plugin
-- with no bundle is listed and can be licensed, it simply has nothing to install yet.
INSERT INTO plugins (id, name, summary, created_at) VALUES (
  'replaycam',
  'Frost''s Replay Mod',
  'Keyframed cinematic camera for replays: spline a path between keys, cut between shots, aim at a rider.',
  unixepoch()
);
