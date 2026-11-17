//! Interní API YouTube Music (InnerTube) – rychlé hledání skladeb s metadaty.
//! Port z Vibe (YouTubeMusicApi.kt). Stahování audia dál řeší yt-dlp.

use crate::models::RemoteTrack;
use crate::util::{find_all, jp, jstr, now, parse_duration};
use anyhow::{bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::LazyLock;
use tokio::sync::RwLock;

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
const FALLBACK_CLIENT_VERSION: &str = "1.20260928.13.00";
const SONGS_FILTER: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA%3D%3D";

pub struct YtMusic {
    http: reqwest::Client,
    /// (verze klienta, čas poslední kontroly)
    version: RwLock<(String, i64)>,
}

enum PostError {
    Rejected,
    Other(anyhow::Error),
}

impl YtMusic {
    pub fn new(http: reqwest::Client) -> YtMusic {
        YtMusic { http, version: RwLock::new((FALLBACK_CLIENT_VERSION.to_string(), 0)) }
    }

    /// Zjistí aktuální verzi webového klienta z music.youtube.com (max. jednou denně, pokud není `force`).
    async fn refresh_version(&self, force: bool) -> String {
        {
            let v = self.version.read().await;
            if !force && now() - v.1 < 86400 {
                return v.0.clone();
            }
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""INNERTUBE_CLIENT_VERSION":"([0-9.]+)""#).unwrap());
        let found = async {
            let html = self
                .http
                .get("https://music.youtube.com/")
                .header("User-Agent", USER_AGENT)
                .header("Accept-Language", "cs,en;q=0.8")
                .header("Cookie", "SOCS=CAI; CONSENT=YES+")
                .send()
                .await
                .ok()?
                .text()
                .await
                .ok()?;
            RE.captures(&html).map(|c| c[1].to_string())
        }
        .await;
        let mut v = self.version.write().await;
        v.1 = now();
        if let Some(f) = found {
            v.0 = f;
        }
        v.0.clone()
    }

    async fn post(&self, endpoint: &str, extra: Value) -> Result<Value> {
        let version = self.refresh_version(false).await;
        match self.post_once(endpoint, &version, &extra).await {
            Ok(v) => Ok(v),
            Err(PostError::Rejected) => {
                let fresh = self.refresh_version(true).await;
                match self.post_once(endpoint, &fresh, &extra).await {
                    Ok(v) => Ok(v),
                    Err(PostError::Rejected) => bail!("YouTube Music odmítl požadavek"),
                    Err(PostError::Other(e)) => Err(e),
                }
            }
            Err(PostError::Other(e)) => Err(e),
        }
    }

    async fn post_once(&self, endpoint: &str, version: &str, extra: &Value) -> Result<Value, PostError> {
        let mut body = json!({
            "context": { "client": { "clientName": "WEB_REMIX", "clientVersion": version, "hl": "cs", "gl": "CZ" } }
        });
        if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                b.insert(k.clone(), v.clone());
            }
        }
        let resp = self
            .http
            .post(format!("https://music.youtube.com/youtubei/v1/{endpoint}?prettyPrint=false"))
            .header("User-Agent", USER_AGENT)
            .header("Origin", "https://music.youtube.com")
            .header("Referer", "https://music.youtube.com/")
            .json(&body)
            .send()
            .await
            .map_err(|e| PostError::Other(e.into()))?;
        let code = resp.status().as_u16();
        if code == 400 || code == 404 {
            return Err(PostError::Rejected);
        }
        if !resp.status().is_success() {
            return Err(PostError::Other(anyhow::anyhow!("YouTube Music: HTTP {code}")));
        }
        resp.json().await.map_err(|e| PostError::Other(e.into()))
    }

    pub async fn search_songs(&self, query: &str) -> Result<Vec<RemoteTrack>> {
        let root = self.post("search", json!({ "query": query, "params": SONGS_FILTER })).await?;
        let mut items = Vec::new();
        find_all(&root, "musicResponsiveListItemRenderer", &mut items);
        Ok(items.into_iter().filter_map(parse_song).collect())
    }
}

