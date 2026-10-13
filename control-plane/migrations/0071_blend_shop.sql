-- The Blends shop on mxbsecure.com/blends (`blendshop.ts`): one-off purchases of a part's source
-- pack through Stripe Checkout, what each Steam account may download, and every download link.
--
-- An order is written `pending` when its Checkout opens and moves only on Stripe's webhook:
-- paid (grants the entitlement), expired, refunded or disputed (both take it away). Orders are
-- kept for tax and accounting; erasing an account takes the Steam ID off them ('erased').
CREATE TABLE blend_orders (
  id                  TEXT PRIMARY KEY,
  -- What the buyer sees and the zip carries: MXBB-XXXXXXXX.
  ref                 TEXT NOT NULL UNIQUE,
  steam_id            TEXT NOT NULL,
  slug                TEXT NOT NULL,
  name                TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'pending'
                      CHECK (status IN ('pending', 'paid', 'expired', 'refunded', 'disputed')),
  -- The site's price when the Checkout opened, USD cents, before tax.
  price_cents         INTEGER NOT NULL,
  currency            TEXT NOT NULL DEFAULT 'usd',
  -- What Stripe charged, from the completed Checkout.
  subtotal_cents      INTEGER,
  tax_cents           INTEGER,
  total_cents         INTEGER,
  customer_id         TEXT,
  checkout_session_id TEXT UNIQUE,
  payment_intent_id   TEXT,
  -- The Terms and License version the buyer agreed to at checkout, and when.
  terms_version       TEXT NOT NULL,
  agreed_at           INTEGER NOT NULL,
  -- The first time the files were served: the refund policy turns on it.
  first_download_at   INTEGER,
  created_at          INTEGER NOT NULL,
  paid_at             INTEGER,
  closed_at           INTEGER,
  updated_at          INTEGER NOT NULL
);

CREATE INDEX blend_orders_by_steam ON blend_orders (steam_id, created_at DESC);
CREATE INDEX blend_orders_by_payment ON blend_orders (payment_intent_id);
CREATE INDEX blend_orders_pending ON blend_orders (status, created_at);

-- What a Steam account may download: one row per pack, pointing at the order that paid for it.
CREATE TABLE blend_entitlements (
  steam_id    TEXT NOT NULL,
  slug        TEXT NOT NULL,
  order_id    TEXT NOT NULL,
  granted_at  INTEGER NOT NULL,
  revoked_at  INTEGER,
  revoked_why TEXT,
  PRIMARY KEY (steam_id, slug)
);

-- Every download link handed out (the rate limit counts these), and when it was used.
CREATE TABLE blend_downloads (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  order_id  TEXT NOT NULL,
  steam_id  TEXT NOT NULL,
  slug      TEXT NOT NULL,
  at        INTEGER NOT NULL,
  served_at INTEGER
);

CREATE INDEX blend_downloads_by_steam ON blend_downloads (steam_id, at);
