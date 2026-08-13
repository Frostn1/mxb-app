-- Somewhere to remember the machine image servers launch from.
--
-- Every provisioned server currently downloads and installs from scratch: 2.4 GB of game and
-- 3.8 GB of bikes, every time, which is the whole of the fifteen minutes it takes to arrive.
-- None of that changes between servers, so it only has to be done once — into an image every
-- later server boots from.
--
-- A key/value table rather than a column somewhere: what is stored is a fact about the
-- deployment, not about any one server, and the build is a small state machine (building,
-- waiting for the image, ready) that wants somewhere to keep its place across cron runs.
CREATE TABLE settings (
  key         TEXT PRIMARY KEY,
  value       TEXT NOT NULL,
  updated_at  INTEGER NOT NULL
);

-- Marks the rows that exist to build an image rather than to be ridden on. They are launched,
-- reported on and reaped like any other server, so they live in the same table; without this
-- the idle reaper would treat a builder as an empty server and destroy it mid-install.
ALTER TABLE servers ADD COLUMN role TEXT;
