use crate::error::ApiError;
use crate::models::{Role, User, UserStatus};
use crate::state::AppState;
use argon2::password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, HeaderMap};
use std::sync::Arc;

pub const COOKIE: &str = "mb_session";
pub const SESSION_DAYS: i64 = 60;

pub async fn hash_password(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h| h.to_string())
            .map_err(|e| anyhow::anyhow!("hash hesla: {e}"))
    })
    .await?
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok()).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn session_cookie(token: &str) -> String {
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}", SESSION_DAYS * 86400)
}

pub fn clear_cookie() -> String {
    format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

pub fn token_from(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(t) = auth.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|c| c.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

/// Přihlášený uživatel (může být i neschválený – kontrolu role dělá `require`).
pub struct AuthUser(pub User);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let token = token_from(&parts.headers).ok_or_else(ApiError::unauthorized)?;
        let user = state.db.session_user(&token)?.ok_or_else(ApiError::unauthorized)?;
        if user.status == UserStatus::Banned {
            return Err(ApiError::forbidden("Účet je zablokovaný"));
        }
        Ok(AuthUser(user))
    }
}

pub fn require(user: &User, role: Role) -> Result<(), ApiError> {
    match user.status {
        UserStatus::Pending => Err(ApiError::forbidden("Účet ještě čeká na schválení administrátorem")),
        UserStatus::Banned => Err(ApiError::forbidden("Účet je zablokovaný")),
        UserStatus::Active if user.role >= role => Ok(()),
        UserStatus::Active => Err(ApiError::forbidden(match role {
            Role::Admin => "Tohle může jen administrátor",
            Role::Dj => "Přidávat písničky můžou jen DJové – požádej admina",
            Role::Guest => "Nemáš oprávnění",
        })),
    }
}
