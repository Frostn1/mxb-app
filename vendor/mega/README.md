Vendored copy of `mega` 0.8.0 (MIT OR Apache-2.0, https://github.com/Hirevo/mega-rs) with three
panics turned into errors / skips: the `MEGA` attribute assert, and the RSA-key `todo!()` plus an
undecryptable node in public folder listings. The release profile aborts on panic, so a bad node
in a MEGA link otherwise closes MXB App. Wired in through `[patch.crates-io]` in the root Cargo.toml.
