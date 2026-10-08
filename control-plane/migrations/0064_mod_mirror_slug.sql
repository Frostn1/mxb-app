-- The MXB App browses mxb-mods.com and names a mod by its slug; `GET /v1/assets/mirror/<slug>`
-- finds our copy of that post by it.
CREATE INDEX IF NOT EXISTS mod_assets_mirror_slug ON mod_assets (slug) WHERE source = 'mirror';
