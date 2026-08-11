//! Recovering a terrain grid from a track's height file.
//!
//! MX Bikes' `.trh` carries a magic and states its own shape, and [`parse_trh`] reads it
//! directly — layout confirmed against a published track. That is the path real tracks take.
//!
//! Everything below it exists for the files that path doesn't fit: a `.map`, a heightfield
//! from a tool that wrote something else, a future revision. For those the reader *probes*:
//! it enumerates the layouts a heightfield could plausibly have, then asks each one to prove
//! itself against the bytes.
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
    /// `trh` when the file described itself, otherwise how the shape was inferred:
    /// `header` (numbers in the file), `ini` (a hint from the track) or `square`.
    pub source: &'static str,
    /// Added to a raw sample before it is scaled.
    ///
    /// A `.trh` stores its samples *signed*, with the terrain's datum at zero, so half the
    /// range is below it. Adding half the range back puts the grid where the game puts it —
    /// verified against the marshal posts of two published tracks, whose stated ground
    /// heights this reproduces to the centimetre.
    pub bias: f32,
    /// Multiply a raw sample by this for metres. `None` when the file doesn't say, in which
    /// case the grid is in raw sample units and its relief means nothing in the world.
    pub height_scale: Option<f32>,
    /// Metres of ground per sample step, when the file states it. `None` means the relief
    /// is real but the footprint it sits on is unknown.
    pub metres_per_sample: Option<f32>,
}

/// The magic that opens a real `.trh`.
const TRH_MAGIC: &[u8; 4] = b"TRH\0";

/// Half the 16-bit range, added back to a `.trh`'s signed samples. See [`Layout::bias`].
const TRH_BIAS: f32 = 32768.0;

/// Read MX Bikes' own heightfield layout, confirmed against a published track:
///
/// ```text
///  0   "TRH\0"
///  4   u32   width
///  8   u32   height
/// 12   i16 × width × height    signed samples, the datum at zero
///  …   trailing block, opening with three floats: size x, relief, size z — all metres
/// ```
///
/// Worth having as its own reader rather than leaving to [`probe`], which cannot see it at
/// all: the grid doesn't run to the end of the file, and the blind search derives the sample
/// offset from the file's length precisely so that it never has to guess. It also can't
/// know that the samples are a signed quantised range rather than metres, so it would report
/// a track's relief as tens of thousands of units — and reading them unsigned puts every
/// sample below the datum a full half-range too high, which draws the ground below a track
/// as an eleven-metre wall around it.
fn parse_trh(bytes: &[u8]) -> Option<Layout> {
    if bytes.len() < 12 || &bytes[..4] != TRH_MAGIC {
        return None;
    }
    let width = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if !(MIN_DIM..=MAX_DIM).contains(&width) || !(MIN_DIM..=MAX_DIM).contains(&height) {
        return None;
    }
    let need = (width as usize).checked_mul(height as usize)?.checked_mul(2)?;
    let end = need.checked_add(12)?;
    if end > bytes.len() {
        return None;
    }

    let c = Candidate {
        offset: 12,
        width,
        height,
        sample: Sample::I16,
        source: "trh",
    };
    // A self-describing file is still measured. A header that says one thing while the body
    // says another is a misread, and falling back to the blind search beats drawing it.
    let Assessment::Ok { confidence, .. } = assess(bytes, &c) else {
        return None;
    };

    let scale = trh_scale(bytes, end, width);
    Some(Layout {
        offset: 12,
        width,
        height,
        sample: Sample::I16,
        confidence,
        source: "trh",
        bias: TRH_BIAS,
        height_scale: scale.map(|(_, h)| h),
        metres_per_sample: scale.map(|(s, _)| s),
    })
}

