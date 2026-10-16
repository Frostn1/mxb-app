-- Dispute records for the Blends shop (`blendshop.ts`): who did what, from where, for each order.
--
-- One row per checkout, download link and download served: the order, the Steam account, the
-- client address (CF-Connecting-IP), the browser's user agent and the time. Kept two years for
-- fraud prevention and chargeback defense, then deleted by the cron tick. Erasing an account
-- deletes its rows too, except those of an order with an open dispute (status 'disputed').
CREATE TABLE blend_evidence (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  order_id    TEXT NOT NULL,
  steam_id    TEXT NOT NULL,
  kind        TEXT NOT NULL CHECK (kind IN ('checkout', 'download_link', 'download')),
  ip          TEXT,
  user_agent  TEXT,
  at          INTEGER NOT NULL
);

CREATE INDEX blend_evidence_by_order ON blend_evidence (order_id, at);
CREATE INDEX blend_evidence_by_steam ON blend_evidence (steam_id);
CREATE INDEX blend_evidence_by_age ON blend_evidence (at);
