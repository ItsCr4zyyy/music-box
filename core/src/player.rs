//! Přehrávač: audio vlákno (rodio) + řídicí úloha, která bere skladby z fronty,
//! připraví soubor (lokální / stažený z YouTube) a hlídá konec skladby.
//! Když je fronta prázdná, může hrát náhodně z knihovny nebo internetové rádio.

use crate::models::{AudioDevice, IdleMode, QueueEntry, RadioNow};
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use crate::sources::{metadata, radio};
use crate::state::AppState;
use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

type RadioDecoder = rodio::Decoder<radio::IcyStream>;

pub enum PlayerCmd {
    /// Fronta nebo nastavení se změnily – zkontroluj, co má hrát.
    Wake,
    Skip,
    Pause,
    Resume,
    SetVolume(f32),
    Resolved { entry_id: i64, result: Result<PathBuf, String>, resume_ms: u64 },
    /// Admin změnil výstupní zařízení – přehrát znovu na novém.
    DeviceChanged,
    RadioReady { generation: u64, result: Result<RadioDecoder, String> },
    RadioTitle { generation: u64, title: String },
    RadioCover { title: String, url: Option<String> },
}

#[derive(Clone, Default, Debug)]
pub struct PlaybackStatus {
    pub current: Option<i64>,
    pub loading: bool,
    pub paused: bool,
}

pub struct PlayerHandle {
    tx: mpsc::UnboundedSender<PlayerCmd>,
    pub status: RwLock<PlaybackStatus>,
    pub pos_ms: Arc<AtomicU64>,
}

impl PlayerHandle {
    pub fn new() -> (PlayerHandle, mpsc::UnboundedReceiver<PlayerCmd>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (PlayerHandle { tx, status: RwLock::new(PlaybackStatus::default()), pos_ms: Arc::new(AtomicU64::new(0)) }, rx)
    }

    pub fn send(&self, cmd: PlayerCmd) {
        let _ = self.tx.send(cmd);
    }

    fn sender(&self) -> mpsc::UnboundedSender<PlayerCmd> {
        self.tx.clone()
    }

    pub fn status(&self) -> PlaybackStatus {
        self.status.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, f: impl FnOnce(&mut PlaybackStatus)) {
        f(&mut self.status.write().unwrap_or_else(|e| e.into_inner()));
    }
}

// ---------- Audio vlákno ----------

enum AudioCmd {
    Play { path: PathBuf, generation: u64, volume: f32, start_ms: u64 },
    /// Použít jiné výstupní zařízení (prázdné = výchozí).
    SetDevice(String),
    PlayRadio { decoder: RadioDecoder, generation: u64, volume: f32 },
    Pause,
    Resume,
    Stop,
    Volume(f32),
}

enum AudioEvent {
    Finished(u64),
    Error(u64, String),
}

/// Vnímaná hlasitost – lineární posuvník zní přirozeněji s kvadratickou křivkou.
fn gain(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    v * v
}

/// Seznam výstupních zařízení (Windows: WASAPI, Linux: ALSA – včetně PipeWire / PulseAudio).
pub fn output_devices() -> Vec<AudioDevice> {
    let host = rodio::cpal::default_host();
    let default_id = host.default_output_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
    let Ok(devices) = host.output_devices() else { return vec![] };
    devices
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            let name = d.description().map(|desc| desc.name().to_string()).unwrap_or_else(|_| id.clone());
            Some(AudioDevice { is_default: default_id.as_deref() == Some(id.as_str()), id, name })
        })
        .collect()
}

/// Otevře zvolené zařízení, nebo výchozí, když zvolené neexistuje.
fn open_device(wanted: &str) -> Result<rodio::MixerDeviceSink, String> {
    if !wanted.is_empty() {
        let host = rodio::cpal::default_host();
        let found = wanted.parse::<rodio::cpal::DeviceId>().ok().and_then(|id| host.device_by_id(&id));
        match found.map(|dev| rodio::DeviceSinkBuilder::from_device(dev).and_then(|b| b.open_stream())) {
            Some(Ok(sink)) => return Ok(sink),
            Some(Err(e)) => tracing::warn!("zvukové zařízení {wanted} nejde otevřít ({e}), používám výchozí"),
            None => tracing::warn!("zvukové zařízení {wanted} nenalezeno, používám výchozí"),
        }
    }
    rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| format!("Nepodařilo se otevřít zvukové zařízení: {e}"))
}

