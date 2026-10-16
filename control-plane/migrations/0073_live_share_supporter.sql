-- Whether the last publish of a live share came from an account with the supporter pack, so
-- subscribers can see a small badge. A flag, not an account: live shares stay unowned by any
-- account (see 0021), and this records nothing about who published beyond that one bit.
ALTER TABLE live_shares ADD COLUMN by_supporter INTEGER NOT NULL DEFAULT 0;
