-- How many GUIDs a creator has asked to lock a file to, lately.
--
-- The GUID lock runs in the creator's browser, so the only thing the control plane sees of it is
-- `POST /v1/web/lock/permit` (`lockpermit.ts`): the list of GUIDs about to be locked to, asked
-- before the locker runs. That endpoint refuses a banned GUID with the same sentence it uses for
-- every other refusal, and a refusal that says nothing is still an oracle if it can be asked
-- without limit. This is the limit: one row per permit asked, with how many GUIDs it named, so
-- "how many in the last hour" is one indexed count.
--
-- Deliberately not an audit log. The GUIDs themselves are not stored — the refusals, which are
-- what an admin needs to see, go to the Worker's log with the reason — and rows are swept after
-- a day, because nothing reads further back than an hour. `erasure.ts` deletes them with
-- everything else that describes a person.
CREATE TABLE lock_attempts (
  account_id   TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  attempted_at INTEGER NOT NULL,
  guids        INTEGER NOT NULL
);

CREATE INDEX lock_attempts_account ON lock_attempts (account_id, attempted_at);
CREATE INDEX lock_attempts_at ON lock_attempts (attempted_at);
