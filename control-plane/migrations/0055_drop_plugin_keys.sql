-- Plugin keys are gone, and so are license rows.
--
-- Every plugin is free: any signed-in account gets a signed license for each one on every
-- check-in (src/plugins.ts), so nothing is redeemed, granted or revoked per account any more.
-- A banned account is still refused by the ban gate before it reaches the plugin routes.

DROP TABLE IF EXISTS plugin_keys;
DROP TABLE IF EXISTS plugin_licenses;

UPDATE plugins SET free = 1;
