use anyhow::Context;
use futures_util::{StreamExt, TryStreamExt};
use obfstr::obfstr;
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
    /// What each part should weigh, in the same order. Carried into the share code so a
    /// download can tell *which* slice came back short instead of only that the total did.
    pub part_sizes: Vec<u64>,
}

/// filebin.net keeps a bin for 6 days. catbox, the host before it, stopped storing uploads
/// on 2026-09-12 — every POST, down to 1 MiB, answered 200 with no link.
const HOST: &str = "filebin";

/// Small parts, so a dropped transfer costs one slice rather than the whole bundle. 24 MiB
/// round-trips to the byte on filebin in about five seconds.
const PART_BYTES: u64 = 24 * 1024 * 1024;

/// The slicings to try, in order — see `upload_file`.
const PART_STEPS: [u64; 3] = [PART_BYTES, PART_BYTES * 3 / 4, PART_BYTES / 2];

/// Attempts per part. A 5xx or 429 is the host busy, and the next attempt usually takes it.
const ATTEMPTS: u32 = 3;

/// Grows with each retry: a busy host wants a moment, not another request.
const RETRY_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// A ceiling on the bundle as a whole. Past this the upload takes long enough, and depends
/// on enough separate files staying alive, that sharing the plain code is the better answer.
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Upload `file`, slicing it if it's over one part. `on_part(done, total)` reports how many
/// parts are stored — `0` before the first — so the caller can draw a bar. A recut starts
/// the count again at `0` of the new total.
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

    // A part the host keeps short is sent again at a different cut, into a fresh bin, before
    // the share gives up. catbox needed this because it stored by content hash and handed a
    // truncated copy back for ever; it costs nothing on a host that stores what it is sent.
    let mut last: Option<anyhow::Error> = None;
    for part_bytes in PART_STEPS {
        let started = std::time::Instant::now();
        match upload_sliced(client, file, size, part_bytes, &on_part).await {
            Ok(up) => {
                log::info!(
                    "share: uploaded {} in {} part(s) of {} in {:.1}s",
                    crate::bundle::human_size(size),
                    up.parts.len(),
                    crate::bundle::human_size(part_bytes),
                    started.elapsed().as_secs_f32()
                );
                return Ok(up);
            }
            Err(Short(e)) => {
                // Worth a line of its own: a recut sends the whole file again, so this is
                // the difference between a twenty-second share and a three-minute one.
                log::warn!(
                    "share: a {} cut came back short after {:.1}s — recutting: {e:#}",
                    crate::bundle::human_size(part_bytes),
                    started.elapsed().as_secs_f32()
                );
                last = Some(e);
            }
            Err(Fatal(e)) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("the upload could not be stored")))
}

/// A failure that a different slicing might get past, and one that never will.
enum UploadFail {
    Short(anyhow::Error),
    Fatal(anyhow::Error),
}
use UploadFail::{Fatal, Short};

/// Slices in flight at once. They are independent files, so sending them one at a time left
/// the link idle; three, not more, so we are never the reason the host is busy.
const UPLOAD_CONCURRENCY: usize = 3;

