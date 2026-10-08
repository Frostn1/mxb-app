-- The Steam name of a host as of their last sign-in, for the operator page.
ALTER TABLE host_users ADD COLUMN display_name TEXT;
