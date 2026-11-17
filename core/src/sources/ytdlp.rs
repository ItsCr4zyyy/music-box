//! Obal kolem yt-dlp (samostatný program). Pokud není v systému, stáhne se do datové složky
//! a dá se aktualizovat bez nové verze aplikace – stejně jako ve Vibe.

use crate::models::RemoteTrack;
use crate::sources::clean_video_title;
use crate::util::{command, find_in_path, jnum, jp, jstr};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};

pub struct YtDlp {
    bin_dir: PathBuf,
    bin: RwLock<Option<PathBuf>>,
    /// Program je naše stažená kopie (můžeme ho aktualizovat).
    managed: RwLock<bool>,
    ffmpeg: Option<PathBuf>,
    http: reqwest::Client,
    install_lock: Mutex<()>,
    download_locks: std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", _) => Some("yt-dlp.exe"),
        ("linux", "x86_64") => Some("yt-dlp_linux"),
        ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
        ("linux", "arm") => Some("yt-dlp_linux_armv7l"),
        ("macos", _) => Some("yt-dlp_macos"),
        _ => None,
    }
}

impl YtDlp {
    pub fn new(data_dir: &Path, http: reqwest::Client) -> YtDlp {
        let ffmpeg = std::env::var_os("MUSICBOX_FFMPEG").map(PathBuf::from).or_else(|| find_in_path("ffmpeg"));
        YtDlp {
            bin_dir: data_dir.join("bin"),
            bin: RwLock::new(None),
            managed: RwLock::new(false),
            ffmpeg,
            http,
            install_lock: Mutex::new(()),
            download_locks: Default::default(),
        }
    }

    pub fn has_ffmpeg(&self) -> bool {
        self.ffmpeg.is_some()
    }

    fn managed_path(&self) -> PathBuf {
        self.bin_dir.join(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" })
    }

    /// Najde nebo stáhne yt-dlp.
    pub async fn ensure(&self) -> Result<PathBuf> {
        if let Some(b) = self.bin.read().await.clone() {
            return Ok(b);
        }
        let _guard = self.install_lock.lock().await;
        if let Some(b) = self.bin.read().await.clone() {
            return Ok(b);
        }
        let (path, managed) = if let Some(p) = std::env::var_os("MUSICBOX_YTDLP").map(PathBuf::from) {
            (p, false)
        } else if self.managed_path().is_file() {
            (self.managed_path(), true)
        } else if let Some(p) = find_in_path("yt-dlp") {
            (p, false)
        } else {
            self.download_binary().await?;
            (self.managed_path(), true)
        };
        tracing::info!("yt-dlp: {}", path.display());
        *self.bin.write().await = Some(path.clone());
        *self.managed.write().await = managed;
        Ok(path)
    }

    async fn download_binary(&self) -> Result<()> {
        let asset = asset_name().ok_or_else(|| anyhow!("yt-dlp pro tuto platformu není k dispozici – nainstaluj ho ručně"))?;
        let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset}");
        tracing::info!("stahuji yt-dlp z {url}");
        let bytes = self
            .http
            .get(&url)
            .timeout(Duration::from_secs(300))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        tokio::fs::create_dir_all(&self.bin_dir).await?;
        let tmp = self.managed_path().with_extension("download");
        tokio::fs::write(&tmp, &bytes).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).await?;
        }
        tokio::fs::rename(&tmp, self.managed_path()).await?;
        Ok(())
    }

    pub async fn version(&self) -> Result<String> {
        let bin = self.ensure().await?;
        let out = command(&bin).arg("--version").output().await?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Aktualizuje yt-dlp (jen naši staženou kopii). Vrací výstup programu.
    pub async fn update(&self) -> Result<String> {
        let bin = self.ensure().await?;
        if !*self.managed.read().await {
            bail!("yt-dlp je nainstalovaný v systému ({}) – aktualizuj ho správcem balíčků", bin.display());
        }
        let out = tokio::time::timeout(Duration::from_secs(180), command(&bin).arg("-U").output()).await??;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    async fn run_json(&self, args: &[&str], timeout: Duration) -> Result<Value> {
        let bin = self.ensure().await?;
        let mut cmd = command(&bin);
        cmd.args(["-J", "--no-warnings", "--force-ipv4"]).args(args);
        let out = tokio::time::timeout(timeout, cmd.output()).await.context("yt-dlp nestihl odpovědět")??;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let line = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("neznámá chyba");
            bail!("yt-dlp: {line}");
        }
        serde_json::from_slice(&out.stdout).context("neplatná odpověď yt-dlp")
    }

    /// Záložní hledání přes yt-dlp (když YouTube Music API selže).
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<RemoteTrack>> {
        let q = format!("ytsearch{limit}:{query}");
        let v = self.run_json(&["--flat-playlist", &q], Duration::from_secs(40)).await?;
        Ok(entries(&v))
    }

    /// Metadata jednoho videa.
    pub async fn info(&self, video_id: &str) -> Result<RemoteTrack> {
        let url = watch_url(video_id);
        let v = self.run_json(&["--no-playlist", &url], Duration::from_secs(40)).await?;
        entry_to_track(&v).ok_or_else(|| anyhow!("video se nepodařilo načíst"))
    }

    /// Skladby playlistu (nebo jedno video).
    pub async fn playlist(&self, url: &str) -> Result<Vec<RemoteTrack>> {
        let url = url.replace("music.youtube.com/playlist", "www.youtube.com/playlist");
        let v = self.run_json(&["--flat-playlist", &url], Duration::from_secs(90)).await?;
        if v.get("entries").is_some() {
            Ok(entries(&v))
        } else {
            Ok(entry_to_track(&v).into_iter().collect())
        }
    }

    /// Stáhne audio do `dir` jako `yt_<id>.<ext>`. Souběžná volání pro stejné ID se nepřekrývají.
    pub async fn download(&self, video_id: &str, dir: &Path) -> Result<PathBuf> {
        let lock = {
            let mut map = self.download_locks.lock().unwrap_or_else(|e| e.into_inner());
            map.entry(video_id.to_string()).or_default().clone()
        };
        let _guard = lock.lock().await;
        let base = format!("yt_{video_id}");
        if let Some(existing) = find_downloaded(dir, &base) {
            return Ok(existing);
        }
        let bin = self.ensure().await?;
        tokio::fs::create_dir_all(dir).await?;
        let template = dir.join(format!("{base}.%(ext)s"));
        let mut cmd = command(&bin);
        // Dekodér (symphonia) neumí Opus → bereme AAC v M4A, případně převedeme přes ffmpeg.
        cmd.args([
            "-f",
            "bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]/bestaudio",
            "--no-playlist",
            "--no-mtime",
            "--no-warnings",
            "--force-ipv4",
            "-o",
        ])
        .arg(&template);
        if let Some(ff) = &self.ffmpeg {
            cmd.args(["-x", "--audio-format", "m4a", "--ffmpeg-location"]).arg(ff);
        }
        cmd.arg(watch_url(video_id));
        let out = tokio::time::timeout(Duration::from_secs(600), cmd.output()).await.context("stahování trvá moc dlouho")??;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let line = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("neznámá chyba");
            bail!("stažení selhalo: {line}");
        }
        find_downloaded(dir, &base).ok_or_else(|| anyhow!("stažený soubor nebyl nalezen"))
    }
}

