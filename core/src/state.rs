use crate::db::Db;
use crate::models::*;
use crate::player::PlayerHandle;
use crate::sources::{local, ytdlp::YtDlp, ytmusic::YtMusic};
use crate::util::{now, now_ms};
use crate::CoreConfig;
use anyhow::Result;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// Událost pro připojené klienty (WebSocket) a pro hostitelskou aplikaci (Tauri).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Změnil se stav přehrávání nebo fronta.
    State,
    /// Změnilo se nastavení.
    Settings,
    /// Změnil se seznam uživatelů (pro adminy).
    Users,
    Toast { level: String, message: String },
}

pub struct AppState {
    pub config: CoreConfig,
    pub db: Db,
    settings: RwLock<Settings>,
    pub events: broadcast::Sender<ServerEvent>,
    pub player: PlayerHandle,
    pub ytdlp: YtDlp,
    pub ytm: YtMusic,
    pub http: reqwest::Client,
    pub themes: crate::themes::Themes,
    /// Co hraje rádio (None = rádio nehraje).
    pub radio: RwLock<Option<RadioNow>>,
}

impl AppState {
    pub fn new(config: CoreConfig, db: Db, player: PlayerHandle) -> Result<AppState> {
        let settings = db.load_settings()?;
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?;
        let (events, _) = broadcast::channel(256);
        Ok(AppState {
            ytdlp: YtDlp::new(&config.data_dir, http.clone()),
            ytm: YtMusic::new(http.clone()),
            themes: crate::themes::Themes::new(&config.data_dir),
            radio: RwLock::new(None),
            http,
            config,
            db,
            settings: RwLock::new(settings),
            events,
            player,
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_settings(&self, s: Settings) -> Result<()> {
        self.db.save_settings(&s)?;
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = s;
        let _ = self.events.send(ServerEvent::Settings);
        self.notify();
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    pub fn notify(&self) {
        let _ = self.events.send(ServerEvent::State);
    }

    pub fn toast(&self, level: &str, message: impl Into<String>) {
        let _ = self.events.send(ServerEvent::Toast { level: level.into(), message: message.into() });
    }

    /// Adresa webu pro QR kód.
    pub fn join_url(&self) -> String {
        let s = self.settings();
        if !s.public_url.trim().is_empty() {
            return s.public_url.trim().to_string();
        }
        let ip = local_ip_address::local_ip().map(|ip| ip.to_string()).unwrap_or_else(|_| "localhost".into());
        format!("http://{ip}:{}/", self.config.port)
    }

    /// URL CSS aktivního motivu (s verzí kvůli cache).
    pub fn theme_css_url(&self) -> String {
        let s = self.settings();
        let id = if self.themes.get(&s.theme).is_some() { s.theme.clone() } else { crate::themes::DEFAULT_THEME.to_string() };
        let fx = self.themes.enabled_effects(&id, &s.display.effects).join(".");
        format!("/api/theme.css?id={id}&v={}&fx={fx}", self.themes.version(&id))
    }

    pub fn radio_now(&self) -> Option<RadioNow> {
        self.radio.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_radio(&self, f: impl FnOnce(&mut Option<RadioNow>)) {
        f(&mut self.radio.write().unwrap_or_else(|e| e.into_inner()));
    }

    /// Text v jazyce boxu (pro hlášky, které server posílá všem).
    pub fn tr<'a>(&self, cs: &'a str, en: &'a str) -> &'a str {
        if self.settings().language == "en" { en } else { cs }
    }

    pub fn snapshot(&self) -> Result<StateSnapshot> {
        let st = self.player.status();
        let now_playing = match st.current {
            Some(id) => self.db.entry(id)?.map(|entry| NowPlaying {
                entry,
                position_ms: self.player.pos_ms.load(Ordering::Relaxed),
                paused: st.paused,
                loading: st.loading,
            }),
            None => None,
        };
        let s = self.settings();
        Ok(StateSnapshot {
            now_playing,
            queue: self.db.queued(s.fair_queue)?,
            history: self.db.history(8)?,
            volume: s.volume,
            join_url: self.join_url(),
            box_name: s.box_name.clone(),
            theme_css: self.theme_css_url(),
            effects: self.themes.enabled_effects(&s.theme, &s.display.effects),
            fair_queue: s.fair_queue,
            volume_zone: s.active_zone().map(|z| (z.name.clone(), z.volume)),
            radio: self.radio_now(),
            language: s.language.clone(),
            display: s.display.clone(),
            server_time_ms: now_ms(),
        })
    }

    /// Přečte nastavené složky a aktualizuje lokální knihovnu. Vrací počet skladeb.
    pub async fn rescan_library(self: &Arc<Self>) -> Result<usize> {
        let mut roots = vec![self.config.music_dir.clone()];
        roots.extend(self.settings().local_folders.iter().map(std::path::PathBuf::from));
        let files = tokio::task::spawn_blocking(move || local::scan(&roots)).await?;
        let count = files.len();
        self.db.with(|c| {
            c.execute_batch("BEGIN")?;
            c.execute("UPDATE tracks SET available = 0 WHERE source = 'local'", [])?;
            {
                let mut up = c.prepare(
                    "INSERT INTO tracks (source, source_id, title, artist, album, duration_sec, file_path, available, created_at, collection, track_no) \
                     VALUES ('local', ?1, ?2, ?3, ?4, ?5, ?1, 1, ?6, ?7, ?8) \
                     ON CONFLICT(source, source_id) DO UPDATE SET \
                       title = CASE WHEN tracks.locked = 1 THEN tracks.title ELSE excluded.title END, \
                       artist = CASE WHEN tracks.locked = 1 THEN tracks.artist ELSE excluded.artist END, \
                       album = CASE WHEN tracks.locked = 1 THEN tracks.album ELSE excluded.album END, \
                       duration_sec = excluded.duration_sec, available = 1, \
                       collection = excluded.collection, track_no = excluded.track_no",
                )?;
                let mut art = c.prepare(
                    "UPDATE tracks SET artwork_url = CASE WHEN ?2 THEN '/api/tracks/' || id || '/artwork' ELSE NULL END \
                     WHERE source = 'local' AND source_id = ?1 AND locked = 0",
                )?;
                for f in &files {
                    let path = f.path.to_string_lossy();
                    up.execute(rusqlite::params![path, f.title, f.artist, f.album, f.duration_sec, now(), f.collection, f.track_no])?;
                    art.execute(rusqlite::params![path, f.has_picture])?;
                }
            }
            c.execute_batch("COMMIT")
        })?;
        tracing::info!("lokální knihovna: {count} skladeb");
        Ok(count)
    }
}
