-- Frozen hosted servers (src/hosting.ts `freezeServer`): when a paid server's grace runs out
-- unpaid, or a plan its owner cancelled reaches its end date, it is unloaded from its box and
-- its slot freed, but the row and every setting are kept so Resume can place it again.
-- host_servers.state gains `frozen` (the column has no CHECK, so no rebuild is needed).
--
-- host_billing.status `suspended` now means "frozen for non-payment" (was "deleted").

-- When the server was frozen.
ALTER TABLE host_servers ADD COLUMN frozen_at INTEGER;
-- JSON: what only the box held (a native slot's permanent bans), put back when it is placed again.
ALTER TABLE host_servers ADD COLUMN saved_state TEXT;

-- When the open Checkout was made. A Resume reopens Checkout on a row made long before, so the
-- one-day drop of unpaid Checkouts counts from here (falling back to created_at).
ALTER TABLE host_billing ADD COLUMN checkout_at INTEGER;
