use anyhow::{bail, ensure, Context, Result};
use std::ops::Range;

/// The self-described portion of an MX Bikes `.trh` terrain file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrhDescriptor {
    pub width: u32,
    pub height: u32,
    pub sample_offset: usize,
    pub sample_end: usize,
    pub bias: f32,
    pub height_scale: Option<f32>,
    pub metres_per_sample: Option<f32>,
}

/// The three TRH-derived values used by beta21e's track content declaration.
///
/// A trusted caller value, when supplied, is checked against the derived sum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Beta21eTrhManifestChecks {
    pub canonical_byte_count: u32,
    pub canonical_byte_sum: Option<u32>,
    pub auxiliary_byte_sum: u32,
}

/// One finite pose on the TRH main centreline, expressed in world X/Z metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CentrelinePose {
    pub position: [f32; 2],
    pub forward: [f32; 2],
}

const MAGIC: &[u8; 4] = b"TRH\0";
const MIN_DIM: u32 = 32;
const MAX_DIM: u32 = 8193;
const BIAS: f32 = 32768.0;

/// Parse only a strict, self-describing TRH. Inference/probing remains a UI concern.
pub fn descriptor(bytes: &[u8]) -> Option<TrhDescriptor> {
    if bytes.len() < 12 || &bytes[..4] != MAGIC {
        return None;
    }
    let width = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if !(MIN_DIM..=MAX_DIM).contains(&width) || !(MIN_DIM..=MAX_DIM).contains(&height) {
        return None;
    }
    let sample_bytes = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(2)?;
    let sample_end = 12usize.checked_add(sample_bytes)?;
    if sample_end > bytes.len() {
        return None;
    }
    let scale = scale(bytes, sample_end, width);
    Some(TrhDescriptor {
        width,
        height,
        sample_offset: 12,
        sample_end,
        bias: BIAS,
        height_scale: scale.map(|(_, height)| height),
        metres_per_sample: scale.map(|(step, _)| step),
    })
}

