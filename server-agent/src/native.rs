//! The deliberately small part of native `mxbserver` configuration the remote manager owns.
//! It uses `toml_edit` so comments and every field outside this allow-list survive a change.

use serde::{Deserialize, Serialize};
use toml_edit::{value, Array, DocumentMut, Item, Table};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Patch {
    pub track: Option<String>,
    pub rotation: Option<Vec<String>>,
    pub bots: Option<u32>,
    pub sessions: Option<SessionsPatch>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionsPatch {
    pub practice_minutes: Option<u64>,
    pub qualifying_minutes: Option<u64>,
    pub warmup_minutes: Option<u64>,
    pub race_minutes: Option<u64>,
    pub race_countdown_seconds: Option<u64>,
    pub race_extra_laps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub name: Option<String>,
    pub track: Option<String>,
    pub rotation: Vec<String>,
    pub bots: u64,
    pub max_clients: Option<u64>,
    pub sessions: SessionsView,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionsView {
    pub practice_minutes: Option<u64>,
    pub qualifying_minutes: Option<u64>,
    pub warmup_minutes: Option<u64>,
    pub race_minutes: Option<u64>,
    pub race_countdown_seconds: Option<u64>,
    pub race_extra_laps: Option<u64>,
}

fn table<'a>(doc: &'a mut DocumentMut, name: &str) -> &'a mut Table {
    let item = doc
        .as_table_mut()
        .entry(name)
        .or_insert_with(|| Item::Table(Table::new()));
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut().expect("just made a table")
}

fn plain_track(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 128
        || name.contains(['/', '\\', '\n', '\r'])
        || name.starts_with('.')
    {
        return Err("track must be one installed track name".into());
    }
    Ok(name)
}

pub fn patch(text: &str, patch: &Patch, installed: &[String]) -> Result<String, String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("invalid native config: {e}"))?;
    let mut changed = false;
    if let Some(track) = patch.track.as_deref() {
        let track = plain_track(track)?;
        if !installed.iter().any(|candidate| candidate == track) {
            return Err(format!("track {track:?} is not installed on this server"));
        }
        // Package paths are resolved from config/server.toml, not the process cwd.
        table(&mut doc, "track")["package"] = value(format!("../tracks/{track}.pkz"));
        changed = true;
    }
    if let Some(rotation) = &patch.rotation {
        let mut array = Array::new();
        for track in rotation {
            let track = plain_track(track)?;
            if !installed.iter().any(|candidate| candidate == track) {
                return Err(format!("track {track:?} is not installed on this server"));
            }
            array.push(format!("../tracks/{track}.pkz"));
        }
        table(&mut doc, "rotation")["tracks"] = value(array);
        changed = true;
    }
    if let Some(bots) = patch.bots {
        if bots > 49 {
            return Err("bots must be between 0 and 49".into());
        }
        table(&mut doc, "ghost")["count"] = value(i64::from(bots));
        changed = true;
    }
    if let Some(sessions) = &patch.sessions {
        let values = [
            ("practice_minutes", sessions.practice_minutes),
            ("qualifying_minutes", sessions.qualifying_minutes),
            ("warmup_minutes", sessions.warmup_minutes),
            ("race_minutes", sessions.race_minutes),
            ("race_countdown_seconds", sessions.race_countdown_seconds),
            ("race_extra_laps", sessions.race_extra_laps.map(u64::from)),
        ];
        for (key, minutes) in values {
            if let Some(number) = minutes {
                let maximum = if key == "race_countdown_seconds" {
                    600
                } else {
                    1440
                };
                if number > maximum
                    || (number == 0 && key != "practice_minutes" && key != "race_countdown_seconds")
                {
                    return Err(format!("{key} is outside the supported range"));
                }
                table(&mut doc, "sessions")[key] = value(number as i64);
                changed = true;
            }
        }
    }
    if !changed {
        return Err("nothing to change".into());
    }
    let checked = view(&doc.to_string())?;
    if checked.bots + checked.max_clients.unwrap_or(0) > 50 {
        return Err("bots plus max_clients cannot exceed 50".into());
    }
    Ok(doc.to_string())
}

fn string(doc: &DocumentMut, section: &str, key: &str) -> Option<String> {
    doc.get(section)?.get(key)?.as_str().map(str::to_owned)
}
fn integer(doc: &DocumentMut, section: &str, key: &str) -> Option<u64> {
    doc.get(section)?
        .get(key)?
        .as_integer()
        .and_then(|n| u64::try_from(n).ok())
}

pub fn view(text: &str) -> Result<View, String> {
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("invalid native config: {e}"))?;
    Ok(View {
        name: string(&doc, "server", "name"),
        track: string(&doc, "track", "package").and_then(|p| {
            std::path::Path::new(&p)
                .file_stem()?
                .to_str()
                .map(str::to_owned)
        }),
        rotation: doc
            .get("rotation")
            .and_then(|v| v.get("tracks"))
            .and_then(Item::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .filter_map(|p| {
                std::path::Path::new(p)
                    .file_stem()?
                    .to_str()
                    .map(str::to_owned)
            })
            .collect(),
        bots: integer(&doc, "ghost", "count").unwrap_or(0),
        // Native server default; carrying it in the view keeps bot validation honest when
        // the operator left the key out of an otherwise valid config.
        max_clients: Some(integer(&doc, "server", "max_clients").unwrap_or(20)),
        sessions: SessionsView {
            practice_minutes: integer(&doc, "sessions", "practice_minutes"),
            qualifying_minutes: integer(&doc, "sessions", "qualifying_minutes"),
            warmup_minutes: integer(&doc, "sessions", "warmup_minutes"),
            race_minutes: integer(&doc, "sessions", "race_minutes"),
            race_countdown_seconds: integer(&doc, "sessions", "race_countdown_seconds"),
            race_extra_laps: integer(&doc, "sessions", "race_extra_laps"),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "# keep me\n[server]\nname = 'Race night'\nmax_clients = 20\n[track]\npackage = 'tracks/old.pkz'\n[sessions]\npractice_minutes = 20\nrace_minutes = 15\n";

    #[test]
    fn changes_only_manager_owned_fields_and_keeps_comments() {
        let out = patch(
            CONFIG,
            &Patch {
                track: Some("new".into()),
                rotation: Some(vec!["third".into()]),
                bots: Some(6),
                sessions: Some(SessionsPatch {
                    practice_minutes: Some(0),
                    qualifying_minutes: Some(10),
                    ..Default::default()
                }),
            },
            &["new".into(), "third".into()],
        )
        .unwrap();
        assert!(out.contains("# keep me"));
        let got = view(&out).unwrap();
        assert_eq!(got.track.as_deref(), Some("new"));
        assert_eq!(got.rotation, ["third"]);
        assert_eq!(got.bots, 6);
        assert_eq!(got.sessions.practice_minutes, Some(0));
        assert_eq!(got.sessions.qualifying_minutes, Some(10));
        assert!(out.contains("package = \"../tracks/new.pkz\""));
        assert!(out.contains("tracks = [\"../tracks/third.pkz\"]"));
    }

    #[test]
    fn refuses_uninstalled_tracks_and_invalid_counts() {
        assert!(patch(
            CONFIG,
            &Patch {
                track: Some("missing".into()),
                ..Default::default()
            },
            &[]
        )
        .is_err());
        assert!(patch(
            CONFIG,
            &Patch {
                bots: Some(50),
                ..Default::default()
            },
            &[]
        )
        .is_err());
    }
}
