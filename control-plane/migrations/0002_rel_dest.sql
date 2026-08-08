-- Where a paint has to land in the receiver's mods folder.
--
-- The uploader already knows this — the app resolves it when it gathers the files — and
-- every install shares the same layout, so carrying it is far more reliable than having
-- each receiver re-derive a path from the slot name. It is also, for exactly that reason,
-- attacker-controlled input that becomes a path on someone else's disk, so it is validated
-- on the way in and again on the way out.
ALTER TABLE loadout_paints ADD COLUMN rel_dest TEXT NOT NULL DEFAULT '';
