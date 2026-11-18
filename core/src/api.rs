use crate::auth::{self, require, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::models::*;
use crate::player::PlayerCmd;
use crate::sources::{self, local, metadata, spotify};
use crate::state::{AppState, ServerEvent};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use futures_util::{stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

type St = State<Arc<AppState>>;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/info", get(info))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/me", get(me).patch(update_me))
        .route("/me/password", post(change_password))
        .route("/state", get(state))
        .route("/ws", get(ws))
        .route("/qr.svg", get(qr))
        .route("/tracks/{id}/artwork", get(artwork))
        .route("/search", get(search))
        .route("/library", get(library))
        .route("/queue", post(add_to_queue).delete(clear_queue))
        .route("/queue/{id}", delete(remove_from_queue))
        .route("/queue/{id}/promote", post(promote))
        .route("/queue/{id}/move", post(move_entry))
        .route("/player/skip", post(skip))
        .route("/player/pause", post(pause))
        .route("/player/resume", post(resume))
        .route("/player/volume", post(volume))
        .route("/admin/users", get(users))
        .route("/admin/users/{id}", patch(update_user).delete(delete_user))
        .route("/admin/settings", get(get_settings).put(put_settings))
        .route("/admin/status", get(admin_status))
        .route("/admin/library/rescan", post(rescan))
        .route("/admin/ytdlp/update", post(update_ytdlp))
        .route("/admin/identify", get(identify_search).post(identify_apply))
        .route("/admin/radio/search", get(radio_search))
        .route("/admin/audio/devices", get(audio_devices))
        .route("/admin/identify/reset", post(identify_reset))
        .route("/covers/{name}", get(cover))
        .route("/logo", get(logo))
        .route("/theme.css", get(theme_css))
        .route("/themes", get(themes_list))
        .route("/themes/{id}/theme.json", get(theme_export))
        .route("/themes/{id}/files/{name}", get(theme_file))
        .route(
            "/admin/themes",
            post(theme_upload).layer(axum::extract::DefaultBodyLimit::max(30 * 1024 * 1024)),
        )
        .route("/admin/themes/{id}", delete(theme_delete))
        .route("/avatars/{name}", get(avatar))
        .route(
            "/me/avatar",
            post(upload_avatar).delete(delete_my_avatar).layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/admin/users/{id}/avatar", delete(delete_user_avatar))
        .route(
            "/admin/logo",
            post(upload_logo).delete(delete_logo).layer(axum::extract::DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
}

// ---------- Veřejné ----------

async fn info(State(st): St) -> ApiResult<Json<Value>> {
    let s = st.settings();
    Ok(Json(json!({
        "needs_setup": st.db.user_count()? == 0,
        "allow_registration": s.allow_registration,
        "version": env!("CARGO_PKG_VERSION"),
        "box_name": s.box_name,
        "dj_sources": s.dj_sources,
        "language": s.language,
    })))
}

async fn qr(State(st): St) -> ApiResult<Response> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(st.join_url().as_bytes()).map_err(|e| anyhow::anyhow!("QR: {e}"))?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .dark_color(svg::Color("#1b1036"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "no-cache")], svg).into_response())
}

async fn artwork(State(st): St, Path(id): Path<i64>) -> ApiResult<Response> {
    let p = st.db.playable(id)?.ok_or_else(|| ApiError::not_found("Skladba neexistuje"))?;
    let path = p.file_path.filter(|_| p.source == "local").ok_or_else(|| ApiError::not_found("Bez obalu"))?;
    let pic = tokio::task::spawn_blocking(move || local::picture(std::path::Path::new(&path)))
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(|| ApiError::not_found("Bez obalu"))?;
    Ok(([(header::CONTENT_TYPE, pic.0), (header::CACHE_CONTROL, "public, max-age=86400".into())], pic.1).into_response())
}

// ---------- Přihlášení ----------

