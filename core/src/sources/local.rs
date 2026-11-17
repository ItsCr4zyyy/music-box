//! Lokální knihovna. Soubory přímo ve složce = singly, každá podsložka = album
//! (např. `Hudba/Orlík - Demise/01 ….mp3`). Obal: vložený v souboru, nebo cover.jpg / folder.jpg.

use lofty::prelude::*;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Formáty, které umí dekodér přehrát (Opus ne).
pub const AUDIO_EXTS: &[&str] = &["mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "wav", "aif", "aiff", "caf"];
const COVER_NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];

#[derive(Debug, Clone)]
pub struct LocalFile {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_sec: u32,
    pub track_no: Option<u32>,
    /// Relativní cesta podsložky (album); None = single v hlavní složce.
    pub collection: Option<String>,
    pub has_picture: bool,
}

pub fn is_audio(path: &Path) -> bool {
    has_ext(path, AUDIO_EXTS)
}

fn has_ext(path: &Path, exts: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| exts.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Projde složky rekurzivně a přečte tagy (blokující – volat ve spawn_blocking).
pub fn scan(roots: &[PathBuf]) -> Vec<LocalFile> {
    let mut out = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !entry.file_type().is_file() || !is_audio(path) {
                continue;
            }
            let mut f = read_file(path);
            let rel_parent = path.parent().and_then(|p| p.strip_prefix(root).ok()).filter(|p| !p.as_os_str().is_empty());
            if let Some(rel) = rel_parent {
                let key = rel.to_string_lossy().replace('\\', "/");
                let folder = rel.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                // Album podle složky; tag má přednost, jen když chybí.
                if f.album.is_none() {
                    f.album = Some(folder);
                }
                f.collection = Some(key);
            }
            if !f.has_picture {
                f.has_picture = cover_file(path).is_some();
            }
            out.push(f);
        }
    }
    out
}

pub fn read_file(path: &Path) -> LocalFile {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Neznámá skladba").to_string();
    let mut f = LocalFile {
        path: path.to_path_buf(),
        title: stem.clone(),
        artist: String::new(),
        album: None,
        duration_sec: 0,
        track_no: None,
        collection: None,
        has_picture: false,
    };
    if let Ok(tagged) = lofty::read_from_path(path) {
        f.duration_sec = tagged.properties().duration().as_secs() as u32;
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(t) = tag.title() {
                f.title = t.to_string();
            }
            if let Some(a) = tag.artist() {
                f.artist = a.to_string();
            }
            f.album = tag.album().map(|a| a.to_string()).filter(|a| !a.trim().is_empty());
            f.track_no = tag.track();
            f.has_picture = !tag.pictures().is_empty();
        }
    }
    if f.title == stem {
        // „01 - Interpret - Název.mp3“ / „01. Název.mp3“ bez tagů
        let mut rest = stem.as_str();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && digits.len() <= 3 {
            let after = rest[digits.len()..].trim_start_matches([' ', '.', '-', '_']);
            if !after.is_empty() {
                f.track_no = f.track_no.or(digits.parse().ok());
                rest = after;
            }
        }
        f.title = rest.trim().to_string();
    }
    if f.artist.is_empty() {
        if let Some((a, t)) = f.title.clone().split_once(" - ") {
            f.artist = a.trim().to_string();
            f.title = t.trim().to_string();
        } else {
            f.artist = "Neznámý interpret".into();
        }
    }
    f
}

/// Obrázek obalu ve složce skladby (cover.jpg, folder.png…, jinak první obrázek ve složce).
fn cover_file(track: &Path) -> Option<PathBuf> {
    let dir = track.parent()?;
    let images: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && has_ext(p, IMAGE_EXTS))
        .collect();
    images
        .iter()
        .find(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            COVER_NAMES.contains(&stem.as_str())
        })
        .or_else(|| images.first())
        .cloned()
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

/// Obal skladby: (mime, data).
pub fn picture(path: &Path) -> Option<(String, Vec<u8>)> {
    if let Ok(tagged) = lofty::read_from_path(path) {
        if let Some(pic) = tagged.primary_tag().or_else(|| tagged.first_tag()).and_then(|t| t.pictures().first().cloned()) {
            let mime = pic.mime_type().map(|m| m.as_str().to_string()).unwrap_or_else(|| "image/jpeg".into());
            return Some((mime, pic.data().to_vec()));
        }
    }
    let cover = cover_file(path)?;
    Some((mime_for(&cover).to_string(), std::fs::read(&cover).ok()?))
}
