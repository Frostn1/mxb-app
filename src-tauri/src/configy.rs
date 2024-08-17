// pub mod config;
use serde_json::{to_string, Value};
use std::fs::File;
use std::io::prelude::*;

static CONFIG_FILE: &str = ".config.json";

pub(crate) fn is_config_file_exist() -> bool {
    Path::new(CONFIG_FILE).exists()
}

pub(crate) fn configure_new(config: Value) -> None {
    let mut file;
    if !is_config_file_exist() {
        file = File::create(CONFIG_FILE);
    } else {
        file = File::open(CONFIG_FILE);
    }
    file.write_all(to_string(config));
}
