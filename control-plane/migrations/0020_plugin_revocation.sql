-- Revocation, for both halves of the paid-plugin path.
--
-- Until now a key could only be spent and a license could only run out. Neither is enough
-- for a refund, a chargeback, or a batch of keys posted to the wrong channel — and neither
-- is enough to test the thing at all, because handing yourself a license was a one-way door.
--
-- A revoked license keeps its `expires_at`. Zeroing the expiry would be simpler to read at
-- the query, but it would also throw away the months that were actually paid for, and a
-- revocation lifted the next day would come back with nothing left on it.

ALTER TABLE plugin_keys ADD COLUMN revoked_at INTEGER;
ALTER TABLE plugin_licenses ADD COLUMN revoked_at INTEGER;

-- The admin list is newest-first over every key ever minted, and keys are minted in batches.
CREATE INDEX plugin_keys_created ON plugin_keys (created_at);
