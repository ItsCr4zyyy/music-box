use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Bezpečná navigace v JSONu: `jp(&v, &["a", "0", "b"])` (číslo = index v poli).
pub fn jp<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for k in path {
        cur = match cur {
            Value::Object(m) => m.get(*k)?,
            Value::Array(a) => a.get(k.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

pub fn jstr(v: &Value, path: &[&str]) -> Option<String> {
    jp(v, path).and_then(|x| x.as_str()).map(|s| s.to_string())
}

pub fn jnum(v: &Value, path: &[&str]) -> Option<f64> {
    jp(v, path).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}

/// Najde rekurzivně všechny objekty uložené pod klíčem `key`.
pub fn find_all<'a>(v: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                if k == key && val.is_object() {
                    out.push(val);
                } else {
                    find_all(val, key, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| find_all(x, key, out)),
        _ => {}
    }
}

/// "3:45" / "1:02:03" → sekundy.
pub fn parse_duration(text: &str) -> Option<u32> {
    let t = text.trim();
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit() || c == ':') || !t.contains(':') {
        return None;
    }
    t.split(':').try_fold(0u32, |acc, p| p.parse::<u32>().ok().map(|n| acc * 60 + n))
}

/// Najde spustitelný soubor v PATH.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ""] } else { &[""] };
    for dir in std::env::split_paths(&path) {
        for ext in exts {
            let p = dir.join(format!("{name}{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Spuštění externího programu bez blikajícího konzolového okna na Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.kill_on_drop(true);
    cmd
}