/// Parse beta21e's structural TRH manifest inputs.
///
/// All three values are derived from the file structure and beta21e's exact
/// instruction-ordered D/EX3 canonicalization. A trusted sum can additionally
/// pin the result to independently captured metadata.
pub fn beta21e_manifest_checks(
    bytes: &[u8],
    trusted_canonical_byte_sum: Option<u32>,
) -> Result<Beta21eTrhManifestChecks> {
    let terrain = descriptor(bytes).context("invalid TRH descriptor")?;
    let tails = TailChunks::discover(bytes)?;
    ensure!(
        terrain.sample_end <= tails.main_end,
        "TRH sample grid overlaps trailing chunks"
    );

    let mut cursor = Cursor::new(bytes, terrain.sample_end, tails.main_end)?;
    cursor.take(24, "initial metadata")?;

    let mut canonical_sum = unsigned_byte_sum(
        bytes
            .get(terrain.sample_offset..terrain.sample_end)
            .context("TRH sample byte range is invalid")?,
    );
    let mut canonical_count = (terrain.width as u64)
        .checked_mul(terrain.height as u64)
        .and_then(|count| count.checked_mul(2))
        .context("TRH canonical sample count overflow")?;

    let a_count = cursor.u32("A count")?;
    for _ in 0..a_count {
        let field0 = cursor.take(4, "A field0")?;
        let field18 = cursor.take(4, "A field18")?;
        let rows = cursor.u32("A rows")?;
        let columns = cursor.u32("A columns")?;
        let payload = checked_product(rows, columns, "A payload")?;
        let payload = cursor.take(payload, "A payload")?;
        canonical_sum = canonical_sum
            .wrapping_add(unsigned_byte_sum(field0))
            .wrapping_add(unsigned_byte_sum(field18))
            .wrapping_add(unsigned_byte_sum(payload));
        canonical_count = checked_add(canonical_count, 8, "A canonical fields")?;
        canonical_count = checked_add(canonical_count, payload.len() as u64, "A payload")?;
    }

    let b_count = cursor.u32("B count")?;
    for _ in 0..b_count {
        let field0 = cursor.take(4, "B field0")?;
        let rows = cursor.u32("B rows")?;
        let columns = cursor.u32("B columns")?;
        let payload = checked_product(rows, columns, "B payload")?;
        let payload = cursor.take(payload, "B payload")?;
        canonical_sum = canonical_sum
            .wrapping_add(unsigned_byte_sum(field0))
            .wrapping_add(unsigned_byte_sum(payload));
        canonical_count = checked_add(canonical_count, 4, "B canonical field")?;
        canonical_count = checked_add(canonical_count, payload.len() as u64, "B payload")?;
    }

    let root_header_0 = cursor.take(16, "root header 0")?;
    let main_profile_headers: [u8; 12] = root_header_0[..12]
        .try_into()
        .expect("twelve-byte subslice");
    cursor.take(24, "root header 1")?;

    let c_count = cursor.u32("C count")?;
    let c_bytes = cursor.records(c_count, 52, "C records")?;
    canonical_sum = canonical_sum.wrapping_add(unsigned_byte_sum(c_bytes));
    canonical_count = checked_records(canonical_count, c_count, 52, "C records")?;

    let d_count = cursor.u32("D count")?;
    let d_bytes = cursor.records(d_count, 60, "D records")?;
    canonical_count = checked_records(canonical_count, d_count, 56, "D records")?;
    canonical_sum = canonical_sum.wrapping_add(beta21e_d_sum(&main_profile_headers, d_bytes)?);
    let mut auxiliary_sum = profile_sum(&main_profile_headers, d_bytes);

    (canonical_count, canonical_sum) =
        parse_flat_records(&mut cursor, canonical_count, canonical_sum, 12, "E")?;
    (canonical_count, canonical_sum) =
        parse_flat_records(&mut cursor, canonical_count, canonical_sum, 20, "F")?;
    (canonical_count, canonical_sum) =
        parse_flat_records(&mut cursor, canonical_count, canonical_sum, 12, "G")?;
    (canonical_count, canonical_sum) =
        parse_flat_records(&mut cursor, canonical_count, canonical_sum, 16, "H")?;

    let i_count = cursor.u32("I count")?;
    cursor.records(i_count, 40, "I records")?;
    auxiliary_sum = auxiliary_sum.wrapping_add(surface_sum(&mut cursor, "main surface")?);
    cursor.finish("main TRH body")?;

    auxiliary_sum = auxiliary_sum.wrapping_add(unsigned_byte_sum(c_bytes));

    if let Some(range) = tails.ex3_body {
        let mut ex3 = Cursor::from_range(bytes, range)?;
        let count = ex3.u32("EX3 count")?;
        let records = ex3.records(count, 56, "EX3 records")?;
        // The loader reorders all fourteen dwords within runtime +0x00..+0x37;
        // its postprocessor writes only at +0x38 and above.
        canonical_sum = canonical_sum.wrapping_add(unsigned_byte_sum(records));
        canonical_count = checked_records(canonical_count, count, 56, "EX3 records")?;
        ex3.finish("EX3 body")?;
    }

    if let Some(range) = tails.ext_body {
        let mut ext = Cursor::from_range(bytes, range)?;
        for index in 0..3 {
            let present = ext.u32("EXT profile presence")?;
            ensure!(
                present <= 1,
                "EXT profile {index} has invalid presence {present}"
            );
            if present == 1 {
                let headers: [u8; 12] = ext
                    .take(12, "EXT profile headers")?
                    .try_into()
                    .expect("twelve-byte slice");
                let count = ext.u32("EXT profile count")?;
                let records = ext.records(count, 60, "EXT profile records")?;
                auxiliary_sum = auxiliary_sum.wrapping_add(profile_sum(&headers, records));
            }
        }
        for _ in 0..3 {
            auxiliary_sum = auxiliary_sum.wrapping_add(surface_sum(&mut ext, "EXT surface")?);
        }
        ext.finish("EXT body")?;
    }

    if let Some(trusted) = trusted_canonical_byte_sum {
        ensure!(
            trusted == canonical_sum,
            "trusted beta21e canonical byte sum {trusted:#010x} does not match derived {canonical_sum:#010x}"
        );
    }

    Ok(Beta21eTrhManifestChecks {
        canonical_byte_count: u32::try_from(canonical_count)
            .context("TRH canonical byte count exceeds u32")?,
        canonical_byte_sum: Some(canonical_sum),
        auxiliary_byte_sum: auxiliary_sum,
    })
}

