// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::to_string_pretty;
use serde_json::*;
use std::fs;
use std::path::{Path, PathBuf};

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
            get_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
