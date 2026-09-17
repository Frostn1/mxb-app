-- Auto-derive every Steam account's GUID from the identity Valve confirmed, and clear the spoofs.
--
-- For a Steam copy of MX Bikes the GUID is a pure function of the SteamID64 — `FF` followed by
-- the id as sixteen uppercase hex digits (`guidFromSteamId` on the control plane, and what the
-- game itself computes). The SteamID64 on an account is the one Valve vouched for at sign-in, so
-- the GUID is not a fact to be claimed and trusted separately: it is derived. From here on
-- `pinGuidFromSteam` sets it on every link; this backfills the accounts that linked before that
-- existed, and in doing so removes any GUID that was only ever a guess or a spoof.
--
-- Two steps, and the order matters because `accounts_guid` is unique:
--
--  1. Null every occurrence of a *derived* GUID, wherever it currently sits. That frees the
--     value whether the account holding it was the rightful Steam owner (a stale first-come
--     claim) or an impostor who claimed the victim's GUID before they linked. Either way the
--     column is now empty and the value is free to assign correctly.
--  2. Set each Steam account's GUID to its derived value. No collision is possible: step 1
--     freed every target, each SteamID64 derives to a distinct GUID, and one SteamID64 belongs
--     to one account (the column is unique).
--
-- A non-Steam (Piboso) account has no SteamID64 to derive from and is left exactly as it is —
-- its GUID stays first-come, corroborated by server sightings.

-- The derived GUID of a Steam account: FF + the SteamID64 as 16 uppercase hex digits. 17-digit
-- SteamID64s fit in a signed 64-bit integer, so the CAST is exact.
UPDATE accounts SET guid = NULL
WHERE guid IN (
  SELECT 'FF' || printf('%016X', CAST(steam_id AS INTEGER))
  FROM accounts
  WHERE steam_id IS NOT NULL AND CAST(steam_id AS INTEGER) >= 76561197960265728
);

UPDATE accounts SET guid = 'FF' || printf('%016X', CAST(steam_id AS INTEGER))
WHERE steam_id IS NOT NULL AND CAST(steam_id AS INTEGER) >= 76561197960265728;

-- Record the derived GUIDs as claims too, so the ban resolution follows them like any other.
-- `first_seen_at` is the account's own age; the derivation is confirmed now.
INSERT INTO guid_claims (account_id, guid, first_seen_at, last_seen_at)
SELECT id, guid, created_at, CAST(strftime('%s', 'now') AS INTEGER) * 1000
FROM accounts
WHERE steam_id IS NOT NULL AND guid IS NOT NULL
ON CONFLICT (account_id, guid) DO UPDATE SET last_seen_at = excluded.last_seen_at;
