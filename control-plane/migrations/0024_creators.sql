-- Creators: accounts that may sell through mxbsecure.com.
--
-- A creator signs in on the site with Steam, which finds the account whose `steam_id`
-- matches. Null means not a creator. A timestamp rather than a flag so it also says when.
ALTER TABLE accounts ADD COLUMN creator_at INTEGER;

-- The creator dashboard reads the key-release log per asset; the existing index is per
-- Steam account.
CREATE INDEX entitlement_grants_asset ON entitlement_grants (asset_id, issued_at);
