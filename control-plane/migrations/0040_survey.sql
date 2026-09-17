-- Asking the player, rather than inferring from counters.
--
-- The usage tables answer "what do people open". They cannot answer "is this any good", and
-- every attempt to read that off a counter is a guess dressed as a number: a feature with high
-- reach is one people find, not one they like. The only way to know is to ask, and nothing in
-- this deployment could ask anything.
--
-- Two tables, for the two halves of asking. `survey_polls` is the question — written here,
-- served to every install, so a new one reaches the field the day it is written rather than
-- the month the next release does. `survey_answers` is what came back.
--
-- Nothing here joins to `accounts`, exactly as `usage_daily` does not: the key is the same
-- install id the counters use, a random UUID the app minted for itself. An answer is tied to
-- no person.

-- The questions in the field, and the ones that used to be.
--
-- Rows rather than a constant in the worker, because that is the whole point: "have you tried
-- Race mode" is worth asking for three weeks and worth nothing afterwards, and neither the
-- writing of it nor the retiring of it should need a deploy — let alone the app release that a
-- question baked into the client would need, plus the month it takes to reach everybody.
CREATE TABLE survey_polls (
  -- A slug, `[a-z0-9-]`, chosen when the poll is written. It is what an answer carries, so it
  -- must never be reused for a different question: the rows would add up as one.
  id            TEXT PRIMARY KEY,
  -- 'mood'   — the standing "how is it going" question. The app draws its own three answers
  --            (bad / fine / good) and its own words for them, in the player's language, so
  --            this one is legible in six languages without anything being written here.
  -- 'choice' — anything else. The question and its answers come from this row, so they are in
  --            whatever languages `ask` and `choices` were written in.
  kind          TEXT NOT NULL DEFAULT 'choice',
  -- Which apps ask it, comma separated, from `manager,studio,coach`. A question about the
  -- track Designer has no business interrupting a rider in Coach.
  apps          TEXT NOT NULL DEFAULT 'manager,studio,coach',
  -- The question, as JSON: locale -> string, e.g. {"en":"…","it":"…"}. The app falls back to
  -- `en`, so a poll written in English alone still asks everybody — in English.
  ask           TEXT NOT NULL DEFAULT '{}',
  -- The answers, as JSON: [{"id":"yes-often","label":{"en":"Yes, often"}}]. The id is what
  -- travels back and what the dashboard groups on; the label is only ever shown.
  choices       TEXT NOT NULL DEFAULT '[]',
  -- The follow-up's chips, same shape. Empty means the app's own built-in list, which is what
  -- the mood poll wants: those five reasons are shipped translated.
  reasons       TEXT NOT NULL DEFAULT '[]',
  -- Choice ids that ALWAYS get the follow-up, as a JSON array. For the mood poll this is
  -- ["bad"]: somebody who just said it is going badly is the one person worth asking why.
  follow_up     TEXT NOT NULL DEFAULT '[]',
  -- How often the follow-up comes after any *other* answer, 0–1. Not always, deliberately: a
  -- second question every time is what turns one tap into a form, and a quarter of the answers
  -- is plenty to read a pattern off.
  follow_up_chance REAL NOT NULL DEFAULT 0.25,
  -- Whether the follow-up offers the optional free-text box at all. It is the one field in
  -- this whole deployment that can carry anything the player types, so a poll that has no use
  -- for prose turns it off rather than inviting it.
  note          INTEGER NOT NULL DEFAULT 1,
  -- Days before the same install is asked again. 0 means ask once, ever, which is what a
  -- question about one feature wants. The mood poll wants to come back, or there is no trend
  -- to read — just not often enough to become furniture.
  again_days    INTEGER NOT NULL DEFAULT 0,
  -- Off without deleting: the answers outlive the question, and a retired poll still has to
  -- be nameable on the dashboard that shows them.
  live          INTEGER NOT NULL DEFAULT 1,
  -- Optional window, in unix ms. A poll about a release can be written the week before it.
  starts_at     INTEGER,
  ends_at       INTEGER,
  -- Only ask builds at or past this version. A question about something that shipped in 0.16
  -- asked of an 0.14 install collects nothing but noise. Empty asks everybody.
  min_version   TEXT NOT NULL DEFAULT '',
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);
CREATE INDEX survey_polls_live ON survey_polls (live);

-- One answer per install per poll per day.
--
-- The day is in the key for the same reason the usage rollups carry one: it bounds what a
-- single install can put in the table however many times it posts, and it makes a re-send
-- after a failed request idempotent rather than a second opinion. A poll with `again_days`
-- set gets one row per time it comes round, which is exactly the trend it exists for.
CREATE TABLE survey_answers (
  poll_id     TEXT NOT NULL,
  install_id  TEXT NOT NULL,
  -- Which app asked. The same machine runs three of them and they are three different
  -- questions about three different products.
  app         TEXT NOT NULL,
  -- UTC, YYYY-MM-DD, as everywhere else here.
  day         TEXT NOT NULL,
  version     TEXT NOT NULL,
  os          TEXT NOT NULL,
  game        TEXT NOT NULL,
  -- The choice id. For the mood poll: 'bad', 'fine' or 'good'.
  choice      TEXT NOT NULL,
  -- The follow-up's chips, comma separated ids, empty when it was not asked or not answered.
  -- A joined string rather than a fourth table: they are only ever counted, never joined, and
  -- the counting is a LIKE over a few thousand rows.
  reasons     TEXT NOT NULL DEFAULT '',
  -- The optional note, and the only free text in this deployment. NULL when none was typed.
  -- Cleared by the sweep well before the row itself goes — see NOTE_RETENTION_DAYS.
  note        TEXT,
  answered_at INTEGER NOT NULL,
  PRIMARY KEY (poll_id, install_id, day)
);
CREATE INDEX survey_answers_day ON survey_answers (day);
CREATE INDEX survey_answers_poll_day ON survey_answers (poll_id, day);

-- The standing question, seeded so the feature works the moment this lands rather than the
-- moment somebody remembers to write a poll. `again_days` is 45 rather than 1: a rating box
-- every day is not research, it is a tax on opening the app, and the trend a monthly-ish
-- answer draws is the same trend at a fraction of the interruption. Raise or lower it here —
-- it is a column precisely so nobody needs a release to change their mind about it.
INSERT INTO survey_polls
  (id, kind, apps, ask, choices, reasons, follow_up, follow_up_chance, note, again_days, live,
   starts_at, ends_at, min_version, created_at, updated_at)
VALUES
  ('mood', 'mood', 'manager,studio,coach', '{}', '[]', '[]', '["bad"]', 0.25, 1, 45, 1,
   NULL, NULL, '', CAST(strftime('%s', 'now') AS INTEGER) * 1000,
   CAST(strftime('%s', 'now') AS INTEGER) * 1000);