#[derive(Deserialize)]
struct RegisterReq {
    username: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

fn valid_username(u: &str) -> bool {
    (3..=32).contains(&u.chars().count()) && u.chars().all(|c| c.is_alphanumeric() || "_.-".contains(c))
}

fn with_session(user: &User, token: &str) -> Response {
    ([(header::SET_COOKIE, auth::session_cookie(token))], Json(json!({ "user": user }))).into_response()
}

async fn register(State(st): St, Json(req): Json<RegisterReq>) -> ApiResult<Response> {
    let username = req.username.trim().to_string();
    if !valid_username(&username) {
        return Err(ApiError::bad_request("Jméno musí mít 3–32 znaků (písmena, čísla, _ . -)"));
    }
    if req.password.chars().count() < 6 {
        return Err(ApiError::bad_request("Heslo musí mít aspoň 6 znaků"));
    }
    let display = match req.display_name.trim() {
        "" => username.clone(),
        d => d.chars().take(40).collect(),
    };
    let first = st.db.user_count()? == 0;
    if !first && !st.settings().allow_registration {
        return Err(ApiError::forbidden("Registrace je vypnutá"));
    }
    if st.db.user_by_name(&username)?.is_some() {
        return Err(ApiError::conflict("Tohle jméno už někdo má"));
    }
    let hash = auth::hash_password(req.password).await?;
    // První účet = administrátor (prvotní nastavení), ostatní čekají na schválení.
    let (role, status) = if first { (Role::Admin, UserStatus::Active) } else { (Role::Guest, UserStatus::Pending) };
    let user = st.db.create_user(&username, &display, &hash, role, status)?;
    let token = auth::new_token();
    st.db.create_session(&token, user.id, auth::SESSION_DAYS)?;
    let _ = st.events.send(ServerEvent::Users);
    Ok(with_session(&user, &token))
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(State(st): St, Json(req): Json<LoginReq>) -> ApiResult<Response> {
    let bad = || ApiError::new(StatusCode::UNAUTHORIZED, "Špatné jméno nebo heslo");
    let (user, hash) = st.db.user_by_name(req.username.trim())?.ok_or_else(bad)?;
    if !auth::verify_password(req.password, hash).await {
        return Err(bad());
    }
    if user.status == UserStatus::Banned {
        return Err(ApiError::forbidden("Účet je zablokovaný"));
    }
    let token = auth::new_token();
    st.db.create_session(&token, user.id, auth::SESSION_DAYS)?;
    Ok(with_session(&user, &token))
}

async fn logout(State(st): St, headers: axum::http::HeaderMap) -> ApiResult<Response> {
    if let Some(t) = auth::token_from(&headers) {
        st.db.delete_session(&t)?;
    }
    Ok(([(header::SET_COOKIE, auth::clear_cookie())], Json(json!({ "ok": true }))).into_response())
}

async fn me(AuthUser(u): AuthUser) -> Json<Value> {
    Json(json!({ "user": u }))
}

#[derive(Deserialize)]
struct PasswordReq {
    old: String,
    new: String,
}

async fn change_password(State(st): St, AuthUser(u): AuthUser, Json(req): Json<PasswordReq>) -> ApiResult<Json<Value>> {
    let (_, hash) = st.db.user_by_name(&u.username)?.ok_or_else(ApiError::unauthorized)?;
    if !auth::verify_password(req.old, hash).await {
        return Err(ApiError::bad_request("Původní heslo nesedí"));
    }
    if req.new.chars().count() < 6 {
        return Err(ApiError::bad_request("Heslo musí mít aspoň 6 znaků"));
    }
    st.db.set_password(u.id, &auth::hash_password(req.new).await?)?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- Stav ----------

/// Stav (co hraje + fronta) je veřejný – vidí ho každý, kdo otevře web.
async fn state(State(st): St) -> ApiResult<Json<StateSnapshot>> {
    Ok(Json(st.snapshot()?))
}

async fn ws(State(st): St, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| ws_loop(st, socket))
}

fn state_message(st: &AppState) -> Option<Message> {
    let snap = st.snapshot().ok()?;
    Some(Message::Text(json!({ "type": "state", "state": snap }).to_string().into()))
}

async fn ws_loop(st: Arc<AppState>, mut socket: WebSocket) {
    let mut rx = st.subscribe();
    if let Some(m) = state_message(&st) {
        if socket.send(m).await.is_err() {
            return;
        }
    }
    loop {
        tokio::select! {
            ev = rx.recv() => {
                let msg = match ev {
                    Ok(ServerEvent::State) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => state_message(&st),
                    Ok(other) => serde_json::to_string(&other).ok().map(|s| Message::Text(s.into())),
                    Err(_) => break,
                };
                if let Some(m) = msg {
                    if socket.send(m).await.is_err() { break; }
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
}

// ---------- Hledání a fronta ----------

#[derive(Deserialize)]
struct SearchQ {
    q: String,
}

fn store_remote(st: &AppState, r: &RemoteTrack) -> rusqlite::Result<Track> {
    st.db.upsert_track("youtube", &r.video_id, &r.title, &r.artist, r.album.as_deref(), r.duration_sec, r.artwork_url.as_deref(), None)
}

async fn search_youtube(st: &AppState, q: &str) -> Vec<RemoteTrack> {
    match st.ytm.search_songs(q).await {
        Ok(v) if !v.is_empty() => v,
        other => {
            if let Err(e) = other {
                tracing::warn!("YouTube Music hledání selhalo, zkouším yt-dlp: {e:#}");
            }
            st.ytdlp.search(q, 10).await.unwrap_or_else(|e| {
                tracing::warn!("yt-dlp hledání selhalo: {e:#}");
                vec![]
            })
        }
    }
}

async fn search(State(st): St, AuthUser(u): AuthUser, Query(q): Query<SearchQ>) -> ApiResult<Json<Value>> {
    require(&u, Role::Dj)?;
    let q = q.q.trim();
    if q.chars().count() < 2 {
        return Ok(Json(json!({ "local": [], "youtube": [] })));
    }
    let allowed = Allowed::for_user(&st, &u);
    let local = if allowed.local { st.db.search_local(q, 20)? } else { vec![] };
    let youtube: Vec<Track> = if allowed.youtube {
        search_youtube(&st, q).await.iter().take(15).filter_map(|r| store_remote(&st, r).ok()).collect()
    } else {
        vec![]
    };
    Ok(Json(json!({ "local": local, "youtube": youtube })))
}

async fn library(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Library>> {
    require(&u, Role::Dj)?;
    if !Allowed::for_user(&st, &u).local {
        return Err(ApiError::forbidden("Lokální knihovnu teď DJové používat nesmí"));
    }
    Ok(Json(st.db.library()?))
}

/// Odkud smí uživatel přidávat (admin vždy všechno, DJ podle nastavení).
struct Allowed {
    youtube: bool,
    links: bool,
    local: bool,
}

impl Allowed {
    fn for_user(st: &AppState, u: &User) -> Allowed {
        if u.role == Role::Admin {
            return Allowed { youtube: true, links: true, local: true };
        }
        let d = st.settings().dj_sources;
        Allowed { youtube: d.youtube, links: d.links, local: d.local }
    }

    fn check_track(&self, t: &Track) -> ApiResult<()> {
        let ok = match t.source.as_str() {
            "local" => self.local,
            // Skladba z YouTube mohla přijít z hledání i z odkazu.
            _ => self.youtube || self.links,
        };
        if ok {
            Ok(())
        } else if t.source == "local" {
            Err(ApiError::forbidden("Lokální knihovnu teď DJové používat nesmí"))
        } else {
            Err(ApiError::forbidden("Písničky z YouTube teď DJové přidávat nesmí"))
        }
    }
}

/// Najde Spotify skladbu na YouTube Music (podle názvu, interpreta a délky).
async fn match_on_youtube(st: &AppState, t: &spotify::SpotifyTrack) -> Option<Track> {
    let q = format!("{} {}", t.artists, t.title);
    let results = search_youtube(st, &q).await;
    let best = results
        .iter()
        .take(5)
        .min_by_key(|r| if t.duration_sec == 0 || r.duration_sec == 0 { 30 } else { (r.duration_sec as i64 - t.duration_sec as i64).abs() })?;
    store_remote(st, best).ok()
}

async fn resolve_url(st: &Arc<AppState>, url: &str, admin: bool) -> ApiResult<Vec<Track>> {
    const MAX: usize = 200;
    if let Some(id) = sources::youtube_id(url) {
        let r = st.ytdlp.info(&id).await?;
        return Ok(vec![store_remote(st, &r)?]);
    }
    if sources::is_youtube_playlist(url) {
        if !admin {
            return Err(ApiError::forbidden("Celé playlisty může přidat jen administrátor"));
        }
        let list = st.ytdlp.playlist(url).await?;
        return Ok(list.iter().take(MAX).filter_map(|r| store_remote(st, r).ok()).collect());
    }
    if let Some((kind, id)) = spotify::parse_url(url) {
        if kind != "track" && !admin {
            return Err(ApiError::forbidden("Celá alba a playlisty může přidat jen administrátor"));
        }
        let (_, list) = spotify::fetch(&st.http, &kind, &id).await?;
        let found: Vec<Track> = stream::iter(list.into_iter().take(MAX))
            .map(|t| {
                let st = st.clone();
                async move { match_on_youtube(&st, &t).await }
            })
            .buffered(6)
            .filter_map(|x| async move { x })
            .collect()
            .await;
        if found.is_empty() {
            return Err(ApiError::not_found("Na YouTube se nic nenašlo"));
        }
        return Ok(found);
    }
    Err(ApiError::bad_request("Odkaz nepoznávám – podporuji YouTube, YouTube Music a Spotify"))
}

#[derive(Deserialize)]
struct AddReq {
    track_id: Option<i64>,
    /// Celé album z lokální knihovny (klíč složky).
    album: Option<String>,
    url: Option<String>,
    /// Přednost – zařadit hned na začátek (jen admin).
    #[serde(default)]
    next: bool,
}

async fn add_to_queue(State(st): St, AuthUser(u): AuthUser, Json(req): Json<AddReq>) -> ApiResult<Json<Value>> {
    require(&u, Role::Dj)?;
    let admin = u.role == Role::Admin;
    if req.next && !admin {
        return Err(ApiError::forbidden("Přednost ve frontě může dát jen administrátor"));
    }
    let url = req.url.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let allowed = Allowed::for_user(&st, &u);
    if url.is_some() && !allowed.links {
        return Err(ApiError::forbidden("Odkazy teď DJové přidávat nesmí"));
    }
    let tracks = if let Some(id) = req.track_id {
        vec![st.db.track(id)?.ok_or_else(|| ApiError::not_found("Skladba neexistuje"))?]
    } else if let Some(key) = &req.album {
        let t = st.db.collection_tracks(key)?;
        if t.is_empty() {
            return Err(ApiError::not_found("Album je prázdné"));
        }
        t
    } else if let Some(url) = url {
        resolve_url(&st, url, admin).await?
    } else {
        return Err(ApiError::bad_request("Chybí skladba"));
    };
    for t in &tracks {
        allowed.check_track(t)?;
    }
    let single = tracks.len() == 1;
    let s = st.settings();
    let mut to_add = Vec::new();
    for t in tracks {
        if s.prevent_duplicates && st.db.is_track_queued(t.id)? {
            continue;
        }
        if !admin && s.max_track_minutes > 0 && t.duration_sec > s.max_track_minutes * 60 {
            if single {
                return Err(ApiError::bad_request(format!("Skladba je delší než {} minut", s.max_track_minutes)));
            }
            continue;
        }
        to_add.push(t);
    }
    if to_add.is_empty() {
        return Err(ApiError::conflict("Tahle písnička už ve frontě je"));
    }
    if !admin && s.max_queued_per_user > 0 {
        let have = st.db.queued_count_by(u.id)?;
        if have + to_add.len() as i64 > s.max_queued_per_user as i64 {
            return Err(ApiError::forbidden(format!(
                "Ve frontě můžeš mít najednou max. {} písniček – počkej, až se nějaká přehraje",
                s.max_queued_per_user
            )));
        }
    }
    // Přednostní skladby se řadí za ostatní přednostní ve stejném pořadí.
    for t in &to_add {
        st.db.enqueue(t.id, Some(u.id), req.next)?;
    }
    st.notify();
    st.player.send(PlayerCmd::Wake);
    Ok(Json(json!({ "added": to_add.len(), "tracks": to_add })))
}

async fn remove_from_queue(State(st): St, AuthUser(u): AuthUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    require(&u, Role::Dj)?;
    let owner = st.db.entry_owner(id)?.ok_or_else(|| ApiError::not_found("Položka neexistuje"))?;
    if u.role != Role::Admin && owner != Some(u.id) {
        return Err(ApiError::forbidden("Můžeš odebrat jen svoje písničky"));
    }
    match st.db.entry_status(id)?.as_deref() {
        Some("playing") => st.player.send(PlayerCmd::Skip),
        Some("queued") => {
            st.db.set_entry_status(id, "removed")?;
            st.notify();
        }
        _ => {}
    }
    Ok(Json(json!({ "ok": true })))
}

async fn clear_queue(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    st.db.clear_queue()?;
    st.notify();
    Ok(Json(json!({ "ok": true })))
}

async fn promote(State(st): St, AuthUser(u): AuthUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    st.db.promote(id)?;
    st.notify();
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct MoveReq {
    /// -1 = nahoru, 1 = dolů
    dir: i32,
}

async fn move_entry(State(st): St, AuthUser(u): AuthUser, Path(id): Path<i64>, Json(req): Json<MoveReq>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    st.db.shift(id, req.dir.signum())?;
    st.notify();
    Ok(Json(json!({ "ok": true })))
}

// ---------- Přehrávač ----------

async fn skip(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Dj)?;
    let current = st.player.status().current;
    if u.role != Role::Admin {
        let own = match current {
            Some(id) => st.db.entry_owner(id)?.flatten() == Some(u.id),
            None => false,
        };
        if !own {
            return Err(ApiError::forbidden("Přeskočit můžeš jen svoji písničku"));
        }
    }
    st.player.send(PlayerCmd::Skip);
    Ok(Json(json!({ "ok": true })))
}

async fn pause(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    st.player.send(PlayerCmd::Pause);
    Ok(Json(json!({ "ok": true })))
}

async fn resume(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    st.player.send(PlayerCmd::Resume);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct VolumeReq {
    volume: f32,
}

async fn volume(State(st): St, AuthUser(u): AuthUser, Json(req): Json<VolumeReq>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let v = req.volume.clamp(0.0, 1.0);
    let mut s = st.settings();
    s.volume = v;
    st.set_settings(s)?;
    st.player.send(PlayerCmd::SetVolume(v));
    Ok(Json(json!({ "volume": v })))
}

// ---------- Administrace ----------

async fn users(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Vec<User>>> {
    require(&u, Role::Admin)?;
    Ok(Json(st.db.users()?))
}

#[derive(Deserialize)]
struct UserPatch {
    role: Option<Role>,
    status: Option<UserStatus>,
    /// Nové heslo (reset administrátorem).
    password: Option<String>,
}

async fn update_user(State(st): St, AuthUser(u): AuthUser, Path(id): Path<i64>, Json(req): Json<UserPatch>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let target = st.db.user(id)?.ok_or_else(|| ApiError::not_found("Uživatel neexistuje"))?;
    let demotes_admin = target.role == Role::Admin
        && target.status == UserStatus::Active
        && (req.role.is_some_and(|r| r != Role::Admin) || req.status.is_some_and(|s| s != UserStatus::Active));
    if demotes_admin && st.db.active_admin_count()? <= 1 {
        return Err(ApiError::forbidden("Musí zůstat aspoň jeden administrátor"));
    }
    if let Some(p) = req.password {
        if p.chars().count() < 6 {
            return Err(ApiError::bad_request("Heslo musí mít aspoň 6 znaků"));
        }
        st.db.set_password(id, &auth::hash_password(p).await?)?;
    }
    st.db.update_user(id, req.role, req.status)?;
    let _ = st.events.send(ServerEvent::Users);
    Ok(Json(json!({ "user": st.db.user(id)? })))
}

async fn delete_user(State(st): St, AuthUser(u): AuthUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    if id == u.id {
        return Err(ApiError::forbidden("Sám sebe smazat nemůžeš"));
    }
    st.db.delete_user(id)?;
    let _ = st.events.send(ServerEvent::Users);
    st.notify();
    Ok(Json(json!({ "ok": true })))
}

async fn get_settings(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Settings>> {
    require(&u, Role::Admin)?;
    Ok(Json(st.settings()))
}

async fn put_settings(State(st): St, AuthUser(u): AuthUser, Json(mut s): Json<Settings>) -> ApiResult<Json<Settings>> {
    require(&u, Role::Admin)?;
    s.volume = s.volume.clamp(0.0, 1.0);
    s.box_name = s.box_name.trim().chars().take(40).collect();
    if s.box_name.is_empty() {
        s.box_name = "music-box".into();
    }
    s.display.qr_caption = s.display.qr_caption.trim().chars().take(80).collect();
    s.display.queue_length = s.display.queue_length.min(15);
    s.crossfade_sec = s.crossfade_sec.min(15);
    for z in &mut s.volume_zones {
        z.name = z.name.trim().chars().take(40).collect();
        z.volume = z.volume.clamp(0.0, 1.0);
        if crate::models::parse_hhmm(&z.from).is_none() || crate::models::parse_hhmm(&z.to).is_none() {
            return Err(ApiError::bad_request("Čas zóny musí být ve tvaru HH:MM"));
        }
    }
    // Verzi loga mění jen nahrání loga (formulář může mít starou hodnotu).
    s.display.logo_rev = st.settings().display.logo_rev;
    if st.themes.get(&s.theme).is_none() {
        s.theme = crate::themes::DEFAULT_THEME.into();
    }
    if !matches!(s.language.as_str(), "cs" | "en") {
        s.language = "cs".into();
    }
    for r in &mut s.radio_stations {
        r.name = r.name.trim().chars().take(60).collect();
        r.url = r.url.trim().to_string();
        if !(r.url.starts_with("http://") || r.url.starts_with("https://")) {
            return Err(ApiError::bad_request("Neplatná adresa rádia"));
        }
        if r.id.trim().is_empty() {
            r.id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        }
    }
    s.local_folders = s.local_folders.iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
    for f in &s.local_folders {
        if !std::path::Path::new(f).is_dir() {
            return Err(ApiError::bad_request(format!("Složka „{f}“ neexistuje")));
        }
    }
    let folders_changed = s.local_folders != st.settings().local_folders;
    let device_changed = s.output_devices() != st.settings().output_devices();
    let volume = s.volume;
    st.set_settings(s)?;
    st.player.send(PlayerCmd::SetVolume(volume));
    st.player.send(PlayerCmd::Wake);
    if device_changed {
        st.player.send(PlayerCmd::DeviceChanged);
    }
    if folders_changed {
        let st2 = st.clone();
        tokio::spawn(async move {
            match st2.rescan_library().await {
                Ok(n) => st2.toast("info", format!("{}: {n}", st2.tr("Knihovna načtena, skladeb", "Library loaded, songs"))),
                Err(e) => st2.toast("error", format!("{}: {e:#}", st2.tr("Skenování knihovny selhalo", "Library scan failed"))),
            }
        });
    }
    Ok(Json(st.settings()))
}

async fn admin_status(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let ytdlp = st.ytdlp.version().await.map_err(|e| format!("{e:#}"));
    Ok(Json(json!({
        "ytdlp_version": ytdlp.as_ref().ok(),
        "ytdlp_error": ytdlp.as_ref().err(),
        "ffmpeg": st.ytdlp.has_ffmpeg(),
        "local_tracks": st.db.local_count()?,
        "join_url": st.join_url(),
        "data_dir": st.config.data_dir.display().to_string(),
        "music_dir": st.config.music_dir.display().to_string(),
    })))
}

async fn rescan(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let n = st.rescan_library().await?;
    Ok(Json(json!({ "count": n })))
}

async fn update_ytdlp(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let out = st.ytdlp.update().await?;
    Ok(Json(json!({ "output": out })))
}

// ---------- Identifikace (obaly a metadata z MusicBrainz / Deezeru) ----------

#[derive(Deserialize)]
struct IdentifyQ {
    /// "album" | "track"
    kind: String,
    q: String,
}

async fn identify_search(State(st): St, AuthUser(u): AuthUser, Query(q): Query<IdentifyQ>) -> ApiResult<Json<Vec<metadata::Candidate>>> {
    require(&u, Role::Admin)?;
    let query = q.q.trim();
    if query.is_empty() {
        return Ok(Json(vec![]));
    }
    let list = match q.kind.as_str() {
        "album" => metadata::search_albums(&st.http, query).await?,
        _ => metadata::search_tracks(&st.http, query).await?,
    };
    Ok(Json(list))
}

#[derive(Deserialize)]
struct IdentifyApply {
    kind: String,
    /// Album: klíč složky. Skladba: ID skladby.
    target: String,
    candidate: metadata::Candidate,
    /// U alba přepsat i názvy skladeb podle pořadí.
    #[serde(default)]
    rename_tracks: bool,
}

/// Stáhne obal do `<data>/covers` a vrátí jeho URL.
async fn store_cover(st: &AppState, url: &str) -> anyhow::Result<String> {
    let (data, ext) = metadata::download_image(&st.http, url).await?;
    let dir = st.config.data_dir.join("covers");
    tokio::fs::create_dir_all(&dir).await?;
    let name = format!("{}.{ext}", uuid::Uuid::new_v4().simple());
    tokio::fs::write(dir.join(&name), data).await?;
    Ok(format!("/api/covers/{name}"))
}

fn target_tracks(st: &AppState, kind: &str, target: &str) -> ApiResult<Vec<Track>> {
    let tracks = if kind == "album" {
        st.db.collection_tracks(target)?
    } else {
        let id: i64 = target.parse().map_err(|_| ApiError::bad_request("Neplatná skladba"))?;
        st.db.track(id)?.into_iter().collect()
    };
    if tracks.is_empty() {
        return Err(ApiError::not_found("Nic k identifikaci"));
    }
    Ok(tracks)
}

async fn identify_apply(State(st): St, AuthUser(u): AuthUser, Json(req): Json<IdentifyApply>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let tracks = target_tracks(&st, &req.kind, &req.target)?;
    let c = &req.candidate;
    let mut warning = None;
    let cover = match &c.cover_url {
        Some(url) => match store_cover(&st, url).await {
            Ok(u) => Some(u),
            Err(e) => {
                warning = Some(format!("Obal se nepodařilo stáhnout: {e:#}"));
                None
            }
        },
        None => None,
    };
    let artist = Some(c.artist.as_str()).filter(|a| !a.is_empty());
    if req.kind == "album" {
        let online = if req.rename_tracks {
            metadata::album_tracks(&st.http, &c.source, &c.id).await.unwrap_or_else(|e| {
                warning = Some(format!("Názvy skladeb se nepodařilo načíst: {e:#}"));
                vec![]
            })
        } else {
            vec![]
        };
        for (i, t) in tracks.iter().enumerate() {
            let renamed = online.get(i);
            st.db.apply_identity(
                t.id,
                renamed.map(|r| r.title.as_str()),
                renamed.map(|r| r.artist.as_str()).filter(|a| !a.is_empty()).or(artist),
                Some(&c.title),
                cover.as_deref(),
            )?;
        }
    } else {
        st.db.apply_identity(tracks[0].id, Some(&c.title), artist, c.album.as_deref(), cover.as_deref())?;
    }
    st.notify();
    Ok(Json(json!({ "updated": tracks.len(), "cover": cover.is_some(), "warning": warning })))
}

#[derive(Deserialize)]
struct IdentifyReset {
    kind: String,
    target: String,
}

async fn identify_reset(State(st): St, AuthUser(u): AuthUser, Json(req): Json<IdentifyReset>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let tracks = target_tracks(&st, &req.kind, &req.target)?;
    st.db.unlock_tracks(&tracks.iter().map(|t| t.id).collect::<Vec<_>>())?;
    st.rescan_library().await?;
    st.notify();
    Ok(Json(json!({ "ok": true })))
}

async fn cover(State(st): St, Path(name): Path<String>) -> ApiResult<Response> {
    // Jen naše vygenerované názvy (žádné „../“).
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') || name.starts_with('.') {
        return Err(ApiError::not_found("Obal neexistuje"));
    }
    let path = st.config.data_dir.join("covers").join(&name);
    let data = tokio::fs::read(&path).await.map_err(|_| ApiError::not_found("Obal neexistuje"))?;
    let ext = name.rsplit('.').next().unwrap_or("jpg");
    Ok(([(header::CONTENT_TYPE, metadata::mime_for_ext(ext)), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], data)
        .into_response())
}

// ---------- Logo ----------

const DEFAULT_LOGO: &str = include_str!("../assets/logo.svg");

fn logo_file(st: &AppState) -> Option<std::path::PathBuf> {
    ["png", "jpg", "webp", "svg", "gif"].iter().map(|e| st.config.data_dir.join(format!("logo.{e}"))).find(|p| p.is_file())
}

async fn logo(State(st): St) -> Response {
    if let Some(p) = logo_file(&st) {
        if let Ok(data) = tokio::fs::read(&p).await {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("png");
            return uploaded_file(metadata::mime_for_ext(ext), "no-cache", data);
        }
    }
    ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "no-cache")], DEFAULT_LOGO).into_response()
}

fn bump_logo(st: &AppState) -> anyhow::Result<()> {
    let mut s = st.settings();
    s.display.logo_rev = crate::util::now_ms();
    st.set_settings(s)
}

async fn upload_logo(State(st): St, AuthUser(u): AuthUser, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let ct = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let ext = metadata::image_ext(ct).ok_or_else(|| ApiError::bad_request("Logo musí být obrázek (PNG, JPG, SVG, WEBP)"))?;
    if body.is_empty() {
        return Err(ApiError::bad_request("Prázdný soubor"));
    }
    if let Some(old) = logo_file(&st) {
        let _ = tokio::fs::remove_file(old).await;
    }
    tokio::fs::write(st.config.data_dir.join(format!("logo.{ext}")), &body).await.map_err(anyhow::Error::from)?;
    bump_logo(&st)?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_logo(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    if let Some(old) = logo_file(&st) {
        let _ = tokio::fs::remove_file(old).await;
    }
    bump_logo(&st)?;
    Ok(Json(json!({ "ok": true })))
}

/// Odpověď se souborem nahraným uživatelem – bez možnosti spustit skript (např. v SVG).
fn uploaded_file(mime: &'static str, cache: &'static str, data: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox"),
        ],
        data,
    )
        .into_response()
}

// ---------- Motivy ----------

#[derive(Deserialize)]
struct ThemeQ {
    id: Option<String>,
}

async fn theme_css(State(st): St, Query(q): Query<ThemeQ>) -> Response {
    let s = st.settings();
    let id = q.id.unwrap_or(s.theme);
    let css = st.themes.css(&id, &s.display.effects);
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], css).into_response()
}

async fn themes_list(State(st): St) -> Json<Value> {
    Json(json!({ "active": st.settings().theme, "themes": st.themes.list() }))
}

async fn theme_export(State(st): St, Path(id): Path<String>) -> ApiResult<Response> {
    let json = st.themes.export(&id).ok_or_else(|| ApiError::not_found("Motiv neexistuje"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{id}.theme.json\"")),
        ],
        json,
    )
        .into_response())
}

