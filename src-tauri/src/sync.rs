use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

#[derive(Debug, serde::Serialize)]
pub struct UploadResult {
    pub imported: u32,
    pub duplicates: u32,
    pub errors: u32,
    pub files: Vec<RunFileStatus>,
}

#[derive(Debug, serde::Serialize)]
pub struct RunFileStatus {
    pub path: String,
    pub status: String, // "uploaded", "duplicate", "error"
    pub error: Option<String>,
}

/// Returns the well-known `.run` file directories for each OS.
pub fn default_run_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // %APPDATA%\SlayTheSpire2\steam\history
        if let Some(appdata) = dirs::data_dir() {
            dirs.push(appdata.join("SlayTheSpire2").join("steam").join("history"));
        }
        // %LOCALAPPDATA%\SlayTheSpire2\steam\history
        if let Some(local) = dirs::data_local_dir() {
            dirs.push(local.join("SlayTheSpire2").join("steam").join("history"));
        }
        // %USERPROFILE%\AppData\LocalLow\Mega Crit\SlayTheSpire2\steam\history
        if let Some(home) = dirs::home_dir() {
            dirs.push(
                home.join("AppData")
                    .join("LocalLow")
                    .join("Mega Crit")
                    .join("SlayTheSpire2")
                    .join("steam")
                    .join("history"),
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        // $HOME/Library/Application Support/SlayTheSpire2/steam/history
        if let Some(home) = dirs::home_dir() {
            dirs.push(
                home.join("Library")
                    .join("Application Support")
                    .join("SlayTheSpire2")
                    .join("steam")
                    .join("history"),
            );
        }
    }

    #[cfg(target_os = "linux")]
    {
        // $HOME/.local/share/SlayTheSpire2/steam/history
        if let Some(home) = dirs::home_dir() {
            dirs.push(
                home.join(".local")
                    .join("share")
                    .join("SlayTheSpire2")
                    .join("steam")
                    .join("history"),
            );
        }
    }

    dirs
}

/// Walks each directory and collects all files with a `.run` extension (case-insensitive).
/// Returns a sorted Vec of paths.
pub fn find_run_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext.to_string_lossy().to_lowercase() == "run" {
                            files.push(path);
                        }
                    }
                }
            }
        }
    }

    files.sort();
    files
}

/// Returns the SHA256 hash of a file's contents as a lowercase hex string.
pub fn hash_file(path: &Path) -> anyhow::Result<String> {
    let contents = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();
    Ok(hex::encode(result))
}

/// Uploads a batch of run files to the dashboard API.
/// Batches up to 50 runs per request.
pub async fn upload_batch(
    app_url: &str,
    token: &str,
    files: &[PathBuf],
) -> anyhow::Result<UploadResult> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/sync/runs/import", app_url.trim_end_matches('/'));

    let mut total_imported = 0u32;
    let mut total_duplicates = 0u32;
    let mut total_errors = 0u32;
    let mut file_statuses: Vec<RunFileStatus> = Vec::new();

    // Parse all files first, collecting (path, json_value) or error
    let mut parsed: Vec<(PathBuf, Result<serde_json::Value, String>)> = Vec::new();
    for path in files {
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string()));
        parsed.push((path.clone(), result));
    }

    // Collect read/parse errors immediately
    let mut good: Vec<(PathBuf, serde_json::Value)> = Vec::new();
    for (path, result) in parsed {
        match result {
            Ok(val) => good.push((path, val)),
            Err(e) => {
                total_errors += 1;
                file_statuses.push(RunFileStatus {
                    path: path.to_string_lossy().into_owned(),
                    status: "error".to_string(),
                    error: Some(format!("Read/parse error: {e}")),
                });
            }
        }
    }

    // Upload in batches of 50
    for chunk in good.chunks(50) {
        let runs: Vec<&serde_json::Value> = chunk.iter().map(|(_, v)| v).collect();
        let body = serde_json::json!({ "runs": runs });

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        match response {
            Err(e) => {
                // Network error — mark all files in this chunk as errors
                for (path, _) in chunk {
                    total_errors += 1;
                    file_statuses.push(RunFileStatus {
                        path: path.to_string_lossy().into_owned(),
                        status: "error".to_string(),
                        error: Some(format!("Network error: {e}")),
                    });
                }
            }
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<serde_json::Value>().await {
                    Err(e) => {
                        for (path, _) in chunk {
                            total_errors += 1;
                            file_statuses.push(RunFileStatus {
                                path: path.to_string_lossy().into_owned(),
                                status: "error".to_string(),
                                error: Some(format!("Response parse error (HTTP {}): {e}", status)),
                            });
                        }
                    }
                    Ok(json) => {
                        let imported = json["imported"].as_u64().unwrap_or(0) as u32;
                        let duplicates = json["duplicates"].as_u64().unwrap_or(0) as u32;
                        let errors = json["errors"].as_u64().unwrap_or(0) as u32;

                        total_imported += imported;
                        total_duplicates += duplicates;
                        total_errors += errors;

                        // Attribute statuses to individual files in the chunk
                        // The API doesn't return per-file results, so we approximate:
                        // first `imported` files = "uploaded", next `duplicates` = "duplicate", rest = "error"
                        let mut remaining_imported = imported as usize;
                        let mut remaining_dupes = duplicates as usize;

                        for (path, _) in chunk {
                            let file_status = if remaining_imported > 0 {
                                remaining_imported -= 1;
                                RunFileStatus {
                                    path: path.to_string_lossy().into_owned(),
                                    status: "uploaded".to_string(),
                                    error: None,
                                }
                            } else if remaining_dupes > 0 {
                                remaining_dupes -= 1;
                                RunFileStatus {
                                    path: path.to_string_lossy().into_owned(),
                                    status: "duplicate".to_string(),
                                    error: None,
                                }
                            } else {
                                RunFileStatus {
                                    path: path.to_string_lossy().into_owned(),
                                    status: "error".to_string(),
                                    error: Some("Server reported error for this run".to_string()),
                                }
                            };
                            file_statuses.push(file_status);
                        }
                    }
                }
            }
        }
    }

    Ok(UploadResult {
        imported: total_imported,
        duplicates: total_duplicates,
        errors: total_errors,
        files: file_statuses,
    })
}
