-- Servers we launched ourselves, and what it takes to turn them off again.
--
-- A registered server costs nothing — someone else runs it. A provisioned one bills by the
-- hour from the moment it boots, so the columns here exist almost entirely to answer "can
-- this be switched off, and should it be?" rather than to describe the server.

-- The EC2 instance behind this row. Null for servers someone else runs, which is most of
-- them; only a non-null value means deleting this row has to stop a machine as well.
ALTER TABLE servers ADD COLUMN instance_id TEXT;

-- The agent's bearer token, for servers we provisioned and therefore generated it for.
--
-- Held because the idle reaper has to ask the agent who is connected, and it runs on a
-- schedule with no user present to supply anything. For a server someone else runs this
-- stays null: their token is theirs, and the app talks to it directly.
ALTER TABLE servers ADD COLUMN agent_token TEXT;

-- When this server was first seen with nobody on it, cleared the moment someone joins.
--
-- The whole idle-shutdown decision hangs off this one column. A single empty poll means
-- nothing — a server is empty between races, and while the first rider is still loading —
-- so shutdown waits for it to have been empty continuously, which is what a timestamp
-- expresses and a boolean cannot.
ALTER TABLE servers ADD COLUMN idle_since INTEGER;

-- The reaper sweeps by instance, so this is the index it actually uses.
CREATE INDEX servers_instance ON servers (instance_id) WHERE instance_id IS NOT NULL;
