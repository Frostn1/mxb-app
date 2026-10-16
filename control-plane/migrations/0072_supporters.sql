-- Who supports the project, by Steam id. One row per grant, so a pack bought on Stripe, a
-- coffee on Buy Me a Coffee and a hand grant are each their own record and can each be undone
-- on their own (a refund revokes the Stripe row, not the coffee). Somebody is a supporter while
-- any row is unrevoked. Keyed by Steam id for the reason `entitlements` is: it outlives an
-- account being erased and re-made, and it is what the shop's buyers sign in with.
--
-- 0072 rather than 0071 because the Blends shop branch already holds 0071.
CREATE TABLE supporters (
  steam_id    TEXT NOT NULL,
  source      TEXT NOT NULL CHECK (source IN ('manual', 'bmac', 'stripe')),
  -- The thing that granted it: a Stripe checkout session, a BMAC email hash, or '' by hand.
  ref         TEXT NOT NULL DEFAULT '',
  granted_at  INTEGER NOT NULL,
  -- The admin's Steam id for a hand grant; null otherwise.
  granted_by  TEXT,
  revoked_at  INTEGER,
  PRIMARY KEY (steam_id, source, ref)
);
