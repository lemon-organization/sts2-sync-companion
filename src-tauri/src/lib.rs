mod commands;
mod config;
mod sync;

use commands::*;
use sync::UploadResult;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_keyring::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Register deep-link handler
            let app_handle = app.handle().clone();
            app.listen("deep-link://new-url", move |event| {
                if let Ok(urls) = serde_json::from_str::<Vec<String>>(event.payload()) {
                    for url in urls {
                        handle_deep_link(app_handle.clone(), url);
                    }
                }
            });

            // System tray
            setup_tray(app)?;

            // Background sync timer: poll every 5 minutes when enabled
            let timer_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5 * 60)).await;

                    let config = match config::load_config(&timer_app) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Timer: failed to load config: {e}");
                            continue;
                        }
                    };

                    if !config.enabled {
                        continue;
                    }

                    match sync_now(timer_app.clone()).await {
                        Ok(result) => {
                            if result.imported > 0 || result.errors > 0 {
                                eprintln!(
                                    "Timer sync: imported={} duplicates={} errors={}",
                                    result.imported, result.duplicates, result.errors
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Timer sync error: {e}");
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            get_token,
            pair_device,
            unpair,
            sync_now,
            set_enabled,
            set_autostart,
            open_run_folder,
            pick_run_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn handle_deep_link(app: tauri::AppHandle, url: String) {
    // Parse sts2sync://pair?code=XXXX&url=https://...
    if let Some(query) = url.strip_prefix("sts2sync://pair?") {
        let params: std::collections::HashMap<String, String> = query
            .split('&')
            .filter_map(|kv| {
                let mut parts = kv.splitn(2, '=');
                let k = parts.next()?.to_string();
                let v = urlencoding::decode(parts.next()?).ok()?.into_owned();
                Some((k, v))
            })
            .collect();

        if let (Some(code), Some(app_url)) = (params.get("code"), params.get("url")) {
            let code = code.clone();
            let app_url = app_url.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pair_device(app.clone(), code, app_url).await {
                    eprintln!("Deep-link pair failed: {e}");
                }
            });
        }
    }
}

fn setup_tray(app: &mut tauri::App) -> anyhow::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
