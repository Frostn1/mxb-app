-- Identify riders by their MX Bikes GUID rather than their rider name.
--
-- A rider name is free text: the player can change it between sessions, two people can pick
-- the same one, and it is trivially spoofable. A GUID is stable per install, and the
-- dedicated server writes it next to the name on every connection
-- (`Connection: %d %s GUID: %s - %s`), so it is available exactly where the roster is read.
--
-- Nullable, because the game's plugin API does not expose another rider's GUID and there is
-- no way to collect one until the player has connected at least once. Name matching stays
-- as the fallback for accounts that have not supplied one yet.
ALTER TABLE accounts ADD COLUMN guid TEXT;
CREATE UNIQUE INDEX accounts_guid ON accounts (guid) WHERE guid IS NOT NULL;
