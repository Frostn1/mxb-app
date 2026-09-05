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

/// catbox takes a 200 MB file in principle, but a POST that big comes back empty far more
/// often than it succeeds — measured against the live endpoint, 190 MiB failed every attempt
/// and 100 MiB two in three, while nothing at 48 MiB or under ever dropped. Slice small
/// enough that each request is a few seconds long.
const PART_BYTES: u64 = 48 * 1024 * 1024;

/// Attempts per part. An empty 200 is catbox dropping an upload it never refused, and the
/// next attempt usually takes it.
const ATTEMPTS: u32 = 3;

/// Grows with each retry: whatever is throttling catbox wants a moment, not another POST.
const RETRY_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

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
        parts.push(catbox_upload(client, endpoint(), &name, &bytes).await?);
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

/// Split out so a test can point the upload at a local server.
fn endpoint() -> String {
    obfstr!("https://catbox.moe/user/api.php").to_string()
}

/// Upload one part, retrying the drops. Refusals — anything catbox puts prose behind — are
/// final: the request itself is what it objected to, so sending it again says nothing new.
async fn catbox_upload(
    client: &Client,
    url: String,
    name: &str,
    bytes: &[u8],
) -> anyhow::Result<String> {
    for attempt in 1..=ATTEMPTS {
        let (status, body) = catbox_post(client, &url, name, bytes.to_vec()).await?;

        // Failures come back as plain prose, sometimes under a 200, so the URL is the only tell.
        if body.starts_with("https://") {
            return Ok(body);
        }

        let dropped = body.is_empty() || status.is_server_error();
        if !dropped || attempt == ATTEMPTS {
            if body.is_empty() {
                anyhow::bail!(
                    "catbox took the upload but returned no link (HTTP {status}) — it drops \
                     large uploads when it's busy. Try again in a minute."
                )
            }
            anyhow::bail!("catbox upload failed: {body}")
        }
        tokio::time::sleep(RETRY_WAIT * attempt).await;
    }
    unreachable!("the loop returns or bails on its last attempt")
}

/// One anonymous multipart POST. On success catbox's response body *is* the download URL.
async fn catbox_post(
    client: &Client,
    url: &str,
    name: &str,
    bytes: Vec<u8>,
) -> anyhow::Result<(reqwest::StatusCode, String)> {
    let form = Form::new().text("reqtype", "fileupload").part(
        "fileToUpload",
        Part::bytes(bytes)
            .file_name(name.to_string())
            .mime_str("application/zip")?,
    );

    let resp = client
        .post(url)
        .multipart(form)
        .send()
        .await
        .context("couldn't reach catbox to upload the bundle")?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("catbox returned an unreadable response")?;
    Ok((status, body.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A stand-in catbox that hands back `replies` in order, one per request.
    fn serve_replies(replies: Vec<&'static str>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for reply in replies {
                let Ok((mut sock, _)) = listener.accept() else { return };
                // Drain the upload before replying — answering a peer that is still sending
                // resets the connection.
                drain_request(&mut sock);
                let _ = sock.write_all(reply.as_bytes());
                let _ = sock.flush();
            }
        });
        format!("http://127.0.0.1:{port}/user/api.php")
    }

    fn drain_request(sock: &mut std::net::TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match sock.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
            let Some(head) = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4) else {
                continue;
            };
            let len = String::from_utf8_lossy(&buf[..head])
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if buf.len() >= head + len {
                return;
            }
        }
    }

    fn reply(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    async fn upload_against(replies: Vec<&'static str>) -> anyhow::Result<String> {
        let url = serve_replies(replies);
        let client = Client::builder().build().unwrap();
        catbox_upload(&client, url, "part.zip", b"zip bytes").await
    }

    /// The reported failure: catbox answers 200 with nothing in it. That is a drop, not a
    /// refusal, and the next attempt takes the file.
    #[tokio::test(start_paused = true)]
    async fn an_empty_200_is_retried() {
        let good: &'static str =
            Box::leak(reply("200 OK", "https://files.catbox.moe/ok.zip").into_boxed_str());
        let empty: &'static str = Box::leak(reply("200 OK", "").into_boxed_str());
        let url = upload_against(vec![empty, good]).await.expect("retry recovers");
        assert_eq!(url, "https://files.catbox.moe/ok.zip");
    }

    /// Out of attempts, the message has to say what to do — not just quote a 200.
    #[tokio::test(start_paused = true)]
    async fn every_attempt_empty_reports_a_drop() {
        let empty: &'static str = Box::leak(reply("200 OK", "").into_boxed_str());
        let err = upload_against(vec![empty; ATTEMPTS as usize])
            .await
            .expect_err("an upload that never lands fails");
        let msg = err.to_string();
        assert!(msg.contains("returned no link"), "{msg}");
        assert!(msg.contains("Try again"), "{msg}");
    }

    /// A refusal is about the request, so it stands on the first answer and keeps its words.
    #[tokio::test(start_paused = true)]
    async fn prose_is_a_refusal_and_is_not_retried() {
        let refused: &'static str =
            Box::leak(reply("200 OK", "File too large, sorry.").into_boxed_str());
        let err = upload_against(vec![refused]).await.expect_err("prose is fatal");
        assert!(err.to_string().contains("File too large"), "{err}");
    }


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

        // Split on a small boundary rather than a whole part: the plan is what scales, and this
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
