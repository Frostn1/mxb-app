//! What the studio calls: a batch of FBX files converted on this machine, with progress.
//!
//! The options are the browser converter's (mxbsecure.com/convert), read the same way, so the
//! same settings make the same file.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The page's options, all optional; anything left out takes the converter's default.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// The text of an `fbx2edf.exe` parameter file or an export script to start from.
    pub params: Option<String>,
    /// "whole" or "parts".
    pub layout: Option<String>,
    /// With parts: the object that is the file's main one. Empty picks `chassis`.
    pub main: Option<String>,
    pub scale: Option<f32>,
    pub merge_distance: Option<f32>,
    /// Recalculate normals with this hard-edge angle.
    pub recalc_normals: Option<f32>,
    /// Keep the file's normals even when `params` says to recalculate.
    pub use_file_normals: bool,
    /// Two textures at once rather than one per core, for a machine short of memory.
    pub low_memory: bool,
}

/// What a batch says as it goes. `index` is the pair's place in the batch.
pub enum Event {
    /// `done` of `total` textures made; `(0, total)` once the model is read.
    Pictures { index: usize, done: usize, total: usize },
    /// Written, or failed with nothing left behind. The report is the converter's, as JSON.
    Finished { index: usize, result: Result<serde_json::Value, String> },
}

/// Whether the converter is in this build.
pub const fn available() -> bool {
    cfg!(fbx2edf)
}

#[cfg(not(fbx2edf))]
const MISSING: &str = "The converter isn't in this build.";

#[cfg(fbx2edf)]
fn params(o: &Options) -> anyhow::Result<crate::params::Params> {
    use crate::geometry::Normals;
    use crate::params::{Layout, Params};
    let mut p = match o.params.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(text) => Params::from_ini(text)?,
        None => Params::default(),
    };
    match o.layout.as_deref() {
        Some("parts") => p.layout = Layout::Parts,
        Some("whole") => p.layout = Layout::Whole,
        None => {}
        Some(other) => anyhow::bail!("unknown layout {other:?}"),
    }
    if let Some(m) = o.main.as_ref().filter(|m| !m.is_empty()) {
        p.main = m.clone();
    }
    if let Some(s) = o.scale {
        p.scale = s;
    }
    if let Some(d) = o.merge_distance {
        p.merge_distance = d;
    }
    if o.use_file_normals {
        p.normals = Normals::FromFile;
    } else if let Some(a) = o.recalc_normals {
        p.normals = Normals::Recalculate { angle: a };
    }
    p.validate()?;
    Ok(p)
}

/// Convert every `(fbx, edf)` pair, every texture of every model through one pool. One result
/// per pair, in order; a model that fails doesn't stop the others. `Err` only when the options
/// are refused, before anything is read.
#[cfg(fbx2edf)]
pub fn convert(pairs: &[(PathBuf, PathBuf)], options: &Options, on: &mut dyn FnMut(Event)) -> Result<Vec<Result<serde_json::Value, String>>, String> {
    use crate::convert::Progress;
    let p = params(options).map_err(|e| format!("{e:#}"))?;
    let mut limits = crate::batch::Limits::default();
    if options.low_memory {
        limits.threads = limits.threads.min(2);
    }
    let json = |r: &anyhow::Result<crate::Report>| match r {
        Ok(report) => serde_json::to_value(report).map_err(|e| e.to_string()),
        Err(e) => Err(format!("{e:#}")),
    };
    let results = crate::convert::convert_files_progress(pairs, &p, limits, &mut |index, progress| match progress {
        Progress::Pictures { done, total } => on(Event::Pictures { index, done, total }),
        Progress::Finished(r) => on(Event::Finished { index, result: json(r) }),
    });
    Ok(results.iter().map(json).collect())
}

#[cfg(not(fbx2edf))]
pub fn convert(_pairs: &[(PathBuf, PathBuf)], _options: &Options, _on: &mut dyn FnMut(Event)) -> Result<Vec<Result<serde_json::Value, String>>, String> {
    Err(MISSING.into())
}

/// The objects an FBX makes as bike parts, by name (the main one empty): enough to tell a
/// bike from a single model before converting it.
#[cfg(fbx2edf)]
pub fn part_names(fbx: &Path) -> Result<Vec<String>, String> {
    use crate::params::{Layout, Params};
    let bytes = std::fs::read(fbx).map_err(|e| format!("{}: {e}", fbx.display()))?;
    let files = crate::files::DiskFiles { root: fbx.parent().map(Path::to_path_buf).unwrap_or_default() };
    let p = Params { layout: Layout::Parts, ..Params::default() };
    let s = crate::session::Session::prepare(&bytes, &files, &p).map_err(|e| format!("{e:#}"))?;
    Ok(s.report().objects.iter().map(|o| o.name.clone()).collect())
}

#[cfg(not(fbx2edf))]
pub fn part_names(_fbx: &Path) -> Result<Vec<String>, String> {
    Err(MISSING.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_converter_every_call_says_so() {
        if available() {
            return;
        }
        assert!(convert(&[], &Options::default(), &mut |_| {}).unwrap_err().contains("isn't in this build"));
        assert!(part_names(Path::new("x.fbx")).is_err());
    }

    #[test]
    fn options_read_as_the_browser_sends_them() {
        let o: Options = serde_json::from_str(r#"{"layout":"parts","main":"","scale":1,"mergeDistance":0,"useFileNormals":true,"lowMemory":true}"#).unwrap();
        assert_eq!(o.layout.as_deref(), Some("parts"));
        assert!(o.use_file_normals && o.low_memory);
        assert_eq!(o.recalc_normals, None);
    }

    #[cfg(fbx2edf)]
    #[test]
    fn refused_settings_are_refused_before_anything_is_read() {
        let o = Options { layout: Some("sideways".into()), ..Default::default() };
        let pairs = [(PathBuf::from("does-not-exist.fbx"), PathBuf::from("out.edf"))];
        assert!(convert(&pairs, &o, &mut |_| panic!("nothing runs")).is_err());
    }
}
