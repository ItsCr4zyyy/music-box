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
    Resolved { entry_id: i64, result: Result<(PathBuf, f32), String>, resume_ms: u64 },
    /// Admin změnil výstupní zařízení – přehrát znovu na novém.
    DeviceChanged,
    RadioReady { generation: u64, result: Result<Vec<RadioDecoder>, String> },
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
    Play {
        path: PathBuf,
        generation: u64,
        volume: f32,
        start_ms: u64,
        gain_db: f32,
        /// Délka skladby (záloha, když ji dekodér nezná).
        duration_ms: u64,
        /// Prolnout se s tím, co hraje teď (0 = tvrdý střih).
        crossfade_ms: u64,
        /// Kolik ms před koncem ohlásit „blíží se konec“ (0 = neohlašovat).
        near_end_ms: u64,
    },
    /// Výstupní zařízení (prázdný seznam = výchozí zařízení systému).
    SetDevices(Vec<String>),
    /// Jeden dekodér streamu na každé zařízení.
    PlayRadio { decoders: Vec<RadioDecoder>, generation: u64, volume: f32 },
    Pause,
    Resume,
    Stop,
    Volume(f32),
}

enum AudioEvent {
    Finished(u64),
    Error(u64, String),
    /// Skladba za chvíli skončí – čas připravit další (crossfade).
    NearEnd(u64),
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

/// Normalizace: pevné zesílení skladby + limiter proti přebuzení.
fn with_gain<S>(src: S, gain_db: f32) -> Box<dyn rodio::Source + Send>
where
    S: rodio::Source + Send + 'static,
{
    use rodio::Source;
    if gain_db.abs() < 0.1 {
        return Box::new(src);
    }
    let factor = 10f32.powf(gain_db / 20.0);
    Box::new(src.amplify(factor as _).limit(rodio::source::LimitSettings::default()))
}

struct Audio {
    /// Zvolená zařízení ("" = výchozí).
    wanted: Vec<String>,
    devices: Vec<rodio::MixerDeviceSink>,
    players: Vec<rodio::Player>,
    generation: u64,
    active: bool,
    /// Hlasitost posuvníku (před křivkou).
    volume: f32,
    /// Dohrávající přehrávače předchozí skladby při crossfade.
    fading: Vec<rodio::Player>,
    fade: Option<(Instant, Duration)>,
    duration_ms: u64,
    near_end_ms: u64,
    near_end_sent: bool,
}

impl Audio {
    fn stop(&mut self) {
        for p in self.players.drain(..).chain(self.fading.drain(..)) {
            p.stop();
        }
        self.fade = None;
        self.active = false;
    }

    /// Průběh prolínání: nová skladba nahoru, stará dolů. Vrací true, dokud se prolíná.
    fn tick_fade(&mut self) -> bool {
        let Some((start, len)) = self.fade else { return false };
        let t = (start.elapsed().as_secs_f32() / len.as_secs_f32().max(0.001)).min(1.0);
        let base = gain(self.volume);
        for p in &self.players {
            p.set_volume((base * t) as _);
        }
        for p in &self.fading {
            p.set_volume((base * (1.0 - t)) as _);
        }
        if t >= 1.0 {
            for p in self.fading.drain(..) {
                p.stop();
            }
            self.fade = None;
            return false;
        }
        true
    }

    /// Otevře zařízení (pokud ještě nejsou) a pro každé vytvoří nový přehrávač.
    fn prepare(&mut self, generation: u64, volume: f32) -> Result<usize, String> {
        self.prepare_with(generation, volume, 0)
    }

    /// Jako `prepare`, ale s `crossfade_ms` > 0 nechá dosavadní přehrávače dohrát se ztlumením.
    fn prepare_with(&mut self, generation: u64, volume: f32, crossfade_ms: u64) -> Result<usize, String> {
        if crossfade_ms > 0 && !self.players.is_empty() {
            for p in self.fading.drain(..) {
                p.stop();
            }
            self.fading = std::mem::take(&mut self.players);
            self.fade = Some((Instant::now(), Duration::from_millis(crossfade_ms)));
            self.active = false;
        } else {
            self.stop();
        }
        self.generation = generation;
        self.volume = volume;
        self.near_end_sent = false;
        self.near_end_ms = 0;
        self.duration_ms = 0;
        if self.devices.is_empty() {
            let wanted = if self.wanted.is_empty() { vec![String::new()] } else { self.wanted.clone() };
            let mut errors = vec![];
            for w in &wanted {
                match open_device(w) {
                    Ok(mut d) => {
                        d.log_on_drop(false);
                        self.devices.push(d);
                    }
                    Err(e) => errors.push(e),
                }
            }
            if self.devices.is_empty() {
                return Err(errors.join("; "));
            }
        }
        let start_volume = if self.fade.is_some() { 0.0 } else { gain(volume) };
        for d in &self.devices {
            let p = rodio::Player::connect_new(d.mixer());
            p.set_volume(start_volume as _);
            self.players.push(p);
        }
        self.active = true;
        Ok(self.players.len())
    }

