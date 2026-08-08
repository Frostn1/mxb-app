-- Template for seeding a fresh environment. Copy to `seed.sql`, fill in real values, run it.
--
-- `seed.sql` is gitignored on purpose: invite codes are credentials, and this repository is
-- public. Issue new codes by hand rather than replaying them from source control.
--
--   npx wrangler d1 execute mxb-control-plane --remote --file seed.sql

INSERT INTO servers (id, name, region, address, agent_url, created_at)
VALUES (
  'eu-frankfurt-1',
  'Frost Test EU',
  'eu-central-1',
  '203.0.113.10:54210',        -- host:port, the form -directconnect takes
  'http://203.0.113.10:8787',  -- mxb-agent base URL, or NULL
  unixepoch() * 1000
);

INSERT INTO invites (code, note, created_at) VALUES
  ('XXXX-XXXX-XXXX', 'who this is for', unixepoch() * 1000);
