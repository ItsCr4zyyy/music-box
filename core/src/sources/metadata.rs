//! Identifikace lokálních skladeb a alb (podobně jako Jellyfin): MusicBrainz + Cover Art Archive a Deezer.
//! Obě služby jsou zdarma a bez API klíče.

use crate::util::{jnum, jp, jstr};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// MusicBrainz vyžaduje popisný User-Agent (bez osobních údajů).
const MB_UA: &str = concat!("music-box/", env!("CARGO_PKG_VERSION"), " (self-hosted jukebox)");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    /// "musicbrainz" | "deezer"
    pub source: String,
    pub id: String,
    pub title: String,
    pub artist: String,
    /// U skladby: album, ze kterého je.
    pub album: Option<String>,
    pub year: Option<String>,
    pub cover_url: Option<String>,
    pub track_count: Option<u32>,
}

/// Skladba alba z online zdroje (pořadí = pořadí na albu).
#[derive(Clone, Debug)]
pub struct AlbumTrack {
    pub title: String,
    pub artist: String,
}

async fn get_json(http: &reqwest::Client, url: &str, query: &[(&str, &str)]) -> Result<Value> {
    let resp = http
        .get(url)
        .query(query)
        .header("User-Agent", MB_UA)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("{}: HTTP {}", url.split('/').nth(2).unwrap_or(url), resp.status().as_u16());
    }
    Ok(resp.json().await?)
}

