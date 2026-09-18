-- Why a sign-in didn't finish, written where the sign-in already lives.
--
-- `steam_logins` recorded two moments: the app minted a login (`created_at`) and Valve confirmed
-- it (`consumed_at`). Everything between them — the browser opening, Steam's page, a Steam Guard
-- code typed off a phone — left no trace at all, so a row that never got consumed said only
-- "this didn't work" and nothing about why. Half of every login ever minted is such a row, and
-- the accounts behind the worst of them tried nine, eleven, sixteen times. There was no way to
-- tell a person who wandered off from a person the flow was refusing, which is the difference
-- between "nothing to fix" and "an outage nobody can see".
--
-- Three columns close that.
--
-- `started_at` is when the browser actually reached `/v1/steam/start`. It matters twice. As a
-- fact it separates "the app minted a URL that was never opened" from "the person was at Steam
-- and did not get back", which are different failures with different fixes. As a clock it is the
-- honest one to measure the sign-in window against: `created_at` starts ticking when the *app*
-- asks, which can be well before the person clicks, and charging them for that is how a sign-in
-- that felt quick came back "expired". Stamped once and never reset, so re-opening the page
-- cannot hold a pending login alive indefinitely.
--
-- `refused_at` and `reason` are the refusal itself. `reason` is finer than what the rider is
-- shown on purpose: four different failures collapse to the one sentence "that sign-in expired",
-- which is the right thing to say and the wrong thing to store, because it is the difference
-- between a browser that never arrived, a tab reloaded after it already worked, and a window
-- that ran out while somebody waited on a Steam Guard code. `signin.ts` names the five and maps
-- them back to what the page says. A refusal is not an erasure: the row stays refused and a
-- retry mints a fresh one, which is what makes a run of them legible as a run rather than as one
-- row changing its mind.
--
-- These are nobody's personal data beyond what the row already held — it is keyed on an account
-- id and `erasure.ts` already deletes it wholesale with everything else about a person.
--
-- Plain ADD COLUMNs with no default: metadata-only in SQLite, no table rebuild, and every
-- existing row reads NULL, which is the truthful answer for a login nobody recorded this about.
ALTER TABLE steam_logins ADD COLUMN started_at INTEGER;
ALTER TABLE steam_logins ADD COLUMN refused_at INTEGER;
ALTER TABLE steam_logins ADD COLUMN reason TEXT;

-- The question this table now answers, asked the way the dashboards ask it: how did the sign-ins
-- of the last while end? Without it that is a scan of every login ever minted.
CREATE INDEX IF NOT EXISTS steam_logins_created ON steam_logins (created_at);
