-- Paid hosting (src/billing.ts): one Stripe subscription per hosted server, through Stripe
-- Checkout. Empty until STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET and both price vars are set;
-- without them deploy stays invite-only and free, and nothing here is read or written.

-- One Stripe customer per Steam account, made the first time that account pays.
CREATE TABLE host_billing_customers (
  steam_id     TEXT PRIMARY KEY,
  customer_id  TEXT NOT NULL UNIQUE,
  created_at   INTEGER NOT NULL
);

-- One row per paid server. `status`:
--   pending   -> Checkout is open; the server holds no slot and no box is ordered for it
--   active    -> paid; the server was placed
--   past_due  -> a renewal failed; suspended at `grace_until` unless it is paid first
--   canceled  -> the subscription ended at Stripe; suspended at `grace_until`
--   suspended -> grace ran out: the server was deleted, its slot freed, the subscription cancelled
--   ended     -> the server was deleted (owner, operator, idle) and the subscription cancelled
--   expired   -> Checkout was never paid
CREATE TABLE host_billing (
  server_id            TEXT PRIMARY KEY,
  steam_id             TEXT NOT NULL,
  type                 TEXT NOT NULL CHECK (type IN ('mxbserver', 'legacy')),
  price_id             TEXT NOT NULL,
  amount_cents         INTEGER NOT NULL,
  status               TEXT NOT NULL,
  customer_id          TEXT NOT NULL,
  checkout_session_id  TEXT,
  subscription_id      TEXT,
  grace_until          INTEGER,
  activated_at         INTEGER,
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL
);
CREATE INDEX host_billing_owner ON host_billing (steam_id, status);
CREATE INDEX host_billing_subscription ON host_billing (subscription_id);
CREATE INDEX host_billing_status ON host_billing (status, grace_until);

-- Stripe event ids already handled. Stripe delivers at least once.
CREATE TABLE host_billing_events (
  id           TEXT PRIMARY KEY,
  type         TEXT NOT NULL,
  received_at  INTEGER NOT NULL
);