/// Project a longitudinal distance onto beta21e's main TRH centreline profile.
///
/// RDF timing lines name a profile and a distance along it. The capture-free server currently
/// supports the main profile (`line = 0`); this exposes only the bounded world-space pose needed
/// to construct timing gates without exposing or copying the package payload.
pub fn beta21e_main_centreline_pose(bytes: &[u8], distance: f32) -> Result<CentrelinePose> {
    ensure!(
        distance.is_finite() && distance >= 0.0,
        "centreline distance is invalid"
    );
    let terrain = descriptor(bytes).context("invalid TRH descriptor")?;
    let tails = TailChunks::discover(bytes)?;
    ensure!(
        terrain.sample_end <= tails.main_end,
        "TRH sample grid overlaps trailing chunks"
    );

    let mut cursor = Cursor::new(bytes, terrain.sample_end, tails.main_end)?;
    cursor.take(24, "initial metadata")?;

    let a_count = cursor.u32("A count")?;
    for _ in 0..a_count {
        cursor.take(8, "A fixed fields")?;
        let rows = cursor.u32("A rows")?;
        let columns = cursor.u32("A columns")?;
        cursor.take(checked_product(rows, columns, "A payload")?, "A payload")?;
    }

    let b_count = cursor.u32("B count")?;
    for _ in 0..b_count {
        cursor.take(4, "B field0")?;
        let rows = cursor.u32("B rows")?;
        let columns = cursor.u32("B columns")?;
        cursor.take(checked_product(rows, columns, "B payload")?, "B payload")?;
    }

    let root = cursor.take(16, "root header 0")?;
    let headers: [f32; 3] = std::array::from_fn(|index| {
        let offset = index * 4;
        f32::from_le_bytes(
            root[offset..offset + 4]
                .try_into()
                .expect("four-byte slice"),
        )
    });
    ensure!(
        headers.iter().all(|value| value.is_finite()),
        "main profile header is non-finite"
    );
    cursor.take(24, "root header 1")?;
    let c_count = cursor.u32("C count")?;
    cursor.records(c_count, 52, "C records")?;
    let d_count = cursor.u32("D count")?;
    let records = cursor.records(d_count, 60, "D records")?;
    centreline_pose_in_profile(records, distance)
}

fn centreline_pose_in_profile(records: &[u8], distance: f32) -> Result<CentrelinePose> {
    ensure!(
        !records.is_empty() && records.len().is_multiple_of(60),
        "main profile has no complete records"
    );
    let mut last_end = 0.0f32;
    for (index, record) in records.chunks_exact(60).enumerate() {
        let scalar = |offset: usize| -> f32 {
            f32::from_le_bytes(
                record[offset..offset + 4]
                    .try_into()
                    .expect("four-byte slice"),
            )
        };
        let flag = u32::from_le_bytes(record[0..4].try_into().expect("four-byte slice"));
        let length = scalar(4);
        let radius = scalar(8);
        let running = scalar(20);
        ensure!(flag <= 1, "D record {index} has invalid curve flag {flag}");
        ensure!(
            length.is_finite() && length > 0.0,
            "D record {index} has invalid length"
        );
        ensure!(
            running.is_finite() && running >= 0.0,
            "D record {index} has invalid running distance"
        );
        ensure!(
            (running - last_end).abs() <= 0.02,
            "D record {index} has discontinuous running distance"
        );
        let end = running + length;
        ensure!(end.is_finite(), "D record {index} has invalid endpoint");
        last_end = end;
        if distance > end && index + 1 < records.len() / 60 {
            continue;
        }
        ensure!(
            distance <= end + 0.02,
            "centreline distance exceeds profile length"
        );

        let matrix = read_matrix(&record[24..60]);
        ensure_finite_matrix(&matrix, &format!("D record {index} matrix"))?;
        let along = (distance - running).clamp(0.0, length);
        let (local_x, local_z, tangent_x, tangent_z) = if flag == 1 {
            ensure!(
                radius.is_finite() && radius != 0.0,
                "D record {index} has invalid radius"
            );
            let angle = f64::from(along / radius);
            let sine = angle.sin() as f32;
            let cosine = angle.cos() as f32;
            (radius - cosine * radius, sine * radius, sine, cosine)
        } else {
            (0.0, along, 0.0, 1.0)
        };
        let position = [
            affine_x(&matrix, local_x, local_z),
            affine_z(&matrix, local_x, local_z),
        ];
        let mut forward = [
            tangent_x * matrix[0] + tangent_z * matrix[1],
            tangent_x * matrix[3] + tangent_z * matrix[4],
        ];
        let magnitude = (forward[0] * forward[0] + forward[1] * forward[1]).sqrt();
        ensure!(
            magnitude.is_finite() && magnitude > 0.0,
            "D record {index} has invalid tangent"
        );
        forward[0] /= magnitude;
        forward[1] /= magnitude;
        ensure!(
            position.iter().all(|value| value.is_finite()),
            "D record {index} has invalid position"
        );
        return Ok(CentrelinePose { position, forward });
    }
    bail!("main profile has no centreline records")
}

