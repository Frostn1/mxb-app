-- Buy Me a Coffee events already announced in Discord.
--
-- BMAC retries a delivery up to four more times, and a response we sent but it never received
-- is indistinguishable from one that failed. Without a record of what has been announced, a
-- single coffee can arrive in the channel five times.
--
-- The id is BMAC's own `event_id`, stored as text: it comes over as a number today, but the
-- only thing this column is ever used for is equality against the last one, and text can hold
-- whatever they send without a migration.
CREATE TABLE bmac_events (
  event_id TEXT PRIMARY KEY,
  -- Which event it was. Not read by the code — it is here so a row means something to a human
  -- looking at why a donation did or didn't get announced.
  type     TEXT NOT NULL,
  seen_at  INTEGER NOT NULL
);