struct Audio {
    wanted_device: String,
    device: Option<rodio::MixerDeviceSink>,
    player: Option<rodio::Player>,
    generation: u64,
    active: bool,
}

impl Audio {
    fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
        self.active = false;
    }

    /// Připraví zvukové zařízení a nový přehrávač.
    fn prepare(&mut self, generation: u64, volume: f32) -> Result<&rodio::Player, String> {
        self.stop();
        self.generation = generation;
        if self.device.is_none() {
            let mut d = open_device(&self.wanted_device)?;
            d.log_on_drop(false);
            self.device = Some(d);
        }
        let d = self.device.as_ref().ok_or("zvukové zařízení chybí")?;
        let p = rodio::Player::connect_new(d.mixer());
        p.set_volume(gain(volume) as _);
        self.player = Some(p);
        self.active = true;
        Ok(self.player.as_ref().expect("přehrávač"))
    }
}

fn audio_thread(rx: std_mpsc::Receiver<AudioCmd>, events: mpsc::UnboundedSender<AudioEvent>, pos: Arc<AtomicU64>) {
    let mut a = Audio { wanted_device: String::new(), device: None, player: None, generation: 0, active: false };
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(AudioCmd::Play { path, generation, volume, start_ms }) => {
                pos.store(start_ms, Ordering::Relaxed);
                let source = std::fs::File::open(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| rodio::Decoder::try_from(f).map_err(|e| format!("Soubor nejde přehrát: {e}")));
                let result = source.and_then(|src| {
                    a.prepare(generation, volume).map(|p| {
                        p.append(src);
                        if start_ms > 0 {
                            let _ = p.try_seek(Duration::from_millis(start_ms));
                        }
                    })
                });
                if let Err(e) = result {
                    a.active = false;
                    let _ = events.send(AudioEvent::Error(generation, e));
                }
            }
            Ok(AudioCmd::PlayRadio { decoder, generation, volume }) => {
                pos.store(0, Ordering::Relaxed);
                if let Err(e) = a.prepare(generation, volume).map(|p| p.append(decoder)) {
                    a.active = false;
                    let _ = events.send(AudioEvent::Error(generation, e));
                }
            }
            Ok(AudioCmd::Pause) => {
                if let Some(p) = &a.player {
                    p.pause();
                }
            }
            Ok(AudioCmd::Resume) => {
                if let Some(p) = &a.player {
                    p.play();
                }
            }
            Ok(AudioCmd::Stop) => {
                a.stop();
                pos.store(0, Ordering::Relaxed);
            }
            Ok(AudioCmd::SetDevice(id)) => {
                // Zavřít staré zařízení; nové se otevře při dalším přehrávání.
                a.stop();
                a.device = None;
                a.wanted_device = id;
            }
            Ok(AudioCmd::Volume(v)) => {
                if let Some(p) = &a.player {
                    p.set_volume(gain(v) as _);
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if a.active {
            if let Some(p) = &a.player {
                pos.store(p.get_pos().as_millis() as u64, Ordering::Relaxed);
                if p.empty() {
                    a.active = false;
                    let _ = events.send(AudioEvent::Finished(a.generation));
                }
            }
        }
    }
}

// ---------- Řídicí úloha ----------

pub fn spawn(state: Arc<AppState>, rx: mpsc::UnboundedReceiver<PlayerCmd>) {
    let (audio_tx, audio_rx) = std_mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel();
    let pos = state.player.pos_ms.clone();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || audio_thread(audio_rx, ev_tx, pos))
        .expect("audio vlákno");
    tokio::spawn(manager(state, rx, audio_tx, ev_rx));
}

