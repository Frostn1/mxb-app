-- Is MX Bikes' own master server answering, or is it just you?
--
-- That question has never had an answer anyone could point at. When the master goes quiet the
-- game says "connection timeout" and nothing else, so every outage arrives in Discord as a
-- dozen people each convinced their firewall broke, and the standing answer is a troubleshooting
-- list that sends them to reinstall a game that is working fine.
--
-- The control plane cannot check for itself. The master speaks its own protocol over UDP, and a
-- Worker has neither — no datagram socket, and the protocol is not ours to ship. What we do have
-- is a few thousand apps that already talk to it every time somebody opens the Servers tab. So
-- each one says whether its own fetch worked, and the answer is what they agree on: the interesting
-- number was never "is it up" in the abstract, it is "how many other people are seeing this too",
-- which is exactly the question being asked in the channel.
--
-- One row per install per minute, not per fetch. Opening the tab and hammering Refresh is one
-- person having one experience, and letting it be twenty rows would let one frustrated player
-- declare an outage on their own.
CREATE TABLE master_probes (
  -- The same anonymous install id the usage counters use: a UUID the app minted for itself, tied
  -- to no account, no rider and no machine. It is here for one reason — to count *people* rather
  -- than requests — and `isInstallId` is what stops a client sending something identifying instead.
  install_id  TEXT NOT NULL,
  -- Epoch milliseconds floored to the minute. The bucket is what makes a row an upsert.
  minute      INTEGER NOT NULL,
  -- Fetches that worked and fetches that didn't, within that minute.
  ok          INTEGER NOT NULL DEFAULT 0,
  failed      INTEGER NOT NULL DEFAULT 0,
  -- Why the last failure failed, from the closed list in `isProbeReason` — 'timeout', 'dns',
  -- 'auth' and so on. Closed rather than free text because it is reported by every install and
  -- read back on a public page: a free-text field would eventually carry somebody's address.
  -- Empty when the minute only ever succeeded.
  reason      TEXT NOT NULL DEFAULT '',
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (install_id, minute)
);

-- Every read is "the last ten minutes" and every sweep is "older than an hour", so both are this
-- index. Without it the status endpoint scans the table, and the status endpoint is the one thing
-- here that gets hit hardest exactly when something is wrong.
CREATE INDEX master_probes_minute ON master_probes (minute);
