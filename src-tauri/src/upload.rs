use anyhow::Context;
use obfstr::obfstr;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct Upload {
    /// Direct download URLs, in order. One entry is the whole bundle; more than one means
    /// the bytes were sliced and have to be concatenated in this order to rebuild the zip.
    pub parts: Vec<String>,
    pub host: String,
    /// Size of the original zip, not of any one part.
    pub size: u64,
}

const HOST: &str = "catbox";

/// catbox refuses a file past 200 MB, so anything bigger goes up in slices — with headroom
/// under the ceiling for the multipart envelope around each one.
const PART_BYTES: u64 = 190 * 1024 * 1024;

/// A ceiling on the bundle as a whole. Past this the upload takes long enough, and depends
/// on enough separate files staying alive, that sharing the plain code is the better answer.
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Upload `file`, slicing it if it's over one part. `on_part(index, total)` is called before
/// each slice goes up, so the caller can say which part is in flight.
pub async fn upload_file(
    client: &Client,
    file: &Path,
    on_part: impl Fn(usize, usize),
) -> anyhow::Result<Upload> {
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size > MAX_TOTAL_BYTES {
        anyhow::bail!(
            "This bundle is {:.1} GB — too large to share (the limit is {} GB). \
             Share the plain code instead.",
            size as f64 / (1024.0 * 1024.0 * 1024.0),
            MAX_TOTAL_BYTES / (1024 * 1024 * 1024)
        );
    }

    let plan = part_plan(size);
    let mut handle = std::fs::File::open(file)
        .with_context(|| format!("reading {}", file.display()))?;
    let stem = file
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "preset-bundle".to_string());

    let mut parts = Vec::with_capacity(plan.len());
    for (i, &(offset, len)) in plan.iter().enumerate() {
        on_part(i + 1, plan.len());
        let bytes = read_slice(&mut handle, offset, len)
            .with_context(|| format!("reading {}", file.display()))?;
        // Keep the .zip extension on every slice — catbox screens uploads by extension.
        let name = if plan.len() == 1 {
            format!("{stem}.zip")
        } else {
            format!("{stem}.part{}of{}.zip", i + 1, plan.len())
        };
        parts.push(catbox_upload(client, &name, bytes).await?);
    }

    Ok(Upload { parts, host: HOST.to_string(), size })
}

/// Slice `size` bytes into `(offset, len)` spans no bigger than one part. A zero-byte file
/// still gets one span, so an empty upload fails at catbox rather than silently succeeding
/// with no parts at all.
fn part_plan(size: u64) -> Vec<(u64, u64)> {
    let mut spans = Vec::new();
    let mut offset = 0u64;
    while offset < size {
        let len = PART_BYTES.min(size - offset);
        spans.push((offset, len));
        offset += len;
    }
    if spans.is_empty() {
        spans.push((0, 0));
    }
    spans
}

fn read_slice(file: &mut std::fs::File, offset: u64, len: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

/// catbox.moe — anonymous multipart POST whose response body *is* the direct download URL.
async fn catbox_upload(client: &Client, name: &str, bytes: Vec<u8>) -> anyhow::Result<String> {
    let form = Form::new().text("reqtype", "fileupload").part(
        "fileToUpload",
        Part::bytes(bytes)
            .file_name(name.to_string())
            .mime_str("application/zip")?,
    );

    let resp = client
        .post(obfstr!("https://catbox.moe/user/api.php"))
        .multipart(form)
        .send()
        .await
        .context("couldn't reach catbox to upload the bundle")?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("catbox returned an unreadable response")?;
    let body = body.trim();

    // Failures come back as plain prose, sometimes under a 200, so the URL is the only tell.
    if body.starts_with("https://") {
        Ok(body.to_string())
    } else if body.is_empty() {
        anyhow::bail!("catbox upload failed: HTTP {status}")
    } else {
        anyhow::bail!("catbox upload failed: {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_part_up_to_the_ceiling() {
        assert_eq!(part_plan(0), vec![(0, 0)]);
        assert_eq!(part_plan(10), vec![(0, 10)]);
        assert_eq!(part_plan(PART_BYTES), vec![(0, PART_BYTES)]);
    }

    #[test]
    fn spans_are_contiguous_and_cover_the_file() {
        let size = PART_BYTES * 3 + 7;
        let plan = part_plan(size);
        assert_eq!(plan.len(), 4);
        let mut expected = 0;
        for &(offset, len) in &plan {
            assert_eq!(offset, expected, "spans must not skip or overlap");
            assert!(len <= PART_BYTES);
            expected += len;
        }
        assert_eq!(expected, size, "spans must cover every byte");
        assert_eq!(plan[3].1, 7);
    }

    #[test]
    fn slices_rebuild_the_original() {
        let dir = std::env::temp_dir().join("mxb-upload-slice-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bundle.zip");

        // Split on a small boundary rather than 190 MB: the plan is what scales, and this
        // exercises the same seek/read/concat path.
        let original: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &original).unwrap();

        let mut handle = std::fs::File::open(&path).unwrap();
        let spans = [(0u64, 1024u64), (1024, 1024), (2048, 1024), (3072, 1928)];
        let mut rebuilt = Vec::new();
        for &(offset, len) in &spans {
            rebuilt.extend_from_slice(&read_slice(&mut handle, offset, len).unwrap());
        }

        assert_eq!(rebuilt, original);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