fn mb_artist(v: &Value) -> String {
    v.get("artist-credit")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .map(|c| format!("{}{}", jstr(c, &["name"]).unwrap_or_default(), jstr(c, &["joinphrase"]).unwrap_or_default()))
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn year(date: Option<String>) -> Option<String> {
    date.and_then(|d| d.get(..4).map(str::to_string)).filter(|y| y.chars().all(|c| c.is_ascii_digit()))
}

fn caa_group(rg: Option<String>) -> Option<String> {
    rg.map(|id| format!("https://coverartarchive.org/release-group/{id}/front-500"))
}

// ---------- Alba ----------

async fn mb_albums(http: &reqwest::Client, q: &str) -> Result<Vec<Candidate>> {
    let v = get_json(http, "https://musicbrainz.org/ws/2/release/", &[("query", q), ("fmt", "json"), ("limit", "8")]).await?;
    Ok(v.get("releases")
        .and_then(|r| r.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|r| {
                    Some(Candidate {
                        source: "musicbrainz".into(),
                        id: jstr(r, &["id"])?,
                        title: jstr(r, &["title"])?,
                        artist: mb_artist(r),
                        album: None,
                        year: year(jstr(r, &["date"])),
                        cover_url: caa_group(jstr(r, &["release-group", "id"])),
                        track_count: jnum(r, &["track-count"]).map(|n| n as u32),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

async fn deezer_albums(http: &reqwest::Client, q: &str) -> Result<Vec<Candidate>> {
    let v = get_json(http, "https://api.deezer.com/search/album", &[("q", q), ("limit", "8")]).await?;
    Ok(v.get("data")
        .and_then(|r| r.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some(Candidate {
                        source: "deezer".into(),
                        id: jnum(a, &["id"])?.to_string(),
                        title: jstr(a, &["title"])?,
                        artist: jstr(a, &["artist", "name"]).unwrap_or_default(),
                        album: None,
                        year: None,
                        cover_url: jstr(a, &["cover_xl"]).or_else(|| jstr(a, &["cover_big"])),
                        track_count: jnum(a, &["nb_tracks"]).map(|n| n as u32),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Prolne výsledky obou zdrojů (Deezer mívá lepší obaly, MusicBrainz přesnější data).
fn interleave(a: Result<Vec<Candidate>>, b: Result<Vec<Candidate>>) -> Result<Vec<Candidate>> {
    if let (Err(e), Err(_)) = (&a, &b) {
        bail!("vyhledávání selhalo: {e}");
    }
    for e in [a.as_ref().err(), b.as_ref().err()].into_iter().flatten() {
        tracing::warn!("identifikace: {e:#}");
    }
    let (a, b) = (a.unwrap_or_default(), b.unwrap_or_default());
    let mut out = Vec::new();
    for i in 0..a.len().max(b.len()) {
        out.extend(a.get(i).cloned());
        out.extend(b.get(i).cloned());
    }
    Ok(out)
}

pub async fn search_albums(http: &reqwest::Client, q: &str) -> Result<Vec<Candidate>> {
    let (dz, mb) = tokio::join!(deezer_albums(http, q), mb_albums(http, q));
    interleave(dz, mb)
}

/// Skladby alba v pořadí.
pub async fn album_tracks(http: &reqwest::Client, source: &str, id: &str) -> Result<Vec<AlbumTrack>> {
    match source {
        "musicbrainz" => {
            let v = get_json(
                http,
                &format!("https://musicbrainz.org/ws/2/release/{id}"),
                &[("inc", "recordings+artist-credits"), ("fmt", "json")],
            )
            .await?;
            let album_artist = mb_artist(&v);
            let mut out = Vec::new();
            for medium in v.get("media").and_then(|m| m.as_array()).into_iter().flatten() {
                for t in medium.get("tracks").and_then(|t| t.as_array()).into_iter().flatten() {
                    let artist = Some(mb_artist(t)).filter(|a| !a.is_empty()).unwrap_or_else(|| album_artist.clone());
                    if let Some(title) = jstr(t, &["title"]) {
                        out.push(AlbumTrack { title, artist });
                    }
                }
            }
            Ok(out)
        }
        "deezer" => {
            let v = get_json(http, &format!("https://api.deezer.com/album/{id}"), &[]).await?;
            Ok(jp(&v, &["tracks", "data"])
                .and_then(|t| t.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|t| {
                            Some(AlbumTrack { title: jstr(t, &["title"])?, artist: jstr(t, &["artist", "name"]).unwrap_or_default() })
                        })
                        .collect()
                })
                .unwrap_or_default())
        }
        other => bail!("neznámý zdroj {other}"),
    }
}

// ---------- Skladby ----------

async fn mb_tracks(http: &reqwest::Client, q: &str) -> Result<Vec<Candidate>> {
    let v = get_json(http, "https://musicbrainz.org/ws/2/recording/", &[("query", q), ("fmt", "json"), ("limit", "8")]).await?;
    Ok(v.get("recordings")
        .and_then(|r| r.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|r| {
                    let release = jp(r, &["releases", "0"]);
                    Some(Candidate {
                        source: "musicbrainz".into(),
                        id: jstr(r, &["id"])?,
                        title: jstr(r, &["title"])?,
                        artist: mb_artist(r),
                        album: release.and_then(|x| jstr(x, &["title"])),
                        year: year(release.and_then(|x| jstr(x, &["date"]))),
                        cover_url: caa_group(release.and_then(|x| jstr(x, &["release-group", "id"]))),
                        track_count: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

async fn deezer_tracks(http: &reqwest::Client, q: &str) -> Result<Vec<Candidate>> {
    let v = get_json(http, "https://api.deezer.com/search/track", &[("q", q), ("limit", "8")]).await?;
    Ok(v.get("data")
        .and_then(|r| r.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    Some(Candidate {
                        source: "deezer".into(),
                        id: jnum(t, &["id"])?.to_string(),
                        title: jstr(t, &["title"])?,
                        artist: jstr(t, &["artist", "name"]).unwrap_or_default(),
                        album: jstr(t, &["album", "title"]),
                        year: None,
                        cover_url: jstr(t, &["album", "cover_xl"]).or_else(|| jstr(t, &["album", "cover_big"])),
                        track_count: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn search_tracks(http: &reqwest::Client, q: &str) -> Result<Vec<Candidate>> {
    let (dz, mb) = tokio::join!(deezer_tracks(http, q), mb_tracks(http, q));
    interleave(dz, mb)
}

/// Stáhne obal; vrací (data, přípona).
pub async fn download_image(http: &reqwest::Client, url: &str) -> Result<(Vec<u8>, &'static str)> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        bail!("neplatná adresa obalu");
    }
    let resp = http.get(url).header("User-Agent", MB_UA).timeout(Duration::from_secs(30)).send().await?;
    if !resp.status().is_success() {
        bail!("obal není k dispozici (HTTP {})", resp.status().as_u16());
    }
    let ext = image_ext(resp.headers().get("content-type").and_then(|c| c.to_str().ok()).unwrap_or(""))
        .ok_or_else(|| anyhow::anyhow!("odkaz nevede na obrázek"))?;
    let bytes = resp.bytes().await?;
    if bytes.len() > 15 * 1024 * 1024 {
        bail!("obrázek je moc velký");
    }
    Ok((bytes.to_vec(), ext))
}

pub fn image_ext(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

pub fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

/// Obal ke skladbě podle textu „Interpret - Název“ (první shoda na Deezeru).
pub async fn find_cover(http: &reqwest::Client, query: &str) -> Option<String> {
    let q = query.replace(" - ", " ");
    deezer_tracks(http, &q).await.ok()?.into_iter().find_map(|c| c.cover_url)
}