/// Stav rádia v řídicí úloze.
#[derive(Default)]
struct RadioCtl {
    /// Generace přehrávání, ke které rádio patří (None = rádio nehraje).
    generation: Option<u64>,
    station_id: String,
    /// Kdy zkusit rádio znovu (po chybě / výpadku streamu).
    retry_at: Option<Instant>,
}

async fn manager(
    state: Arc<AppState>,
    mut rx: mpsc::UnboundedReceiver<PlayerCmd>,
    audio: std_mpsc::Sender<AudioCmd>,
    mut events: mpsc::UnboundedReceiver<AudioEvent>,
) {
    let mut generation = 0u64;
    let mut radio_ctl = RadioCtl::default();
    let mut sync = tokio::time::interval(Duration::from_secs(5));
    let _ = audio.send(AudioCmd::SetDevice(state.settings().audio_device));

    let stop_radio = |ctl: &mut RadioCtl, audio: &std_mpsc::Sender<AudioCmd>, state: &AppState| {
        if ctl.generation.take().is_some() {
            let _ = audio.send(AudioCmd::Stop);
        }
        state.set_radio(|r| *r = None);
    };

    loop {
        if state.player.status().current.is_none() {
            let settings = state.settings();
            if let Some(entry) = pick_next(&state) {
                stop_radio(&mut radio_ctl, &audio, &state);
                start(&state, entry);
            } else if settings.idle_mode == IdleMode::Radio {
                let station = settings.radio_stations.iter().find(|s| s.id == settings.radio_station).cloned();
                match station {
                    Some(st) if radio_ctl.generation.is_some() && radio_ctl.station_id != st.id => {
                        // Admin přepnul stanici.
                        stop_radio(&mut radio_ctl, &audio, &state);
                        radio_ctl.retry_at = None;
                    }
                    Some(st) if radio_ctl.generation.is_none() && radio_ctl.retry_at.is_none_or(|t| Instant::now() >= t) => {
                        generation += 1;
                        radio_ctl.generation = Some(generation);
                        radio_ctl.station_id = st.id.clone();
                        radio_ctl.retry_at = None;
                        state.set_radio(|r| {
                            *r = Some(RadioNow { station: st.name.clone(), station_logo: st.logo.clone(), loading: true, ..Default::default() })
                        });
                        state.notify();
                        let tx = state.player.sender();
                        let g = generation;
                        let url = st.url.clone();
                        tokio::task::spawn_blocking(move || {
                            let title_tx = tx.clone();
                            let result = radio::open(&url, move |title| {
                                let _ = title_tx.send(PlayerCmd::RadioTitle { generation: g, title });
                            })
                            .map_err(|e| format!("{e:#}"));
                            let _ = tx.send(PlayerCmd::RadioReady { generation: g, result });
                        });
                    }
                    None if radio_ctl.generation.is_some() => stop_radio(&mut radio_ctl, &audio, &state),
                    _ => {}
                }
            } else if radio_ctl.generation.is_some() {
                stop_radio(&mut radio_ctl, &audio, &state);
                state.notify();
            }
        }

        tokio::select! {
            Some(cmd) = rx.recv() => match cmd {
                PlayerCmd::Wake => {}
                PlayerCmd::Skip => {
                    if let Some(id) = state.player.status().current {
                        let _ = state.db.set_entry_status(id, "skipped");
                        let _ = audio.send(AudioCmd::Stop);
                        state.player.update(|s| *s = Default::default());
                        state.notify();
                    }
                }
                PlayerCmd::Pause => {
                    if state.player.status().current.is_some() {
                        let _ = audio.send(AudioCmd::Pause);
                        state.player.update(|s| s.paused = true);
                        state.notify();
                    } else if radio_ctl.generation.is_some() {
                        let _ = audio.send(AudioCmd::Pause);
                        state.set_radio(|r| if let Some(r) = r { r.paused = true });
                        state.notify();
                    }
                }
                PlayerCmd::Resume => {
                    let _ = audio.send(AudioCmd::Resume);
                    state.player.update(|s| s.paused = false);
                    state.set_radio(|r| if let Some(r) = r { r.paused = false });
                    state.notify();
                }
                PlayerCmd::SetVolume(v) => {
                    let _ = audio.send(AudioCmd::Volume(v));
                    state.notify();
                }
                PlayerCmd::DeviceChanged => {
                    let _ = audio.send(AudioCmd::SetDevice(state.settings().audio_device));
                    if let Some(id) = state.player.status().current {
                        // Pokračovat na novém zařízení od stejného místa.
                        let resume_ms = state.player.pos_ms.load(Ordering::Relaxed);
                        if let Ok(Some(entry)) = state.db.entry(id) {
                            let st = state.clone();
                            tokio::spawn(async move {
                                let result = resolve_path(&st, entry.track.id).await.map_err(|e| format!("{e:#}"));
                                st.player.send(PlayerCmd::Resolved { entry_id: id, result, resume_ms });
                            });
                        }
                    } else if radio_ctl.generation.is_some() {
                        // Rádio se hned znovu připojí na novém zařízení.
                        radio_ctl.generation = None;
                        radio_ctl.retry_at = None;
                        state.set_radio(|r| *r = None);
                    }
                }
                PlayerCmd::Resolved { entry_id, result, resume_ms } => {
                    if state.player.status().current != Some(entry_id) {
                        continue; // mezitím přeskočeno
                    }
                    match result {
                        Ok(path) => {
                            generation += 1;
                            let paused = state.player.status().paused;
                            let _ = audio.send(AudioCmd::Play { path, generation, volume: state.settings().volume, start_ms: resume_ms });
                            if paused {
                                let _ = audio.send(AudioCmd::Pause);
                            }
                            state.player.update(|s| s.loading = false);
                            state.notify();
                            if resume_ms == 0 {
                                prefetch(&state);
                            }
                        }
                        Err(e) => fail(&state, entry_id, &e),
                    }
                }
                PlayerCmd::RadioReady { generation: g, result } => {
                    if radio_ctl.generation != Some(g) || state.player.status().current.is_some() {
                        continue; // mezitím se začala hrát fronta
                    }
                    match result {
                        Ok(decoder) => {
                            let _ = audio.send(AudioCmd::PlayRadio { decoder, generation: g, volume: state.settings().volume });
                            state.set_radio(|r| if let Some(r) = r { r.loading = false });
                        }
                        Err(e) => {
                            tracing::warn!("rádio: {e}");
                            state.toast("error", format!("{}: {e}", state.tr("Rádio nejde pustit", "Radio failed")));
                            radio_ctl.generation = None;
                            radio_ctl.retry_at = Some(Instant::now() + Duration::from_secs(30));
                            state.set_radio(|r| *r = None);
                        }
                    }
                    state.notify();
                }
                PlayerCmd::RadioTitle { generation: g, title } => {
                    if radio_ctl.generation != Some(g) {
                        continue;
                    }
                    let (artist, song) = radio::split_title(&title);
                    let changed = state.radio_now().map(|r| r.stream_title.as_deref() != Some(title.as_str())).unwrap_or(false);
                    if !changed {
                        continue;
                    }
                    state.set_radio(|r| if let Some(r) = r {
                        r.stream_title = Some(title.clone()).filter(|t| !t.trim().is_empty());
                        r.artist = artist.clone();
                        r.title = song.clone();
                        r.cover_url = None;
                    });
                    state.notify();
                    if song.is_some() {
                        // Obal k aktuální písničce z Deezeru.
                        let st = state.clone();
                        tokio::spawn(async move {
                            let url = metadata::find_cover(&st.http, &title).await;
                            st.player.send(PlayerCmd::RadioCover { title, url });
                        });
                    }
                }
                PlayerCmd::RadioCover { title, url } => {
                    state.set_radio(|r| if let Some(r) = r {
                        if r.stream_title.as_deref() == Some(title.as_str()) {
                            r.cover_url = url;
                        }
                    });
                    state.notify();
                }
            },
            Some(ev) = events.recv() => match ev {
                AudioEvent::Finished(g) | AudioEvent::Error(g, _) if radio_ctl.generation == Some(g) => {
                    // Stream rádia skončil nebo vypadl – za chvíli znovu.
                    radio_ctl.generation = None;
                    radio_ctl.retry_at = Some(Instant::now() + Duration::from_secs(5));
                    state.set_radio(|r| *r = None);
                    state.notify();
                }
                AudioEvent::Finished(g) if g == generation => {
                    if let Some(id) = state.player.status().current {
                        let _ = state.db.set_entry_status(id, "played");
                    }
                    state.player.update(|s| { s.current = None; s.loading = false; });
                    state.notify();
                }
                AudioEvent::Error(g, e) if g == generation => {
                    if let Some(id) = state.player.status().current {
                        fail(&state, id, &e);
                    }
                }
                _ => {}
            },
            _ = sync.tick() => {
                if state.player.status().current.is_some() {
                    state.notify();
                }
            }
        }
    }
}