/// `(metres per sample, metres per raw unit)` from the block that follows the grid.
///
/// Taken from one published track rather than from a specification, so every figure has to
/// be plausible before any of it is believed — and it's all or nothing, because a footprint
/// without a height scale would draw relief in raw units across real ground.
fn trh_scale(bytes: &[u8], at: usize, width: u32) -> Option<(f32, f32)> {
    let f32_at = |i: usize| -> Option<f32> {
        let o = at + i * 4;
        Some(f32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
    };
    let size_x = f32_at(0)?;
    let relief = f32_at(1)?;
    let size_z = f32_at(2)?;

    let sane = |v: f32, max: f32| v.is_finite() && v > 0.0 && v < max;
    if !(sane(size_x, 100_000.0) && sane(size_z, 100_000.0) && sane(relief, 10_000.0)) {
        return None;
    }
    Some((size_x / (width.max(2) - 1) as f32, relief / u16::MAX as f32))
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
/// decoding it would cost more than the view is worth. The ceiling is a power of two plus
/// one because heightmaps are — the track guide's own examples run 2049² and 4097×257.
const MIN_DIM: u32 = 32;
const MAX_DIM: u32 = 8193;

/// The largest header we'll believe sits in front of the samples. Generous — the cost of a
/// wrong guess is one rejected candidate, and a header this big is still cheap to skip.
const MAX_HEADER: usize = 4096;

/// The largest trailing block we'll believe sits after the samples. A real `.trh` carries
/// one — its size and relief in metres — so a grid is not required to run to the end of its
/// file. See [`seam`] for what makes that safe to allow.
const MAX_TRAILER: usize = 4096;

/// A row whose largest step is this many times its typical step has a discontinuity in it,
/// not a slope.
const SEAM_STEP_RATIO: f32 = 8.0;

/// Rejected once this fraction of rows break at the same column.
const MAX_SEAM_ROWS: f32 = 0.75;

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

    // Any (width, height) we have reason to believe in, wherever it came from, and the byte
    // just past whatever stated it — the samples most often start there.
    let mut dims: Vec<(u32, u32, &'static str, Option<usize>)> = Vec::new();

    if let Some((w, h)) = hint {
        // An `.ini` is a separate file; it says nothing about where in *this* one the
        // samples begin.
        dims.push((w, h, "ini", None));
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
            dims.push((w, h, "header", Some(off + 8)));
        }
    }

    for (w, h, source, stated_end) in dims {
        let count = w as usize * h as usize;
        for sample in [Sample::F32, Sample::U16, Sample::I16] {
            let Some(need) = count.checked_mul(sample.size()) else {
                continue;
            };
            // Two ways the block can sit in the file, and both are derived rather than
            // guessed: it runs to the end, or it begins right after whatever stated the
            // dimensions. A real `.trh` is the second — its size and relief are written
            // after the samples, so requiring the grid to reach the end reads it 1566
            // samples late. [`seam`] is what separates the two once both are on the table;
            // the mean roughness cannot, which is why only one used to be offered.
            let mut offsets = Vec::new();
            if let Some(offset) = len.checked_sub(need) {
                offsets.push(offset);
            }
            if let Some(start) = stated_end {
                if start + need <= len && !offsets.contains(&start) {
                    offsets.push(start);
                }
            }
            for offset in offsets {
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
    }

    // Nothing stated the shape: try a square grid, which is what a terrain heightfield
    // usually is. Every header length that leaves room for a square of samples is a
    // candidate, and the roughness and seam tests decide between them.
    for sample in [Sample::F32, Sample::U16, Sample::I16] {
        let size = sample.size();
        for offset in (0..=MAX_HEADER.min(len)).step_by(4) {
            let rest = len - offset;
            let count = rest / size;
            let side = isqrt(count);
            // The square needn't end at the file's last byte — what it doesn't fill is a
            // trailing block, so long as that block is small enough to be one.
            if rest - side * side * size > MAX_TRAILER {
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

/// How much of a *wrapped* read a candidate looks like: the fraction of rows whose largest
/// step falls in the same column.
///
/// This is the measurement that lets the offset be searched rather than derived. A grid read
/// at the wrong offset is the true terrain shifted by some samples, so every row spans the
/// end of one true row and the start of the next, and every row carries one big step at the
/// same column. Terrain doesn't do that: a real ridge doesn't sit in the same column of
/// every row and dwarf everything around it.
///
/// The mean roughness can't see this — it samples a few dozen columns of a grid thousands
/// wide, so it misses the seam entirely and averages away what it does catch. That is why a
/// shifted read scores as well as the truth there, and why this is measured separately.
///
/// Every column of the sampled rows is scanned, because a seam is one column wide and
/// subsampling would step straight over it.
fn seam(bytes: &[u8], c: &Candidate) -> f32 {
    let size = c.sample.size();
    let w = c.width as usize;
    let h = c.height as usize;
    if w < 4 || h < 4 {
        return 0.0;
    }

    let rows = h.min(8);
    let row_step = (h / rows).max(1);

    // Where each row broke, for rows that broke at all.
    let mut breaks: Vec<usize> = Vec::new();
    let mut examined = 0usize;

    let mut y = 0usize;
    while y < h && examined < rows {
        examined += 1;
        let base = y * w;
        let mut total = 0.0f64;
        let mut worst = f32::NEG_INFINITY;
        let mut worst_at = 0usize;
        let mut ok = true;

        for x in 0..w - 1 {
            let a = c.sample.read(bytes, c.offset + (base + x) * size);
            let b = c.sample.read(bytes, c.offset + (base + x + 1) * size);
            if !a.is_finite() || !b.is_finite() {
                ok = false;
                break;
            }
            let d = (a - b).abs();
            total += d as f64;
            if d > worst {
                worst = d;
                worst_at = x;
            }
        }

        if ok && worst.is_finite() {
            let mean = (total / (w - 1) as f64) as f32;
            // A flat row has no worst step worth the name, and a row that is all step is
            // noise rather than a seam. Neither votes.
            if mean > 0.0 && worst >= mean * SEAM_STEP_RATIO {
                breaks.push(worst_at);
            }
        }
        y += row_step;
    }

    if examined == 0 || breaks.is_empty() {
        return 0.0;
    }

    // The modal break column, allowing a column either side: a wrap seam lands in the same
    // place every row, but the exact argmax can drift by one where the terrain is steep.
    let mut best = 0usize;
    for &col in &breaks {
        let agree = breaks
            .iter()
            .filter(|&&other| other.abs_diff(col) <= 1)
            .count();
        best = best.max(agree);
    }
    best as f32 / examined as f32
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
    // Last, because it costs a full row where the two above cost a sample of one, and a
    // layout that fails either of them is not worth asking about alignment.
    if seam(bytes, c) >= MAX_SEAM_ROWS {
        return Assessment::Rejected("every row breaks in the same column (offset is shifted)");
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

    if let Some(l) = parse_trh(bytes) {
        let _ = writeln!(
            out,
            "reads as a self-describing .trh: {}x{} i16 at 12, confidence {:.2}\n\
             spacing {:?} m/sample, height scale {:?} m/unit",
            l.width, l.height, l.confidence, l.metres_per_sample, l.height_scale
        );
        return out;
    }

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
    // A file that says what it is doesn't need guessing at.
    if let Some(known) = parse_trh(bytes) {
        return Some(known);
    }

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
            // Nothing inferred carries a scale or a datum: a grid recovered by its shape
            // says nothing about the ground it covers or what its numbers mean.
            bias: 0.0,
            height_scale: None,
            metres_per_sample: None,
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

/// Read a probed grid out, downsampled so the longest edge is at most `max_dim`.
///
/// In metres where the file gave a height scale; in raw sample units where it didn't.
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
                    let v = (layout.sample.read(bytes, layout.offset + (y * w + x) * size)
                        + layout.bias)
                        * layout.height_scale.unwrap_or(1.0);
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

    /// Build a file to the layout a published track actually uses.
    fn real_trh(w: u32, h: u32, footer: &[u8]) -> Vec<u8> {
        let heights = terrain(w, h);
        let lo = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TRH\0");
        bytes.extend_from_slice(&w.to_le_bytes());
        bytes.extend_from_slice(&h.to_le_bytes());
        for v in &heights {
            // Quantised across the whole 16-bit range, which is what makes the trailing
            // block's relief figure the only thing that turns these back into metres.
            // Stored the way a real file stores it: signed, so the bottom of the range is
            // the most negative value rather than zero.
            let q = ((v - lo) / (hi - lo) * u16::MAX as f32) as i32 - 32768;
            bytes.extend_from_slice(&(q as i16).to_le_bytes());
        }
        bytes.extend_from_slice(footer);
        bytes
    }

    /// `size_x`, relief, `size_z`, then the rest of the block we don't read.
    fn real_footer() -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&250.0f32.to_le_bytes());
        f.extend_from_slice(&18.36f32.to_le_bytes());
        f.extend_from_slice(&250.0f32.to_le_bytes());
        f.extend_from_slice(&[0u8; 512]);
        f
    }

    #[test]
    fn reads_the_real_trh_layout() {
        let bytes = real_trh(257, 257, &real_footer());
        let l = probe(&bytes, None).expect("a real-layout .trh should be read directly");

        assert_eq!(l.source, "trh", "it describes itself — nothing should be inferred");
        assert_eq!((l.width, l.height), (257, 257));
        assert_eq!(l.sample, Sample::I16, "a .trh stores its samples signed");
        assert_eq!(l.offset, 12);
        // 250 m spread across 256 steps.
        assert!((l.metres_per_sample.unwrap() - 250.0 / 256.0).abs() < 1e-4);

        // And the heights come out in metres rather than raw quantised units — the whole
        // point of reading the trailing block.
        let (_, _, grid) = read_grid(&bytes, &l, 512);
        let max = grid.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min = grid.iter().copied().fold(f32::INFINITY, f32::min);
        assert!((max - 18.36).abs() < 0.05, "relief should be metres, got {max}");
        // The samples are signed, so half of them are below the datum. Read unsigned they
        // land a half-range high, which is what drew the ground below a track as a wall
        // around it — the grid has to start at the datum, not at half the range.
        assert!(min.abs() < 0.05, "the bottom of the range is the datum, got {min}");
        assert!((max - min - 18.36).abs() < 0.05, "relief spans the stated height");
    }

    #[test]
    fn a_trh_whose_grid_doesnt_run_to_the_end_is_still_read() {
        // The blind search derives the sample offset from the file's length, so a grid with
        // anything after it is invisible to it. This is exactly that shape.
        let bytes = real_trh(257, 257, &real_footer());
        assert!(
            bytes.len() > 12 + 257 * 257 * 2,
            "the fixture has to actually carry a trailing block",
        );
        assert_eq!(probe(&bytes, None).unwrap().width, 257);
    }

    #[test]
    fn a_trh_with_an_unusable_footer_reads_its_grid_but_claims_no_scale() {
        // Zeros aren't a footprint. The shape is still trustworthy — it's stated in the
        // header — but nothing should be asserted about the ground or the units.
        let bytes = real_trh(257, 257, &[0u8; 64]);
        let l = probe(&bytes, None).expect("the grid is still readable");
        assert_eq!(l.source, "trh");
        assert_eq!(l.metres_per_sample, None, "no footprint should be claimed");
        assert_eq!(l.height_scale, None, "samples stay raw when nothing scales them");
    }

    #[test]
    fn a_trh_header_that_disagrees_with_its_body_is_not_believed() {
        // Magic and dimensions alone aren't enough: if the body doesn't read as terrain at
        // the stated shape, this is a misread and the blind search should get its turn.
        let mut bytes = real_trh(257, 257, &real_footer());
        bytes[4..8].copy_from_slice(&64u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&64u32.to_le_bytes());
        let found = probe(&bytes, None);
        assert!(
            found.is_none_or(|l| (l.width, l.height) != (64, 64)),
            "a stated shape the body contradicts must not be taken at its word",
        );
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

    /// Terrain that drains one way, so its left edge and its right edge don't match. Real
    /// ground rarely joins up across a row boundary; [`terrain`] happens to nearly do so,
    /// which would hide the very discontinuity these tests are about.
    fn ramped(w: u32, h: u32) -> Vec<f32> {
        let mut out = terrain(w, h);
        for y in 0..h as usize {
            for x in 0..w as usize {
                out[y * w as usize + x] += x as f32 / w as f32 * 40.0;
            }
        }
        out
    }

    /// A grid that stops before the file does is still found where it sits.
    ///
    /// This is the shape of a real `.trh`: samples, then a small block stating the track's
    /// size and relief. Reading the grid flush against the end of the file instead lands it
    /// a row and a half late — smooth, plausible, and wrong.
    #[test]
    fn a_grid_that_ends_before_the_file_does_is_read_where_it_sits() {
        let heights = ramped(96, 96);
        let mut bytes = with_header(&heights, &[]);
        bytes.extend_from_slice(&[7u8; 48]);

        let layout = probe(&bytes, None).expect("a grid followed by a footer still reads");
        assert_eq!(layout.offset, 0, "the grid starts at the top of the file");
        assert_eq!((layout.width, layout.height), (96, 96));
    }

    /// The same, with the dimensions stated up front — the grid begins after them, not at
    /// whatever offset makes it reach the last byte.
    #[test]
    fn stated_dimensions_are_read_from_the_front_when_a_footer_follows() {
        let heights = ramped(160, 96);
        let mut header = Vec::new();
        header.extend_from_slice(&160u32.to_le_bytes());
        header.extend_from_slice(&96u32.to_le_bytes());
        let mut bytes = with_header(&heights, &header);
        bytes.extend_from_slice(&[0u8; 64]);

        let layout = probe(&bytes, None).expect("stated dimensions with a footer should read");
        assert_eq!((layout.width, layout.height), (160, 96));
        assert_eq!(layout.offset, 8, "samples begin right after the dimensions");
        assert_eq!(layout.source, "header");
    }

    /// The measurement that lets the offset be searched at all: a read that starts in the
    /// wrong place wraps mid-row, so every row breaks in the same column.
    #[test]
    fn a_shifted_read_breaks_in_the_same_column_every_row() {
        let bytes = with_header(&ramped(128, 128), &[]);
        let aligned = Candidate {
            offset: 0,
            width: 128,
            height: 128,
            sample: Sample::F32,
            source: "square",
        };
        let shifted = Candidate {
            offset: 40 * 4,
            width: 128,
            height: 128,
            sample: Sample::F32,
            source: "square",
        };

        assert!(seam(&bytes, &aligned) < MAX_SEAM_ROWS, "terrain read straight has no seam");
        assert!(
            seam(&bytes, &shifted) >= MAX_SEAM_ROWS,
            "a read 40 samples late wraps in every row",
        );
        assert!(
            matches!(assess(&bytes, &shifted), Assessment::Rejected(_)),
            "and the scorer throws it out rather than drawing it",
        );
    }

    #[test]
    fn recovers_dimensions_stated_in_the_header() {
        // Non-square, so nothing but the header could have supplied the shape.
        let heights = terrain(320, 192);
        let mut header = Vec::new();
        header.extend_from_slice(b"HGT\0");
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
