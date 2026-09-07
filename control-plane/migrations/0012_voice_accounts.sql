-- Accounts that can talk, but not publish.
--
-- Voice is the first feature meant for everyone who has the app, not only the invited: a
-- rider on a community server has nobody to talk to unless the people beside them can sign
-- up too. That breaks the one assumption `0001_init.sql` made about accounts — that each one
-- came from an invite, and so was rare enough to hold a globally unique rider name.
--
-- Two strangers really can both be called "Ryan". Paint sync could rule that out by fiat
-- because an invite list is small and we hand out the codes; open signup cannot, and the
-- unique index would simply reject the second one. So the uniqueness now applies only to
-- invited accounts, where it is still both true and needed (the paint roster joins on the
-- rider name). Voice needs no such join: a peer is matched to a rider by the race number in
-- the local race-entry list, and a name that collides is only ever a label.
--
-- `kind` is what the privileged endpoints check. A device account may report presence and
-- join a voice room; it may not publish a loadout, register a server or provision one.
ALTER TABLE accounts ADD COLUMN kind TEXT NOT NULL DEFAULT 'invited';

DROP INDEX accounts_rider_name;
CREATE UNIQUE INDEX accounts_rider_name ON accounts (lower(rider_name)) WHERE kind = 'invited';

-- Open signup is a new abuse surface: without a cost, one script can mint accounts until the
-- rate limiter is the only thing standing up. Recording the claim lets the limiter be per-IP
-- per-day rather than global, and lets a burst be traced after the fact.
CREATE TABLE device_claims (
  -- Not the raw address: an IP is personal data and we have no need to keep one. A daily
  -- salted digest is enough to count claims and useless afterwards.
  ip_digest   TEXT NOT NULL,
  day         TEXT NOT NULL,
  claims      INTEGER NOT NULL DEFAULT 1,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (ip_digest, day)
);
