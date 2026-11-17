//! Překlad chybových hlášek API. Server píše česky; když klient pošle hlavičku `X-Lang: en`,
//! middleware hlášku v odpovědi přeloží (přesná shoda nebo začátek textu).

use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

/// (česky, anglicky) – přesná shoda.
const EXACT: &[(&str, &str)] = &[
    ("Album je prázdné", "The album is empty"),
    ("Bez obalu", "No cover"),
    ("Celá alba a playlisty může přidat jen administrátor", "Only an administrator can add whole albums and playlists"),
    ("Celé playlisty může přidat jen administrátor", "Only an administrator can add whole playlists"),
    ("Chybí skladba", "No song given"),
    ("Fotka musí být JPG, PNG, WEBP nebo GIF", "The photo must be JPG, PNG, WEBP or GIF"),
    ("Fotka neexistuje", "Photo not found"),
    ("Heslo musí mít aspoň 6 znaků", "The password must have at least 6 characters"),
    ("Jméno musí mít 3–32 znaků (písmena, čísla, _ . -)", "The username must have 3–32 characters (letters, digits, _ . -)"),
    ("Logo musí být obrázek (PNG, JPG, SVG, WEBP)", "The logo must be an image (PNG, JPG, SVG, WEBP)"),
    ("Motiv neexistuje", "Theme not found"),
    ("Musí zůstat aspoň jeden administrátor", "At least one administrator must remain"),
    ("Můžeš odebrat jen svoje písničky", "You can only remove your own songs"),
    ("Na YouTube se nic nenašlo", "Nothing found on YouTube"),
    ("Nejsi přihlášený", "You are not logged in"),
    ("Neplatná skladba", "Invalid song"),
    ("Nic k identifikaci", "Nothing to identify"),
    ("Obal neexistuje", "Cover not found"),
    ("Odkaz nepoznávám – podporuji YouTube, YouTube Music a Spotify", "Unknown link – YouTube, YouTube Music and Spotify are supported"),
    ("Položka neexistuje", "Item not found"),
    ("Prázdný soubor", "Empty file"),
    ("Přednost ve frontě může dát jen administrátor", "Only an administrator can give priority in the queue"),
    ("Přeskočit můžeš jen svoji písničku", "You can only skip your own song"),
    ("Původní heslo nesedí", "The current password is wrong"),
    ("Registrace je vypnutá", "Registration is closed"),
    ("Skladba neexistuje", "Song not found"),
    ("Soubor neexistuje", "File not found"),
    ("Sám sebe smazat nemůžeš", "You can't delete yourself"),
    ("Tahle písnička už ve frontě je", "This song is already in the queue"),
    ("Tohle jméno už někdo má", "This username is taken"),
    ("Uživatel neexistuje", "User not found"),
    ("Účet je zablokovaný", "The account is blocked"),
    ("Účet ještě čeká na schválení administrátorem", "The account is waiting for approval by an administrator"),
    ("Špatné jméno nebo heslo", "Wrong username or password"),
    ("Tohle může jen administrátor", "Only an administrator can do this"),
    ("Přidávat písničky můžou jen DJové – požádej admina", "Only DJs can add songs – ask the admin"),
    ("Nemáš oprávnění", "You don't have permission"),
    ("Neplatná adresa rádia", "Invalid radio URL"),
    ("Přezdívka nesmí být prázdná", "The nickname can't be empty"),
];

/// (začátek česky, začátek anglicky) – zbytek textu (čísla, názvy) zůstane.
const PREFIX: &[(&str, &str)] = &[
    ("Skladba je delší než ", "The song is longer than "),
    ("Ve frontě můžeš mít najednou max. ", "You can have at most "),
    ("Složka „", "Folder „"),
    ("chyba v theme.json: ", "error in theme.json: "),
    ("chybí soubor fontu ", "missing font file "),
    ("v ZIPu chybí theme.json", "theme.json is missing in the ZIP"),
    ("vestavěný motiv nejde smazat", "a built-in theme can't be deleted"),
];

pub fn translate(msg: &str, lang: &str) -> String {
    if lang != "en" {
        return msg.to_string();
    }
    if let Some((_, en)) = EXACT.iter().find(|(cs, _)| *cs == msg) {
        return en.to_string();
    }
    for (cs, en) in PREFIX {
        if let Some(rest) = msg.strip_prefix(cs) {
            let rest = rest
                .replace(" minut", " minutes")
                .replace(" písniček – počkej, až se nějaká přehraje", " songs in the queue – wait until one is played")
                .replace("“ neexistuje", "“ doesn't exist");
            return format!("{en}{rest}");
        }
    }
    msg.to_string()
}

/// Middleware: přeloží `{"error": "…"}` podle hlavičky `X-Lang`.
pub async fn translate_errors(req: Request, next: Next) -> Response {
    let lang = req.headers().get("x-lang").and_then(|v| v.to_str().ok()).unwrap_or("cs").to_string();
    let resp = next.run(req).await;
    if lang == "cs" || resp.status().is_success() {
        return resp;
    }
    let is_json = resp.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|c| c.starts_with("application/json"));
    if !is_json {
        return resp;
    }
    let (mut parts, body) = resp.into_parts();
    let Ok(bytes) = to_bytes(body, 64 * 1024).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let out = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(mut v) => {
            if let Some(msg) = v.get("error").and_then(|e| e.as_str()).map(str::to_string) {
                v["error"] = serde_json::Value::String(translate(&msg, &lang));
            }
            serde_json::to_vec(&v).unwrap_or_else(|_| bytes.to_vec())
        }
        Err(_) => bytes.to_vec(),
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates() {
        assert_eq!(translate("Nejsi přihlášený", "en"), "You are not logged in");
        assert_eq!(translate("Nejsi přihlášený", "cs"), "Nejsi přihlášený");
        assert_eq!(translate("Skladba je delší než 12 minut", "en"), "The song is longer than 12 minutes");
        assert_eq!(translate("Něco neznámého", "en"), "Něco neznámého");
    }
}