async fn upload_sliced(
    client: &Client,
    file: &Path,
    size: u64,
    part_bytes: u64,
    on_part: &impl Fn(usize, usize),
) -> Result<Upload, UploadFail> {
    let plan = part_plan_of(size, part_bytes);
    let n = plan.len();
    let stem = file
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "preset-bundle".to_string());
    // One bin per cut, so a recut never lands beside the parts of the cut it replaces.
    let bin = format!("mxb{}", uuid::Uuid::new_v4().simple());
    let base = endpoint();

    // Counted rather than indexed: with several in flight the useful number is how many are
    // behind us, so the bar never jumps backwards.
    let done = std::sync::atomic::AtomicUsize::new(0);
    on_part(0, n);

    let jobs: Vec<_> = plan
        .iter()
        .enumerate()
        .map(|(i, &(offset, len))| {
            let name = if n == 1 {
                format!("{stem}.zip")
            } else {
                format!("{stem}.part{}of{}.zip", i + 1, n)
            };
            let done = &done;
            let src = file.to_path_buf();
            let (base, bin) = (&base, &bin);
            async move {
                // Its own handle per slice, and off the runtime: the reads are interleaved
                // now, so a shared cursor would have them seeking over each other — and a
                // blocking 24 MiB read on a runtime thread stalls the slices beside it.
                let started = std::time::Instant::now();
                let bytes = tokio::task::spawn_blocking({
                    let src = src.clone();
                    move || read_slice_at(&src, offset, len)
                })
                .await
                .map_err(|e| Fatal(anyhow::anyhow!("reading {} failed: {e}", src.display())))?
                .with_context(|| format!("reading {}", src.display()))
                .map_err(Fatal)?;
                let url = part_url(base, bin, &name).map_err(Fatal)?;
                let url = put_part(client, &url, &bytes, len).await?;
                log::info!(
                    "share: part {} of {n} ({}) took {:.1}s",
                    i + 1,
                    crate::bundle::human_size(len),
                    started.elapsed().as_secs_f32()
                );
                let behind = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                on_part(behind, n);
                Ok::<(String, u64), UploadFail>((url, len))
            }
        })
        .collect();

    let uploaded: Vec<(String, u64)> = futures_util::stream::iter(jobs)
        .buffered(UPLOAD_CONCURRENCY)
        .try_collect()
        .await?;

    let (parts, sizes) = uploaded.into_iter().unzip();
    Ok(Upload { parts, host: HOST.to_string(), size, part_sizes: sizes })
}

/// Slice `size` bytes into `(offset, len)` spans no bigger than one part. A zero-byte file
/// still gets one span, so an empty upload fails at the host rather than silently succeeding
/// with no parts at all.
fn part_plan(size: u64) -> Vec<(u64, u64)> {
    part_plan_of(size, PART_BYTES)
}

/// The same, cut at a given part size — see [`upload_file`] on why the size varies between
/// attempts.
fn part_plan_of(size: u64, part: u64) -> Vec<(u64, u64)> {
    let mut spans = Vec::new();
    let mut offset = 0u64;
    while offset < size {
        let len = part.min(size - offset);
        spans.push((offset, len));
        offset += len;
    }
    if spans.is_empty() {
        spans.push((0, 0));
    }
    spans
}

/// Read one slice through a handle of its own — see [`upload_sliced`].
fn read_slice_at(path: &Path, offset: u64, len: u64) -> std::io::Result<Vec<u8>> {
    read_slice(&mut std::fs::File::open(path)?, offset, len)
}

fn read_slice(file: &mut std::fs::File, offset: u64, len: u64) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Split out so a test can point the upload at a local server.
fn endpoint() -> String {
    obfstr!("https://filebin.net").to_string()
}

/// `{base}/{bin}/{name}` — both where a part is PUT and where it downloads from. Built as
/// path segments so a track name with spaces in it is escaped rather than cut short.
fn part_url(base: &str, bin: &str, name: &str) -> anyhow::Result<String> {
    let mut url = reqwest::Url::parse(base).context("the upload host's address is malformed")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("the upload host's address can't take a path"))?
        .pop_if_empty()
        .push(bin)
        .push(name);
    Ok(url.into())
}

/// What the host says it is holding at `url`, if it will say.
///
/// `None` means the question could not be answered, and a caller must not read that as zero.
///
/// It asks for the first byte rather than sending a HEAD, because catbox — still behind every
/// code made before filebin — answers HEAD with `content-length: 0` for a file it holds fine.
/// A `Range: bytes=0-0` answers `206` with `content-range: bytes 0-0/<total>`.
pub(crate) async fn hosted_len(client: &Client, url: &str) -> Option<u64> {
    let ask = || {
        client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send()
    };
    let mut resp = ask().await.ok()?;
    if crate::install::is_cookie_gate(&resp) {
        resp = ask().await.ok()?;
    }
    // A web page is not the file, whatever length it is.
    if !resp.status().is_success() || crate::install::is_web_page(&resp) {
        return None;
    }
    // `content-range: bytes 0-0/2000000` — the size is what follows the slash.
    if let Some(range) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
        if let Some(total) = range
            .to_str()
            .ok()
            .and_then(|v| v.rsplit('/').next().map(|s| s.trim().to_string()))
            .and_then(|v| v.parse::<u64>().ok())
        {
            return Some(total);
        }
    }
    // A host that ignored the range and sent the whole thing has still answered the question.
    // So has an empty 200: that is how catbox serves a part it kept none of, and reading it as
    // "won't say" let an upload of eight empty parts through as a working code.
    match (resp.status(), resp.content_length()) {
        (_, Some(n)) if n > 1 => Some(n),
        (reqwest::StatusCode::OK, Some(0)) => Some(0),
        _ => None,
    }
}

