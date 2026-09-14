-- Every Steam link Valve has confirmed, kept apart from `accounts.steam_id`.
--
-- `accounts.steam_id` is a single mutable cell, so anything that clears it silently unlinks a
-- player: "never linked" and "was linked until something dropped it" are both NULL, and the
-- app can only refuse. This is the record that the link happened, written in the same batch
-- as the link itself and never deleted here, so a lost column is recoverable rather than
-- fatal.
--
-- Keyed on the pair, not on the account: re-linking the same identity is the common case and
-- should move a timestamp, while a genuine change of Steam account keeps the old row so the
-- history stays readable.
CREATE TABLE steam_links (
  account_id  TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  steam_id    TEXT NOT NULL,
  linked_at   INTEGER NOT NULL,
  PRIMARY KEY (account_id, steam_id)
);

-- The two lookups: an account asking "what was mine", and the site asking "whose was this".
CREATE INDEX steam_links_account ON steam_links (account_id, linked_at);
CREATE INDEX steam_links_steam ON steam_links (steam_id, linked_at);
