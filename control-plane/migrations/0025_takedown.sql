-- An operator's takedown of a secured asset, apart from the creator's own withdrawal.
--
-- `withdrawn_at` is the creator's switch: they pull an asset from sale and can put it back.
-- A takedown is ours — for an infringing or abusive upload — and the creator must not be able to
-- undo it by restoring. So it is its own column, set and cleared only with an admin key, and
-- `/v1/keys/grant` refuses while it is set whatever `withdrawn_at` says.
ALTER TABLE assets ADD COLUMN taken_down_at INTEGER;
