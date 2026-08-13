-- Let a booting server say how it is getting on.
--
-- A provisioned instance runs ~150 lines of PowerShell with nobody watching: no console, no
-- key pair, so no RDP and no way to read `C:\mxb-bootstrap.log`. Worse, every failure path
-- ends in `Stop-Computer`, and the instance is launched with
-- `InstanceInitiatedShutdownBehavior=terminate` — so a failure destroys the only record of
-- why it failed. From the outside a broken bootstrap and a slow one look identical: a server
-- that never arrives.
--
-- These three columns are where the box reports what it is doing, and what went wrong if it
-- did. They double as the answer to "why has Create a server been spinning for ten minutes".

ALTER TABLE servers ADD COLUMN bootstrap_stage TEXT;
ALTER TABLE servers ADD COLUMN bootstrap_at INTEGER;
-- The tail of the transcript, only ever written on failure. Capped by the Worker before it
-- gets here; a runaway log is not worth a row nobody can read.
ALTER TABLE servers ADD COLUMN bootstrap_log TEXT;
