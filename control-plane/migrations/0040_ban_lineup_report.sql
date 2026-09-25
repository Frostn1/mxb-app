-- This migration used to seed six more bans and annotate two others (2026-09-17). Ban records no
-- longer live in this public repository (2026-09-25): a GUID beside a ban reason is an accusation
-- against a person that no later commit can take back. Bans go through `/v1/web/admin/bans`, and
-- their records are kept privately. What this file once did was applied long ago and stays in the
-- database; D1 never re-runs an applied migration, so this no-op changes nothing there. It is kept,
-- not deleted, so the list of applied migrations and the files still match.
SELECT 1;