fn join_runs(runs: &[&Value]) -> String {
    runs.iter().filter_map(|r| r.get("text").and_then(|t| t.as_str())).collect::<String>().trim().to_string()
}

fn page_type(link: Option<&Value>) -> Option<&str> {
    link.and_then(|l| {
        jp(l, &["browseEndpointContextSupportedConfigs", "browseEndpointContextMusicConfig", "pageType"])
    })
    .and_then(|v| v.as_str())
}

fn link_of(run: &Value) -> Option<&Value> {
    jp(run, &["navigationEndpoint", "browseEndpoint"])
}

/// Rozdělí „A, B • Album • 3:45“ na segmenty podle oddělovače „•“.
fn segments(runs: &[Value]) -> Vec<Vec<&Value>> {
    let mut out: Vec<Vec<&Value>> = vec![vec![]];
    for r in runs {
        if r.get("text").and_then(|t| t.as_str()).map(|t| t.trim() == "•").unwrap_or(false) {
            out.push(vec![]);
        } else if let Some(last) = out.last_mut() {
            last.push(r);
        }
    }
    out
}

fn first_thumbnails(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Object(m) => {
            if let Some(Value::Array(a)) = m.get("thumbnails") {
                return Some(a);
            }
            m.values().find_map(first_thumbnails)
        }
        Value::Array(a) => a.iter().find_map(first_thumbnails),
        _ => None,
    }
}

pub fn upscale_artwork(url: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=w\d+-h\d+.*$").unwrap());
    RE.replace(url, "=w544-h544-l90-rj").into_owned()
}

fn parse_song(item: &Value) -> Option<RemoteTrack> {
    let cols = item.get("flexColumns")?.as_array()?;
    let runs = |i: usize| -> Vec<Value> {
        cols.get(i)
            .and_then(|c| jp(c, &["musicResponsiveListItemFlexColumnRenderer", "text", "runs"]))
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default()
    };
    let title_runs = runs(0);
    let video_id = jstr(item, &["playlistItemData", "videoId"]).or_else(|| {
        title_runs.iter().find_map(|r| jstr(r, &["navigationEndpoint", "watchEndpoint", "videoId"]))
    })?;
    let title = join_runs(&title_runs.iter().collect::<Vec<_>>());
    if title.is_empty() {
        return None;
    }
    let subtitle = runs(1);
    let mut segs: Vec<Vec<&Value>> = segments(&subtitle).into_iter().filter(|s| !join_runs(s).is_empty()).collect();
    // Bez filtru začíná řádek typem („Skladba“, „Video“) – ten zahodíme.
    if segs.len() > 1
        && segs[0].iter().all(|r| link_of(r).is_none())
        && segs[1].iter().any(|r| page_type(link_of(r)) == Some("MUSIC_PAGE_TYPE_ARTIST"))
    {
        segs.remove(0);
    }
    let artist = segs.first().map(|s| join_runs(s)).filter(|s| !s.is_empty()).unwrap_or_else(|| "Neznámý interpret".into());
    let album = subtitle
        .iter()
        .find(|r| page_type(link_of(r)) == Some("MUSIC_PAGE_TYPE_ALBUM"))
        .and_then(|r| jstr(r, &["text"]));
    let fixed = item.get("fixedColumns").and_then(|f| f.as_array()).and_then(|a| {
        a.iter().find_map(|c| {
            let runs = jp(c, &["musicResponsiveListItemFixedColumnRenderer", "text", "runs"])?.as_array()?;
            parse_duration(&join_runs(&runs.iter().collect::<Vec<_>>()))
        })
    });
    let duration = fixed.or_else(|| segs.last().and_then(|s| parse_duration(&join_runs(s)))).unwrap_or(0);
    let artwork = item
        .get("thumbnail")
        .and_then(first_thumbnails)
        .and_then(|a| a.last())
        .and_then(|t| jstr(t, &["url"]))
        .map(|u| upscale_artwork(&u))
        .unwrap_or_else(|| format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"));
    Some(RemoteTrack { video_id, title, artist, album, duration_sec: duration, artwork_url: Some(artwork) })
}
