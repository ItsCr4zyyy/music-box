// V release buildu na Windows bez konzolového okna.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use musicbox_core::{AppState, CoreConfig, ServerEvent};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast::error::RecvError;

struct Backend {
    port: u16,
}

/// Adresa lokálního serveru pro displej (WebSocket + obrázky).
#[tauri::command]
fn backend_url(backend: tauri::State<'_, Backend>) -> String {
    format!("http://127.0.0.1:{}", backend.port)
}

/// F11 na displeji – dočasné přepnutí celé obrazovky.
#[tauri::command]
fn toggle_fullscreen(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let on = w.is_fullscreen().unwrap_or(false);
        let _ = w.set_fullscreen(!on);
    }
}

fn apply_fullscreen(app: &AppHandle, core: &AppState) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_fullscreen(core.settings().fullscreen);
    }
}

fn main() {
    musicbox_core::init_logging();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![backend_url, toggle_fullscreen])
        .setup(|app| {
            let mut config = CoreConfig::from_env();
            // V nainstalované aplikaci je web přibalený jako resource.
            if config.web_dir.is_none() {
                if let Ok(res) = app.path().resource_dir() {
                    let web = res.join("web");
                    if web.join("200.html").is_file() {
                        config.web_dir = Some(web);
                    }
                }
            }
            let port = config.port;
            let core: Arc<AppState> = tauri::async_runtime::block_on(musicbox_core::start(config))?;
            app.manage(Backend { port });

            let handle = app.handle().clone();
            apply_fullscreen(&handle, &core);

            // Admin změnil nastavení na webu → hned přepnout celou obrazovku.
            let mut rx = core.subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ServerEvent::Settings) | Err(RecvError::Lagged(_)) => apply_fullscreen(&handle, &core),
                        Ok(_) => {}
                        Err(RecvError::Closed) => break,
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("music-box se nepodařilo spustit");
}