/// The gaps between asking the host what it is holding, in seconds — and so how many times
/// it is asked before we believe the answer.
///
/// A host can hand back the link before it has finished writing the part: catbox did, and a
/// 20 MB part read as 2.8 MB, then as 20 MB a few seconds later. A part that is already whole
/// answers on the first ask and waits for none of these.
const SETTLE_WAITS: [u64; 7] = [1, 2, 3, 3, 4, 4, 5];

/// How many times running the host may decline to answer before we stop asking.
///
/// A `None` is not a short part — it is the host not saying, and [`put_part`] accepts the
/// part on `None` however long we wait for it, so sitting out the whole ladder bought nothing.
/// Two, not one: a 404 straight after the upload can be the host still registering the file.
const NONE_PROBES: usize = 2;

/// What the host is holding, once it has stopped growing — or the last thing it said.
async fn settled_len(client: &Client, url: &str, expect: u64) -> Option<u64> {
    let mut last = hosted_len(client, url).await;
    let mut unanswered = usize::from(last.is_none());
    for wait in SETTLE_WAITS {
        if last == Some(expect) || unanswered >= NONE_PROBES {
            return last;
        }
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        last = hosted_len(client, url).await;
        unanswered = if last.is_none() { unanswered + 1 } else { 0 };
    }
    last
}

