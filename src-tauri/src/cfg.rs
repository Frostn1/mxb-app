//! Parser for MX Bikes' plain-text config files — a bike's **`gfx.cfg`** and its
//! per-part **`.hrc`** files. Both ship *unencrypted* inside the bike's `.pkz`, and
//! they state outright what the viewer used to guess from mesh-group names:
//! which node is a part's level0 (vs. its LOD variants), and which texture a named
//! mesh group binds to.
//!
//! The format is `key = value` lines plus nested `name { … }` blocks; `;` starts a
//! comment. Real `gfx.cfg`:
//!
//! ```text
//! chassis
//! {
//!     model { file = chassis.hrc }
//!     chain { name = chain  texture = chain  axis = u-  ratio = 0.035 }
//!     plate { texture = w_plate }
//! }
//! tyres = oem_mx
//! ```
//!
//! and a `.hrc`, which names the LODs explicitly:
//!
//! ```text
//! level0 { scene = model.edf              switch = 0  }
//! level1 { scene = model.edf  name = chassisb  switch = 10 }
//! ```
//!
//! `level0` is the renderable detail level. Its node name is the block's `name` if
//! present, else the `.hrc`'s own stem (`chassis.hrc` → node `chassis`) — verified
//! against the real Honda CRF450R, whose `chassis.hrc` omits `name` on level0 but
//! whose `fsusp.hrc` states `name = fsusp`.

use std::collections::HashMap;

/// A parsed config node: scalar `key = value` entries plus nested blocks. Keys are
/// lowercased; a repeated key keeps the last value (the format has no arrays).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CfgNode {
    pub values: HashMap<String, String>,
    pub blocks: HashMap<String, CfgNode>,
}

impl CfgNode {
    /// Scalar value of `key`, if present.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Nested block named `key`, if present.
    pub fn block(&self, key: &str) -> Option<&CfgNode> {
        self.blocks.get(key)
    }
}

/// Parse a `.cfg` / `.hrc` byte buffer into a tree.
///
/// Tolerant by design — these files are hand-authored by mod makers, so an
/// unparseable line is skipped rather than failing the whole bike. An opening
/// brace may sit on the key's line or the next one; both appear in the wild
/// (`gfx.cfg`'s `cockpit` block indents inconsistently).
pub fn parse(bytes: &[u8]) -> CfgNode {
    let text = String::from_utf8_lossy(bytes);
    let mut toks: Vec<&str> = Vec::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        toks.push(line);
    }
    let mut i = 0;
    parse_block(&toks, &mut i)
}

/// Parse tokens into a node, consuming up to (and including) the matching `}`.
fn parse_block(toks: &[&str], i: &mut usize) -> CfgNode {
    let mut node = CfgNode::default();
    // A key seen with no `=` yet — its `{` may be on the following line.
    let mut pending: Option<String> = None;
    while *i < toks.len() {
        let line = toks[*i];
        *i += 1;
        if line == "}" {
            break;
        }
        if line == "{" {
            if let Some(name) = pending.take() {
                let child = parse_block(toks, i);
                node.blocks.insert(name, child);
            }
            continue;
        }
        // `key { ... }` / `key {` on one line.
        if let Some((head, rest)) = line.split_once('{') {
            let head = head.trim();
            if !head.is_empty() && !head.contains('=') {
                let name = head.to_ascii_lowercase();
                // Re-feed the remainder of the line so `k { a = 1 }` parses.
                let rest = rest.trim();
                let mut inner: Vec<&str> = Vec::new();
                if !rest.is_empty() {
                    inner.push(rest);
                }
                if inner.is_empty() {
                    let child = parse_block(toks, i);
                    node.blocks.insert(name, child);
                } else {
                    // One-line block: split on whitespace-separated `k = v` pairs is
                    // ambiguous, so only handle the common `k { key = value }` form.
                    let mut j = 0;
                    let child = parse_block(&inner, &mut j);
                    node.blocks.insert(name, child);
                }
                continue;
            }
        }
        if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim(), v.trim().trim_end_matches('}').trim());
            if !k.is_empty() {
                node.values.insert(k.to_ascii_lowercase(), v.to_string());
            }
            pending = None;
            continue;
        }
        pending = Some(line.trim().to_ascii_lowercase());
    }
    node
}

