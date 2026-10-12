//! The log bundle both apps share: scan a folder for logs, zip them with a summary, and
//! put the zip on the file host for a link.
//!
//! What counts as a log, and which folders are looked in, is each app's own business: MXB
//! App reads the game's and FrostMod's, Coach reads its own and the recorder's. This module
//! is everything after that: the file listing, the archive, the upload and the progress
//! events, so Settings → Logs behaves the same in both.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::upload::{self, human_size};

/// Skip anything absurd rather than build a 400 MB attachment nobody can upload. Log
/// files are text and rotate; one this big is a runaway, and the tail of it would be no
/// more diagnostic than the tail of a smaller one.
const MAX_EXPORT_FILE: u64 = 64 * 1024 * 1024;

/// Crash dumps per folder: the newest few say what is crashing now, and a folder of thirty
/// would make a bundle nobody can upload.
const MAX_DUMPS: usize = 3;

/// A minidump is a few MB; a full-memory dump can be gigabytes. Past this it is left out.
const MAX_DUMP_FILE: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    /// Last-modified in Unix ms, or `None` when the platform wouldn't say. The UI leads
    /// with this: how old the newest one is says whether a log covers the run the player
    /// is complaining about.
    pub modified: Option<u64>,
}

/// One place logs come from, whether or not it currently holds any.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogGroup {
    /// The folder the files below came from, or, when there are none, the folder they'd
    /// be in. Always populated: a path the player can go and look at beats "not found".
    pub dir: String,
    /// Whether `dir` is actually there. A missing folder and an empty one are different
    /// problems (nothing has run yet vs. nothing was written), and only one is worrying.
    pub exists: bool,
    /// Newest first.
    pub files: Vec<LogFile>,
}

impl LogGroup {
    pub fn missing(dir: PathBuf) -> Self {
        LogGroup { dir: dir.to_string_lossy().into_owned(), exists: false, files: Vec::new() }
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    /// How many log files made it in. Zero is possible and still worth writing: the
    /// summary alone says which folders were looked in and found empty.
    pub files: usize,
    pub bytes: u64,
}

pub fn is_dump(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".dmp")
}

/// Whether a file in a *game* folder is a log.
///
/// The game folder is the player's, full of things that are none of our business, so this
/// matches by name rather than sweeping the folder up. `log.txt` is what PiBoSo titles
/// write; the rest of the patterns cost nothing and cover a rotated or crash-time sibling
/// without needing to know its exact name in advance.
pub fn is_game_log(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".log")
        || (lower.starts_with("log") && lower.ends_with(".txt"))
        // The engine's and the sound layer's own logs beside `log.txt`: `e3dlog.txt`,
        // `sllog.txt`.
        || lower.ends_with("log.txt")
        || (lower.starts_with("crash") && (lower.ends_with(".txt") || lower.ends_with(".log")))
        // Crash dumps the game leaves beside the exe; [`scan`] keeps only the newest few.
        || is_dump(&lower)
}

/// Whether a file in the game's `plugins` folder goes in the bundle. That folder holds other
/// people's plugins too, so it is matched by name: logs, and FrostMod's crash reports and
/// dumps. Never a `.dlo`, a `.dll` or a settings file.
pub fn is_plugin_log(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".log")
        || (lower.starts_with("frostmod")
            && lower.contains("crash")
            && (lower.ends_with(".json") || is_dump(&lower)))
}
fn describe(path: &Path) -> Option<LogFile> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    Some(LogFile {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        bytes: meta.len(),
        modified,
    })
}

/// Everything in `dir`, newest first. For folders that are ours alone, like the app log
/// dir, where every file is a log whatever it's called or however the plugin rotated it.
pub fn scan_all(dir: &Path) -> LogGroup {
    scan(dir, |_| true)
}

pub fn scan(dir: &Path, keep: impl Fn(&str) -> bool) -> LogGroup {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return LogGroup::missing(dir.to_path_buf());
    };
    let mut files: Vec<LogFile> = rd
        .flatten()
        .filter(|e| keep(&e.file_name().to_string_lossy()))
        .filter_map(|e| describe(&e.path()))
        .collect();
    // Newest first: the run that just went wrong is the one anyone opens.
    files.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name.cmp(&b.name)));
    // Crash dumps: the newest few, and none too big to send.
    let mut dumps = 0usize;
    files.retain(|f| {
        if !is_dump(&f.name) {
            return true;
        }
        if f.bytes > MAX_DUMP_FILE || dumps >= MAX_DUMPS {
            return false;
        }
        dumps += 1;
        true
    });
    LogGroup { dir: dir.to_string_lossy().into_owned(), exists: true, files }
}

/// Open the folder a group lives in, with its newest file selected where the platform can
/// do that. Selecting matters: an app log dir holds several files and the one worth
/// opening is not the one sorted first alphabetically.
pub fn open_location(group: &LogGroup) -> anyhow::Result<()> {
    match group.files.first() {
        Some(newest) => crate::library::reveal_in_explorer(&newest.path),
        None => crate::library::open_folder(&group.dir),
    }
}