async fn theme_file(State(st): St, Path((id, name)): Path<(String, String)>) -> ApiResult<Response> {
    let p = st.themes.file(&id, &name).ok_or_else(|| ApiError::not_found("Soubor neexistuje"))?;
    let data = tokio::fs::read(&p).await.map_err(|_| ApiError::not_found("Soubor neexistuje"))?;
    let mime = match name.rsplit('.').next().unwrap_or("") {
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    };
    Ok(uploaded_file(mime, "public, max-age=3600", data))
}

async fn theme_upload(State(st): St, AuthUser(u): AuthUser, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    let st2 = st.clone();
    let id = tokio::task::spawn_blocking(move || st2.themes.install(&body))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    // Když se aktualizoval aktivní motiv, klienti si ho načtou znovu.
    st.notify();
    let _ = st.events.send(ServerEvent::Settings);
    Ok(Json(json!({ "id": id })))
}

async fn theme_delete(State(st): St, AuthUser(u): AuthUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    st.themes.delete(&id).map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    if st.settings().theme == id {
        let mut s = st.settings();
        s.theme = crate::themes::DEFAULT_THEME.into();
        st.set_settings(s)?;
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------- Profilovky ----------

async fn avatar(State(st): St, Path(name): Path<String>) -> ApiResult<Response> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') || name.starts_with('.') {
        return Err(ApiError::not_found("Fotka neexistuje"));
    }
    let data = tokio::fs::read(st.config.data_dir.join("avatars").join(&name)).await.map_err(|_| ApiError::not_found("Fotka neexistuje"))?;
    let ext = name.rsplit('.').next().unwrap_or("jpg");
    Ok(uploaded_file(metadata::mime_for_ext(ext), "public, max-age=31536000, immutable", data))
}

