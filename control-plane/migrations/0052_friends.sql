-- Friends: accounts that have agreed to see each other, and where an accepted friend is playing.
--
-- Three tables, none of them on `accounts`: identity is permanent, a friendship is a relation
-- between two accounts, and presence is rewritten every half minute.
--
-- `friend_profiles` holds the shareable friend code and the privacy switch. It is created
-- lazily the first time an account opens the Friends panel, so no backfill is needed.
CREATE TABLE friend_profiles (
  account_id    TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  -- Eight characters from an alphabet without look-alikes, stored without the dash.
  friend_code   TEXT NOT NULL UNIQUE,
  -- 1 = "hide my presence": nothing is stored and friends see the rider as offline.
  hide_presence INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL
);

-- One row per pair, whoever asked first. `declined` is kept rather than deleted so a declined
-- request cannot simply be sent again; the requester is never told, and sees it as pending.
CREATE TABLE friendships (
  requester_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  addressee_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  status       TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'declined')),
  created_at   INTEGER NOT NULL,
  responded_at INTEGER,
  PRIMARY KEY (requester_id, addressee_id),
  CHECK (requester_id <> addressee_id)
);
-- The pair is unique in either direction, so two riders asking each other at once is one row.
CREATE UNIQUE INDEX friendships_pair
  ON friendships (min(requester_id, addressee_id), max(requester_id, addressee_id));
CREATE INDEX friendships_addressee ON friendships (addressee_id, status);

-- Where the rider is, as their own app reports it. Only ever read for accepted friends.
-- A row older than the TTL is treated as gone, so a crashed app does not haunt a server.
CREATE TABLE friend_presence (
  account_id  TEXT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
  -- The server's name as the game shows it. Also how a friend's app finds the row in its list.
  server_name TEXT NOT NULL,
  -- `host:port` when the rider's app launched the game itself; null when it only saw the name.
  address     TEXT,
  track       TEXT,
  riders      INTEGER,
  updated_at  INTEGER NOT NULL
);
