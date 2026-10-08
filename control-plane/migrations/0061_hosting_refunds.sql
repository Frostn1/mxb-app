-- Paid hosting (src/billing.ts): a paid server that could not be placed is refunded
-- automatically. The Stripe refund id and when it was made.
ALTER TABLE host_billing ADD COLUMN refund_id TEXT;
ALTER TABLE host_billing ADD COLUMN refunded_at INTEGER;