fn fail(state: &Arc<AppState>, entry_id: i64, error: &str) {
    tracing::warn!("skladbu {entry_id} nejde přehrát: {error}");
    let title = state.db.entry(entry_id).ok().flatten().map(|e| e.track.title).unwrap_or_default();
    let _ = state.db.set_entry_status(entry_id, "failed");
    state.player.update(|s| {
        s.current = None;
        s.loading = false;
    });
    state.toast("error", format!("„{title}“ {}: {error}", state.tr("nejde přehrát", "can't be played")));
    state.notify();
}

fn pick_next(state: &Arc<AppState>) -> Option<QueueEntry> {
    if let Ok(Some(e)) = state.db.next_queued() {
        return Some(e);
    }
    if state.settings().idle_mode == IdleMode::RandomLocal {
        let track = state.db.random_local().ok().flatten()?;
        let id = state.db.enqueue(track, None, false).ok()?;
        return state.db.entry(id).ok().flatten();
    }
    None
}

fn start(state: &Arc<AppState>, entry: QueueEntry) {
    let _ = state.db.set_entry_status(entry.id, "playing");
    state.player.pos_ms.store(0, Ordering::Relaxed);
    state.player.update(|s| {
        s.current = Some(entry.id);
        s.loading = true;
    });
    state.notify();
    let st = state.clone();
    tokio::spawn(async move {
        let result = resolve_path(&st, entry.track.id).await.map_err(|e| format!("{e:#}"));
        st.player.send(PlayerCmd::Resolved { entry_id: entry.id, result, resume_ms: 0 });
    });
}

/// Předem stáhne další skladby z YouTube, aby přechod byl okamžitý.
fn prefetch(state: &Arc<AppState>) {
    let Ok(next) = state.db.upcoming(2) else { return };
    for e in next.into_iter().filter(|e| e.track.source == "youtube") {
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(err) = resolve_path(&st, e.track.id).await {
                tracing::warn!("předstažení {} selhalo: {err:#}", e.track.source_id);
            }
        });
    }
}

/// Cesta k souboru skladby (u YouTube případně stáhne).
pub async fn resolve_path(state: &Arc<AppState>, track_id: i64) -> Result<PathBuf> {
    let p = state.db.playable(track_id)?.ok_or_else(|| anyhow!("skladba neexistuje"))?;
    if let Some(f) = p.file_path.as_deref().map(PathBuf::from) {
        if f.is_file() {
            return Ok(f);
        }
    }
    match p.source.as_str() {
        "youtube" => {
            let path = state.ytdlp.download(&p.source_id, &state.config.cache_dir()).await?;
            state.db.set_track_file(track_id, path.to_str())?;
            Ok(path)
        }
        "local" => bail!("soubor už neexistuje"),
        other => bail!("neznámý zdroj {other}"),
    }
}