    #[allow(clippy::too_many_arguments)]
    fn play_files(
        &mut self,
        path: &PathBuf,
        generation: u64,
        volume: f32,
        start_ms: u64,
        gain_db: f32,
        duration_ms: u64,
        crossfade_ms: u64,
        near_end_ms: u64,
    ) -> Result<(), String> {
        use rodio::Source;
        // Dekodéry připravit dřív, než se sáhne na to, co hraje (chyba nesmí utnout předchozí skladbu).
        let n = if self.devices.is_empty() { self.wanted.len().max(1) } else { self.devices.len() };
        let mut decoders = Vec::with_capacity(n);
        for _ in 0..n {
            let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
            decoders.push(rodio::Decoder::try_from(f).map_err(|e| format!("Soubor nejde přehrát: {e}"))?);
        }
        let known = decoders.first().and_then(|d| d.total_duration()).map(|d| d.as_millis() as u64);
        let n = self.prepare_with(generation, volume, crossfade_ms)?;
        self.duration_ms = known.filter(|d| *d > 0).unwrap_or(duration_ms);
        self.near_end_ms = near_end_ms;
        // Každé zařízení má vlastní dekodér stejného souboru – spustí se současně.
        let sources: Vec<_> = decoders.into_iter().take(n).map(|d| with_gain(d, gain_db)).collect();
        for (p, src) in self.players.iter().zip(sources) {
            p.append(src);
            if start_ms > 0 {
                let _ = p.try_seek(Duration::from_millis(start_ms));
            }
        }
        Ok(())
    }

