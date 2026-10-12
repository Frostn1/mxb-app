-- Website orders feed DGS Clarence, the Discord ticket bot the swap creators work from (`clarence.ts`).
--
-- `discord` is the rider's Discord username, so Clarence can add them to the private ticket.
-- `clarence` is whether the order reached Clarence: 'sent', or 'failed' (the queue can resend);
-- null when the feed is off. `ticket_url` is the Discord ticket Clarence opened for it.
ALTER TABLE builds ADD COLUMN discord TEXT;
ALTER TABLE builds ADD COLUMN clarence TEXT;
ALTER TABLE builds ADD COLUMN ticket_url TEXT;