/// The node name a `.hrc` declares as **level0** — the full-detail mesh.
///
/// `stem` is the `.hrc`'s file stem (`chassis.hrc` → `"chassis"`), which level0
/// falls back to when its block omits `name`. Returns `None` if the file declares
/// no `level0`.
pub fn hrc_level0(cfg: &CfgNode, stem: &str) -> Option<String> {
    let lvl = cfg.block("level0")?;
    Some(lvl.get("name").unwrap_or(stem).to_string())
}

/// Every node name a `.hrc` mentions (level0 **and** its LOD variants), so the
/// caller can tell "an LOD we should drop" from "a node this `.hrc` never claims".
pub fn hrc_all_levels(cfg: &CfgNode, stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (name, blk) in &cfg.blocks {
        if !name.starts_with("level") {
            continue;
        }
        out.push(blk.get("name").unwrap_or(stem).to_string());
    }
    out
}

/// A bike's `gfx.cfg` reduced to what the viewer needs: for each top-level part
/// (`chassis`, `steer`, `front_susp`, `rear_susp`), its `.hrc` file and the
/// `group -> texture` overrides it declares.
#[derive(Debug, Default, Clone)]
pub struct GfxPart {
    /// `model { file = chassis.hrc }`.
    pub hrc: Option<String>,
    /// Mesh-group name (lowercased) → texture name, from blocks carrying
    /// `texture = X` (`chain { texture = chain }`, `plate { texture = w_plate }`).
    /// The group is the block's `name` if it has one, else the block's own name —
    /// `chain { name = chain … }` states both; `plate { texture = w_plate }` only
    /// the latter.
    pub textures: HashMap<String, String>,
}

/// The four top-level part sections a bike's `gfx.cfg` declares. The `cockpit`
/// block repeats them for the first-person view and is ignored.
pub const GFX_PARTS: [&str; 4] = ["chassis", "steer", "front_susp", "rear_susp"];

