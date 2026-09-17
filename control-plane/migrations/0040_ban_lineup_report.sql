-- Six more installs, from the report of 2026-09-17, and what that report added about two that
-- `0038_guid_bans.sql` had already banned.
--
-- Seeded in a migration for the same reason the first six were: this is the switch that refuses
-- somebody who looks like a paying customer, so turning it on for a new install should leave a
-- diff a person can read, review and revert, rather than a row typed into the admin page at
-- night. Bans decided later still go through `/v1/web/admin/bans`, which records the admin.
--
-- The report describes one group around one unlocking tool: the install that shared the tool,
-- the install that made the Discord leak it spread through, the install selling a drive of
-- unlocked content (already banned here, with its second install), and two more that took part.
-- One more is not part of that group and is here for a heavier reason: it builds cheating
-- clients, and unlocks locked content with them. That is the one offence on this list that is
-- both of the things this company exists to stop — the anti-cheat's problem and the locking's,
-- in one install — so it is written as both rather than filed under the group above.
-- Each is banned on what was said about it, and the roles are kept apart in `reason` rather than
-- flattened into one sentence, because `reason` is what the rider is shown on the website and
-- what an appeal is argued against. The install at the head of the list is the one the operator
-- describes as involved throughout rather than in any single act, and its row says that — a
-- wider reason than the others, and deliberately not one of theirs borrowed.
--
-- Handles are deliberately not in this file. This repository is public. A GUID identifies an
-- install to us without publishing an accusation against a named person, and the handles the
-- report carries are kept beside the evidence in the private incident record
-- (`mxbapp-private`, `anti-cheat/BANS.md`), which is where an appeal is reviewed from.
--
-- `ON CONFLICT DO NOTHING`, not the upsert `addBan` uses: if one of these is ever lifted, a
-- later redeploy must not silently re-ban it off this same report — stopping that is what the
-- lift columns are for. And a `VALUES` list rather than a chain of `SELECT ... UNION ALL`,
-- because D1 refuses a compound SELECT of more than five terms (`too many terms in compound
-- SELECT`), which is the failure 0038 shipped and had to fix.
WITH seed(guid, reason, evidence) AS (VALUES
  ('FF011000015B9B8606',
   'shared the unlocker used to strip protection from paid content',
   'reported 2026-09-17: named as the install that handed the unlocking tool around'),
  ('FF011000012F987D96',
   'created the Discord leak that put unlocked content out',
   'reported 2026-09-17: named as the install behind the Discord leak the unlocked content spread through'),
  ('FF011000016112EBE7',
   'took part in unlocking protected content and passing it around',
   'reported 2026-09-17: named among those taking part in the group above'),
  ('FF011000015900502F',
   'took part in unlocking protected content and passing it around',
   'reported 2026-09-17: named among those taking part in the group above'),
  ('FF01100001423F97F0',
   'builds cheating clients, and unlocks protected content with them',
   'reported 2026-09-17: named as the author of cheating clients and as unlocking locked content. Not part of the group above; banned on its own account'),
  ('FF011000012E746802',
   'unlocked protected content and shared it, across the whole of what this report describes',
   'reported 2026-09-17: named at the head of the list as the one involved throughout rather than in a single act; the operator holds the evidence and an appeal is judged on it')
)
INSERT INTO guid_bans (guid, reason, evidence, alt_of, banned_at, banned_by)
SELECT guid, reason, evidence, NULL, CAST(strftime('%s', 'now') AS INTEGER) * 1000, 'seed:0040'
FROM seed
-- SQLite needs a WHERE before an UPSERT on INSERT ... SELECT, or the ON CONFLICT is read as
-- part of the select. `true` is the whole of it.
WHERE true
ON CONFLICT (guid) DO NOTHING;

-- The same report, about two installs already banned. Nothing here re-bans them or moves a date:
-- the ban they are under says what it says and is the one their appeal is judged against. This
-- adds only what is newly known, which is what makes the row reviewable a year from now.
--
-- `FF011000012B467ED8` was banned in 0038 with no stated relationship; the report now states one
-- — it is a second install of `FF011000016EAE6204`, which 0038 in turn recorded as a second
-- install of `FF0110000162638666`. A note, never a mechanism: each of the three is banned in its
-- own right, so the chain changes no outcome. Only filled in if it is still empty, and only
-- while the ban is live.
UPDATE guid_bans
SET alt_of = 'FF011000016EAE6204'
WHERE guid = 'FF011000012B467ED8' AND alt_of IS NULL AND lifted_at IS NULL;

-- And what the two were reported for this time: selling a drive of unlocked content. Appended to
-- the evidence rather than written over it — the older line is why they were banned, and a
-- record that loses it is a record an appeal cannot be argued against.
UPDATE guid_bans
SET evidence = COALESCE(evidence || '; ', '')
  || 'also reported 2026-09-17: selling a drive of unlocked content, the two installs being one person'
WHERE guid IN ('FF011000016EAE6204', 'FF011000012B467ED8') AND lifted_at IS NULL;
