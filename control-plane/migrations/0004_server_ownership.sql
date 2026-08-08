-- Let a player register their own server, instead of the list being hand-seeded SQL.
--
-- Until now every row in `servers` was inserted by hand, so the join picker could only ever
-- show servers we put there ourselves. An operator who ran one had no way to make it
-- joinable except to send the address around privately — which is exactly the problem the
-- registry exists to solve.

-- Who may edit or remove this row. Null for the hand-seeded rows, which nobody owns and so
-- nobody can delete through the API — they stay ours to manage with SQL.
ALTER TABLE servers ADD COLUMN owner_account_id TEXT REFERENCES accounts (id);

-- Whether the row appears in the public list.
--
-- Defaulting to 1 keeps the existing hand-seeded rows visible; anything registered through
-- the API sets it explicitly once the address has been checked. This is what lets an
-- unreachable server be recorded without being advertised to everyone.
ALTER TABLE servers ADD COLUMN published INTEGER NOT NULL DEFAULT 1;

-- When the address was last confirmed to answer. Null means never checked.
ALTER TABLE servers ADD COLUMN checked_at INTEGER;

-- An operator manages one server far more often than several, and the cap on how many an
-- account may register is enforced in the Worker — but the index is what makes that count
-- cheap, and what makes "my servers" a lookup rather than a scan.
CREATE INDEX servers_owner ON servers (owner_account_id);
