use std::collections::HashSet;
use tauri::Manager;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AppConfig {
    pub app_url: String,
    pub device_id: Option<String>,
    pub run_root: Option<String>,
    pub enabled: bool,
    pub uploaded_hashes: HashSet<String>,
}

pub fn load_config(app: &tauri::AppHandle) -> anyhow::Result<AppConfig> {
    let config_dir = app.path().app_config_dir()?;
    let config_path = config_dir.join("config.json");

    if !config_path.exists() {
        return Ok(AppConfig::default());
    }

    let contents = std::fs::read_to_string(&config_path)?;
    let config: AppConfig = serde_json::from_str(&contents)?;
    Ok(config)
}

pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> anyhow::Result<()> {
    let config_dir = app.path().app_config_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.json");
    let contents = serde_json::to_string_pretty(config)?;
    std::fs::write(&config_path, contents)?;
    Ok(())
}
