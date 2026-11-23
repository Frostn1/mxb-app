use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::Manager;

use crate::{extensions as ext, extmcp};

const MAX_AUTHOR_KEY_BYTES: u64 = 16 * 1024;

fn write_author_key(path: &Path, pem: &str) -> Result<(), String> {
    let tmp = path.with_extension(format!("pem.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
    file.write_all(pem.as_bytes()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

fn author_key(app: &tauri::AppHandle) -> Result<(ed25519_dalek::SigningKey, PathBuf), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("extensions");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("author-key.pem");
    if !path.exists() {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|e| e.to_string())?;
        let mut der = vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        der.extend_from_slice(&seed);
        let pem = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            B64.encode(der)
        );
        write_author_key(&path, &pem)?;
    }
    let pem = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    Ok((extmcp::signing_key_from_pem(&pem)?, path))
}

fn author_key_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("extensions")
        .join("author-key.pem"))
}

fn restore_author_key_file(src: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(src).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_AUTHOR_KEY_BYTES {
        return Err("That file is too large to be an author key.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_AUTHOR_KEY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_AUTHOR_KEY_BYTES {
        return Err("That file is too large to be an author key.".into());
    }
    let pem = std::str::from_utf8(&bytes)
        .map_err(|_| "That file is not a valid author key.".to_string())?;
    extmcp::signing_key_from_pem(pem)
        .map_err(|_| "That file is not a valid author key.".to_string())?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_author_key(dest, pem)
}

fn check_folder_types(root: &Path) -> Result<(), String> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with('.') || rel.contains("/.") || rel.starts_with("node_modules/") {
            continue;
        }
        let extension = Path::new(&rel)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !ext::ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
            return Err(format!(
                "that package contains a file type extensions may not carry: {rel}"
            ));
        }
    }
    Ok(())
}

