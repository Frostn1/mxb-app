//! Recovering a terrain grid from a track's height file.
//!
//! MX Bikes' `.trh` is undocumented, and no two builds of the track tools have to agree on
//! what precedes the samples. Rather than hard-code a header this build can only guess at,
//! the reader *probes*: it enumerates the layouts a heightfield could plausibly have, then
//! asks each one to prove itself against the bytes.
//!
//! The proof is a property terrain has and noise does not — neighbouring samples are close
//! together. Two independent measurements fall out of that, and between them they pin down
//! the whole layout:
//!
//! * **Horizontal roughness** compares memory-adjacent samples. It doesn't depend on the row
//!   width at all, so it judges only the sample type and where the samples begin. Read a
//!   float grid as 16-bit, or start one byte late, and adjacent values stop being neighbours
//!   — the figure explodes.
//! * **Vertical roughness** compares samples one row apart, so it is exactly the measurement
//!   the row width controls. At the true width it sees real neighbours; at any other width it
//!   is sampling unrelated parts of the terrain, and reads as noise.
//!
//! A layout has to satisfy both to be believed, and [`probe`] reports the margin it won by,
//! so the app can show a recovered terrain as the inference it is rather than as fact.

/// How a height sample is stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sample {
    F32,
    U16,
    I16,
}

impl Sample {
    pub fn size(self) -> usize {
        match self {
            Sample::F32 => 4,
            Sample::U16 | Sample::I16 => 2,
        }
    }

    fn read(self, bytes: &[u8], at: usize) -> f32 {
        match self {
            Sample::F32 => f32::from_le_bytes([
                bytes[at],
                bytes[at + 1],
                bytes[at + 2],
                bytes[at + 3],
            ]),
            Sample::U16 => u16::from_le_bytes([bytes[at], bytes[at + 1]]) as f32,
            Sample::I16 => i16::from_le_bytes([bytes[at], bytes[at + 1]]) as f32,
        }
    }
}

/// One accepted interpretation of a height file.
///
/// Deliberately not `Deserialize`: what gets persisted is the terrain built from a layout,
/// not the layout itself, and `source` being a `&'static str` says so.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// Byte offset the samples start at.
    pub offset: usize,
    pub width: u32,
    pub height: u32,
    pub sample: Sample,
    /// 0–1. How cleanly this layout beat the roughness thresholds — surfaced so a probed
    /// terrain can be labelled as inferred rather than read.
    pub confidence: f32,
    /// Whether the dimensions came from numbers in the file's own header (`header`), from a
    /// hint in the track's `.ini` (`ini`), or from assuming a square grid (`square`).
    pub source: &'static str,
}

