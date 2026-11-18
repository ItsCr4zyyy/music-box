use serde::{Deserialize, Serialize};

/// Role uživatele. Pořadí odpovídá oprávněním (Guest < Dj < Admin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Vidí jen co hraje a frontu.
    Guest,
    /// Hledá a přidává písničky do fronty.
    Dj,
    /// Ovládá přehrávání, schvaluje lidi, nastavení, přednost ve frontě.
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Guest => "guest",
            Role::Dj => "dj",
            Role::Admin => "admin",
        }
    }
    pub fn parse(s: &str) -> Role {
        match s {
            "admin" => Role::Admin,
            "dj" => Role::Dj,
            _ => Role::Guest,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
    /// Zaregistrovaný, čeká na schválení administrátorem.
    Pending,
    Active,
    /// Zablokovaný – nemůže se přihlásit.
    Banned,
}

impl UserStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            UserStatus::Pending => "pending",
            UserStatus::Active => "active",
            UserStatus::Banned => "banned",
        }
    }
    pub fn parse(s: &str) -> UserStatus {
        match s {
            "active" => UserStatus::Active,
            "banned" => UserStatus::Banned,
            _ => UserStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub status: UserStatus,
    pub created_at: i64,
    /// Profilová fotka (`/api/avatars/…`), None = barevné písmeno.
    pub avatar_url: Option<String>,
}

impl User {
    pub fn can(&self, role: Role) -> bool {
        self.status == UserStatus::Active && self.role >= role
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Track {
    pub id: i64,
    /// "youtube" | "local"
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_sec: u32,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserRef {
    pub id: i64,
    pub name: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct QueueEntry {
    pub id: i64,
    pub track: Track,
    /// None = přidal sám music-box (automatické přehrávání).
    pub added_by: Option<UserRef>,
    /// Admin dal skladbě přednost.
    pub priority: bool,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct NowPlaying {
    pub entry: QueueEntry,
    pub position_ms: u64,
    pub paused: bool,
    /// Skladba se teprve stahuje / připravuje.
    pub loading: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateSnapshot {
    pub now_playing: Option<NowPlaying>,
    pub queue: Vec<QueueEntry>,
    pub history: Vec<QueueEntry>,
    pub volume: f32,
    pub join_url: String,
    pub box_name: String,
    pub display: DisplaySettings,
    /// Adresa CSS aktivního motivu (mění se s motivem → klient ho načte znovu).
    pub theme_css: String,
    /// Zapnuté efekty aktivního motivu (vykresluje je displej).
    pub effects: Vec<String>,
    /// Hraje rádio (fronta je prázdná).
    pub radio: Option<RadioNow>,
    pub language: String,
    /// Čas serveru (ms) – klient podle něj dopočítává pozici.
    pub server_time_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdleMode {
    /// Když je fronta prázdná, nic nehraje.
    Silence,
    /// Když je fronta prázdná, hraje náhodné skladby z lokální knihovny.
    #[default]
    RandomLocal,
    /// Když je fronta prázdná, hraje internetové rádio.
    Radio,
}

/// Internetové rádio (Icecast / SHOUTcast stream).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RadioStation {
    pub id: String,
    pub name: String,
    pub url: String,
    pub logo: Option<String>,
}

/// Co právě hraje rádio.
#[derive(Clone, Debug, Serialize, Default)]
pub struct RadioNow {
    pub station: String,
    pub station_logo: Option<String>,
    /// Celý název z rádia („Interpret - Název“).
    pub stream_title: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub loading: bool,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    #[default]
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

/// Vzhled displeje (nastavuje admin na webu, displej se překreslí hned).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub qr_enabled: bool,
    pub qr_position: Corner,
    pub qr_size: Size,
    /// Text vedle QR kódu.
    pub qr_caption: String,
    /// Na které straně je fronta.
    pub queue_side: Side,
    /// Kolik skladeb z fronty ukázat (0 = fronta se neukazuje).
    pub queue_length: u32,
    /// Ukazovat, kdo písničku přidal.
    pub show_added_by: bool,
    /// Zastaralé (dřív poletující noty) – nahrazeno `effects`.
    #[serde(skip_serializing)]
    pub show_notes: bool,
    /// Zapnuté/vypnuté speciální efekty motivů (ID efektu → zapnuto). Chybějící = výchozí podle motivu.
    pub effects: std::collections::BTreeMap<String, bool>,
    /// Ukazovat logo s názvem.
    pub show_logo: bool,
    /// Roh s logem a názvem.
    pub logo_position: Corner,
    pub logo_size: Size,
    /// Verze nahraného loga (0 = výchozí logo). Mění ji jen server při nahrání.
    pub logo_rev: i64,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        DisplaySettings {
            qr_enabled: true,
            qr_position: Corner::BottomRight,
            qr_size: Size::Medium,
            qr_caption: "Přidej svoji písničku!".into(),
            queue_side: Side::Right,
            queue_length: 6,
            show_added_by: true,
            show_notes: true,
            effects: Default::default(),
            show_logo: true,
            logo_position: Corner::TopLeft,
            logo_size: Size::Medium,
            logo_rev: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Název music-boxu (na displeji i na webu).
    pub box_name: String,
    /// ID aktivního motivu vzhledu.
    pub theme: String,
    /// Jazyk displeje a výchozí jazyk webu ("cs" | "en").
    pub language: String,
    /// Uložené stanice rádia.
    pub radio_stations: Vec<RadioStation>,
    /// ID stanice, která hraje, když je fronta prázdná (idle_mode = radio).
    pub radio_station: String,
    /// Displej na celou obrazovku.
    pub fullscreen: bool,
    pub display: DisplaySettings,
    /// Adresa webu pro QR kód; prázdné = automaticky http://<ip v síti>:<port>.
    pub public_url: String,
    /// Další složky s lokální hudbou (hlavní složka music-boxu se prochází vždy).
    pub local_folders: Vec<String>,
    pub idle_mode: IdleMode,
    /// Kolik skladeb může mít DJ najednou ve frontě (0 = neomezeně).
    pub max_queued_per_user: u32,
    /// Maximální délka skladby pro DJ v minutách (0 = neomezeně).
    pub max_track_minutes: u32,
    /// Povolit registraci nových lidí.
    pub allow_registration: bool,
    /// Písnička, která už je ve frontě (nebo hraje), nejde přidat znovu.
    pub prevent_duplicates: bool,
    /// Hlasitost 0.0–1.0.
    pub volume: f32,
    /// Zastaralé – jedno výstupní zařízení (nahrazeno `audio_devices`).
    pub audio_device: String,
    /// Výstupní zvuková zařízení (prázdné = výchozí zařízení systému). Víc zařízení = hraje se na všech.
    pub audio_devices: Vec<String>,
    /// Vyrovnat hlasitost skladeb (aby jedna nebyla potichu a druhá nahlas).
    pub normalize_volume: bool,
    /// Odkud smí DJ přidávat písničky (admin smí vždycky všechno).
    pub dj_sources: DjSources,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DjSources {
    /// Hledání na YouTube Music.
    pub youtube: bool,
    /// Vložené odkazy (YouTube, Spotify).
    pub links: bool,
    /// Lokální knihovna.
    pub local: bool,
}

impl Default for DjSources {
    fn default() -> Self {
        DjSources { youtube: true, links: true, local: true }
    }
}

impl Settings {
    /// Výstupní zařízení k použití (podporuje i staré nastavení s jedním zařízením).
    pub fn output_devices(&self) -> Vec<String> {
        if !self.audio_devices.is_empty() {
            self.audio_devices.clone()
        } else if !self.audio_device.is_empty() {
            vec![self.audio_device.clone()]
        } else {
            vec![]
        }
    }
}

/// Výstupní zvukové zařízení pro výběr v administraci.
#[derive(Clone, Debug, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            box_name: "music-box".into(),
            theme: crate::themes::DEFAULT_THEME.into(),
            language: "cs".into(),
            radio_stations: vec![
                RadioStation {
                    id: "fajn".into(),
                    name: "Fajn Rádio".into(),
                    url: "http://ice.abradio.cz/fajn128.mp3".into(),
                    logo: Some("https://www.fajnradio.cz/favicon.ico".into()),
                },
                RadioStation {
                    id: "evropa2".into(),
                    name: "Evropa 2".into(),
                    url: "https://ice.actve.net/fm-evropa2-128".into(),
                    logo: None,
                },
            ],
            radio_station: "fajn".into(),
            fullscreen: true,
            display: DisplaySettings::default(),
            public_url: String::new(),
            local_folders: Vec::new(),
            idle_mode: IdleMode::RandomLocal,
            max_queued_per_user: 5,
            max_track_minutes: 12,
            allow_registration: true,
            prevent_duplicates: true,
            volume: 0.8,
            audio_device: String::new(),
            audio_devices: vec![],
            normalize_volume: true,
            dj_sources: DjSources::default(),
        }
    }
}

/// Album (podsložka v knihovně) pro procházení na webu.
#[derive(Clone, Debug, Serialize)]
pub struct Album {
    /// Relativní cesta složky – klíč pro přidání celého alba.
    pub key: String,
    pub name: String,
    pub artist: String,
    pub artwork_url: Option<String>,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Library {
    pub albums: Vec<Album>,
    /// Skladby přímo v hlavní složce.
    pub singles: Vec<Track>,
}

/// Skladba z online zdroje, ještě neuložená v databázi.
#[derive(Clone, Debug)]
pub struct RemoteTrack {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_sec: u32,
    pub artwork_url: Option<String>,
}