/// Read a bike's `gfx.cfg` into its per-part `.hrc` + texture overrides.
pub fn parse_gfx(bytes: &[u8]) -> HashMap<String, GfxPart> {
    let root = parse(bytes);
    let mut out = HashMap::new();
    for part in GFX_PARTS {
        let Some(sec) = root.block(part) else { continue };
        let mut gp = GfxPart {
            hrc: sec.block("model").and_then(|m| m.get("file")).map(str::to_string),
            textures: HashMap::new(),
        };
        for (blk_name, blk) in &sec.blocks {
            let Some(tex) = blk.get("texture") else { continue };
            let group = blk.get("name").unwrap_or(blk_name).to_ascii_lowercase();
            gp.textures.insert(group, tex.to_string());
        }
        out.insert(part.to_string(), gp);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real Honda CRF450R `gfx.cfg`, verbatim from the shipped `.pkz` (trimmed
    /// to the sections under test — braces, tabs and all).
    const HONDA_GFX: &[u8] = br#"chassis
{
	model
	{
		file = chassis.hrc
	}
	shadow
	{
		file = model_shadow.edf
	}

	rearbrakepedal
	{
		name = rearbrake_lever
		axis = x
		maxrot = 10
	}

	chain
	{
		name = chain
		pos
		{
			x = -0.65
			y = 0
			z = 0
		}
		texture = chain
		axis = u-
		ratio = 0.035
	}

	plate
	{
		texture = w_plate
	}

	exhaust
	{
		pos
		{
			x = 0.13
			y = 0.9
			z = -0.8
		}
	}
}

steer
{
	model
	{
		file = steer.hrc
	}
	leftgrip
	{
		type = 1
		pos
		{
			x = -0.335
			y = 0.270
			z = -0.050
		}
	}
	plate
	{
		texture = w_plate
	}
}

front_susp
{
	model
	{
		file = fsusp.hrc
	}
}

rear_susp
{
	model
	{
		file = rsusp.hrc
	}
}

tyres = oem_mx
dirt_color=1
"#;

    /// The real `chassis.hrc` — note level0 carries **no** `name`.
    const HONDA_CHASSIS_HRC: &[u8] = br#"level0
{
	scene = model.edf
	switch = 0
}
level1
{
	scene = model.edf
	name = chassisb
	switch = 10
}
level2
{
	scene = model.edf
	name = chassisc
	switch = 20
}"#;

    /// The real `fsusp.hrc` — here level0 **does** carry a `name`.
    const HONDA_FSUSP_HRC: &[u8] = br#"level0
{
	scene = model.edf
	name = fsusp
	switch = 0
}
level1
{
	scene = model.edf
	name = fsuspb
	switch = 10
}"#;

    #[test]
    fn parses_nested_blocks_and_scalars() {
        let root = parse(HONDA_GFX);
        assert_eq!(root.get("tyres"), Some("oem_mx"));
        assert_eq!(root.get("dirt_color"), Some("1"));
        let chassis = root.block("chassis").expect("chassis section");
        assert_eq!(
            chassis.block("model").and_then(|m| m.get("file")),
            Some("chassis.hrc")
        );
        // Deeply nested: chassis > chain > pos > x.
        let pos = chassis
            .block("chain")
            .and_then(|c| c.block("pos"))
            .expect("chain pos");
        assert_eq!(pos.get("x"), Some("-0.65"));
        // The grip position is an independent check on the parse — `gfx.cfg` puts
        // the left grip at exactly (-0.335, 0.270, -0.050).
        let grip = root
            .block("steer")
            .and_then(|s| s.block("leftgrip"))
            .and_then(|g| g.block("pos"))
            .expect("leftgrip pos");
        assert_eq!(
            (grip.get("x"), grip.get("y"), grip.get("z")),
            (Some("-0.335"), Some("0.270"), Some("-0.050"))
        );
    }

    #[test]
    fn strips_comments() {
        let c = parse(b"a = 1 ; trailing comment\n; whole line\nb = 2\n");
        assert_eq!(c.get("a"), Some("1"));
        assert_eq!(c.get("b"), Some("2"));
    }

    #[test]
    fn reads_gfx_hrc_and_texture_overrides() {
        let parts = parse_gfx(HONDA_GFX);
        assert_eq!(parts["chassis"].hrc.as_deref(), Some("chassis.hrc"));
        assert_eq!(parts["steer"].hrc.as_deref(), Some("steer.hrc"));
        assert_eq!(parts["front_susp"].hrc.as_deref(), Some("fsusp.hrc"));
        assert_eq!(parts["rear_susp"].hrc.as_deref(), Some("rsusp.hrc"));

        // `chain { name = chain  texture = chain }` — group taken from `name`.
        assert_eq!(parts["chassis"].textures.get("chain").map(String::as_str), Some("chain"));
        // `plate { texture = w_plate }` — group taken from the block's own name.
        assert_eq!(parts["chassis"].textures.get("plate").map(String::as_str), Some("w_plate"));
        assert_eq!(parts["steer"].textures.get("plate").map(String::as_str), Some("w_plate"));
        // Everything else takes the default — gfx.cfg overrides only these.
        assert_eq!(parts["chassis"].textures.len(), 2);
        assert!(parts["front_susp"].textures.is_empty());
    }

    #[test]
    fn hrc_level0_falls_back_to_the_file_stem() {
        // chassis.hrc's level0 omits `name` → the part's own name.
        let c = parse(HONDA_CHASSIS_HRC);
        assert_eq!(hrc_level0(&c, "chassis").as_deref(), Some("chassis"));
        let mut all = hrc_all_levels(&c, "chassis");
        all.sort();
        assert_eq!(all, vec!["chassis", "chassisb", "chassisc"]);
    }

    #[test]
    fn hrc_level0_uses_an_explicit_name() {
        // fsusp.hrc's level0 states `name = fsusp`.
        let f = parse(HONDA_FSUSP_HRC);
        assert_eq!(hrc_level0(&f, "fsusp").as_deref(), Some("fsusp"));
        let mut all = hrc_all_levels(&f, "fsusp");
        all.sort();
        assert_eq!(all, vec!["fsusp", "fsuspb"]);
    }

    #[test]
    fn missing_level0_reads_as_none() {
        let c = parse(b"level1\n{\nname = chassisb\n}\n");
        assert_eq!(hrc_level0(&c, "chassis"), None);
    }
}
