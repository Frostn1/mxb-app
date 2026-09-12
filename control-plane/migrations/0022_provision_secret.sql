-- A per-buyer secret folded into the .mxbkey seal, so a leaked key can't be re-derived from
-- the (public) Steam ID alone.
--
-- On the `entitlements` row, keyed on (steam_id, asset_id), because the secret belongs to one
-- buyer's ownership of one asset and must be *stable*: the same buyer re-provisioning on
-- another of their machines has to derive the same key, so the server hands back the secret it
-- minted the first time rather than a fresh one. Minted lazily on the first key grant and left
-- untouched after — never rotated, or old .mxbkey files would stop opening.
ALTER TABLE entitlements ADD COLUMN provision_secret TEXT;
