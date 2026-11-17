//! Motivy vzhledu (web i displej). Motiv = `theme.json` (barvy, fonty, zaoblení, stíny, vzor, animace,
//! vlastní CSS), případně ZIP s `theme.json` a soubory fontů. Server z něj vygeneruje `/api/theme.css`,
//! které přepíše CSS proměnné Tailwindu.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DEFAULT_THEME: &str = "hravy";

const BUILTIN: &[(&str, &str)] = &[
    ("hravy", include_str!("../assets/themes/hravy.json")),
    ("devadesatky", include_str!("../assets/themes/devadesatky.json")),
    ("ostry", include_str!("../assets/themes/ostry.json")),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeColors {
    pub background: String,
    pub surface: String,
    pub surface_2: String,
    pub surface_3: String,
    pub text: String,
    pub primary: String,
    pub secondary: String,
    pub success: String,
    pub info: String,
    pub extra: String,
    /// Barva textu na barevných tlačítkách (prázdné = barva pozadí).
    pub on_accent: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        ThemeColors {
            background: "#1b1036".into(),
            surface: "#251648".into(),
            surface_2: "#33205e".into(),
            surface_3: "#43307a".into(),
            text: "#ffffff".into(),
            primary: "#ff5fa2".into(),
            secondary: "#ffd23f".into(),
            success: "#5ce1b9".into(),
            info: "#6ec8ff".into(),
            extra: "#b69cff".into(),
            on_accent: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeFonts {
    pub heading: String,
    pub body: String,
}

impl Default for ThemeFonts {
    fn default() -> Self {
        ThemeFonts { heading: "Baloo 2 Variable".into(), body: "Nunito Variable".into() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FontFile {
    pub family: String,
    pub file: String,
    pub weight: String,
    pub style: String,
}

impl Default for FontFile {
    fn default() -> Self {
        FontFile { family: String::new(), file: String::new(), weight: "400".into(), style: "normal".into() }
    }
}

/// Speciální efekt motivu. Vestavěné (vykresluje displej): "notes", "keys", "ticker", "pulse".
/// Ostatní jsou čistě CSS – `css` se použije jen, když je efekt zapnutý.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Effect {
    pub id: String,
    pub name: String,
    /// Anglický název (volitelný).
    pub name_en: String,
    /// Animovaný (true) nebo statický (false) – jen popisek pro admina.
    pub animated: bool,
    /// Zapnutý ve výchozím stavu.
    pub default: bool,
    pub css: String,
}

impl Default for Effect {
    fn default() -> Self {
        Effect { id: String::new(), name: String::new(), name_en: String::new(), animated: true, default: true, css: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub description: String,
    /// Anglický popis (volitelný).
    pub description_en: String,
    pub author: String,
    pub colors: ThemeColors,
    pub fonts: ThemeFonts,
    /// Vlastní fonty přibalené v ZIPu.
    pub font_files: Vec<FontFile>,
    /// Násobek zaoblení rohů (0 = hranaté, 1 = výchozí).
    pub radius: f32,
    /// "chunky" | "soft" | "hard" | "none"
    pub shadow: String,
    /// "dots" | "grid" | "lines" | "none"
    pub pattern: String,
    pub animations: bool,
    /// Vlastní CSS navíc (třídy: .card, .btn, .input, .chip, .mb-brand, .mb-panel, .mb-notes, .mb-vinyl).
    pub css: String,
    /// Speciální efekty, které admin může zapnout/vypnout.
    pub effects: Vec<Effect>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            name: "Bez názvu".into(),
            description: String::new(),
            description_en: String::new(),
            author: String::new(),
            colors: ThemeColors::default(),
            fonts: ThemeFonts::default(),
            font_files: vec![],
            radius: 1.0,
            shadow: "chunky".into(),
            pattern: "dots".into(),
            animations: true,
            css: String::new(),
            effects: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub description_en: String,
    pub author: String,
    pub builtin: bool,
    pub colors: ThemeColors,
    pub fonts: ThemeFonts,
    pub effects: Vec<Effect>,
    /// Mění se při úpravě motivu (pro obnovení CSS v prohlížeči).
    pub version: String,
}

pub struct Themes {
    dir: PathBuf,
}

fn safe_color(c: &str, fallback: &str) -> String {
    let ok = !c.is_empty() && c.len() <= 60 && c.chars().all(|ch| ch.is_ascii_alphanumeric() || "#(),.% /-".contains(ch));
    if ok { c.to_string() } else { fallback.to_string() }
}

fn safe_font(f: &str) -> String {
    f.chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).take(60).collect::<String>().trim().to_string()
}

fn safe_file(f: &str) -> bool {
    !f.is_empty() && !f.starts_with('.') && f.len() <= 100 && f.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// Přibližný jas barvy #rrggbb (pro světlý / tmavý režim formulářů prohlížeče).
fn is_light(hex: &str) -> bool {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return false;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32;
    0.299 * p(0) + 0.587 * p(2) + 0.114 * p(4) > 150.0
}

fn slug(name: &str) -> String {
    let map = |c: char| match c {
        'á' | 'Á' | 'ä' => 'a',
        'č' | 'Č' => 'c',
        'ď' | 'Ď' => 'd',
        'é' | 'É' | 'ě' | 'Ě' => 'e',
        'í' | 'Í' => 'i',
        'ň' | 'Ň' => 'n',
        'ó' | 'Ó' | 'ö' => 'o',
        'ř' | 'Ř' => 'r',
        'š' | 'Š' => 's',
        'ť' | 'Ť' => 't',
        'ú' | 'Ú' | 'ů' | 'Ů' | 'ü' => 'u',
        'ý' | 'Ý' => 'y',
        'ž' | 'Ž' => 'z',
        c => c.to_ascii_lowercase(),
    };
    let s: String = name.chars().map(map).map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "motiv".into() } else { s.chars().take(40).collect() }
}

impl Themes {
    pub fn new(data_dir: &Path) -> Themes {
        Themes { dir: data_dir.join("themes") }
    }

    fn builtin(id: &str) -> Option<Theme> {
        BUILTIN.iter().find(|(b, _)| *b == id).and_then(|(_, json)| serde_json::from_str(json).ok())
    }

    pub fn is_builtin(id: &str) -> bool {
        BUILTIN.iter().any(|(b, _)| *b == id)
    }

    fn custom_dir(&self, id: &str) -> Option<PathBuf> {
        if !safe_file(id) {
            return None;
        }
        let d = self.dir.join(id);
        d.join("theme.json").is_file().then_some(d)
    }

    pub fn get(&self, id: &str) -> Option<Theme> {
        if let Some(t) = Self::builtin(id) {
            return Some(t);
        }
        let d = self.custom_dir(id)?;
        serde_json::from_str(&std::fs::read_to_string(d.join("theme.json")).ok()?).ok()
    }

    pub fn version(&self, id: &str) -> String {
        if Self::is_builtin(id) {
            return format!("{id}-{}", env!("CARGO_PKG_VERSION"));
        }
        let mtime = self
            .custom_dir(id)
            .and_then(|d| d.join("theme.json").metadata().ok())
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{id}-{mtime}")
    }

    pub fn list(&self) -> Vec<ThemeInfo> {
        let mut ids: Vec<(String, bool)> = BUILTIN.iter().map(|(id, _)| (id.to_string(), true)).collect();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            let mut custom: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().join("theme.json").is_file())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|id| !Self::is_builtin(id))
                .collect();
            custom.sort();
            ids.extend(custom.into_iter().map(|id| (id, false)));
        }
        ids.into_iter()
            .filter_map(|(id, builtin)| {
                let t = self.get(&id)?;
                Some(ThemeInfo {
                    version: self.version(&id),
                    id,
                    name: t.name,
                    description: t.description,
                    description_en: t.description_en,
                    author: t.author,
                    builtin,
                    colors: t.colors,
                    fonts: t.fonts,
                    effects: t.effects,
                })
            })
            .collect()
    }

    /// Soubor přibalený k vlastnímu motivu (font, obrázek).
    pub fn file(&self, id: &str, name: &str) -> Option<PathBuf> {
        if !safe_file(name) || name == "theme.json" {
            return None;
        }
        let p = self.custom_dir(id)?.join(name);
        p.is_file().then_some(p)
    }

    /// ID zapnutých efektů motivu (podle nastavení admina, jinak výchozí).
    pub fn enabled_effects(&self, id: &str, overrides: &BTreeMap<String, bool>) -> Vec<String> {
        self.get(id)
            .map(|t| t.effects.into_iter().filter(|e| *overrides.get(&e.id).unwrap_or(&e.default)).map(|e| e.id).collect())
            .unwrap_or_default()
    }

    pub fn css(&self, id: &str, overrides: &BTreeMap<String, bool>) -> String {
        let t = self.get(id).or_else(|| Self::builtin(DEFAULT_THEME)).unwrap_or_default();
        let d = ThemeColors::default();
        let c = &t.colors;
        let mut css = format!("/* motiv: {} */\n", t.name.replace("*/", ""));

        for f in &t.font_files {
            if !safe_file(&f.file) {
                continue;
            }
            let fmt = match f.file.rsplit('.').next().unwrap_or("") {
                "woff2" => "woff2",
                "woff" => "woff",
                "otf" => "opentype",
                _ => "truetype",
            };
            css += &format!(
                "@font-face {{ font-family: \"{}\"; src: url(\"/api/themes/{id}/files/{}\") format(\"{fmt}\"); font-weight: {}; font-style: {}; font-display: swap; }}\n",
                safe_font(&f.family),
                f.file,
                safe_color(&f.weight, "400"),
                if f.style == "italic" { "italic" } else { "normal" },
            );
        }

        let r = t.radius.clamp(0.0, 3.0);
        let radius = [("xs", 0.125), ("sm", 0.25), ("md", 0.375), ("lg", 0.5), ("xl", 0.75), ("2xl", 1.0), ("3xl", 1.5), ("4xl", 2.0)]
            .iter()
            .map(|(n, v)| format!("--radius-{n}: {:.3}rem;", v * r))
            .collect::<Vec<_>>()
            .join(" ");
        let (card, btn, btn_active) = match t.shadow.as_str() {
            "soft" => ("0 6px 24px rgb(0 0 0 / 0.22)", "0 2px 10px rgb(0 0 0 / 0.22)", "0 1px 4px rgb(0 0 0 / 0.2)"),
            "hard" => ("5px 5px 0 0 var(--color-white)", "3px 3px 0 0 var(--color-white)", "1px 1px 0 0 var(--color-white)"),
            "none" => ("none", "none", "none"),
            _ => ("0 5px 0 0 rgb(0 0 0 / 0.25)", "0 4px 0 0 rgb(0 0 0 / 0.3)", "0 1px 0 0 rgb(0 0 0 / 0.3)"),
        };
        let (pattern, size) = match t.pattern.as_str() {
            "grid" => (
                "linear-gradient(color-mix(in srgb, var(--color-white) 5%, transparent) 1px, transparent 1px), linear-gradient(90deg, color-mix(in srgb, var(--color-white) 5%, transparent) 1px, transparent 1px)",
                "28px 28px",
            ),
            "lines" => ("repeating-linear-gradient(0deg, color-mix(in srgb, var(--color-white) 4%, transparent) 0 1px, transparent 1px 26px)", "auto"),
            "none" => ("none", "auto"),
            _ => ("radial-gradient(color-mix(in srgb, var(--color-white) 6%, transparent) 1.5px, transparent 1.5px)", "22px 22px"),
        };
        let bg = safe_color(&c.background, &d.background);
        css += &format!(
            ":root {{\n  color-scheme: {scheme};\n  --color-ink: {bg}; --color-ink-2: {}; --color-ink-3: {}; --color-ink-4: {};\n  --color-white: {}; --color-on-accent: {}; --color-pink: {}; --color-sun: {}; --color-mint: {}; --color-sky: {}; --color-lilac: {};\n  --font-display: \"{}\", ui-rounded, system-ui, sans-serif; --font-sans: \"{}\", ui-rounded, system-ui, sans-serif;\n  {radius} --mb-radius: {r};\n  --mb-shadow-card: {card}; --mb-shadow-btn: {btn}; --mb-shadow-btn-active: {btn_active};\n  --mb-pattern: {pattern}; --mb-pattern-size: {size};\n}}\n",
            safe_color(&c.surface, &d.surface),
            safe_color(&c.surface_2, &d.surface_2),
            safe_color(&c.surface_3, &d.surface_3),
            safe_color(&c.text, &d.text),
            safe_color(&c.on_accent, &bg),
            safe_color(&c.primary, &d.primary),
            safe_color(&c.secondary, &d.secondary),
            safe_color(&c.success, &d.success),
            safe_color(&c.info, &d.info),
            safe_color(&c.extra, &d.extra),
            safe_font(&t.fonts.heading),
            safe_font(&t.fonts.body),
            scheme = if is_light(&bg) { "light" } else { "dark" },
        );
        if !t.animations {
            // Vypne běžné animace rozhraní; speciální efekty (.mb-fx) mají vlastní přepínače.
            css += "*:not(.mb-fx, .mb-fx *), *::before, *::after { animation: none !important; transition: none !important; }\n";
        }
        for e in t.effects.iter().filter(|e| *overrides.get(&e.id).unwrap_or(&e.default)) {
            if !e.css.trim().is_empty() {
                css += &format!("\n/* efekt: {} */\n{}\n", e.name.replace("*/", ""), e.css);
            }
        }
        if !t.css.trim().is_empty() {
            css += "\n/* vlastní CSS motivu */\n";
            css += &t.css;
            css += "\n";
        }
        css
    }

    /// Nainstaluje motiv z `theme.json` nebo ZIPu (theme.json + fonty). Vrací ID.
    pub fn install(&self, data: &[u8]) -> Result<String> {
        let is_zip = data.starts_with(b"PK");
        let (theme_json, files): (Vec<u8>, Vec<(String, Vec<u8>)>) = if is_zip {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).context("neplatný ZIP")?;
            let mut json = None;
            let mut files = Vec::new();
            for i in 0..zip.len() {
                let mut f = zip.by_index(i)?;
                if f.is_dir() {
                    continue;
                }
                let Some(name) = f.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())) else {
                    continue;
                };
                if f.size() > 8 * 1024 * 1024 {
                    bail!("soubor {name} je moc velký");
                }
                let mut buf = Vec::new();
                f.read_to_end(&mut buf)?;
                if name == "theme.json" {
                    json = Some(buf);
                } else if safe_file(&name) {
                    files.push((name, buf));
                }
            }
            (json.ok_or_else(|| anyhow!("v ZIPu chybí theme.json"))?, files)
        } else {
            (data.to_vec(), vec![])
        };
        let theme: Theme = serde_json::from_slice(&theme_json).map_err(|e| anyhow!("chyba v theme.json: {e}"))?;
        let mut id = slug(&theme.name);
        if Self::is_builtin(&id) {
            id = format!("{id}-vlastni");
        }
        for f in &theme.font_files {
            if !files.iter().any(|(n, _)| n == &f.file) {
                bail!("chybí soubor fontu {}", f.file);
            }
        }
        let dir = self.dir.join(&id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("theme.json"), serde_json::to_vec_pretty(&theme)?)?;
        for (name, buf) in files {
            std::fs::write(dir.join(name), buf)?;
        }
        Ok(id)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if Self::is_builtin(id) {
            bail!("vestavěný motiv nejde smazat");
        }
        let d = self.custom_dir(id).ok_or_else(|| anyhow!("motiv neexistuje"))?;
        std::fs::remove_dir_all(d)?;
        Ok(())
    }

    /// Motiv jako JSON (šablona pro vlastní úpravy).
    pub fn export(&self, id: &str) -> Option<String> {
        self.get(id).and_then(|t| serde_json::to_string_pretty(&t).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse() {
        for (id, json) in BUILTIN {
            let t: Theme = serde_json::from_str(json).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(!t.name.is_empty());
        }
        let th = Themes::new(Path::new("."));
        let none = BTreeMap::new();
        assert!(th.css("devadesatky", &none).contains("--color-ink: #f3ede0"));
        assert!(th.css("devadesatky", &none).contains("color-scheme: light"));
        assert!(th.css("hravy", &none).contains("color-scheme: dark"));
        assert!(th.enabled_effects("hravy", &none).contains(&"notes".to_string()));
        let off = BTreeMap::from([("notes".to_string(), false)]);
        assert!(!th.enabled_effects("hravy", &off).contains(&"notes".to_string()));
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Můj Žlutý motiv!"), "muj-zluty-motiv");
        assert_eq!(slug("Devadesátky"), "devadesatky");
    }
}
