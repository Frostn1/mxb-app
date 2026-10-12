-- Builds from the Build page on mxbsecure.com/build (`builds.ts`): a signed-in rider's saved builds,
-- and the ones they sent as a model swap order, worked by the swap creator at /admin/builds.
--
-- `spec` is the build sheet (format `mxb-build/1`) as validated by `parseSpec`: catalogue ids,
-- options and colours only, never a part's real files. The picture lives in the private bucket
-- under `shot_key`. The finished swap ships through the locked-asset flow like any other paid file.
--
-- Stages follow the DGS ticket flow: saved (not sent) -> quote -> pending_payment -> paid ->
-- approval -> complete, or cancelled. One step at a time; payment is confirmed by hand.
CREATE TABLE builds (
  id           TEXT PRIMARY KEY,
  steam_id     TEXT NOT NULL,
  name         TEXT NOT NULL DEFAULT '',
  bike         TEXT NOT NULL,
  ref          TEXT NOT NULL,
  spec         TEXT NOT NULL,
  shot_key     TEXT,
  stage        TEXT NOT NULL DEFAULT 'saved'
               CHECK (stage IN ('saved', 'quote', 'pending_payment', 'paid', 'approval', 'complete', 'cancelled')),
  -- How many GUIDs the swap is locked to. The first is the rider's own Steam ID.
  guids        INTEGER NOT NULL DEFAULT 1,
  note         TEXT,
  -- The creator's quote, USD cents; null until quoted.
  price_cents  INTEGER,
  admin_note   TEXT,
  -- Approval: 'asked' when the creator asks, then 'approved' or 'changes' from the rider.
  approval     TEXT,
  approval_note TEXT,
  sent_at      INTEGER,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);

CREATE INDEX builds_by_steam ON builds (steam_id, created_at DESC);
CREATE INDEX builds_by_stage ON builds (stage, sent_at DESC);