fn parse_flat_records(
    cursor: &mut Cursor<'_>,
    canonical_count: u64,
    canonical_sum: u32,
    stride: usize,
    name: &str,
) -> Result<(u64, u32)> {
    let count = cursor.u32(&format!("{name} count"))?;
    let records = cursor.records(count, stride, &format!("{name} records"))?;
    Ok((
        checked_records(canonical_count, count, stride as u64, name)?,
        canonical_sum.wrapping_add(unsigned_byte_sum(records)),
    ))
}

const PI_BETA21E: f64 = f64::from_bits(0x4009_21fb_5452_4550);
const DEGREES_TO_RADIANS_BETA21E: f64 = f64::from_bits(0x3f91_df46_a25c_a311);

fn beta21e_d_sum(headers: &[u8; 12], records: &[u8]) -> Result<u32> {
    ensure!(
        records.len().is_multiple_of(60),
        "D records have a partial stride"
    );
    let header = |offset: usize| -> f32 {
        f32::from_le_bytes(
            headers[offset..offset + 4]
                .try_into()
                .expect("four-byte slice"),
        )
    };
    let mut matrix = multiply_matrix(
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        rotation_degrees(header(8)),
    );
    matrix[2] = header(0);
    matrix[5] = header(4);
    ensure_finite_matrix(&matrix, "D profile header")?;

    let mut running = 0.0f32;
    let mut sum = 0u32;
    for (index, record) in records.chunks_exact(60).enumerate() {
        let word = |offset: usize| -> [u8; 4] {
            record[offset..offset + 4]
                .try_into()
                .expect("four-byte slice")
        };
        let scalar = |offset: usize| -> f32 { f32::from_le_bytes(word(offset)) };
        let flag = u32::from_le_bytes(word(0));
        ensure!(flag <= 1, "D record {index} has invalid curve flag {flag}");
        let length = scalar(4);
        let radius = scalar(8);
        let file_sweep = scalar(12);
        ensure!(length.is_finite(), "D record {index} has non-finite length");
        ensure!(radius.is_finite(), "D record {index} has non-finite radius");

        let stored_running = scalar(20);
        ensure!(
            stored_running.to_bits() == running.to_bits(),
            "D record {index} running distance is not beta21e-canonical"
        );
        let stored_matrix = read_matrix(&record[24..60]);
        ensure!(
            stored_matrix
                .iter()
                .zip(matrix.iter())
                .all(|(stored, derived)| stored.to_bits() == derived.to_bits()),
            "D record {index} matrix is not beta21e-canonical"
        );

        let sweep = if flag == 1 {
            ensure!(radius != 0.0, "D record {index} has a zero curve radius");
            let denominator = f64::from(radius.abs() * 2.0f32) * PI_BETA21E;
            let quotient = (f64::from(length) / denominator) as f32;
            let derived = quotient * 360.0f32;
            ensure!(
                derived.is_finite(),
                "D record {index} produces a non-finite sweep"
            );
            ensure!(
                derived.to_bits() == file_sweep.to_bits(),
                "D record {index} sweep is not beta21e-canonical"
            );
            derived
        } else {
            file_sweep
        };

        let pre_matrix = matrix;
        if flag == 1 {
            matrix = multiply_matrix(
                matrix,
                rotation_degrees(if radius > 0.0 { sweep } else { -sweep }),
            );
            ensure_finite_matrix(&matrix, &format!("D record {index} rotation"))?;
        }
        let (local_x, local_z) = if flag == 1 {
            let denominator = f64::from(radius * 2.0f32) * PI_BETA21E;
            let signed_sweep = ((f64::from(length) / denominator) * 360.0f64) as f32;
            let radians = f64::from(signed_sweep) * DEGREES_TO_RADIANS_BETA21E;
            let cosine = radians.cos() as f32;
            let sine = radians.sin() as f32;
            (radius - cosine * radius, sine * radius)
        } else {
            (0.0, length)
        };
        let end_x = affine_x(&pre_matrix, local_x, local_z);
        let end_z = affine_z(&pre_matrix, local_x, local_z);
        ensure!(
            end_x.is_finite() && end_z.is_finite(),
            "D record {index} produces a non-finite endpoint"
        );

        for bytes in [
            word(4),
            word(0),
            word(8),
            sweep.to_le_bytes(),
            word(16),
            0u32.to_le_bytes(),
            pre_matrix[2].to_le_bytes(),
            pre_matrix[5].to_le_bytes(),
            end_x.to_le_bytes(),
            end_z.to_le_bytes(),
            running.to_le_bytes(),
            pre_matrix[0].to_le_bytes(),
            pre_matrix[1].to_le_bytes(),
            pre_matrix[2].to_le_bytes(),
        ] {
            sum = sum.wrapping_add(unsigned_byte_sum(&bytes));
        }

        matrix[2] = end_x;
        matrix[5] = end_z;
        running += length;
        ensure!(
            running.is_finite(),
            "D record {index} produces a non-finite running distance"
        );
    }
    Ok(sum)
}

