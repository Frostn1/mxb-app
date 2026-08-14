-- What each client's own cheat scan found, so a server admin can see it.
--
-- MX Bikes has no anti-cheat, so a server has no way to ask a joining client anything about
-- itself. This is the smallest thing that changes that: the player's app scans the running
-- game (see `src-tauri/src/integrity.rs`) and publishes the verdict here, and the admin of
-- the server they are on can read it back.
--
-- It is attestation, not enforcement, and the table is shaped to keep that honest. A cheater
-- who does not run MXB App has no row. One who turns reporting off has no row. So the read
-- answers "who has attested and what did they say" — never "who is clean" — and a missing
-- row is the most common state, not a suspicious one.
--
-- One row per account, rewritten on every heartbeat, exactly like `presence`: this is live
-- state, not a history. Deliberately not a column on `accounts` for the same reason presence
-- isn't — a verdict is short-lived and rewritten constantly, an identity is neither.
CREATE TABLE integrity (
  account_id    TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  -- 'unknown' | 'clean' | 'suspect' | 'flagged'. Text rather than an integer so a row read
  -- straight out of the database says what it means.
  verdict       TEXT NOT NULL,
  -- Which rule list produced it. A 'clean' checked against an empty list means much less
  -- than one checked against the current one, and an admin can only tell them apart if the
  -- version travels with the verdict.
  rules_version INTEGER NOT NULL,
  -- How many loaded modules were unrecognised but matched no rule. A count and never a
  -- list: "software we don't recognise" is not evidence, and on somebody's machine it could
  -- be anything. The client never sends their names.
  suspect_count INTEGER NOT NULL,
  -- JSON array of {name,label,sha256} for rule-matched detections only. Those are named,
  -- because a known cheat is the thing worth naming.
  flagged       TEXT NOT NULL,
  -- The worst verdict this account has reported recently, and when.
  --
  -- Without it the feature is trivially defeated: load the cheat for one lap, unload it, and
  -- the next 45-second heartbeat overwrites 'flagged' with 'clean' as if nothing happened.
  -- The read only surfaces this while `worst_at` is inside the same freshness window the
  -- roster uses, so it describes the current session and then lets go — a verdict about
  -- somebody should not outlive the race it was about.
  worst_verdict TEXT NOT NULL,
  worst_at      INTEGER NOT NULL,
  -- Refreshed on every publish. A row older than the cutoff is treated as gone, so an admin
  -- can tell "reported clean a moment ago" from "stopped reporting when the race started".
  updated_at    INTEGER NOT NULL
);