fn package(app: &tauri::AppHandle, path: &Path) -> Result<(Vec<u8>, String), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        check_folder_types(path)?;
        let files = ext::read_folder(path).map_err(|e| e.to_string())?;
        let (key, _) = author_key(app)?;
        return Ok((extmcp::pack_files(files, Some(&key))?, "author-key".into()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let files = ext::read_zip(&bytes).map_err(|e| e.to_string())?;
    if files.contains_key("signature.json") {
        Ok((bytes, "package".into()))
    } else {
        let (key, _) = author_key(app)?;
        Ok((extmcp::pack_files(files, Some(&key))?, "author-key".into()))
    }
}

async fn send(app: tauri::AppHandle, path: String, dry_run: bool) -> Result<Value, String> {
    let token = ext::account_token(&app)
        .map_err(|_| "Sign in to MXB App to publish extensions.".to_string())?;
    let (bytes, signed_with) = package(&app, Path::new(&path))?;
    let suffix = if dry_run { "?dryRun=1" } else { "" };
    let response = ext::catalog_http()
        .post(format!("{}/v1/extensions{suffix}", ext::catalog_base()))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/zip")
        .body(bytes)
        .send()
        .await
        .map_err(|_| ext::CATALOG_DOWN.to_string())?;
    let mut value = ext::catalog_json(response).await?;
    if dry_run {
        value["signedWith"] = json!(signed_with);
    }
    Ok(value)
}

#[tauri::command]
pub async fn ext_publish_check(app: tauri::AppHandle, path: String) -> Result<Value, String> {
    send(app, path, true).await
}

#[tauri::command]
pub fn ext_author_key_backup(app: tauri::AppHandle, dest: String) -> Result<(), String> {
    let (_, source) = author_key(&app)?;
    std::fs::copy(source, dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn ext_author_key_restore(app: tauri::AppHandle, src: String) -> Result<(), String> {
    restore_author_key_file(Path::new(&src), &author_key_path(&app)?)
}

#[tauri::command]
pub async fn ext_publish(app: tauri::AppHandle, path: String) -> Result<Value, String> {
    send(app, path, false).await
}

#[tauri::command]
pub async fn ext_my_listings(app: tauri::AppHandle) -> Result<Value, String> {
    let token = ext::account_token(&app)
        .map_err(|_| "Sign in to MXB App to publish extensions.".to_string())?;
    let base = ext::catalog_base();
    let response = ext::catalog_http()
        .get(format!("{base}/v1/me/extensions"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| ext::CATALOG_DOWN.to_string())?;
    let mut value = ext::catalog_json(response).await?;
    ext::absolutize(&mut value, &base);
    Ok(value)
}

#[tauri::command]
pub async fn ext_request_verified(app: tauri::AppHandle, id: String) -> Result<Value, String> {
    ext::catalog_id(&id)?;
    let token = ext::account_token(&app)
        .map_err(|_| "Sign in to MXB App to publish extensions.".to_string())?;
    let response = ext::catalog_http()
        .post(format!("{}/v1/extensions/{id}/verify", ext::catalog_base()))
        .bearer_auth(token)
        .json(&json!({}))
        .send()
        .await
        .map_err(|_| ext::CATALOG_DOWN.to_string())?;
    ext::catalog_json(response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn test_key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    fn temp_dir(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mxb-extpublish-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn test_pem() -> String {
        let mut der = vec![
            0x30, 0x2e, 0x02, 0x01, 0, 0x30, 5, 6, 3, 0x2b, 0x65, 0x70, 4, 0x22, 4, 0x20,
        ];
        der.extend(test_key().to_bytes());
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            B64.encode(der)
        )
    }

    #[test]
    fn author_key_pem_round_trips() {
        let key = test_key();
        let pem = test_pem();
        assert_eq!(
            extmcp::signing_key_from_pem(&pem).unwrap().to_bytes(),
            key.to_bytes()
        );
    }

    #[test]
    fn restore_rejects_bad_pem_without_replacing_the_key() {
        let root = temp_dir("restore-bad");
        let src = root.join("bad.pem");
        let dest = root.join("author-key.pem");
        std::fs::write(&src, b"not a private key").unwrap();
        std::fs::write(&dest, test_pem()).unwrap();
        let before = std::fs::read(&dest).unwrap();
        let error = restore_author_key_file(&src, &dest).unwrap_err();
        assert_eq!(error, "That file is not a valid author key.");
        assert_eq!(std::fs::read(&dest).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restore_round_trips_a_good_pem() {
        let root = temp_dir("restore-good");
        let src = root.join("backup.pem");
        let dest = root.join("author-key.pem");
        let pem = test_pem();
        std::fs::write(&src, &pem).unwrap();
        std::fs::write(&dest, b"old key").unwrap();
        restore_author_key_file(&src, &dest).unwrap();
        let restored = std::fs::read_to_string(&dest).unwrap();
        assert_eq!(
            extmcp::signing_key_from_pem(&restored).unwrap().to_bytes(),
            test_key().to_bytes()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signing_an_unsigned_package_is_verified_as_signed() {
        let key = test_key();
        let mut files = BTreeMap::new();
        files.insert(
            "extension.json".into(),
            br#"{"manifestVersion":1,"id":"tester.sample","name":"Sample","version":"1.0.0","author":"Tester","permissions":[]}"#.to_vec(),
        );
        files.insert("view/index.html".into(), b"<p>sample</p>".to_vec());
        let bytes = extmcp::pack_files(files, Some(&key)).unwrap();
        let package = ext::open_package(ext::read_zip(&bytes).unwrap(), "1.0.0").unwrap();
        assert_eq!(package.trust, ext::Trust::Signed);
        assert_eq!(
            package.key.unwrap(),
            ext::key_fingerprint(&key.verifying_key().to_bytes())
        );
    }

    #[test]
    fn folder_packing_refuses_an_executable() {
        let root = temp_dir("bad-file-type");
        std::fs::write(root.join("bad.exe"), b"MZ").unwrap();
        let error = check_folder_types(&root).unwrap_err();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(error.contains("file type"), "{error}");
    }
}
