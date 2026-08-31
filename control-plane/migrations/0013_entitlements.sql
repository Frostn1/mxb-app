-- Secured assets, and who is allowed to use them.
--
-- A creator can sell content that stays unusable without a live, authorized session. The
-- decision "may this person use this asset" is made here and nowhere else: the client asks,
-- the server answers, and no client-side flag can grant itself anything.
--
-- Nothing in this migration is cryptographic. It records identity and entitlement, which is
-- the part that has to be right before any of the rest is worth building.

-- One secured asset. The creator sells it; we gate it.
CREATE TABLE assets (
  id           TEXT PRIMARY KEY,
  -- Who published it. Their account, so a creator can be paid, contacted, and — if it
  -- comes to it — held responsible for what they uploaded.
  creator_id   TEXT NOT NULL REFERENCES accounts (id),
  title        TEXT NOT NULL,
  -- Where the packed blob lives. No key material is stored here, ever: `key_id` names
  -- *which* key it was packed under so keys can rotate without re-packing everything,
  -- and the key itself lives in the secret store.
  blob_key     TEXT,
  key_id       TEXT,
  created_at   INTEGER NOT NULL,
  -- Pulled from sale, or taken down. Set rather than deleted: entitlements and the audit
  -- log both reference assets, and the history of a withdrawn asset is exactly what you
  -- want to still have.
  withdrawn_at INTEGER
);
CREATE INDEX assets_creator ON assets (creator_id);

-- Who may use what.
--
-- Keyed on the **SteamID**, not on our own account id. Identity comes from Valve — that is
-- the whole design — and a purchase belongs to the Steam account that made it, which
-- outlives any community account we happen to have attached to it. It also makes the
-- operation that matters in an incident ("stop this person") a single predicate on the
-- identity Valve gave us, rather than a join through our own records.
CREATE TABLE entitlements (
  steam_id    TEXT NOT NULL,
  asset_id    TEXT NOT NULL REFERENCES assets (id) ON DELETE CASCADE,
  -- How they came by it: a purchase, a creator granting it, or the creator's own asset.
  source      TEXT NOT NULL,
  granted_at  INTEGER NOT NULL,
  -- Revocation is a timestamp, not a delete. A refunded or charged-back entitlement is a
  -- thing you need to be able to see afterwards.
  revoked_at  INTEGER,
  PRIMARY KEY (steam_id, asset_id)
);
CREATE INDEX entitlements_asset ON entitlements (asset_id);

-- Every decision, append-only.
--
-- This is the "theft has a name on it" ledger. Denials are recorded as well as grants:
-- one identity sweeping the whole catalogue, or a burst of re-authentication, looks like
-- extraction tooling and is only visible if the refusals are written down too.
CREATE TABLE entitlement_grants (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  steam_id    TEXT NOT NULL,
  asset_id    TEXT NOT NULL,
  -- Which authorized session asked. Ties a series of requests together.
  session_id  TEXT NOT NULL,
  decision    TEXT NOT NULL,
  reason      TEXT,
  issued_at   INTEGER NOT NULL
);
CREATE INDEX entitlement_grants_steam ON entitlement_grants (steam_id, issued_at);

-- A Steam sign-in in progress.
--
-- The browser leaves for Steam and comes back, and the two halves have to be tied together
-- by something the round trip cannot forge: `id` travels in the return URL, and the row it
-- names says which account was signing in. Rows are short-lived and single-use — a replayed
-- return is a row that has already been consumed.
CREATE TABLE steam_logins (
  id          TEXT PRIMARY KEY,
  account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  created_at  INTEGER NOT NULL,
  consumed_at INTEGER
);
