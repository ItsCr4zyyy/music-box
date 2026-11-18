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
            if let Some(title) = parse_stream_title(&meta) {
                (self.on_title)(title);
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

/// Vytáhne `StreamTitle='…'` z ICY metadat a převede text na UTF-8.
/// Stanice posílají UTF-8, Windows-1250 i ISO-8859-2 a někdy HTML entity (`&#353;`).
pub fn parse_stream_title(meta: &[u8]) -> Option<String> {
    const KEY: &[u8] = b"StreamTitle='";
    let start = meta.windows(KEY.len()).position(|w| w == KEY)? + KEY.len();
    let rest = &meta[start..];
    let end = rest.windows(2).position(|w| w == b"';").unwrap_or_else(|| rest.iter().position(|b| *b == 0).unwrap_or(rest.len()));
    let raw = &rest[..end];
    let text = match std::str::from_utf8(raw) {
        Ok(t) => t.to_string(),
        Err(_) => {
            // Bajty 0x80–0x9F jsou ve Windows-1250 písmena (š, ž, ť…), v ISO-8859-2 řídicí znaky.
            let enc = if raw.iter().any(|b| (0x80..=0x9f).contains(b)) {
                encoding_rs::WINDOWS_1250
            } else {
                encoding_rs::ISO_8859_2
            };
            enc.decode_without_bom_handling(raw).0.into_owned()
        }
    };
    Some(decode_entities(&text).trim().to_string())
}

/// `&#353;`, `&#x161;`, `&amp;`, `&quot;`… → znaky.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..semi];
        let decoded = if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else if let Some(dec) = entity.strip_prefix('#') {
            dec.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "nbsp" => Some(' '),
                _ => None,
            }
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_titles() {
        // UTF-8
        assert_eq!(parse_stream_title("StreamTitle='KAYA - Odevzdávám se';".as_bytes()).unwrap(), "KAYA - Odevzdávám se");
        // Windows-1250 + HTML entita (jako posílá Fajn Rádio)
        let mut cp1250 = b"StreamTitle='KAYA - odevzd".to_vec();
        cp1250.extend_from_slice(&[0xE1]); // á
        cp1250.extend_from_slice(b"v");
        cp1250.extend_from_slice(&[0xE1]);
        cp1250.extend_from_slice(b"m se v&#353;e ");
        cp1250.extend_from_slice(&[0x9A, 0x9E]); // š ž
        cp1250.extend_from_slice(b"';StreamUrl='';");
        assert_eq!(parse_stream_title(&cp1250).unwrap(), "KAYA - odevzdávám se vše šž");
        // ISO-8859-2: š = 0xB9
        let latin2 = [b"StreamTitle='".as_slice(), &[0xB9, 0xE1], b"';"].concat();
        assert_eq!(parse_stream_title(&latin2).unwrap(), "šá");
        assert_eq!(parse_stream_title(b"StreamTitle='Rock &amp; Roll';").unwrap(), "Rock & Roll");
        assert!(parse_stream_title(b"StreamUrl='';").is_none());
    }
}
