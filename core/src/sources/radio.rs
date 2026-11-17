//! Internetové rádio: Icecast / SHOUTcast streamy (MP3, AAC).
//! Název právě hrající skladby se čte z ICY metadat („StreamTitle“) přímo ve streamu.
//! Stanice se dají vyhledat v otevřeném katalogu radio-browser.info (bez klíče).

use crate::util::{jnum, jstr};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::time::Duration;

const UA: &str = concat!("music-box/", env!("CARGO_PKG_VERSION"), " (self-hosted jukebox)");

#[derive(Clone, Debug, Serialize)]
pub struct StationHit {
    pub name: String,
    pub url: String,
    pub logo: Option<String>,
    pub codec: String,
    pub bitrate: u32,
    pub country: String,
}

/// Vyhledá stanice v katalogu radio-browser.info (zkouší víc zrcadel).
pub async fn search(http: &reqwest::Client, q: &str) -> Result<Vec<StationHit>> {
    let mut last_err = None;
    for host in ["de1", "de2", "fi1"] {
        let resp = http
            .get(format!("https://{host}.api.radio-browser.info/json/stations/search"))
            .query(&[("name", q), ("hidebroken", "true"), ("order", "clickcount"), ("reverse", "true"), ("limit", "25")])
            .header("User-Agent", UA)
            .timeout(Duration::from_secs(10))
            .send()
            .await;
        match resp.and_then(|r| r.error_for_status()) {
            Ok(r) => {
                let list: serde_json::Value = r.json().await?;
                return Ok(list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| {
                                let codec = jstr(s, &["codec"]).unwrap_or_default();
                                // Dekodér neumí Opus/HLS – jen MP3 a AAC.
                                if !matches!(codec.to_uppercase().as_str(), "MP3" | "AAC" | "AAC+") {
                                    return None;
                                }
                                Some(StationHit {
                                    name: jstr(s, &["name"])?.trim().to_string(),
                                    url: jstr(s, &["url_resolved"]).filter(|u| !u.is_empty()).or_else(|| jstr(s, &["url"]))?,
                                    logo: jstr(s, &["favicon"]).filter(|f| f.starts_with("http")),
                                    codec,
                                    bitrate: jnum(s, &["bitrate"]).unwrap_or(0.0) as u32,
                                    country: jstr(s, &["countrycode"]).unwrap_or_default(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default());
            }
            Err(e) => last_err = Some(e),
        }
    }
    bail!("katalog rádií nedostupný: {}", last_err.map(|e| e.to_string()).unwrap_or_default())
}

/// „Interpret - Název“ → (interpret, název).
pub fn split_title(t: &str) -> (Option<String>, Option<String>) {
    let t = t.trim();
    if t.is_empty() {
        return (None, None);
    }
    match t.split_once(" - ") {
        Some((a, b)) => (Some(a.trim().to_string()), Some(b.trim().to_string())),
        None => (None, Some(t.to_string())),
    }
}

/// Stream rádia: odstraňuje ICY metadata z dat a hlásí změny názvu skladby.
pub struct IcyStream {
    inner: Mutex<Box<dyn Read + Send>>,
    metaint: Option<usize>,
    until_meta: usize,
    pos: u64,
    on_title: Box<dyn Fn(String) + Send + Sync>,
}

impl IcyStream {
    fn read_exact_inner(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.inner.get_mut().unwrap_or_else(|e| e.into_inner()).read_exact(buf)
    }

    fn read_metadata(&mut self) -> std::io::Result<()> {
        let mut len = [0u8; 1];
        self.read_exact_inner(&mut len)?;
        let n = len[0] as usize * 16;
        if n > 0 {
            let mut meta = vec![0u8; n];
            self.read_exact_inner(&mut meta)?;
            let text = String::from_utf8_lossy(&meta);
            if let Some(start) = text.find("StreamTitle='") {
                let rest = &text[start + 13..];
                let end = rest.find("';").unwrap_or(rest.trim_end_matches('\0').len());
                (self.on_title)(rest[..end].to_string());
            }
        }
        Ok(())
    }
}

impl Read for IcyStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(metaint) = self.metaint else {
            let n = self.inner.get_mut().unwrap_or_else(|e| e.into_inner()).read(buf)?;
            self.pos += n as u64;
            return Ok(n);
        };
        if self.until_meta == 0 {
            self.read_metadata()?;
            self.until_meta = metaint;
        }
        let want = buf.len().min(self.until_meta);
        let n = self.inner.get_mut().unwrap_or_else(|e| e.into_inner()).read(&mut buf[..want])?;
        self.until_meta -= n;
        self.pos += n as u64;
        Ok(n)
    }
}

/// Živý stream nejde přetáčet – povolený je jen dotaz na aktuální pozici.
impl Seek for IcyStream {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        match to {
            SeekFrom::Current(0) => Ok(self.pos),
            SeekFrom::Start(p) if p == self.pos => Ok(self.pos),
            _ => Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "živý stream nejde přetáčet")),
        }
    }
}

/// Otevře stream a připraví dekodér (blokující – volat ve spawn_blocking).
pub fn open(url: &str, on_title: impl Fn(String) + Send + Sync + 'static) -> Result<rodio::Decoder<IcyStream>> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(None::<Duration>)
        .build()?;
    let resp = client
        .get(url)
        .header("User-Agent", UA)
        .header("Icy-MetaData", "1")
        .send()
        .context("rádio neodpovídá")?
        .error_for_status()
        .context("rádio odmítlo spojení")?;
    let metaint = resp
        .headers()
        .get("icy-metaint")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0);
    let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    let hint = if ct.contains("aac") || ct.contains("mp4") { "aac" } else { "mp3" };
    if ct.contains("ogg") || ct.contains("opus") || ct.contains("mpegurl") {
        bail!("formát streamu ({ct}) není podporovaný – zvol MP3 nebo AAC stanici");
    }
    let stream = IcyStream {
        inner: Mutex::new(Box::new(resp)),
        metaint,
        until_meta: metaint.unwrap_or(0),
        pos: 0,
        on_title: Box::new(on_title),
    };
    rodio::Decoder::builder()
        .with_data(stream)
        .with_seekable(false)
        .with_hint(hint)
        .build()
        .map_err(|e| anyhow::anyhow!("stream nejde dekódovat: {e}"))
}
