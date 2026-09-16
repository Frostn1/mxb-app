-- How someone became a creator, now that mxbsecure.com is open to sign up for.
--
-- 'self' is a Steam account that signed itself up on the site; 'admin' is one added from the
-- creators page. Null is every creator from before signup opened: invited by hand, and left
-- as they are rather than relabelled as something they never did.
--
-- Standing itself is still `creator_at`, and nothing reads this column to decide anything.
-- It exists so the creators page can tell a wave of signups from the people we invited, which
-- is the question an open door creates and the one `creator_at` alone cannot answer.
ALTER TABLE accounts ADD COLUMN creator_source TEXT;
