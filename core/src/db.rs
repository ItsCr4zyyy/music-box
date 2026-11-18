use crate::models::*;
use crate::util::now;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::Mutex;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name  TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL,
    status        TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    token      TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tracks (
    id           INTEGER PRIMARY KEY,
    source       TEXT NOT NULL,
    source_id    TEXT NOT NULL,
    title        TEXT NOT NULL,
    artist       TEXT NOT NULL,
    album        TEXT,
    duration_sec INTEGER NOT NULL DEFAULT 0,
    artwork_url  TEXT,
    file_path    TEXT,
    available    INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL,
    UNIQUE (source, source_id)
);

CREATE TABLE IF NOT EXISTS queue (
    id          INTEGER PRIMARY KEY,
    track_id    INTEGER NOT NULL REFERENCES tracks(id),
    added_by    INTEGER REFERENCES users(id) ON DELETE SET NULL,
    priority    INTEGER NOT NULL DEFAULT 0,
    sort        REAL NOT NULL,
    status      TEXT NOT NULL DEFAULT 'queued',
    created_at  INTEGER NOT NULL,
    finished_at INTEGER
);
CREATE INDEX IF NOT EXISTS queue_status ON queue (status, priority DESC, sort);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub struct Db(Mutex<Connection>);

const TRACK_COLS: &str = "t.id, t.source, t.source_id, t.title, t.artist, t.album, t.duration_sec, t.artwork_url";
const USER_COLS: &str = "id, username, display_name, role, status, created_at, avatar";

pub fn avatar_url(file: Option<String>) -> Option<String> {
    file.map(|f| format!("/api/avatars/{f}"))
}

fn track_from(row: &Row, o: usize) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(o)?,
        source: row.get(o + 1)?,
        source_id: row.get(o + 2)?,
        title: row.get(o + 3)?,
        artist: row.get(o + 4)?,
        album: row.get(o + 5)?,
        duration_sec: row.get(o + 6)?,
        artwork_url: row.get(o + 7)?,
    })
}

fn user_from(row: &Row) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        username: row.get(1)?,
        display_name: row.get(2)?,
        role: Role::parse(&row.get::<_, String>(3)?),
        status: UserStatus::parse(&row.get::<_, String>(4)?),
        created_at: row.get(5)?,
        avatar_url: avatar_url(row.get(6)?),
    })
}

fn entry_from(row: &Row) -> rusqlite::Result<QueueEntry> {
    // q.id, q.priority, q.created_at, u.id, u.display_name, u.avatar, TRACK_COLS...
    let user_id: Option<i64> = row.get(3)?;
    let user_name: Option<String> = row.get(4)?;
    let user_avatar: Option<String> = row.get(5)?;
    Ok(QueueEntry {
        id: row.get(0)?,
        priority: row.get::<_, i64>(1)? > 0,
        created_at: row.get(2)?,
        added_by: user_id.map(|id| UserRef { id, name: user_name.unwrap_or_default(), avatar_url: avatar_url(user_avatar) }),
        track: track_from(row, 6)?,
    })
}

const ENTRY_SELECT: &str = "SELECT q.id, q.priority, q.created_at, u.id, u.display_name, u.avatar, \
    t.id, t.source, t.source_id, t.title, t.artist, t.album, t.duration_sec, t.artwork_url \
    FROM queue q JOIN tracks t ON t.id = q.track_id LEFT JOIN users u ON u.id = q.added_by";

/// Řazení fronty: přednost (admin) první, pak podle pořadí přidání.
const QUEUE_ORDER: &str = "ORDER BY q.priority DESC, q.sort ASC";

