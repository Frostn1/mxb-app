-- Two indexes, for the two queries that were reading the database dry.
--
-- On 2026-09-01 the account passed D1's free-tier ceiling of 5,000,000 rows read in a day,
-- with hours of the day left. Everything that touches the database answered 500 from then
-- until midnight UTC. `wrangler d1 insights` named the cost precisely, and it was not spread
-- around: one query was 11.77M of the ~12.2M rows read, and a second was most of the rest.

-- ── The roster ───────────────────────────────────────────────────────────────────────────
--
-- `/v1/roster` reads 1,022 rows to return about 215 of them — 21% efficiency, 11,514 times a
-- day. The waste is the join's fan-out: loadouts are per bike, and a rider's gear repeats
-- for every bike they own, so a rider with ~94 stored paint rows has only ~20 distinct
-- destinations. The `GROUP BY a.id, p.rel_dest, p.sha256` throws the other three quarters
-- away *after* reading them.
--
-- `loadout_paints_account` is on `(account_id)` alone, so every one of those 94 rows costs an
-- index seek and then the table row behind it. This index carries every column the roster
-- selects, in the order it groups by: the join, the dedup and the projection are all
-- satisfied from the index, and the table is never touched.
--
-- Deliberately not a replacement for `loadout_paints_account` — that one still serves the
-- plain `WHERE account_id = ?` reads and deletes, and is much narrower to maintain on write.
CREATE INDEX loadout_paints_roster
  ON loadout_paints (account_id, rel_dest, sha256, slot, file_name, size);

-- ── The daily claims sweep ───────────────────────────────────────────────────────────────
--
-- `DELETE FROM device_claims WHERE day < ?` read 1,327 rows every time it ran and 469,845
-- across the day — 4% of the whole ceiling to delete a handful of rows. The primary key is
-- `(ip_digest, day, kind)`, so `day` on its own is not a prefix of it and the sweep had no
-- way to find its rows but to read all of them.
CREATE INDEX device_claims_day ON device_claims (day);