fn read_matrix(bytes: &[u8]) -> [f32; 9] {
    std::array::from_fn(|index| {
        let offset = index * 4;
        f32::from_le_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("four-byte slice"),
        )
    })
}

fn rotation_degrees(degrees: f32) -> [f32; 9] {
    let radians = f64::from(degrees) * DEGREES_TO_RADIANS_BETA21E;
    let cosine = radians.cos() as f32;
    let sine = radians.sin() as f32;
    [cosine, sine, 0.0, -sine, cosine, 0.0, 0.0, 0.0, 1.0]
}

fn multiply_matrix(left: [f32; 9], right: [f32; 9]) -> [f32; 9] {
    std::array::from_fn(|index| {
        let row = index / 3;
        let column = index % 3;
        let first = left[row * 3] * right[column];
        let second = left[row * 3 + 1] * right[3 + column];
        let third = left[row * 3 + 2] * right[6 + column];
        first + second + third
    })
}

fn affine_x(matrix: &[f32; 9], x: f32, z: f32) -> f32 {
    let first = z * matrix[1];
    let second = x * matrix[0];
    first + second + matrix[2]
}

fn affine_z(matrix: &[f32; 9], x: f32, z: f32) -> f32 {
    let first = x * matrix[3];
    let second = z * matrix[4];
    first + second + matrix[5]
}

fn ensure_finite_matrix(matrix: &[f32; 9], name: &str) -> Result<()> {
    ensure!(
        matrix.iter().all(|value| value.is_finite()),
        "{name} produces a non-finite matrix"
    );
    Ok(())
}

fn profile_sum(headers: &[u8; 12], records: &[u8]) -> u32 {
    debug_assert_eq!(records.len() % 60, 0);
    let mut running = 0.0f32;
    let mut sum = unsigned_byte_sum(headers);
    for record in records.chunks_exact(60) {
        // The loader swaps file0/file1. Runtime field zero is therefore file1.
        running += f32::from_le_bytes(record[4..8].try_into().expect("four-byte slice"));
        sum = sum.wrapping_add(unsigned_byte_sum(&record[..20]));
    }
    sum.wrapping_add(unsigned_byte_sum(&running.to_le_bytes()))
}

fn surface_sum(cursor: &mut Cursor<'_>, name: &str) -> Result<u32> {
    let header_0 = cursor.take(4, &format!("{name} header 0"))?;
    let header_1 = cursor.take(4, &format!("{name} header 1"))?;
    let pair_count = cursor.u32(&format!("{name} pair count"))?;
    let pairs = cursor.records(pair_count, 8, &format!("{name} pairs"))?;
    if pair_count == 0 {
        return Ok(0);
    }
    Ok(unsigned_byte_sum(header_0)
        .wrapping_add(unsigned_byte_sum(header_1))
        .wrapping_add(unsigned_byte_sum(pairs)))
}

fn unsigned_byte_sum(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |sum, byte| sum.wrapping_add(u32::from(*byte)))
}

fn checked_product(left: u32, right: u32, name: &str) -> Result<usize> {
    let value = (left as u64)
        .checked_mul(right as u64)
        .with_context(|| format!("{name} length overflow"))?;
    usize::try_from(value).with_context(|| format!("{name} does not fit in memory"))
}

