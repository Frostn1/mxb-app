-- The server manager now talks to mxbserver's own admin API directly (mxb-agent is dead);
-- rename the stored credential columns to match. Same values, same meaning: an HTTPS base URL
-- and its bearer token, just no longer named after the agent that used to sit in front of it.
ALTER TABLE managed_servers RENAME COLUMN agent_url TO server_url;
ALTER TABLE managed_servers RENAME COLUMN agent_token TO admin_token;