async fn replace_avatar(st: &AppState, user_id: i64, file: Option<String>) -> anyhow::Result<()> {
    if let Some(old) = st.db.set_avatar(user_id, file.as_deref())? {
        let _ = tokio::fs::remove_file(st.config.data_dir.join("avatars").join(old)).await;
    }
    let _ = st.events.send(ServerEvent::Users);
    st.notify();
    Ok(())
}

async fn upload_avatar(State(st): St, AuthUser(u): AuthUser, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> ApiResult<Json<Value>> {
    require(&u, Role::Guest)?;
    let ct = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    // SVG ne – mohlo by obsahovat skript.
    let ext = metadata::image_ext(ct).filter(|e| *e != "svg").ok_or_else(|| ApiError::bad_request("Fotka musí být JPG, PNG, WEBP nebo GIF"))?;
    if body.is_empty() {
        return Err(ApiError::bad_request("Prázdný soubor"));
    }
    let dir = st.config.data_dir.join("avatars");
    tokio::fs::create_dir_all(&dir).await.map_err(anyhow::Error::from)?;
    let name = format!("{}-{}.{ext}", u.id, crate::util::now_ms());
    tokio::fs::write(dir.join(&name), &body).await.map_err(anyhow::Error::from)?;
    replace_avatar(&st, u.id, Some(name)).await?;
    Ok(Json(json!({ "user": st.db.user(u.id)? })))
}

async fn delete_my_avatar(State(st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Value>> {
    replace_avatar(&st, u.id, None).await?;
    Ok(Json(json!({ "user": st.db.user(u.id)? })))
}

async fn delete_user_avatar(State(st): St, AuthUser(u): AuthUser, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    require(&u, Role::Admin)?;
    replace_avatar(&st, id, None).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- Rádio ----------

#[derive(Deserialize)]
struct RadioQ {
    q: String,
}

async fn radio_search(State(st): St, AuthUser(u): AuthUser, Query(q): Query<RadioQ>) -> ApiResult<Json<Vec<crate::sources::radio::StationHit>>> {
    require(&u, Role::Admin)?;
    if q.q.trim().len() < 2 {
        return Ok(Json(vec![]));
    }
    Ok(Json(crate::sources::radio::search(&st.http, q.q.trim()).await?))
}

async fn audio_devices(State(_st): St, AuthUser(u): AuthUser) -> ApiResult<Json<Vec<AudioDevice>>> {
    require(&u, Role::Admin)?;
    let list = tokio::task::spawn_blocking(crate::player::output_devices).await.map_err(anyhow::Error::from)?;
    Ok(Json(list))
}

// ---------- Vlastní účet ----------

#[derive(Deserialize)]
struct MePatch {
    display_name: Option<String>,
}

async fn update_me(State(st): St, AuthUser(u): AuthUser, Json(req): Json<MePatch>) -> ApiResult<Json<Value>> {
    if let Some(name) = req.display_name {
        let name: String = name.trim().chars().take(40).collect();
        if name.is_empty() {
            return Err(ApiError::bad_request("Přezdívka nesmí být prázdná"));
        }
        st.db.set_display_name(u.id, &name)?;
        let _ = st.events.send(ServerEvent::Users);
        st.notify();
    }
    Ok(Json(json!({ "user": st.db.user(u.id)? })))
}
