-- A share code that outlives the files it points at.
--
-- `MXBS1-` codes are self-contained: the item list and the catbox URLs are both baked into
-- the string, so recompiling a track invalidates every code anyone was given. This holds the
-- pointer instead. The code is permanent, the manifest under it is replaced, and everyone
-- who pasted it once follows along.
--
-- No account column, deliberately. Publishing is open — most people who run the app never
-- enroll, and a share that required a token would be a share nobody could make. What stands
-- in for ownership is `update_hash`: the publisher is whoever holds the update key minted at
-- first publish, which the app keeps for them and never shows. Stored as a digest for the
-- same reason account tokens are (see `auth.ts`) — a dump of this table yields nothing that
-- can be presented as a credential.
CREATE TABLE IF NOT EXISTS live_shares (
  code        TEXT PRIMARY KEY,
  -- SHA-256 of the update key, hex. Never the key.
  update_hash TEXT NOT NULL,
  -- What the publisher calls it, for the "v3 of RedBud 2026" line in a subscriber's list.
  name        TEXT NOT NULL,
  -- Bumped on every publish. This, not a timestamp, is what a subscriber compares — it is
  -- also the ETag, so an unchanged share costs a 304 and no body.
  version     INTEGER NOT NULL DEFAULT 1,
  -- The app's own `FileShare` JSON, verbatim: items (name/rel/size/isDir) and the bundle
  -- reference the downloader already knows how to fetch. Stored rather than modelled — the
  -- server never needs to reason about its parts, only to hand it back, and keeping it
  -- opaque means a new field in the app is not a migration here.
  manifest    TEXT NOT NULL,
  -- Bytes of the packed zip, so a subscriber can be told what an update will cost before
  -- it starts. Denormalised out of the manifest on purpose: it is shown in a list.
  size        INTEGER NOT NULL,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

-- For the pruner, and for any later "what has moved recently" view.
CREATE INDEX IF NOT EXISTS live_shares_updated ON live_shares (updated_at);
