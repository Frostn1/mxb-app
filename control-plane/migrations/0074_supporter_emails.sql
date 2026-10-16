-- Buy Me a Coffee supporters get the supporter pack, matched by an email the player confirms
-- once in the app (`supporteremail.ts`). No address is stored anywhere: every column below
-- holds a SHA-256 of the normalised address.

-- Addresses that have paid through BMAC, from its webhook.
CREATE TABLE bmac_supporters (
  email_hash  TEXT PRIMARY KEY,
  first_seen  INTEGER NOT NULL,
  last_seen   INTEGER NOT NULL
);

-- The address an account confirmed, and the Steam id it confirmed it under. Erased with the
-- account; the pack it granted is in `supporters` and stays.
CREATE TABLE supporter_emails (
  account_id    TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  steam_id      TEXT NOT NULL,
  email_hash    TEXT NOT NULL,
  confirmed_at  INTEGER NOT NULL
);
CREATE INDEX supporter_emails_hash ON supporter_emails (email_hash);

-- A code on its way: one per account, replaced by the next. `sends` counts the codes asked for
-- since `window_start`, which is the rate limit.
CREATE TABLE supporter_email_codes (
  account_id    TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  email_hash    TEXT NOT NULL,
  code_hash     TEXT NOT NULL,
  expires_at    INTEGER NOT NULL,
  attempts      INTEGER NOT NULL DEFAULT 0,
  window_start  INTEGER NOT NULL,
  sends         INTEGER NOT NULL DEFAULT 1
);
