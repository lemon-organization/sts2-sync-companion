use tauri::Manager;
use tauri_plugin_keyring::KeyringExt;

use crate::config::{load_config, save_config, AppConfig};
use crate::sync::{default_run_dirs, find_run_files, hash_file, upload_batch, UploadResult};

const KEYRING_SERVICE: &str = "dev.lemoncode.sts2sync";
const KEYRING_ACCOUNT: &str = "device_token";

// ---------------------------------------------------------------------------
// Config commands
// ---------------------------------------------------------------------------

/// Return the current config (bearer token is NOT included).
#[tauri::command]
pub async fn get_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    load_config(&app).map_err(|e| e.to_string())
}

/// Persist the config received from the frontend.
#[tauri::command]
pub async fn set_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    save_config(&app, &config).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Token / keychain commands
// ---------------------------------------------------------------------------

/// Return the device token from the OS keychain, or None if not set.
#[tauri::command]
pub async fn get_token(app: tauri::AppHandle) -> Result<Option<String>, String> {
    match app.keyring().get_password(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(token) => Ok(Some(token)),
        Err(e) => {
            // Treat "no entry" / "not found" as None rather than an error
            let msg = e.to_string().to_lowercase();
            if msg.contains("no entry") || msg.contains("not found") || msg.contains("noentry") {
                Ok(None)
            } else {
                Err(e.to_string())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pairing commands
// ---------------------------------------------------------------------------

/// Pair with the dashboard using a short-lived pairing code.
/// POSTs to `{app_url}/api/sync/pair`, stores the token in the OS keychain,
/// and saves `deviceId` + `app_url` to the persistent config.
#[tauri::command]
pub async fn pair_device(
    app: tauri::AppHandle,
    code: String,
    app_url: String,
) -> Result<(), String> {
    let hostname = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
        .to_string_lossy()
        .into_owned();

    let client = reqwest::Client::new();
    let url = format!("{}/api/sync/pair", app_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "code": code,
        "deviceName": hostname,
    });

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Pair failed (HTTP {}): {}", status, text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    let token = json["token"]
        .as_str()
        .ok_or_else(|| "Missing 'token' in pair response".to_string())?
        .to_string();

    let device_id = json["deviceId"]
        .as_str()
        .map(|s| s.to_string());

    // Store token in OS keychain
    app.keyring()
        .set_password(KEYRING_SERVICE, KEYRING_ACCOUNT, &token)
        .map_err(|e| format!("Keychain error: {e}"))?;

    // Persist device_id, app_url, and enable syncing immediately
    let mut config = load_config(&app).map_err(|e| e.to_string())?;
    config.device_id = device_id;
    config.app_url = app_url;
    config.enabled = true;
    save_config(&app, &config).map_err(|e| e.to_string())?;

    Ok(())
}

/// Unpair: delete the token from the OS keychain and clear `device_id` from config.
#[tauri::command]
pub async fn unpair(app: tauri::AppHandle) -> Result<(), String> {
    // Delete from keychain (ignore "no entry" errors — already unpaired)
    match app.keyring().delete_password(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(_) => {}
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if !msg.contains("no entry") && !msg.contains("not found") && !msg.contains("noentry") {
                return Err(format!("Keychain error: {e}"));
            }
        }
    }

    let mut config = load_config(&app).map_err(|e| e.to_string())?;
    config.device_id = None;
    save_config(&app, &config).map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Sync commands
// ---------------------------------------------------------------------------

/// Run a sync cycle: discover run files, skip already-uploaded hashes,
/// upload the rest, and persist the updated hash set.
#[tauri::command]
pub async fn sync_now(app: tauri::AppHandle) -> Result<UploadResult, String> {
    let config = load_config(&app).map_err(|e| e.to_string())?;

    let token = app
        .keyring()
        .get_password(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| format!("No device token ({}). Pair first.", e))?;

    if config.app_url.is_empty() {
        return Err("app_url not configured. Pair with the dashboard first.".to_string());
    }

    // Determine directories to scan
    let dirs: Vec<std::path::PathBuf> = if let Some(root) = &config.run_root {
        vec![std::path::PathBuf::from(root)]
    } else {
        default_run_dirs()
    };

    let all_files = find_run_files(&dirs);

    // Filter out already-uploaded files
    let mut new_files: Vec<std::path::PathBuf> = Vec::new();
    let mut new_hashes: Vec<(std::path::PathBuf, String)> = Vec::new();

    for path in &all_files {
        match hash_file(path) {
            Ok(hash) => {
                if !config.uploaded_hashes.contains(&hash) {
                    new_files.push(path.clone());
                    new_hashes.push((path.clone(), hash));
                }
            }
            Err(e) => {
                eprintln!("Failed to hash {:?}: {e}", path);
            }
        }
    }

    if new_files.is_empty() {
        return Ok(UploadResult {
            imported: 0,
            duplicates: 0,
            errors: 0,
            files: vec![],
        });
    }

    let result = upload_batch(&config.app_url, &token, &new_files)
        .await
        .map_err(|e| e.to_string())?;

    // Add hashes of successfully uploaded files to config
    let uploaded_paths: std::collections::HashSet<String> = result
        .files
        .iter()
        .filter(|f| f.status == "uploaded")
        .map(|f| f.path.clone())
        .collect();

    let mut updated_config = load_config(&app).map_err(|e| e.to_string())?;
    for (path, hash) in new_hashes {
        if uploaded_paths.contains(&path.to_string_lossy().as_ref()) {
            updated_config.uploaded_hashes.insert(hash);
        }
    }
    save_config(&app, &updated_config).map_err(|e| e.to_string())?;

    Ok(result)
}

/// Toggle the enabled flag.
#[tauri::command]
pub async fn set_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = load_config(&app).map_err(|e| e.to_string())?;
    config.enabled = enabled;
    save_config(&app, &config).map_err(|e| e.to_string())
}

/// Toggle OS autostart via tauri-plugin-autostart.
#[tauri::command]
pub async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())
    } else {
        autolaunch.disable().map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Folder commands
// ---------------------------------------------------------------------------

/// Open the run save folder (or the first default dir) in the OS file manager.
#[tauri::command]
pub async fn open_run_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;

    let config = load_config(&app).map_err(|e| e.to_string())?;

    let folder = if let Some(root) = &config.run_root {
        std::path::PathBuf::from(root)
    } else {
        default_run_dirs()
            .into_iter()
            .next()
            .ok_or_else(|| "No run directories found for this OS".to_string())?
    };

    let folder_str = folder.to_string_lossy().into_owned();

    #[cfg(target_os = "windows")]
    app.shell()
        .command("explorer")
        .args([&folder_str])
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    app.shell()
        .command("open")
        .args([&folder_str])
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    app.shell()
        .command("xdg-open")
        .args([&folder_str])
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Pick a custom run folder via a file dialog.
///
/// TODO: implement via tauri-plugin-dialog when it is wired up.
/// For MVP, returns None — the folder can be set via set_config directly.
#[tauri::command]
pub async fn pick_run_folder(_app: tauri::AppHandle) -> Result<Option<String>, String> {
    // TODO: use tauri_plugin_dialog::DialogExt to show a folder picker.
    // Requires adding tauri-plugin-dialog to Cargo.toml and capabilities.
    Ok(None)
}