/// Data o souboru skladby potřebná pro přehrání.
#[derive(Clone, Debug)]
pub struct PlayableTrack {
    pub track_id: i64,
    pub source: String,
    pub source_id: String,
    pub file_path: Option<String>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        // Migrace starších databází (sloupec už může existovat).
        for sql in [
            "ALTER TABLE tracks ADD COLUMN collection TEXT",
            "ALTER TABLE tracks ADD COLUMN track_no INTEGER",
            // 1 = metadata nastavená ručně (identifikace) – skenování je nepřepíše.
            "ALTER TABLE tracks ADD COLUMN locked INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE users ADD COLUMN avatar TEXT",
            // Zesílení pro normalizaci hlasitosti (dB), NULL = ještě nezměřeno.
            "ALTER TABLE tracks ADD COLUMN gain_db REAL",
        ] {
            let _ = conn.execute(sql, []);
        }
        // Skladba, která hrála při vypnutí, půjde znovu na začátek fronty.
        conn.execute("UPDATE queue SET status = 'queued', priority = 2 WHERE status = 'playing'", [])?;
        conn.execute("DELETE FROM sessions WHERE expires_at < ?1", [now()])?;
        Ok(Db(Mutex::new(conn)))
    }

    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> rusqlite::Result<T> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        f(&conn)
    }

    // ---------- Nastavení ----------

    pub fn load_settings(&self) -> rusqlite::Result<Settings> {
        let raw: Option<String> = self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0)).optional()
        })?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> rusqlite::Result<()> {
        let json = serde_json::to_string(s).unwrap_or_default();
        self.with(|c| {
            c.execute(
                "INSERT INTO settings (key, value) VALUES ('app', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json],
            )
            .map(|_| ())
        })
    }

    // ---------- Uživatelé ----------

    pub fn user_count(&self) -> rusqlite::Result<i64> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)))
    }

    pub fn create_user(&self, username: &str, display_name: &str, hash: &str, role: Role, status: UserStatus) -> rusqlite::Result<User> {
        self.with(|c| {
            c.execute(
                "INSERT INTO users (username, display_name, password_hash, role, status, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![username, display_name, hash, role.as_str(), status.as_str(), now()],
            )?;
            let id = c.last_insert_rowid();
            c.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id = ?1"), [id], user_from)
        })
    }

    /// Vrací uživatele + hash hesla.
    pub fn user_by_name(&self, username: &str) -> rusqlite::Result<Option<(User, String)>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {USER_COLS}, password_hash FROM users WHERE username = ?1"),
                [username],
                |r| Ok((user_from(r)?, r.get(7)?)),
            )
            .optional()
        })
    }

    pub fn user(&self, id: i64) -> rusqlite::Result<Option<User>> {
        self.with(|c| c.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id = ?1"), [id], user_from).optional())
    }

    pub fn users(&self) -> rusqlite::Result<Vec<User>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {USER_COLS} FROM users ORDER BY CASE status WHEN 'pending' THEN 0 ELSE 1 END, created_at DESC"
            ))?;
            st.query_map([], user_from)?.collect()
        })
    }

    pub fn update_user(&self, id: i64, role: Option<Role>, status: Option<UserStatus>) -> rusqlite::Result<()> {
        self.with(|c| {
            if let Some(r) = role {
                c.execute("UPDATE users SET role = ?1 WHERE id = ?2", params![r.as_str(), id])?;
            }
            if let Some(s) = status {
                c.execute("UPDATE users SET status = ?1 WHERE id = ?2", params![s.as_str(), id])?;
                if s != UserStatus::Active {
                    c.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
                }
            }
            Ok(())
        })
    }

    pub fn set_display_name(&self, id: i64, name: &str) -> rusqlite::Result<()> {
        self.with(|c| c.execute("UPDATE users SET display_name = ?1 WHERE id = ?2", params![name, id]).map(|_| ()))
    }

    pub fn set_password(&self, id: i64, hash: &str) -> rusqlite::Result<()> {
        self.with(|c| c.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![hash, id]).map(|_| ()))
    }

    /// Nastaví soubor profilovky; vrací původní soubor (ke smazání).
    pub fn set_avatar(&self, id: i64, file: Option<&str>) -> rusqlite::Result<Option<String>> {
        self.with(|c| {
            let old: Option<String> = c.query_row("SELECT avatar FROM users WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten();
            c.execute("UPDATE users SET avatar = ?1 WHERE id = ?2", params![file, id])?;
            Ok(old)
        })
    }

    pub fn delete_user(&self, id: i64) -> rusqlite::Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM queue WHERE added_by = ?1 AND status = 'queued'", [id])?;
            c.execute("DELETE FROM users WHERE id = ?1", [id]).map(|_| ())
        })
    }

    pub fn active_admin_count(&self) -> rusqlite::Result<i64> {
        self.with(|c| {
            c.query_row("SELECT COUNT(*) FROM users WHERE role = 'admin' AND status = 'active'", [], |r| r.get(0))
        })
    }

    // ---------- Relace ----------

    pub fn create_session(&self, token: &str, user_id: i64, days: i64) -> rusqlite::Result<()> {
        let n = now();
        self.with(|c| {
            c.execute(
                "INSERT INTO sessions (token, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params![token, user_id, n, n + days * 86400],
            )
            .map(|_| ())
        })
    }

    pub fn session_user(&self, token: &str) -> rusqlite::Result<Option<User>> {
        self.with(|c| {
            c.query_row(
                "SELECT u.id, u.username, u.display_name, u.role, u.status, u.created_at, u.avatar \
                 FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token = ?1 AND s.expires_at > ?2",
                params![token, now()],
                user_from,
            )
            .optional()
        })
    }

    pub fn delete_session(&self, token: &str) -> rusqlite::Result<()> {
        self.with(|c| c.execute("DELETE FROM sessions WHERE token = ?1", [token]).map(|_| ()))
    }

    // ---------- Skladby ----------

    /// Vloží nebo aktualizuje skladbu a vrátí ji.
    pub fn upsert_track(
        &self,
        source: &str,
        source_id: &str,
        title: &str,
        artist: &str,
        album: Option<&str>,
        duration_sec: u32,
        artwork_url: Option<&str>,
        file_path: Option<&str>,
    ) -> rusqlite::Result<Track> {
        self.with(|c| {
            c.execute(
                "INSERT INTO tracks (source, source_id, title, artist, album, duration_sec, artwork_url, file_path, available, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9) \
                 ON CONFLICT(source, source_id) DO UPDATE SET title = excluded.title, artist = excluded.artist, \
                   album = COALESCE(excluded.album, tracks.album), \
                   duration_sec = CASE WHEN excluded.duration_sec > 0 THEN excluded.duration_sec ELSE tracks.duration_sec END, \
                   artwork_url = COALESCE(excluded.artwork_url, tracks.artwork_url), \
                   file_path = COALESCE(excluded.file_path, tracks.file_path), available = 1",
                params![source, source_id, title, artist, album, duration_sec, artwork_url, file_path, now()],
            )?;
            c.query_row(
                &format!("SELECT {TRACK_COLS} FROM tracks t WHERE t.source = ?1 AND t.source_id = ?2"),
                params![source, source_id],
                |r| track_from(r, 0),
            )
        })
    }

    pub fn track(&self, id: i64) -> rusqlite::Result<Option<Track>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {TRACK_COLS} FROM tracks t WHERE t.id = ?1"), [id], |r| track_from(r, 0))
                .optional()
        })
    }

    pub fn playable(&self, track_id: i64) -> rusqlite::Result<Option<PlayableTrack>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, source, source_id, file_path FROM tracks WHERE id = ?1",
                [track_id],
                |r| Ok(PlayableTrack { track_id: r.get(0)?, source: r.get(1)?, source_id: r.get(2)?, file_path: r.get(3)? }),
            )
            .optional()
        })
    }

    pub fn track_gain(&self, track_id: i64) -> rusqlite::Result<Option<f32>> {
        self.with(|c| {
            c.query_row("SELECT gain_db FROM tracks WHERE id = ?1", [track_id], |r| r.get::<_, Option<f64>>(0))
                .optional()
                .map(|g| g.flatten().map(|v| v as f32))
        })
    }

    pub fn set_track_gain(&self, track_id: i64, gain_db: f32) -> rusqlite::Result<()> {
        self.with(|c| c.execute("UPDATE tracks SET gain_db = ?1 WHERE id = ?2", params![gain_db as f64, track_id]).map(|_| ()))
    }

    pub fn set_track_file(&self, track_id: i64, path: Option<&str>) -> rusqlite::Result<()> {
        self.with(|c| c.execute("UPDATE tracks SET file_path = ?1 WHERE id = ?2", params![path, track_id]).map(|_| ()))
    }

    pub fn set_track_artwork(&self, track_id: i64, url: Option<&str>) -> rusqlite::Result<()> {
        self.with(|c| c.execute("UPDATE tracks SET artwork_url = ?1 WHERE id = ?2", params![url, track_id]).map(|_| ()))
    }

    pub fn search_local(&self, q: &str, limit: u32) -> rusqlite::Result<Vec<Track>> {
        let like = format!("%{}%", q.replace('%', "").replace('_', ""));
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks t WHERE t.source = 'local' AND t.available = 1 \
                 AND (t.title LIKE ?1 OR t.artist LIKE ?1 OR t.album LIKE ?1) ORDER BY t.artist, t.title LIMIT ?2"
            ))?;
            st.query_map(params![like, limit], |r| track_from(r, 0))?.collect()
        })
    }

    /// Lokální knihovna rozdělená na alba (podsložky) a singly.
    pub fn library(&self) -> rusqlite::Result<Library> {
        let rows: Vec<(Option<String>, Track)> = self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT t.collection, {TRACK_COLS} FROM tracks t WHERE t.source = 'local' AND t.available = 1                  ORDER BY t.collection COLLATE NOCASE, t.track_no, t.file_path COLLATE NOCASE"
            ))?;
            st.query_map([], |r| Ok((r.get(0)?, track_from(r, 1)?)))?.collect()
        })?;
        let mut albums: Vec<Album> = Vec::new();
        let mut singles = Vec::new();
        for (key, track) in rows {
            match key {
                None => singles.push(track),
                Some(key) => {
                    if albums.last().map(|a| a.key != key).unwrap_or(true) {
                        albums.push(Album {
                            name: track.album.clone().unwrap_or_else(|| key.rsplit('/').next().unwrap_or(&key).to_string()),
                            artist: track.artist.clone(),
                            artwork_url: track.artwork_url.clone(),
                            key,
                            tracks: Vec::new(),
                        });
                    }
                    let album = albums.last_mut().expect("album");
                    if album.artist != track.artist && album.artist != "Různí interpreti" {
                        album.artist = "Různí interpreti".into();
                    }
                    if album.artwork_url.is_none() {
                        album.artwork_url = track.artwork_url.clone();
                    }
                    album.tracks.push(track);
                }
            }
        }
        singles.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()).then(a.title.cmp(&b.title)));
        Ok(Library { albums, singles })
    }

    /// Skladby jednoho alba (podsložky) v pořadí.
    pub fn collection_tracks(&self, key: &str) -> rusqlite::Result<Vec<Track>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {TRACK_COLS} FROM tracks t WHERE t.source = 'local' AND t.available = 1 AND t.collection = ?1                  ORDER BY t.track_no, t.file_path COLLATE NOCASE"
            ))?;
            st.query_map([key], |r| track_from(r, 0))?.collect()
        })
    }

    /// Uloží identifikovaná metadata a zamkne je proti přepsání při skenování.
    pub fn apply_identity(
        &self,
        track_id: i64,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        artwork_url: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE tracks SET title = COALESCE(?1, title), artist = COALESCE(?2, artist), album = COALESCE(?3, album), \
                 artwork_url = COALESCE(?4, artwork_url), locked = 1 WHERE id = ?5",
                params![title, artist, album, artwork_url, track_id],
            )
            .map(|_| ())
        })
    }

    /// Zruší ruční metadata (při dalším skenování se načtou znovu z tagů).
    pub fn unlock_tracks(&self, ids: &[i64]) -> rusqlite::Result<()> {
        self.with(|c| {
            for id in ids {
                c.execute("UPDATE tracks SET locked = 0 WHERE id = ?1", [id])?;
            }
            Ok(())
        })
    }

    pub fn local_count(&self) -> rusqlite::Result<i64> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM tracks WHERE source = 'local' AND available = 1", [], |r| r.get(0)))
    }

    /// Před skenováním knihovny označí všechny lokální skladby jako nedostupné.
    pub fn mark_local_unavailable(&self) -> rusqlite::Result<()> {
        self.with(|c| c.execute("UPDATE tracks SET available = 0 WHERE source = 'local'", []).map(|_| ()))
    }

    pub fn random_local(&self) -> rusqlite::Result<Option<i64>> {
        self.with(|c| {
            c.query_row(
                "SELECT id FROM tracks WHERE source = 'local' AND available = 1 \
                 AND id NOT IN (SELECT track_id FROM queue WHERE finished_at > ?1) ORDER BY RANDOM() LIMIT 1",
                [now() - 3 * 3600],
                |r| r.get(0),
            )
            .optional()
        })
    }

    // ---------- Fronta ----------

    pub fn enqueue(&self, track_id: i64, added_by: Option<i64>, priority: bool) -> rusqlite::Result<i64> {
        self.with(|c| {
            let sort: f64 = if priority {
                c.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM queue WHERE status = 'queued' AND priority = 1", [], |r| r.get(0))?
            } else {
                c.query_row("SELECT COALESCE(MAX(sort), 0) + 1 FROM queue", [], |r| r.get(0))?
            };
            c.execute(
                "INSERT INTO queue (track_id, added_by, priority, sort, status, created_at) VALUES (?1, ?2, ?3, ?4, 'queued', ?5)",
                params![track_id, added_by, priority as i64, sort, now()],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn queued(&self) -> rusqlite::Result<Vec<QueueEntry>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("{ENTRY_SELECT} WHERE q.status = 'queued' {QUEUE_ORDER}"))?;
            st.query_map([], entry_from)?.collect()
        })
    }

    pub fn history(&self, limit: u32) -> rusqlite::Result<Vec<QueueEntry>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "{ENTRY_SELECT} WHERE q.status = 'played' ORDER BY q.finished_at DESC LIMIT ?1"
            ))?;
            st.query_map([limit], entry_from)?.collect()
        })
    }

    pub fn entry(&self, id: i64) -> rusqlite::Result<Option<QueueEntry>> {
        self.with(|c| c.query_row(&format!("{ENTRY_SELECT} WHERE q.id = ?1"), [id], entry_from).optional())
    }

    pub fn next_queued(&self) -> rusqlite::Result<Option<QueueEntry>> {
        self.with(|c| {
            c.query_row(&format!("{ENTRY_SELECT} WHERE q.status = 'queued' {QUEUE_ORDER} LIMIT 1"), [], entry_from)
                .optional()
        })
    }

    /// Další skladby po aktuální (pro předstahování).
    pub fn upcoming(&self, limit: u32) -> rusqlite::Result<Vec<QueueEntry>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("{ENTRY_SELECT} WHERE q.status = 'queued' {QUEUE_ORDER} LIMIT ?1"))?;
            st.query_map([limit], entry_from)?.collect()
        })
    }

    pub fn entry_status(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.with(|c| c.query_row("SELECT status FROM queue WHERE id = ?1", [id], |r| r.get(0)).optional())
    }

    pub fn entry_owner(&self, id: i64) -> rusqlite::Result<Option<Option<i64>>> {
        self.with(|c| c.query_row("SELECT added_by FROM queue WHERE id = ?1", [id], |r| r.get(0)).optional())
    }

    pub fn set_entry_status(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        let finished = matches!(status, "played" | "skipped" | "failed" | "removed").then(now);
        self.with(|c| {
            c.execute("UPDATE queue SET status = ?1, finished_at = ?2 WHERE id = ?3", params![status, finished, id])
                .map(|_| ())
        })
    }

    pub fn promote(&self, id: i64) -> rusqlite::Result<()> {
        self.with(|c| {
            let sort: f64 = c.query_row(
                "SELECT COALESCE(MIN(sort), 1) - 1 FROM queue WHERE status = 'queued' AND priority >= 1",
                [],
                |r| r.get(0),
            )?;
            c.execute("UPDATE queue SET priority = 1, sort = ?1 WHERE id = ?2 AND status = 'queued'", params![sort, id])
                .map(|_| ())
        })
    }

    /// Posune položku o jedno místo nahoru (-1) nebo dolů (+1) prohozením se sousedem.
    pub fn shift(&self, id: i64, dir: i32) -> rusqlite::Result<()> {
        let queue = self.queued()?;
        let Some(idx) = queue.iter().position(|e| e.id == id) else { return Ok(()) };
        let other = idx as i64 + dir as i64;
        if other < 0 || other as usize >= queue.len() {
            return Ok(());
        }
        let other_id = queue[other as usize].id;
        self.with(|c| {
            let a: (i64, f64) = c.query_row("SELECT priority, sort FROM queue WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let b: (i64, f64) = c.query_row("SELECT priority, sort FROM queue WHERE id = ?1", [other_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            c.execute("UPDATE queue SET priority = ?1, sort = ?2 WHERE id = ?3", params![b.0, b.1, id])?;
            c.execute("UPDATE queue SET priority = ?1, sort = ?2 WHERE id = ?3", params![a.0, a.1, other_id])?;
            Ok(())
        })
    }

    pub fn queued_count_by(&self, user_id: i64) -> rusqlite::Result<i64> {
        self.with(|c| {
            c.query_row("SELECT COUNT(*) FROM queue WHERE added_by = ?1 AND status = 'queued'", [user_id], |r| r.get(0))
        })
    }

    pub fn is_track_queued(&self, track_id: i64) -> rusqlite::Result<bool> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM queue WHERE track_id = ?1 AND status IN ('queued', 'playing')",
                [track_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
        })
    }

    pub fn clear_queue(&self) -> rusqlite::Result<()> {
        self.with(|c| {
            c.execute("UPDATE queue SET status = 'removed', finished_at = ?1 WHERE status = 'queued'", [now()]).map(|_| ())
        })
    }
}
