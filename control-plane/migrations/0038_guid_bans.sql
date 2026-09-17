-- Banning a rider from mxbsecure, by the identity MX Bikes itself issues.
--
-- Everything else here revokes *content*: `withdrawn_at` pulls an asset from sale,
-- `taken_down_at` is ours to set on an abusive upload, `keys_revoked_at` destroys a content
-- key. None of them reach the person. What was missing is the other direction: somebody who
-- unlocked protected content and handed it around should not go on using the system whose
-- locks they broke — not with the account we caught them on, and not with the next one.
--
-- Keyed on the **GUID**, not on our account id and not on the Steam ID:
--
-- * It is the identity the game issues per install, and the one a report about cracked
--   content actually carries (`0003_guid.sql`, and every diagnostics sighting).
-- * Our own account id is free to mint — `POST /v1/account` hands one to anybody — so a ban
--   on it is a ban on one row.
-- * A Steam ID is the right key for *entitlement* (a purchase belongs to the Steam account
--   that made it) and the wrong one for a ban: it is what a second Steam account replaces
--   for the price of one, while the GUID follows the install that did the unlocking.
--
-- The Steam side is not lost, though: `bans.ts` resolves a ban through every identity we
-- have ever tied to a banned GUID — the account holding it, the Steam ID linked to that
-- account (`0028_steam_links.sql`), and every GUID that account has ever claimed — so a
-- fresh account, a fresh GUID or a fresh Steam link only bans the new identity too.
CREATE TABLE guid_bans (
  -- Upper-cased and trimmed by `bans.ts` before it ever reaches here, so the lookup is a
  -- primary-key hit rather than a scan with UPPER() over the table.
  guid        TEXT PRIMARY KEY,
  -- Why, in words a person can read back in a year. This ends up in front of the banned
  -- rider and in front of whoever reviews an appeal, so it is required.
  reason      TEXT NOT NULL,
  -- What was actually seen, and where it is kept. A ban with no evidence recorded is a ban
  -- nobody can review, which is how a mistaken one becomes permanent.
  evidence    TEXT,
  -- The GUID this one was found to be a second install of, when that is known. A note, not
  -- a mechanism: each alt is banned in its own right, so the chain is documentation.
  alt_of      TEXT,
  banned_at   INTEGER NOT NULL,
  -- The admin's Steam ID, or `seed:<migration>` for a ban that arrived in a deploy.
  banned_by   TEXT NOT NULL,
  -- Reversible, and reversed the way everything else here is: a timestamp, never a DELETE.
  -- A lifted ban has to stay readable — it is the record that we were wrong, or that an
  -- appeal was upheld, and it is what stops the same GUID being re-banned off the same
  -- stale report.
  lifted_at   INTEGER,
  lifted_by   TEXT,
  lifted_note TEXT
);

-- The admin list reads the live ones first, newest first.
CREATE INDEX guid_bans_live ON guid_bans (lifted_at, banned_at);

-- Every (account, GUID) pair we have ever seen, which is what makes a ban outlive a rename.
--
-- `accounts.guid` is a single mutable cell: `PUT /v1/me/guid` UPDATEs it, so an account
-- caught on a banned GUID could claim a different one and the join that found the ban would
-- come up empty — the same failure `steam_links` was added to fix for the Steam link. This is
-- the append-only record beside it. Written when a GUID is claimed and when a diagnostics
-- report names one, and never deleted here.
--
-- Keyed on the pair rather than the account: one account legitimately reinstalls and reports
-- a new GUID, and both are worth keeping. `last_seen_at` moves; `first_seen_at` does not.
CREATE TABLE guid_claims (
  account_id    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  guid          TEXT NOT NULL,
  first_seen_at INTEGER NOT NULL,
  last_seen_at  INTEGER NOT NULL,
  PRIMARY KEY (account_id, guid)
);

-- The two directions: what an account has ever used, and who has ever used a GUID.
CREATE INDEX guid_claims_account ON guid_claims (account_id, last_seen_at);
CREATE INDEX guid_claims_guid ON guid_claims (guid, last_seen_at);

-- What we already know, so the history does not start empty: every GUID an account is
-- currently holding counts as claimed. `created_at` is the honest lower bound for when —
-- nothing recorded the claim itself before this table existed.
INSERT INTO guid_claims (account_id, guid, first_seen_at, last_seen_at)
SELECT id, UPPER(TRIM(guid)), created_at, created_at
FROM accounts
WHERE guid IS NOT NULL AND TRIM(guid) <> '';

-- The first bans: six installs reported with proof of unlocking protected content and
-- sharing it unlocked.
--
-- Seeded in a migration rather than typed into the admin page for the same reason
-- `MXB_ADMIN_STEAM_IDS` is config: this is the switch that refuses a paying-looking customer,
-- so turning it on should leave a diff somebody can read, review and revert. Bans added later
-- go through `/v1/web/admin/bans`, which records the admin who added them.
--
-- `FF011000016EAE6204` was reported as a second install of `FF0110000162638666`; the other
-- four arrived in the same list without a stated relationship, so none is claimed for them.
-- Each is banned on its own evidence in any case — the link is a note, never the reason.
--
-- Written as a VALUES list rather than a chain of `SELECT ... UNION ALL`, which is what
-- this was and why the deploy failed: D1 refuses a compound SELECT of more than five
-- terms (`too many terms in compound SELECT`, SQLITE_ERROR 7500), and six riders is six
-- terms. Plain SQLite allows five hundred, so it applies locally and in every test, and
-- fails only against the real database. A VALUES list has no such limit.
WITH seed(guid, alt_of) AS (VALUES
  ('FF011000016EAE6204', 'FF0110000162638666'),
  ('FF0110000162638666', NULL),
  ('FF011000012D2FBD46', NULL),
  ('FF01100001308ED7FA', NULL),
  ('FF0110000164B7DCE8', NULL),
  ('FF011000012B467ED8', NULL)
)
INSERT INTO guid_bans (guid, reason, evidence, alt_of, banned_at, banned_by)
SELECT guid, 'unlocked protected content and shared it unlocked',
       'reported 2026-09-16 with proof of unlocking or sharing unlocked content', alt_of,
       CAST(strftime('%s', 'now') AS INTEGER) * 1000, 'seed:0038'
FROM seed;