/// Write every group into one zip at `dest`, a folder each, beside a `summary.txt`.
///
/// `groups` pairs each group with the folder it takes inside the archive; the caller names
/// it once so the layout and the summary listing it can't drift apart.
pub fn export(dest: &Path, groups: &[(&str, &LogGroup)], summary: &str) -> anyhow::Result<ExportResult> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    // Deflate rather than the `Stored` a mod bundle uses: this payload is plain text,
    // which is the one thing compression is dramatic on.
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("summary.txt", opts)?;
    std::io::Write::write_all(&mut zip, summary.as_bytes())?;

    let mut written = 0usize;
    let mut bytes = 0u64;
    for (folder, group) in groups {
        for log in &group.files {
            if log.bytes > MAX_EXPORT_FILE {
                continue;
            }
            // A file that vanished or turned unreadable between the scan and now is not a
            // reason to fail the export; the rest of the logs are still worth having.
            let Ok(data) = std::fs::read(&log.path) else { continue };
            zip.start_file(format!("{folder}/{}", log.name), opts)?;
            std::io::Write::write_all(&mut zip, &data)?;
            written += 1;
            bytes += data.len() as u64;
        }
    }
    zip.finish()?;

    Ok(ExportResult { path: dest.to_string_lossy().into_owned(), files: written, bytes })
}

/// A log bundle that went up to the file host, and the link that came back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareResult {
    /// The direct download link: the whole point of the button, pasteable as it stands.
    pub url: String,
    /// Every slice's link, in order, on the rare occasion the zip was too big to go up in
    /// one piece. Empty for the single-part upload every log bundle should be.
    pub parts: Vec<String>,
    /// How many log files made it in, as [`ExportResult`] counts them.
    pub files: usize,
    /// Bytes of log collected, before compression.
    pub bytes: u64,
    /// Bytes actually uploaded: the zip, which for plain text is a fraction of `bytes`.
    pub size: u64,
}

/// Phase updates ride their own event, so the Settings share never hears another upload's
/// progress or the other way round.
pub const SHARE_EVENT: &str = "logs-share-progress";

/// One phase update, shaped as the apps' `BundleProgress` so the same bar draws it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fraction: Option<f64>,
}

fn emit(app: &tauri::AppHandle, phase: &'static str, message: Option<String>) {
    use tauri::Emitter;
    let _ = app.emit(SHARE_EVENT, Progress { phase, message, done: None, total: None, fraction: None });
}

fn emit_upload(app: &tauri::AppHandle, size: &str, p: upload::UploadProgress) {
    use tauri::Emitter;
    let n = p.parts;
    let message = if n > 1 {
        format!("Uploading part {} of {n} ({size})…", (p.done + 1).min(n))
    } else {
        format!("Uploading {size}…")
    };
    let _ = app.emit(
        SHARE_EVENT,
        Progress {
            phase: "uploading",
            message: Some(message),
            done: Some(p.done),
            total: Some(n),
            fraction: Some(p.fraction()),
        },
    );
}

/// Pack every group exactly as [`export`] does, upload the zip, and hand back the link.
///
/// The save button asks for a folder and leaves the player to attach the file somewhere.
/// This is the same archive with the last step done for them: one link to paste into a
/// bug report. What goes in is unchanged, logs and the summary and never a config file,
/// because the link is public to anyone who has it.
///
/// `zip_name` is the archive's file name on the host (`mxb-app-logs.zip`).
pub async fn share(
    app: &tauri::AppHandle,
    groups: Vec<(&'static str, LogGroup)>,
    summary: &str,
    zip_name: &str,
) -> anyhow::Result<ShareResult> {
    let work = std::env::temp_dir().join(format!("mxb-logs-share-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)?;
    let zip_path = work.join(zip_name);

    emit(app, "bundling", None);
    // Reading every log is blocking work, and an app log that has been growing for a month
    // is not small: off the UI thread, the same as the save-to-disk export.
    let written = {
        let (summary, dest) = (summary.to_string(), zip_path.clone());
        tauri::async_runtime::spawn_blocking(move || {
            let refs: Vec<(&str, &LogGroup)> = groups.iter().map(|(n, g)| (*n, g)).collect();
            export(&dest, &refs, &summary)
        })
        .await
        .map_err(|e| anyhow::anyhow!("packing the logs failed: {e}"))??
    };

    let size = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
    let total = human_size(size);
    emit(app, "uploading", Some(format!("Uploading {total}…")));
    let client = upload::client()?;
    let up = upload::upload_file(&client, &zip_path, |p| emit_upload(app, &total, p)).await?;

    // The zip is on the host now; the copy under temp has done its job either way.
    let _ = std::fs::remove_dir_all(&work);

    let mut parts = up.parts;
    let Some(url) = parts.first().cloned() else {
        anyhow::bail!("the upload returned no link")
    };
    // The first slice is the link, and the rest are only carried when there is more than
    // one to stitch back together.
    if parts.len() == 1 {
        parts.clear();
    }

    emit(app, "done", None);
    Ok(ShareResult { url, parts, files: written.files, bytes: written.bytes, size: up.size })
}
