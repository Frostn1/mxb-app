// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use serde_json::*;
use std::fs::{self, ReadDir};
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

static CONFIG_FILE: &str = ".config.json";

pub(crate) fn is_config_file_exist() -> bool {
    Path::new(CONFIG_FILE).exists()
}

pub(crate) fn configure_new(config: &mut serde_json::Value) -> std::io::Result<()> {
    println!("{}", config);
    if config["modsPath"].as_str().unwrap().len() == 0 {
        let mut new_path: PathBuf = dirs_next::document_dir().unwrap();
        new_path.push("PiBoSo\\MX Bikes");
        println!("{}", new_path.clone().into_os_string().to_str().unwrap());

        config["modsPath"] = json!(new_path.clone().into_os_string().to_str().unwrap());
    }
    fs::write(CONFIG_FILE, to_string_pretty(config).unwrap()).expect("Unable to write file");
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct Track {
    pub path: String,
    pub name: String,
    pub image: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct TrackFolder {
    pub path: String,
    pub name: String,
    pub tracks: Vec<Track>,
}

fn map_track_file(track_path: std::fs::DirEntry) -> Track {
    let entry_path = track_path.path();

    let file_name = entry_path.file_name().unwrap();
    println!("Track path is {}", entry_path.display());
    return Track {
        path: entry_path.to_str().unwrap().to_string().to_owned(),
        name: file_name.to_str().unwrap().to_string().to_owned(),
        image: vec![],
    };
}

fn map_track_folder(dir_path: std::fs::DirEntry) -> TrackFolder {
    println!("Folder Path is {}", dir_path.path().display());
    let tracks: Vec<Track> = fs::read_dir(dir_path.path().to_owned())
        .unwrap()
        .map(|track| map_track_file(track.unwrap()))
        .collect();
    return TrackFolder {
        path: dir_path.path().to_str().unwrap().to_string().to_owned(),
        name: dir_path
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
            .to_owned(),
        tracks,
    };
}

#[tauri::command]
fn get_library_mods(library_path: &str) -> Value {
    println!("Library path {}", library_path);
    let matching_files: Vec<TrackFolder> = fs::read_dir(library_path)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|x| map_track_folder(x))
        .collect();

    // println!("Files {}", matching_files);

    return serde_json::json!(matching_files);

    // for entry in WalkDir::new(library_path)
    //     .into_iter()
    //     .filter_map(|e| e.ok())
    // {
    //     println!("{}", entry.path().display());
    // }
    // return true;
}

#[tauri::command]
fn is_configured() -> bool {
    return is_config_file_exist();
}

#[tauri::command]
fn create_config(config: &str) -> bool {
    let _ = configure_new(&mut serde_json::from_str(config).unwrap());
    true
}
#[tauri::command]
fn get_config() -> Value {
    let mut contents = "{}".to_string();
    if is_configured() {
        contents = fs::read_to_string(CONFIG_FILE).expect("{}");
    }

    return serde_json::from_str(contents.as_str()).unwrap();
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            is_configured,
            create_config,
            get_config,
            get_library_mods
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