    fn for_each(&self, f: impl Fn(&rodio::Player)) {
        self.players.iter().chain(self.fading.iter()).for_each(f);
    }
}

fn audio_thread(rx: std_mpsc::Receiver<AudioCmd>, events: mpsc::UnboundedSender<AudioEvent>, pos: Arc<AtomicU64>) {
    let mut a = Audio {
        wanted: vec![],
        devices: vec![],
        players: vec![],
        generation: 0,
        active: false,
        volume: 0.8,
        fading: vec![],
        fade: None,
        duration_ms: 0,
        near_end_ms: 0,
        near_end_sent: false,
    };
    loop {
        // Při prolínání častější krok, ať je přechod plynulý.
        let wait = if a.fade.is_some() { 40 } else { 200 };
        match rx.recv_timeout(Duration::from_millis(wait)) {
            Ok(AudioCmd::Play { path, generation, volume, start_ms, gain_db, duration_ms, crossfade_ms, near_end_ms }) => {
                pos.store(start_ms, Ordering::Relaxed);
                if let Err(e) = a.play_files(&path, generation, volume, start_ms, gain_db, duration_ms, crossfade_ms, near_end_ms) {
                    a.stop();
                    let _ = events.send(AudioEvent::Error(generation, e));
                }
            }
            Ok(AudioCmd::PlayRadio { decoders, generation, volume }) => {
                pos.store(0, Ordering::Relaxed);
                match a.prepare(generation, volume) {
                    Ok(_) => {
                        for (p, dec) in a.players.iter().zip(decoders) {
                            p.append(dec);
                        }
                    }
                    Err(e) => {
                        a.stop();
                        let _ = events.send(AudioEvent::Error(generation, e));
                    }
                }
            }
            Ok(AudioCmd::Pause) => a.for_each(|p| p.pause()),
            Ok(AudioCmd::Resume) => a.for_each(|p| p.play()),
            Ok(AudioCmd::Stop) => {
                a.stop();
                pos.store(0, Ordering::Relaxed);
            }
            Ok(AudioCmd::SetDevices(list)) => {
                // Zavřít stará zařízení; nová se otevřou při dalším přehrávání.
                a.stop();
                a.devices.clear();
                a.wanted = list.into_iter().filter(|d| !d.is_empty()).collect();
            }
            Ok(AudioCmd::Volume(v)) => {
                a.volume = v;
                if a.fade.is_none() {
                    a.for_each(|p| p.set_volume(gain(v) as _));
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        }
        a.tick_fade();
        if a.active {
            if let Some(p) = a.players.first() {
                let at = p.get_pos().as_millis() as u64;
                pos.store(at, Ordering::Relaxed);
                if a.near_end_ms > 0 && !a.near_end_sent && a.duration_ms > a.near_end_ms * 2 && at + a.near_end_ms >= a.duration_ms {
                    a.near_end_sent = true;
                    let _ = events.send(AudioEvent::NearEnd(a.generation));
                }
                // Konec, až dohrají všechna zařízení.
                if a.players.iter().all(|p| p.empty()) {
                    a.active = false;
                    let _ = events.send(AudioEvent::Finished(a.generation));
                }
            }
        }
    }
}

/// Změří hlasitost souboru a vrátí zesílení v dB pro normalizaci (blokující – volat ve spawn_blocking).
/// Zjednodušené měření podle EBU R128: průměrný výkon 400ms bloků s hradlováním ticha.
pub fn measure_gain(path: &std::path::Path) -> Result<f32> {
    use rodio::Source;
    const TARGET_DB: f32 = -16.0; // cílová hlasitost (průměr RMS v dBFS)
    let dec = rodio::Decoder::try_from(std::fs::File::open(path)?).map_err(|e| anyhow!("{e}"))?;
    let block_len = ((dec.sample_rate().get() as usize * dec.channels().get() as usize) * 2 / 5).max(1); // 400 ms
    let mut blocks: Vec<f64> = Vec::new();
    let (mut acc, mut n, mut peak) = (0f64, 0usize, 0f32);
    for s in dec {
        let v = s as f32;
        peak = peak.max(v.abs());
        acc += (v as f64) * (v as f64);
        n += 1;
        if n == block_len {
            blocks.push(acc / n as f64);
            acc = 0.0;
            n = 0;
        }
    }
    let to_db = |p: f64| 10.0 * p.max(1e-12).log10();
    // Absolutní hradlo -70 dB, pak relativní -10 dB pod průměrem.
    let loud: Vec<f64> = blocks.into_iter().filter(|p| to_db(*p) > -70.0).collect();
    if loud.is_empty() {
        return Ok(0.0);
    }
    let mean = loud.iter().sum::<f64>() / loud.len() as f64;
    let gated: Vec<f64> = loud.iter().copied().filter(|p| to_db(*p) > to_db(mean) - 10.0).collect();
    let level = to_db(gated.iter().sum::<f64>() / gated.len().max(1) as f64) as f32;
    let peak_db = 20.0 * peak.max(1e-6).log10();
    // Zesílení omezené rozsahem a špičkou (limiter zachytí zbytek).
    let g = (TARGET_DB - level).clamp(-15.0, 10.0).min(-peak_db + 3.0);
    Ok((g * 10.0).round() / 10.0)
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
    // Crossfade: generace předchozí skladby, která ještě dohrává, zatímco se připravuje další.
    let mut handover: Option<u64> = None;
    let mut applied_volume = state.settings().effective_volume();
    let mut sync = tokio::time::interval(Duration::from_secs(5));
    let _ = audio.send(AudioCmd::SetDevices(state.settings().output_devices()));

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
                        let outputs = settings.output_devices().len().max(1);
                        tokio::task::spawn_blocking(move || {
                            // Jeden stream na každé výstupní zařízení; název skladby čteme z prvního.
                            let mut decoders = Vec::with_capacity(outputs);
                            let mut result = Ok(());
                            for i in 0..outputs {
                                let title_tx = tx.clone();
                                match radio::open(&url, move |title| {
                                    if i == 0 {
                                        let _ = title_tx.send(PlayerCmd::RadioTitle { generation: g, title });
                                    }
                                }) {
                                    Ok(d) => decoders.push(d),
                                    Err(e) => {
                                        result = Err(format!("{e:#}"));
                                        break;
                                    }
                                }
                            }
                            let result = result.map(|_| decoders);
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
                PlayerCmd::Wake => {
                    // Fronta se změnila – další písničky stáhnout/změřit hned, ne až na konci té aktuální.
                    if state.player.status().current.is_some() {
                        prefetch(&state);
                    }
                }
                PlayerCmd::Skip => {
                    handover = None;
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
                PlayerCmd::SetVolume(_) => {
                    // Zóna hlasitosti má přednost před posuvníkem.
                    applied_volume = state.settings().effective_volume();
                    let _ = audio.send(AudioCmd::Volume(applied_volume));
                    state.notify();
                }
                PlayerCmd::DeviceChanged => {
                    let _ = audio.send(AudioCmd::SetDevices(state.settings().output_devices()));
                    if let Some(id) = state.player.status().current {
                        // Pokračovat na novém zařízení od stejného místa.
                        let resume_ms = state.player.pos_ms.load(Ordering::Relaxed);
                        if let Ok(Some(entry)) = state.db.entry(id) {
                            let st = state.clone();
                            tokio::spawn(async move {
                                let result = prepare_track(&st, entry.track.id).await.map_err(|e| format!("{e:#}"));
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
                        Ok((path, gain_db)) => {
                            generation += 1;
                            let settings = state.settings();
                            let paused = state.player.status().paused;
                            let xf_ms = settings.crossfade_sec as u64 * 1000;
                            // Prolnout jen při přechodu z dohrávající skladby (ne po přeskočení / pauze).
                            let crossfade_ms = if handover.take().is_some() && !paused { xf_ms } else { 0 };
                            let duration_ms = state.db.entry(entry_id).ok().flatten().map(|e| e.track.duration_sec as u64 * 1000).unwrap_or(0);
                            applied_volume = settings.effective_volume();
                            let _ = audio.send(AudioCmd::Play {
                                path,
                                generation,
                                volume: applied_volume,
                                start_ms: resume_ms,
                                gain_db,
                                duration_ms,
                                crossfade_ms,
                                near_end_ms: xf_ms,
                            });
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
                        Ok(decoders) => {
                            applied_volume = state.settings().effective_volume();
                            let _ = audio.send(AudioCmd::PlayRadio { decoders, generation: g, volume: applied_volume });
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
                AudioEvent::NearEnd(g) if g == generation && handover.is_none() => {
                    // Crossfade: předchozí skladba dohrává, další se připraví a prolne.
                    let st = state.player.status();
                    if let (Some(id), false) = (st.current, st.paused) {
                        if let Some(next) = pick_next(&state) {
                            let _ = state.db.set_entry_status(id, "played");
                            handover = Some(g);
                            start(&state, next);
                        }
                    }
                }
                AudioEvent::Finished(g) | AudioEvent::Error(g, _) if handover == Some(g) => {
                    // Předchozí skladba dohrála dřív, než byla další připravená – prostě navážeme bez prolnutí.
                    handover = None;
                }
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
                // Zóny hlasitosti podle času.
                let v = state.settings().effective_volume();
                if (v - applied_volume).abs() > 0.001 {
                    applied_volume = v;
                    let _ = audio.send(AudioCmd::Volume(v));
                    state.notify();
                } else if state.player.status().current.is_some() {
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
    if let Ok(Some(e)) = state.db.next_queued(state.settings().fair_queue) {
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
        let result = prepare_track(&st, entry.track.id).await.map_err(|e| format!("{e:#}"));
        st.player.send(PlayerCmd::Resolved { entry_id: entry.id, result, resume_ms: 0 });
    });
}

/// Předem stáhne další skladby z YouTube, aby přechod byl okamžitý.
fn prefetch(state: &Arc<AppState>) {
    let Ok(next) = state.db.upcoming(2, state.settings().fair_queue) else { return };
    // YouTube předem stáhnout; s normalizací předem změřit i lokální soubory.
    let normalize = state.settings().normalize_volume;
    for e in next.into_iter().filter(|e| e.track.source == "youtube" || normalize) {
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(err) = prepare_track(&st, e.track.id).await {
                tracing::warn!("předstažení {} selhalo: {err:#}", e.track.source_id);
            }
        });
    }
}

/// Soubor skladby + zesílení pro normalizaci (0 dB, když je normalizace vypnutá).
/// Hlasitost se měří jen jednou a ukládá se do databáze.
pub async fn prepare_track(state: &Arc<AppState>, track_id: i64) -> Result<(PathBuf, f32)> {
    let path = resolve_path(state, track_id).await?;
    if !state.settings().normalize_volume {
        return Ok((path, 0.0));
    }
    if let Some(g) = state.db.track_gain(track_id)? {
        return Ok((path, g));
    }
    let p = path.clone();
    match tokio::task::spawn_blocking(move || measure_gain(&p)).await? {
        Ok(g) => {
            state.db.set_track_gain(track_id, g)?;
            tracing::debug!("normalizace skladby {track_id}: {g:+.1} dB");
            Ok((path, g))
        }
        Err(e) => {
            tracing::warn!("hlasitost skladby {track_id} nejde změřit: {e:#}");
            Ok((path, 0.0))
        }
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