/// PUT one part to `url`, retrying while the host is busy. Anything else it refuses is
/// final: the request itself is what it objected to, so sending it again says nothing new.
///
/// A `201` is not proof the part is *there*, so the link is checked before it is trusted, and
/// a short one is sent again like any other drop.
async fn put_part(
    client: &Client,
    url: &str,
    bytes: &[u8],
    expect: u64,
) -> Result<String, UploadFail> {
    for attempt in 1..=ATTEMPTS {
        let resp = client
            .put(url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await
            .context("couldn't reach filebin to upload the bundle")
            .map_err(Fatal)?;
        let status = resp.status();

        if status.is_success() {
            match settled_len(client, url, expect).await {
                Some(got) if got != expect => {
                    if attempt == ATTEMPTS {
                        return Err(Short(anyhow::anyhow!(
                            "filebin is holding {} of a {} part after {ATTEMPTS} tries.",
                            crate::bundle::human_size(got),
                            crate::bundle::human_size(expect)
                        )));
                    }
                }
                // The right size, or the host would not say — nothing to act on either way.
                _ => return Ok(url.to_string()),
            }
        } else {
            let body = resp.text().await.unwrap_or_default();
            // 429 is filebin's "storage limit reached, retry later".
            let busy = status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            if !busy {
                return Err(Fatal(anyhow::anyhow!(
                    "filebin refused the upload (HTTP {status}): {}",
                    body.trim()
                )));
            }
            if attempt == ATTEMPTS {
                return Err(Fatal(anyhow::anyhow!(
                    "filebin is busy (HTTP {status}) — try sharing again in a few minutes."
                )));
            }
        }
        tokio::time::sleep(RETRY_WAIT * attempt).await;
    }
    unreachable!("the loop returns or bails on its last attempt")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The whole round trip against the live host: upload a real file, ask what came back,
    /// and put the parts together again.
    ///
    /// ```text
    /// FROST_UPLOAD=…/track.pkz cargo test -- --ignored --nocapture round_trips_a_real_file
    /// ```
    ///
    /// This is the only thing that answers "why did a shared bundle come back short" — the
    /// stubbed tests below cannot, because what goes wrong is the host keeping some of a file
    /// and still handing back a link for it.
    #[test]
    #[ignore = "uploads to the live host — set FROST_UPLOAD"]
    fn round_trips_a_real_file() {
        let path = std::path::PathBuf::from(std::env::var("FROST_UPLOAD").expect("set FROST_UPLOAD"));
        let size = std::fs::metadata(&path).unwrap().len();
        let plan = part_plan(size);
        println!("  {} is {} in {} part(s)", path.display(), crate::bundle::human_size(size), plan.len());

        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let client = match std::env::var("FROST_H1") {
                Ok(_) => Client::builder()
                    .user_agent(crate::frostmod_manage::UA)
                    .http1_only()
                    .cookie_store(true)
                    .build()
                    .unwrap(),
                Err(_) => crate::install::build_client().expect("a client"),
            };
            let up = upload_file(&client, &path, |i, n| println!("  {i} of {n} part(s) stored"))
                .await
                .expect("the upload goes through");
            println!("  host says {} bytes total across {:?}", up.size, up.part_sizes);

            let mut joined = 0u64;
            for (i, url) in up.parts.iter().enumerate() {
                let head = hosted_len(&client, url).await;
                let body = client.get(url).send().await.expect("a part downloads");
                let len = body.bytes().await.expect("its bytes").len() as u64;
                println!(
                    "  part {}: uploaded {}, host says {:?}, GET gave {} — {}",
                    i + 1,
                    up.part_sizes[i],
                    head,
                    len,
                    if len == up.part_sizes[i] { "matches" } else { "SHORT" }
                );
                joined += len;
            }
            println!(
                "  joined {} against {} — {}",
                crate::bundle::human_size(joined),
                crate::bundle::human_size(size),
                if joined == size { "matches" } else { "SHORT" }
            );
            assert_eq!(joined, size, "the parts do not add up to the file");
        });
    }

    /// A stand-in host that hands back `replies` in order, one per request, then hangs up.
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
        format!("http://127.0.0.1:{port}")
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

    /// The same stand-in, but it keeps answering — the last reply repeats — and says how
    /// many times it was asked. Counting the asks is the whole point: what went wrong with
    /// the settle ladder was never a wrong answer, it was how long it kept asking for one.
    fn serve_counting(
        replies: Vec<&'static str>,
    ) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = hits.clone();
        std::thread::spawn(move || loop {
            let Ok((mut sock, _)) = listener.accept() else { return };
            let i = seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            drain_request(&mut sock);
            let _ = sock.write_all(replies[i.min(replies.len() - 1)].as_bytes());
            let _ = sock.flush();
        });
        (format!("http://127.0.0.1:{port}/part.zip"), hits)
    }

    /// What a host holding `total` bytes answers a `Range: bytes=0-0` with.
    fn ranged(total: u64) -> &'static str {
        Box::leak(
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/{total}\r\n\
                 Content-Length: 1\r\nConnection: close\r\n\r\nx"
            )
            .into_boxed_str(),
        )
    }

    /// A 200 web page, with or without the cookie filebin gates its downloads behind.
    fn page(cookie: bool) -> &'static str {
        let body = "<html>verify</html>";
        let set = if cookie { "Set-Cookie: verified=1; Path=/\r\n" } else { "" };
        Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n{set}\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_boxed_str(),
        )
    }

    /// A host that will not say what it is holding is not a host holding a short part, and
    /// [`put_part`] takes the part either way. So the ladder must stop asking rather than sit
    /// out all seven waits to reach the answer it had at the second one.
    #[tokio::test(start_paused = true)]
    async fn an_unanswerable_probe_stops_asking() {
        let missing: &'static str = Box::leak(reply("404 Not Found", "nope").into_boxed_str());
        let (url, hits) = serve_counting(vec![missing]);
        let client = Client::builder().build().unwrap();
        assert_eq!(settled_len(&client, &url, 100).await, None);
        assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), NONE_PROBES);
    }

    /// The other direction, and the reason the ladder exists: a host that *is* answering,
    /// with a part it is still writing, gets waited on until the number stops moving.
    #[tokio::test(start_paused = true)]
    async fn a_part_still_being_written_is_waited_out() {
        let (url, hits) = serve_counting(vec![ranged(50), ranged(50), ranged(100)]);
        let client = Client::builder().build().unwrap();
        assert_eq!(settled_len(&client, &url, 100).await, Some(100));
        assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    /// An empty 200 is the host holding nothing, not the host declining to say — so the part
    /// is short and gets recut, instead of going out in a code that downloads as 0 B.
    #[tokio::test(start_paused = true)]
    async fn an_empty_200_is_a_part_holding_nothing() {
        let empty: &'static str = Box::leak(reply("200 OK", "").into_boxed_str());
        let (url, _) = serve_counting(vec![empty]);
        let client = Client::builder().build().unwrap();
        assert_eq!(settled_len(&client, &url, 100).await, Some(0));
    }

    /// filebin answers a first GET with a page that sets a cookie. Asking again gets the file —
    /// read the page's length instead and every part looks short.
    #[tokio::test(start_paused = true)]
    async fn a_cookie_gate_is_asked_past() {
        let (url, hits) = serve_counting(vec![page(true), ranged(100)]);
        let client = Client::builder().cookie_store(true).build().unwrap();
        assert_eq!(hosted_len(&client, &url).await, Some(100));
        assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    /// A page with no cookie is not a gate, and its length is not the part's.
    #[tokio::test(start_paused = true)]
    async fn a_web_page_is_not_a_size() {
        let (url, _) = serve_counting(vec![page(false)]);
        let client = Client::builder().build().unwrap();
        assert_eq!(hosted_len(&client, &url).await, None);
    }

    fn reply(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    async fn upload_against(replies: Vec<&'static str>) -> anyhow::Result<String> {
        let url = part_url(&serve_replies(replies), "bin", "part.zip").unwrap();
        let client = Client::builder().build().unwrap();
        put_part(&client, &url, b"zip bytes", b"zip bytes".len() as u64)
            .await
            .map_err(|e| match e {
                Short(e) | Fatal(e) => e,
            })
    }

    /// A 201 hands back the part's own address — filebin serves it where it was PUT.
    #[tokio::test(start_paused = true)]
    async fn a_stored_part_is_its_own_link() {
        let created: &'static str = Box::leak(reply("201 Created", "{}").into_boxed_str());
        let url = upload_against(vec![created]).await.expect("a 201 is stored");
        assert!(url.ends_with("/bin/part.zip"), "{url}");
    }

    /// A busy host is a drop, not a refusal, and the next attempt takes the file.
    #[tokio::test(start_paused = true)]
    async fn a_busy_host_is_retried() {
        let busy: &'static str = Box::leak(reply("503 Service Unavailable", "busy").into_boxed_str());
        let created: &'static str = Box::leak(reply("201 Created", "{}").into_boxed_str());
        let url = upload_against(vec![busy, created]).await.expect("retry recovers");
        assert!(url.ends_with("/bin/part.zip"), "{url}");
    }

    /// Out of attempts, the message has to say what to do — not just quote a status.
    #[tokio::test(start_paused = true)]
    async fn a_host_busy_every_time_says_so() {
        let full: &'static str =
            Box::leak(reply("429 Too Many Requests", "Storage limit reached").into_boxed_str());
        let err = upload_against(vec![full; ATTEMPTS as usize])
            .await
            .expect_err("an upload that never lands fails");
        let msg = err.to_string();
        assert!(msg.contains("busy") && msg.contains("try sharing again"), "{msg}");
    }

    /// A refusal is about the request, so it stands on the first answer and keeps its words.
    /// The stand-in hangs up after one reply, so a retry would fail as "couldn't reach".
    #[tokio::test(start_paused = true)]
    async fn a_refusal_is_not_retried() {
        let locked: &'static str =
            Box::leak(reply("405 Method Not Allowed", "The bin is locked").into_boxed_str());
        let err = upload_against(vec![locked]).await.expect_err("a refusal is fatal");
        assert!(err.to_string().contains("The bin is locked"), "{err}");
    }

    /// A track name with a space in it is escaped, not cut off at the space.
    #[test]
    fn part_urls_escape_the_name() {
        let url = part_url("https://filebin.net", "mxbabc", "Draycott Valley.part1of8.zip").unwrap();
        assert_eq!(url, "https://filebin.net/mxbabc/Draycott%20Valley.part1of8.zip");
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