/// Integer square root, for testing whether a sample count is a square grid.
fn isqrt(n: usize) -> usize {
    if n < 2 {
        return n;
    }
    let mut x = (n as f64).sqrt() as usize;
    // The float root can land a step either side on large inputs; walk it back on.
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

/// Grids outside this are not terrain we can use: too small to show, or big enough that
/// decoding it would cost more than the view is worth.
const MIN_DIM: u32 = 32;
const MAX_DIM: u32 = 8192;

/// The largest header we'll believe sits in front of the samples. Generous — the cost of a
/// wrong guess is one rejected candidate, and a header this big is still cheap to skip.
const MAX_HEADER: usize = 4096;

/// Roughness above this is noise, not terrain. Expressed as a fraction of the grid's own
/// height spread, so it holds whether the track is a supercross floor or an alpine hillside.
const MAX_ROUGHNESS: f32 = 0.12;

/// Heights beyond this many metres from sea level are not a motocross track — they're a
/// misread. Applies to float layouts only, which can otherwise "succeed" on exponent noise;
/// integer samples are raw steps rather than metres, so this would not be a bound on them.
const MAX_ABS_HEIGHT: f32 = 20_000.0;

/// A candidate layout, before it has been scored.
struct Candidate {
    offset: usize,
    width: u32,
    height: u32,
    sample: Sample,
    source: &'static str,
}

/// Read the layouts worth testing out of `bytes`.
///
/// Three ways in, cheapest first. Dimensions written in the file's own header are believed
/// over anything inferred; a hint from the track's `.ini` comes next; a square grid is the
/// last resort, and the one most in need of the roughness test to back it up.
fn candidates(bytes: &[u8], hint: Option<(u32, u32)>) -> Vec<Candidate> {
    let len = bytes.len();
    let mut out = Vec::new();

    // Any (width, height) we have reason to believe in, wherever it came from. For each one
    // the sample block's size is known, so the header length is whatever is left over —
    // which means we never have to guess where the samples start.
    let mut dims: Vec<(u32, u32, &'static str)> = Vec::new();

    if let Some((w, h)) = hint {
        dims.push((w, h, "ini"));
    }

    // A header that states its own dimensions almost always does so as two adjacent 32-bit
    // integers near the front. Read every aligned pair in the first 64 bytes and keep the
    // plausible ones; the size check below throws out the coincidences.
    let scan = len.min(64);
    for off in (0..scan.saturating_sub(8)).step_by(4) {
        let w = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
        let h = u32::from_le_bytes([
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]);
        if (MIN_DIM..=MAX_DIM).contains(&w) && (MIN_DIM..=MAX_DIM).contains(&h) {
            dims.push((w, h, "header"));
        }
    }

    for (w, h, source) in dims {
        let count = w as usize * h as usize;
        for sample in [Sample::F32, Sample::U16, Sample::I16] {
            let Some(need) = count.checked_mul(sample.size()) else {
                continue;
            };
            // The samples are taken to run to the end of the file, which derives the header
            // length exactly instead of guessing at it. Guessing is what it looks like it
            // should do, and it can't: terrain read a few samples early is still smooth, so
            // a wrong offset scores as well as the right one and there is nothing in the
            // roughness to separate them. Deriving the offset admits exactly one answer.
            // The cost is that a file with a *trailing* section wouldn't be recognised.
            let Some(offset) = len.checked_sub(need) else {
                continue;
            };
            if offset > MAX_HEADER {
                continue;
            }
            out.push(Candidate {
                offset,
                width: w,
                height: h,
                sample,
                source,
            });
        }
    }

    // Nothing stated the shape: try a square grid, which is what a terrain heightfield
    // usually is. Every header length that leaves a perfect square of samples is a
    // candidate, and the roughness test decides between them.
    for sample in [Sample::F32, Sample::U16, Sample::I16] {
        let size = sample.size();
        for offset in (0..=MAX_HEADER.min(len)).step_by(4) {
            let rest = len - offset;
            if rest % size != 0 {
                continue;
            }
            let count = rest / size;
            let side = isqrt(count);
            if side * side != count {
                continue;
            }
            let side = side as u32;
            if !(MIN_DIM..=MAX_DIM).contains(&side) {
                continue;
            }
            out.push(Candidate {
                offset,
                width: side,
                height: side,
                sample,
                source: "square",
            });
        }
    }

    out
}

/// Mean absolute difference between pairs of samples `stride` apart, and the value spread,
/// measured over a bounded sample of the grid.
///
/// Bounded because this runs for every candidate: a full pass over a 4096² float grid, times
/// the dozens of layouts probed, is seconds of work to answer a question a few thousand
/// samples settle just as well.
fn roughness(bytes: &[u8], c: &Candidate, stride: u32) -> Result<(f32, f32), &'static str> {
    let size = c.sample.size();
    let w = c.width as usize;
    let h = c.height as usize;

    // Walk a grid of probe points rather than a contiguous block: a corner of a heightfield
    // can be flat apron while the middle is the track itself.
    let rows = h.min(48);
    let cols = w.min(48);
    let row_step = (h / rows).max(1);
    let col_step = (w / cols).max(1);

    let mut diff_total = 0.0f64;
    let mut diff_count = 0usize;
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;

    let mut y = 0usize;
    while y < h {
        let mut x = 0usize;
        while x < w {
            let i = y * w + x;
            // The partner sample: one column over, or one row down.
            let j = if stride == 1 {
                if x + 1 >= w {
                    x += col_step;
                    continue;
                }
                i + 1
            } else {
                if y + 1 >= h {
                    break;
                }
                i + w
            };

            let a = c.sample.read(bytes, c.offset + i * size);
            let b = c.sample.read(bytes, c.offset + j * size);
            // A float layout that isn't one reads as NaN and infinity long before it reads
            // as rough, so reject on sight rather than letting it poison the average.
            if !a.is_finite() || !b.is_finite() {
                return Err("samples aren't finite numbers");
            }
            // Metres only. An integer sample is a raw step whose scale the file sets
            // elsewhere, so judging it against a height in metres would throw out any
            // 16-bit heightfield that happens to use most of its range.
            if c.sample == Sample::F32 && (a.abs() > MAX_ABS_HEIGHT || b.abs() > MAX_ABS_HEIGHT) {
                return Err("heights are implausibly far from sea level");
            }
            diff_total += (a - b).abs() as f64;
            diff_count += 1;
            min = min.min(a);
            max = max.max(a);
            x += col_step;
        }
        y += row_step;
    }

    if diff_count == 0 {
        return Err("no sample pairs to compare");
    }
    Ok(((diff_total / diff_count as f64) as f32, max - min))
}

/// What the roughness test made of a candidate.
enum Assessment {
    /// Confidence 0–1, with the two measurements that produced it.
    Ok { confidence: f32, rough_x: f32, rough_y: f32 },
    /// Why it was thrown out. A short phrase, for the diagnostic report.
    Rejected(&'static str),
}

/// Measure a candidate against the bytes.
fn assess(bytes: &[u8], c: &Candidate) -> Assessment {
    let need = c.offset + c.width as usize * c.height as usize * c.sample.size();
    if need > bytes.len() {
        return Assessment::Rejected("runs past the end of the file");
    }

    let (dx, spread) = match roughness(bytes, c, 1) {
        Ok(v) => v,
        Err(why) => return Assessment::Rejected(why),
    };
    // A grid with no relief is either genuinely empty or a run of padding. Either way there
    // is nothing to show and nothing to validate against.
    if !(spread.is_finite() && spread > 0.0) {
        return Assessment::Rejected("no relief at all");
    }
    let (dy, _) = match roughness(bytes, c, c.width) {
        Ok(v) => v,
        Err(why) => return Assessment::Rejected(why),
    };

    // Both measured against the terrain's own relief, so the thresholds don't care whether
    // heights are metres or raw 16-bit steps.
    let rough_x = dx / spread;
    let rough_y = dy / spread;
    if rough_x > MAX_ROUGHNESS {
        return Assessment::Rejected("adjacent samples aren't neighbours (wrong type/offset)");
    }
    if rough_y > MAX_ROUGHNESS {
        return Assessment::Rejected("rows aren't neighbours (wrong width)");
    }

    // How far inside the threshold it landed, worst axis first — a layout that only just
    // scraped in should not present itself as a confident read.
    let worst = rough_x.max(rough_y);
    Assessment::Ok {
        confidence: (1.0 - worst / MAX_ROUGHNESS).clamp(0.0, 1.0),
        rough_x,
        rough_y,
    }
}

/// Score a candidate 0–1, or reject it.
fn score(bytes: &[u8], c: &Candidate) -> Option<f32> {
    match assess(bytes, c) {
        Assessment::Ok { confidence, .. } => Some(confidence),
        Assessment::Rejected(_) => None,
    }
}

/// A human-readable account of what the probe made of `bytes`: every layout it considered,
/// what each measured, and why the losers lost.
///
/// This exists because the format is undocumented. When a real height file doesn't read as
/// terrain, "no terrain found" is not enough to act on — the useful question is which
/// layouts were tried and how close each came, and that answer has to be obtainable from a
/// machine that has the track on it rather than from the one holding this code.
pub fn report(bytes: &[u8], hint: Option<(u32, u32)>) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(out, "{} bytes, hint {hint:?}", bytes.len());

    let all = candidates(bytes, hint);
    if all.is_empty() {
        out.push_str(
            "no candidate layouts at all — the file size doesn't fit any grid this reads.\n",
        );
        return out;
    }

    // Accepted first and best-scoring at the top, since that's what the probe would pick;
    // the near-misses that follow are what tell you how to widen the search.
    let mut rows: Vec<(f32, String)> = Vec::new();
    for c in &all {
        let dims = format!(
            "{:>5}x{:<5} {:?} @{:<6} [{}]",
            c.width, c.height, c.sample, c.offset, c.source
        );
        match assess(bytes, c) {
            Assessment::Ok {
                confidence,
                rough_x,
                rough_y,
            } => rows.push((
                confidence,
                format!("  OK   {dims} confidence {confidence:.2} (x {rough_x:.4}, y {rough_y:.4})"),
            )),
            Assessment::Rejected(why) => rows.push((-1.0, format!("  --   {dims} {why}"))),
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));

    let _ = writeln!(out, "{} candidate layouts (threshold {MAX_ROUGHNESS}):", all.len());
    for (_, line) in rows.iter().take(40) {
        let _ = writeln!(out, "{line}");
    }
    if rows.len() > 40 {
        let _ = writeln!(out, "  … {} more", rows.len() - 40);
    }

    match probe(bytes, hint) {
        Some(l) => {
            let _ = writeln!(
                out,
                "chose {}x{} {:?} at {} via {} (confidence {:.2})",
                l.width, l.height, l.sample, l.offset, l.source, l.confidence
            );
        }
        None => out.push_str("chose nothing — no layout read as terrain.\n"),
    }
    out
}

/// Recover the terrain grid in `bytes`, or `None` if nothing in it reads as terrain.
///
/// `hint` is a (width, height) from the track's `.ini` when it names one — believed ahead of
/// anything inferred, but still made to pass the same roughness test as every other layout.
pub fn probe(bytes: &[u8], hint: Option<(u32, u32)>) -> Option<Layout> {
    let mut best: Option<Layout> = None;

    for c in candidates(bytes, hint) {
        let Some(confidence) = score(bytes, &c) else {
            continue;
        };
        let layout = Layout {
            offset: c.offset,
            width: c.width,
            height: c.height,
            sample: c.sample,
            confidence,
            source: c.source,
        };
        // Ties on confidence go to the layout with the better provenance, then to the larger
        // grid: a stated shape beats a guessed one, and a half-width misread of a real grid
        // scores similarly to the truth while showing half the track.
        let better = match &best {
            None => true,
            Some(b) => (
                confidence,
                rank(layout.source),
                layout.width as u64 * layout.height as u64,
            ) > (b.confidence, rank(b.source), b.width as u64 * b.height as u64),
        };
        if better {
            best = Some(layout);
        }
    }

    best
}

fn rank(source: &str) -> u8 {
    match source {
        "header" => 3,
        "ini" => 2,
        _ => 1,
    }
}

/// Read a probed grid out as metres, downsampled so the longest edge is at most `max_dim`.
///
/// Downsampling happens here rather than in the app because the whole point is to not ship a
/// 4096² grid over the IPC channel: at four bytes a sample that is 64 MB for a view that
/// resolves a few hundred points across.
///
/// Samples are averaged over the block they replace rather than picked from its corner.
/// Point-sampling a heightfield drops every berm narrower than the step and leaves the rest
/// crawling as the level of detail changes.
pub fn read_grid(bytes: &[u8], layout: &Layout, max_dim: u32) -> (u32, u32, Vec<f32>) {
    let w = layout.width as usize;
    let h = layout.height as usize;
    let size = layout.sample.size();

    let step = ((w.max(h) as f32) / max_dim.max(1) as f32).ceil().max(1.0) as usize;
    let out_w = w.div_ceil(step);
    let out_h = h.div_ceil(step);

    let mut out = Vec::with_capacity(out_w * out_h);
    for oy in 0..out_h {
        for ox in 0..out_w {
            let mut total = 0.0f64;
            let mut count = 0usize;
            for y in oy * step..((oy + 1) * step).min(h) {
                for x in ox * step..((ox + 1) * step).min(w) {
                    let v = layout.sample.read(bytes, layout.offset + (y * w + x) * size);
                    if v.is_finite() {
                        total += v as f64;
                        count += 1;
                    }
                }
            }
            out.push(if count == 0 {
                0.0
            } else {
                (total / count as f64) as f32
            });
        }
    }

    (out_w as u32, out_h as u32, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth synthetic heightfield — rolling relief with a ridge through it, which is
    /// close enough to terrain for the roughness test to behave as it would on a real track.
    fn terrain(w: u32, h: u32) -> Vec<f32> {
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let fx = x as f32 / w as f32;
                let fy = y as f32 / h as f32;
                let rolling = (fx * 6.0).sin() * 4.0 + (fy * 4.5).cos() * 3.0;
                let ridge = (-((fy - 0.5) * (fy - 0.5)) * 30.0).exp() * 8.0;
                out.push(20.0 + rolling + ridge);
            }
        }
        out
    }

    fn with_header(heights: &[f32], header: &[u8]) -> Vec<u8> {
        let mut bytes = header.to_vec();
        for v in heights {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn recovers_a_square_grid_with_no_header() {
        let heights = terrain(256, 256);
        let bytes = with_header(&heights, &[]);
        let layout = probe(&bytes, None).expect("a plain square grid should be recovered");
        assert_eq!((layout.width, layout.height), (256, 256));
        assert_eq!(layout.offset, 0);
        assert_eq!(layout.sample, Sample::F32);
    }

    #[test]
    fn recovers_dimensions_stated_in_the_header() {
        // Non-square, so nothing but the header could have supplied the shape.
        let heights = terrain(320, 192);
        let mut header = Vec::new();
        header.extend_from_slice(b"TRH\0");
        header.extend_from_slice(&320u32.to_le_bytes());
        header.extend_from_slice(&192u32.to_le_bytes());
        header.extend_from_slice(&[0u8; 20]);
        let bytes = with_header(&heights, &header);

        let layout = probe(&bytes, None).expect("stated dimensions should be recovered");
        assert_eq!((layout.width, layout.height), (320, 192));
        assert_eq!(layout.source, "header");
        assert_eq!(layout.offset, header.len());
    }

    #[test]
    fn recovers_a_16_bit_grid() {
        let heights = terrain(256, 256);
        let mut bytes = Vec::new();
        for v in &heights {
            // Raw 16-bit steps, as a packed heightfield would store them.
            bytes.extend_from_slice(&((v * 100.0) as u16).to_le_bytes());
        }
        let layout = probe(&bytes, None).expect("a 16-bit grid should be recovered");
        assert_eq!(layout.sample, Sample::U16);
        assert_eq!((layout.width, layout.height), (256, 256));
    }

    #[test]
    fn recovers_a_16_bit_grid_that_uses_most_of_its_range() {
        // Raw steps spanning nearly the whole 16-bit range — a packed heightfield with a
        // fine vertical resolution. These are not metres, and judging them as if they were
        // is what used to throw this out.
        let heights = terrain(256, 256);
        let lo = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut bytes = Vec::new();
        for v in &heights {
            let step = ((v - lo) / (hi - lo) * 64_000.0) as u16;
            bytes.extend_from_slice(&step.to_le_bytes());
        }

        let layout = probe(&bytes, None).expect("a full-range 16-bit grid should be recovered");
        assert_eq!(layout.sample, Sample::U16);
        assert_eq!((layout.width, layout.height), (256, 256));
    }

    #[test]
    fn takes_the_dimensions_hinted_by_the_ini() {
        let heights = terrain(384, 128);
        let bytes = with_header(&heights, &[0u8; 16]);
        let layout = probe(&bytes, Some((384, 128))).expect("the hinted shape should be recovered");
        assert_eq!((layout.width, layout.height), (384, 128));
        assert_eq!(layout.source, "ini");
    }

    #[test]
    fn rejects_noise() {
        // A deterministic pseudo-random fill: the same size as a real grid, none of the
        // structure. Nothing here should read as terrain at any layout.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut bytes = Vec::new();
        for _ in 0..256 * 256 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.extend_from_slice(&((state >> 40) as u16).to_le_bytes());
        }
        assert!(probe(&bytes, None).is_none(), "noise must not read as terrain");
    }

    #[test]
    fn rejects_a_wrong_width_in_favour_of_the_right_one() {
        // The vertical measurement is the only thing separating these: read at half width,
        // rows stop being neighbours even though the bytes are unchanged.
        let heights = terrain(256, 256);
        let bytes = with_header(&heights, &[]);
        let layout = probe(&bytes, Some((128, 512))).expect("recovered");
        assert_eq!(
            (layout.width, layout.height),
            (256, 256),
            "the hint was wrong and should have lost to the layout that reads as terrain",
        );
    }

    #[test]
    fn rejects_a_file_that_is_all_one_height() {
        let bytes = with_header(&vec![12.0f32; 256 * 256], &[]);
        assert!(
            probe(&bytes, None).is_none(),
            "a flat grid has no relief to show and nothing to validate against",
        );
    }

    #[test]
    fn downsamples_by_averaging() {
        let heights = terrain(256, 256);
        let bytes = with_header(&heights, &[]);
        let layout = probe(&bytes, None).unwrap();

        let (w, h, grid) = read_grid(&bytes, &layout, 64);
        assert_eq!((w, h), (64, 64));
        assert_eq!(grid.len(), 64 * 64);

        // Averaging preserves the overall relief, so the downsampled range should sit inside
        // the original's without collapsing to nothing.
        let full_min = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let full_max = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min = grid.iter().copied().fold(f32::INFINITY, f32::min);
        let max = grid.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(min >= full_min - 0.01 && max <= full_max + 0.01);
        assert!(max - min > (full_max - full_min) * 0.5);
    }

    #[test]
    fn downsampling_a_small_grid_leaves_it_alone() {
        let heights = terrain(64, 64);
        let bytes = with_header(&heights, &[]);
        let layout = probe(&bytes, None).unwrap();
        let (w, h, _) = read_grid(&bytes, &layout, 256);
        assert_eq!((w, h), (64, 64));
    }

    /// Point this at a real height file to see exactly what the probe makes of it:
    ///
    /// ```text
    /// FROST_PROBE_FILE=/path/to/Track.trh \
    ///   cargo test -- --ignored --nocapture probe_a_real_height_file
    /// ```
    ///
    /// Optionally set `FROST_PROBE_DIMS=512x512` to feed it a shape hint. Ignored by
    /// default because it needs a file this repository can't carry: the format is
    /// undocumented, so the only way to confirm the probe reads a real track is to run it
    /// against one, on a machine that has one.
    #[test]
    #[ignore = "needs a real height file — set FROST_PROBE_FILE"]
    fn probe_a_real_height_file() {
        let path = std::env::var("FROST_PROBE_FILE")
            .expect("set FROST_PROBE_FILE to a .trh/.map to inspect");
        let bytes = std::fs::read(&path).expect("read the height file");
        let hint = std::env::var("FROST_PROBE_DIMS").ok().and_then(|v| {
            let (w, h) = v.split_once(['x', 'X', ','])?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        });
        println!("{path}\n{}", report(&bytes, hint));
    }

    #[test]
    fn isqrt_is_exact_at_boundaries() {
        for n in [0usize, 1, 2, 3, 4, 255, 256, 257, 65535, 65536, 16_777_216] {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "isqrt({n}) = {r}");
        }
    }
}
