pub mod local;
pub mod metadata;
pub mod radio;
pub mod spotify;
pub mod ytdlp;
pub mod ytmusic;

use regex::Regex;
use std::sync::LazyLock;

/// „Interpret - Název (Official Video)“ → („Interpret“, „Název“). Port z Vibe.
pub fn clean_video_title(raw: &str, channel: Option<&str>) -> (String, String) {
    static NOISE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\s*[(\[](official|lyrics?|audio|video|music video|hd|hq|4k|visuali[sz]er|clip officiel|videoclip)[^)\]]*[)\]]").unwrap()
    });
    let title = NOISE.replace_all(raw, "").trim().to_string();
    let topic = channel.map(|c| c.ends_with(" - Topic")).unwrap_or(false);
    if !topic {
        if let Some((a, t)) = title.split_once(" - ") {
            return (a.trim().to_string(), t.trim().to_string());
        }
    }
    let artist = channel
        .map(|c| c.trim_end_matches(" - Topic").trim_end_matches("VEVO").trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "Neznámý interpret".into());
    (artist, title)
}

/// Vytáhne YouTube video ID z odkazu (youtube.com/watch?v=, youtu.be/, music.youtube.com, shorts).
pub fn youtube_id(url: &str) -> Option<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:youtube\.com/(?:watch\?(?:.*&)?v=|shorts/|embed/|live/)|youtu\.be/)([A-Za-z0-9_-]{11})").unwrap()
    });
    RE.captures(url).map(|c| c[1].to_string())
}

/// Odkaz na YouTube playlist (bez konkrétního videa).
pub fn is_youtube_playlist(url: &str) -> bool {
    (url.contains("youtube.com/playlist") || url.contains("music.youtube.com/browse/")) && youtube_id(url).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_ids() {
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://youtu.be/dQw4w9WgXcQ?si=x").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD").as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(youtube_id("https://www.youtube.com/watch?list=PL1&v=dQw4w9WgXcQ").as_deref(), Some("dQw4w9WgXcQ"));
        assert!(youtube_id("https://example.com").is_none());
    }

    #[test]
    fn titles() {
        assert_eq!(
            clean_video_title("Rick Astley - Never Gonna Give You Up (Official Music Video)", Some("Rick Astley")),
            ("Rick Astley".into(), "Never Gonna Give You Up".into())
        );
        assert_eq!(clean_video_title("Song", Some("Band - Topic")), ("Band".into(), "Song".into()));
    }

    #[test]
    fn spotify_urls() {
        assert_eq!(
            spotify::parse_url("https://open.spotify.com/intl-cs/track/4cOdK2wGLETKBW3PvgPWqT?si=1"),
            Some(("track".into(), "4cOdK2wGLETKBW3PvgPWqT".into()))
        );
        assert_eq!(spotify::parse_url("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"), Some(("playlist".into(), "37i9dQZF1DXcBWIGoYBM5M".into())));
    }
}
