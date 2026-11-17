//! Načtení skladby / alba / playlistu ze Spotify přes veřejnou embed stránku (bez API klíče).
//! Audio ze Spotify stáhnout nejde (DRM) – skladby se dohledají na YouTube Music (stejně jako ve Vibe).

use crate::sources::ytmusic::USER_AGENT;
use crate::util::{jnum, jp, jstr};
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub struct SpotifyTrack {
    pub title: String,
    pub artists: String,
    pub duration_sec: u32,
}

/// Rozpozná odkaz `open.spotify.com/(intl-xx/)?(track|album|playlist)/<id>` nebo `spotify:track:<id>`.
pub fn parse_url(url: &str) -> Option<(String, String)> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:open\.spotify\.com/(?:intl-[a-z]+/)?|spotify:)(track|album|playlist)[/:]([A-Za-z0-9]+)").unwrap()
    });
    RE.captures(url).map(|c| (c[1].to_string(), c[2].to_string()))
}

pub async fn fetch(http: &reqwest::Client, kind: &str, id: &str) -> Result<(String, Vec<SpotifyTrack>)> {
    static DATA: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)<script id="__NEXT_DATA__" type="application/json">(.*?)</script>"#).unwrap()
    });
    let html = http
        .get(format!("https://open.spotify.com/embed/{kind}/{id}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let raw = DATA.captures(&html).map(|c| c[1].to_string()).ok_or_else(|| anyhow!("Spotify: stránku se nepodařilo načíst"))?;
    let data: Value = serde_json::from_str(&raw)?;
    let entity = jp(&data, &["props", "pageProps", "state", "data", "entity"])
        .ok_or_else(|| anyhow!("Spotify: nenalezeno (je to veřejné?)"))?;
    let name = jstr(entity, &["name"]).or_else(|| jstr(entity, &["title"])).unwrap_or_else(|| "Spotify".into());

    let mut tracks: Vec<SpotifyTrack> = entity
        .get("trackList")
        .and_then(|t| t.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|t| {
                    Some(SpotifyTrack {
                        title: jstr(t, &["title"])?,
                        artists: jstr(t, &["subtitle"]).unwrap_or_default().replace('\u{a0}', " "),
                        duration_sec: (jnum(t, &["duration"]).unwrap_or(0.0) / 1000.0) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Jedna skladba nemá trackList – údaje jsou přímo v entitě.
    if tracks.is_empty() && kind == "track" {
        let artists = entity
            .get("artists")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|x| jstr(x, &["name"])).collect::<Vec<_>>().join(", "))
            .or_else(|| jstr(entity, &["subtitle"]))
            .unwrap_or_default();
        let duration = jnum(entity, &["duration"]).or_else(|| jnum(entity, &["duration", "totalMilliseconds"])).unwrap_or(0.0);
        tracks.push(SpotifyTrack { title: name.clone(), artists, duration_sec: (duration / 1000.0) as u32 });
    }
    if tracks.is_empty() {
        bail!("Spotify: nic k přehrání (prázdné nebo soukromé)");
    }
    Ok((name, tracks))
}