pub fn find_downloaded(dir: &Path, base: &str) -> Option<PathBuf> {
    let prefix = format!("{base}.");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(&prefix)
                && ![".part", ".ytdl", ".temp", ".webm", ".download"].iter().any(|s| name.ends_with(s))
                // dočasné formáty yt-dlp (např. yt_x.f140.m4a)
                && !name.split('.').any(|p| p.len() > 1 && p.starts_with('f') && p[1..].chars().all(|c| c.is_ascii_digit()))
        })
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}

pub fn watch_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

fn entries(v: &Value) -> Vec<RemoteTrack> {
    v.get("entries").and_then(|e| e.as_array()).map(|a| a.iter().filter_map(entry_to_track).collect()).unwrap_or_default()
}

fn entry_to_track(e: &Value) -> Option<RemoteTrack> {
    let id = jstr(e, &["id"])?;
    if id.len() != 11 {
        return None; // kanály / playlisty ve výsledcích
    }
    let raw_title = jstr(e, &["title"])?;
    if raw_title == "[Deleted video]" || raw_title == "[Private video]" {
        return None;
    }
    let explicit_artist = jp(e, &["artists"])
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
        .filter(|s| !s.is_empty())
        .or_else(|| jstr(e, &["artist"]));
    let channel = jstr(e, &["channel"]).or_else(|| jstr(e, &["uploader"]));
    let (artist, title) = match explicit_artist {
        Some(a) => (a, raw_title),
        None => clean_video_title(&raw_title, channel.as_deref()),
    };
    Some(RemoteTrack {
        video_id: id.clone(),
        title,
        artist,
        album: jstr(e, &["album"]),
        duration_sec: jnum(e, &["duration"]).unwrap_or(0.0) as u32,
        artwork_url: Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")),
    })
}
