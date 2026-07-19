use std::collections::HashMap;

/// Keys are lowercased; a repeated key keeps the last value (the format has no arrays).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CfgNode {
    pub values: HashMap<String, String>,
    pub blocks: HashMap<String, CfgNode>,
}

impl CfgNode {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    pub fn block(&self, key: &str) -> Option<&CfgNode> {
        self.blocks.get(key)
    }
}

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

pub fn hrc_level0(cfg: &CfgNode, stem: &str) -> Option<String> {
    let lvl = cfg.block("level0")?;
    Some(lvl.get("name").unwrap_or(stem).to_string())
}

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

#[derive(Debug, Default, Clone)]
pub struct GfxPart {
    /// `model { file = chassis.hrc }`.
    pub hrc: Option<String>,
    /// Mesh-group name (lowercased) → texture name; group is the block's `name`, else the block's own name.
    pub textures: HashMap<String, String>,
}

pub const GFX_PARTS: [&str; 4] = ["chassis", "steer", "front_susp", "rear_susp"];

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
