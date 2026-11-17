//! Jádro music-boxu: databáze, přehrávač, zdroje hudby a HTTP/WebSocket API.
//! Používá ho Tauri aplikace (displej) i samostatný server bez displeje (`musicbox-headless`).

pub mod api;
pub mod auth;
pub mod db;
pub mod error;
pub mod i18n;
pub mod models;
pub mod player;
pub mod sources;
pub mod state;
pub mod themes;
pub mod util;

use anyhow::{Context, Result};
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

pub use state::{AppState, ServerEvent};

#[derive(Clone, Debug)]
pub struct CoreConfig {
    /// Databáze, cache stažené hudby, yt-dlp.
    pub data_dir: PathBuf,
    /// Port HTTP serveru (web + API).
    pub port: u16,
    /// Sestavený web (SvelteKit build). None = API bez webu.
    pub web_dir: Option<PathBuf>,
    /// Hlavní složka s hudbou – soubory v ní jsou singly, podsložky alba.
    pub music_dir: PathBuf,
}

pub const DEFAULT_PORT: u16 = 8787;

impl CoreConfig {
    /// Konfigurace z proměnných prostředí `MUSICBOX_DATA`, `MUSICBOX_PORT`, `MUSICBOX_WEB`, `MUSICBOX_MUSIC`.
    pub fn from_env() -> CoreConfig {
        let data_dir = std::env::var_os("MUSICBOX_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("music-box"));
        let port = std::env::var("MUSICBOX_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
        let web_dir = std::env::var_os("MUSICBOX_WEB").map(PathBuf::from).or_else(find_web_dir);
        let music_dir = std::env::var_os("MUSICBOX_MUSIC").map(PathBuf::from).unwrap_or_else(default_music_dir);
        CoreConfig { data_dir, port, web_dir, music_dir }
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }
}

/// Složka `music` u aplikace:
/// - vývoj (`target/debug`, `target/release`) → `music/` v kořeni repozitáře (cargo clean ji nesmaže),
/// - AppImage → vedle souboru `.AppImage`,
/// - jinak vedle spustitelného souboru.
/// Když tam nejde zapisovat (např. Program Files, /usr/bin), použije se Hudba/music-box uživatele.
fn default_music_dir() -> PathBuf {
    let app_dir = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    if let Some(dir) = app_dir {
        let in_target = dir.parent().and_then(|p| p.file_name()).map(|n| n == "target").unwrap_or(false);
        let base = if in_target { dir.parent().and_then(|t| t.parent()).map(|r| r.to_path_buf()) } else { Some(dir) };
        if let Some(base) = base {
            let music = base.join("music");
            if is_writable_dir(&music) {
                return music;
            }
        }
    }
    dirs::audio_dir()
        .map(|d| d.join("music-box"))
        .or_else(|| dirs::data_dir().map(|d| d.join("music-box").join("music")))
        .unwrap_or_else(|| PathBuf::from("music"))
}

/// Vytvoří složku (pokud chybí) a ověří, že do ní jde zapisovat.
fn is_writable_dir(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".musicbox-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Hledá sestavený web vedle programu nebo ve vývojové struktuře repozitáře.
fn find_web_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let mut candidates = vec![];
    if let Some(d) = &exe_dir {
        // Vývoj: aktuální build v repozitáři má přednost před kopií, kterou si Tauri dává do target/.
        candidates.push(d.join("../../web/build")); // target/debug → repo
        candidates.push(d.join("../../../web/build"));
        candidates.push(d.join("web"));
    }
    candidates.push(PathBuf::from("web/build"));
    candidates.push(PathBuf::from("../web/build"));
    candidates.push(PathBuf::from("../../web/build"));
    candidates.into_iter().find(|p| spa_entry(p).is_some()).and_then(|p| p.canonicalize().ok())
}

/// Vstupní stránka SPA (SvelteKit adapter-static s fallbackem `200.html`).
fn spa_entry(dir: &std::path::Path) -> Option<PathBuf> {
    ["200.html", "index.html"].iter().map(|f| dir.join(f)).find(|p| p.is_file())
}

/// Spustí jádro: databázi, přehrávač a HTTP server. Vrací sdílený stav.
pub async fn start(config: CoreConfig) -> Result<Arc<AppState>> {
    std::fs::create_dir_all(&config.data_dir).with_context(|| format!("nelze vytvořit {}", config.data_dir.display()))?;
    std::fs::create_dir_all(config.cache_dir())?;
    if let Err(e) = std::fs::create_dir_all(&config.music_dir) {
        tracing::warn!("složku s hudbou {} nelze vytvořit: {e}", config.music_dir.display());
    }
    let db = db::Db::open(&config.data_dir.join("music-box.db"))?;
    let (player, player_rx) = player::PlayerHandle::new();
    let state = Arc::new(AppState::new(config.clone(), db, player)?);

    let api = api::router().layer(axum::middleware::from_fn(i18n::translate_errors));
    let mut app = Router::new().nest("/api", api);
    if let Some((web, entry)) = config.web_dir.as_ref().and_then(|w| spa_entry(w).map(|e| (w, e))) {
        tracing::info!("web: {}", web.display());
        app = app.fallback_service(ServeDir::new(web).fallback(ServeFile::new(entry)));
    } else {
        tracing::warn!("sestavený web nenalezen – běží jen API (nastav MUSICBOX_WEB nebo spusť `npm run build` ve web/)");
    }
    let app = app
        // Displej (Tauri) běží na jiném originu a čte stav bez cookies.
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("port {} je obsazený", config.port))?;
    tracing::info!("music-box běží na http://{addr} – web pro lidi: {}", state.join_url());
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await {
            tracing::error!("HTTP server skončil: {e}");
        }
    });

    player::spawn(state.clone(), player_rx);

    // Na pozadí: připravit yt-dlp (případně stáhnout) a načíst lokální knihovnu.
    let st = state.clone();
    tokio::spawn(async move {
        match st.ytdlp.ensure().await {
            Ok(_) => {
                if let Ok(v) = st.ytdlp.version().await {
                    tracing::info!("yt-dlp {v}");
                }
            }
            Err(e) => tracing::warn!("yt-dlp není k dispozici: {e:#}"),
        }
        match st.rescan_library().await {
            Ok(n) => tracing::info!("hudba v {}: {n} skladeb", st.config.music_dir.display()),
            Err(e) => tracing::warn!("skenování knihovny: {e:#}"),
        }
        st.player.send(player::PlayerCmd::Wake);
    });

    Ok(state)
}

pub fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,musicbox_core=debug,symphonia=error,symphonia_bundle_mp3=error".into()),
        )
        .try_init();
}