fn checked_add(value: u64, increment: u64, name: &str) -> Result<u64> {
    value
        .checked_add(increment)
        .with_context(|| format!("{name} count overflow"))
}

fn checked_records(value: u64, count: u32, stride: u64, name: &str) -> Result<u64> {
    let bytes = (count as u64)
        .checked_mul(stride)
        .with_context(|| format!("{name} count overflow"))?;
    checked_add(value, bytes, name)
}

#[derive(Debug)]
struct TailChunks {
    main_end: usize,
    ex3_body: Option<Range<usize>>,
    ext_body: Option<Range<usize>>,
}

impl TailChunks {
    fn discover(bytes: &[u8]) -> Result<Self> {
        let mut prefix_end = bytes.len();
        let ext_body = take_trailing_chunk(bytes, &mut prefix_end, b"EXT\0")?;
        let ex3_body = take_trailing_chunk(bytes, &mut prefix_end, b"EX3\0")?;
        Ok(Self {
            main_end: prefix_end,
            ex3_body,
            ext_body,
        })
    }
}

fn take_trailing_chunk(
    bytes: &[u8],
    prefix_end: &mut usize,
    tag: &[u8; 4],
) -> Result<Option<Range<usize>>> {
    if *prefix_end < 8 || &bytes[*prefix_end - 8..*prefix_end - 4] != tag {
        return Ok(None);
    }
    let chunk_size = u32::from_le_bytes(
        bytes[*prefix_end - 4..*prefix_end]
            .try_into()
            .expect("four-byte slice"),
    ) as usize;
    ensure!(
        chunk_size >= 8,
        "{} chunk is smaller than its trailer",
        tag_name(tag)
    );
    let start = prefix_end
        .checked_sub(chunk_size)
        .with_context(|| format!("{} chunk starts before the file", tag_name(tag)))?;
    let body_end = *prefix_end - 8;
    *prefix_end = start;
    Ok(Some(start..body_end))
}

fn tag_name(tag: &[u8; 4]) -> &str {
    if tag == b"EXT\0" {
        "EXT"
    } else {
        "EX3"
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], position: usize, end: usize) -> Result<Self> {
        ensure!(
            position <= end && end <= bytes.len(),
            "invalid TRH cursor bounds"
        );
        Ok(Self {
            bytes,
            position,
            end,
        })
    }

    fn from_range(bytes: &'a [u8], range: Range<usize>) -> Result<Self> {
        Self::new(bytes, range.start, range.end)
    }

    fn take(&mut self, length: usize, name: &str) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .with_context(|| format!("{name} offset overflow"))?;
        if end > self.end {
            bail!("{name} exceeds TRH section bounds");
        }
        let value = &self.bytes[self.position..end];
        self.position = end;
        Ok(value)
    }

    fn u32(&mut self, name: &str) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4, name)?.try_into().expect("four-byte slice"),
        ))
    }

    fn records(&mut self, count: u32, stride: usize, name: &str) -> Result<&'a [u8]> {
        let length = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(stride))
            .with_context(|| format!("{name} length overflow"))?;
        self.take(length, name)
    }

    fn finish(&self, name: &str) -> Result<()> {
        ensure!(
            self.position == self.end,
            "{name} has {} unparsed bytes",
            self.end - self.position
        );
        Ok(())
    }
}

fn scale(bytes: &[u8], at: usize, width: u32) -> Option<(f32, f32)> {
    let value = |index: usize| -> Option<f32> {
        let offset = at.checked_add(index.checked_mul(4)?)?;
        Some(f32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let size_x = value(0)?;
    let relief = value(1)?;
    let size_z = value(2)?;
    let sane = |number: f32, max: f32| number.is_finite() && number > 0.0 && number < max;
    if !(sane(size_x, 100_000.0) && sane(size_z, 100_000.0) && sane(relief, 10_000.0)) {
        return None;
    }
    Some((size_x / (width.max(2) - 1) as f32, relief / u16::MAX as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_record(flag: u32, length: f32, radius: f32, running: f32) -> Vec<u8> {
        let mut record = vec![0; 60];
        record[0..4].copy_from_slice(&flag.to_le_bytes());
        record[4..8].copy_from_slice(&length.to_le_bytes());
        record[8..12].copy_from_slice(&radius.to_le_bytes());
        record[20..24].copy_from_slice(&running.to_le_bytes());
        for (index, value) in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
            .into_iter()
            .enumerate()
        {
            let offset = 24 + index * 4;
            record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        record
    }

    #[test]
    fn centreline_projection_handles_straights_curves_and_bounds() {
        let straight = profile_record(0, 10.0, 0.0, 0.0);
        let pose = centreline_pose_in_profile(&straight, 4.0).unwrap();
        assert_eq!(pose.position, [0.0, 4.0]);
        assert_eq!(pose.forward, [0.0, 1.0]);

        let quarter_length = std::f32::consts::FRAC_PI_2 * 10.0;
        let curve = profile_record(1, quarter_length, 10.0, 0.0);
        let pose = centreline_pose_in_profile(&curve, quarter_length).unwrap();
        assert!((pose.position[0] - 10.0).abs() < 0.001);
        assert!((pose.position[1] - 10.0).abs() < 0.001);
        assert!((pose.forward[0] - 1.0).abs() < 0.001);
        assert!(pose.forward[1].abs() < 0.001);

        assert!(centreline_pose_in_profile(&straight, 11.0).is_err());
    }

    fn trh(width: u32, height: u32, samples: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.resize(12 + samples * 2, 0);
        bytes.extend_from_slice(&2048.0f32.to_le_bytes());
        bytes.extend_from_slice(&64.0f32.to_le_bytes());
        bytes.extend_from_slice(&1024.0f32.to_le_bytes());
        bytes
    }

    fn word(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn d_record(file0: u32, file1: f32, file2: u32, file3: u32, file4: u32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(60);
        word(&mut bytes, file0);
        bytes.extend_from_slice(&file1.to_le_bytes());
        word(&mut bytes, file2);
        word(&mut bytes, file3);
        word(&mut bytes, file4);
        bytes.resize(60, 0);
        bytes
    }

    fn canonical_straight_d_record(
        length: f32,
        file4: u32,
        running: f32,
        translation_z: f32,
    ) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(60);
        word(&mut bytes, 0);
        bytes.extend_from_slice(&length.to_le_bytes());
        word(&mut bytes, 0);
        word(&mut bytes, 0);
        word(&mut bytes, file4);
        bytes.extend_from_slice(&running.to_le_bytes());
        for value in [1.0f32, 0.0, 0.0, 0.0, 1.0, translation_z, 0.0, 0.0, 1.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn surface(bytes: &mut Vec<u8>, header0: u32, header1: u32, pairs: &[[u8; 8]]) {
        word(bytes, header0);
        word(bytes, header1);
        word(bytes, pairs.len() as u32);
        for pair in pairs {
            bytes.extend_from_slice(pair);
        }
    }

    fn chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
        bytes.extend_from_slice(body);
        bytes.extend_from_slice(tag);
        word(bytes, (body.len() + 8) as u32);
    }

    fn manifest_trh() -> Vec<u8> {
        let mut bytes = trh(32, 32, 32 * 32);

        // `trh` already supplied the first three of the six metadata words.
        bytes.extend_from_slice(&[0; 12]);

        word(&mut bytes, 1); // A count
        word(&mut bytes, 10);
        word(&mut bytes, 11);
        word(&mut bytes, 2);
        word(&mut bytes, 3);
        bytes.extend_from_slice(&[0; 6]);

        word(&mut bytes, 1); // B count
        word(&mut bytes, 12);
        word(&mut bytes, 1);
        word(&mut bytes, 2);
        bytes.extend_from_slice(&[0; 2]);

        for value in [0, 0, 0, 0xffff_ffff] {
            word(&mut bytes, value);
        }
        bytes.extend_from_slice(&[0; 24]);

        word(&mut bytes, 1); // C count
        bytes.extend_from_slice(&[1; 52]);

        word(&mut bytes, 2); // D count
        bytes.extend_from_slice(&canonical_straight_d_record(1.0, 2, 0.0, 0.0));
        bytes.extend_from_slice(&canonical_straight_d_record(2.0, 3, 1.0, 1.0));

        for (count, stride) in [(1u32, 12usize), (1, 20), (1, 12), (1, 16)] {
            word(&mut bytes, count);
            bytes.resize(bytes.len() + stride, 0);
        }

        word(&mut bytes, 1); // I count: parsed but excluded from both checks
        bytes.extend_from_slice(&[0xff; 40]);
        surface(&mut bytes, 0x0102_0304, 0x0506_0708, &[[2; 8], [2; 8]]);

        let mut ex3 = Vec::new();
        word(&mut ex3, 1);
        ex3.extend_from_slice(&[0; 56]);
        chunk(&mut bytes, b"EX3\0", &ex3);

        let mut ext = Vec::new();
        word(&mut ext, 0); // profile 0 absent
        word(&mut ext, 1); // profile 1 present
        ext.extend_from_slice(&[1; 12]);
        word(&mut ext, 1);
        ext.extend_from_slice(&d_record(1, 4.0, 2, 3, 4));
        word(&mut ext, 0); // profile 2 absent
        surface(&mut ext, u32::MAX, u32::MAX, &[]); // empty headers are excluded
        surface(&mut ext, 9, 10, &[[3; 8]]);
        surface(&mut ext, u32::MAX, u32::MAX, &[]);
        chunk(&mut bytes, b"EXT\0", &ext);
        bytes
    }

    #[test]
    fn reads_square_and_rectangular_server_terrain_shapes() {
        let square = trh(2049, 2049, 2049 * 2049);
        let rectangular = trh(2049, 1025, 2049 * 1025);
        assert_eq!(
            descriptor(&square).map(|d| (d.width, d.height)),
            Some((2049, 2049))
        );
        assert_eq!(
            descriptor(&rectangular).map(|d| (d.width, d.height)),
            Some((2049, 1025))
        );
    }

    #[test]
    fn rejects_wrong_magic_dimensions_overflow_and_truncation() {
        assert!(descriptor(b"not a trh").is_none());
        assert!(descriptor(&trh(31, 32, 31 * 32)).is_none());
        assert!(descriptor(&trh(8194, 32, 0)).is_none());
        assert!(descriptor(&trh(2049, 2049, 10)).is_none());
    }

    #[test]
    fn derives_all_beta21e_checks_and_validates_a_trusted_second_sum() {
        let bytes = manifest_trh();
        let checks = beta21e_manifest_checks(&bytes, None).unwrap();
        assert_eq!(checks.canonical_byte_count, 2_348);
        assert_eq!(checks.canonical_byte_sum, Some(1_428));
        assert_eq!(checks.auxiliary_byte_sum, 957);

        let trusted = beta21e_manifest_checks(&bytes, checks.canonical_byte_sum).unwrap();
        assert_eq!(trusted.canonical_byte_sum, checks.canonical_byte_sum);
        assert_eq!(trusted.canonical_byte_count, checks.canonical_byte_count);
        assert_eq!(trusted.auxiliary_byte_sum, checks.auxiliary_byte_sum);
        assert!(beta21e_manifest_checks(&bytes, Some(0x1234_5678)).is_err());
    }

    #[test]
    fn rejects_noncanonical_d_running_distance_and_matrix() {
        let headers = [0; 12];
        let canonical = canonical_straight_d_record(1.0, 2, 0.0, 0.0);
        assert!(beta21e_d_sum(&headers, &canonical).is_ok());

        let mut bad_running = canonical.clone();
        bad_running[20..24].copy_from_slice(&1.0f32.to_le_bytes());
        assert!(beta21e_d_sum(&headers, &bad_running).is_err());

        let mut bad_matrix = canonical;
        bad_matrix[24..28].copy_from_slice(&0.0f32.to_le_bytes());
        assert!(beta21e_d_sum(&headers, &bad_matrix).is_err());
    }

    #[test]
    fn strictly_rejects_unparsed_bytes_bad_chunk_lengths_and_bad_profile_presence() {
        let mut trailing = manifest_trh();
        trailing.extend_from_slice(&[0]);
        assert!(beta21e_manifest_checks(&trailing, None).is_err());

        let mut bad_chunk = manifest_trh();
        let length = bad_chunk.len();
        bad_chunk[length - 4..].copy_from_slice(&7u32.to_le_bytes());
        assert!(beta21e_manifest_checks(&bad_chunk, None).is_err());

        let mut bad_profile = manifest_trh();
        let ext_size = u32::from_le_bytes(bad_profile[bad_profile.len() - 4..].try_into().unwrap());
        let ext_start = bad_profile.len() - ext_size as usize;
        bad_profile[ext_start..ext_start + 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(beta21e_manifest_checks(&bad_profile, None).is_err());
    }
}
