//! Bible lookups against the two upstream Hungarian Bible APIs, normalised into
//! one passage shape. Lives in the core so every UI — desktop, browser or remote
//! client — gets the same result without its own CORS workaround.
//!
//! szentiras.eu requires a free API key on every `/api/*` call, sent as an
//! `X-API-Key` header. It is held by the `szentiras` connector config, so the
//! caller passes it in. Reference autocomplete (`/kereses/suggest`) is public.

use serde::{Deserialize, Serialize};

/// Upstream base URLs; overridable for testing or a self-hosted mirror.
fn v2_api_url() -> String {
    std::env::var("METOCAST_BIBLE_V2_URL")
        .unwrap_or_else(|_| "https://api.nyiregyhazimetodista.hu".to_string())
}

fn legacy_api_url() -> String {
    std::env::var("METOCAST_BIBLE_LEGACY_URL")
        .unwrap_or_else(|_| "https://szentiras.eu".to_string())
}

pub use metocast_core::events::{BiblePassage, BibleVerse};

// V2 API types (nyiregyhazimetodista.hu)
#[derive(Debug, Serialize, Deserialize, Clone)]
struct V2Verse {
    pub chapter: i32,
    pub verse: i32,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct V2ParsedRef {
    pub book: String,
    pub book_id: i32,
    pub chapter_from: i32,
    pub chapter_to: Option<i32>,
    pub verse_from: Option<i32>,
    pub verse_to: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct V2SuggestResponse {
    pub label: String,
    pub link: String,
    pub hungarian_label: String,
    pub parsed_refs: Vec<V2ParsedRef>,
    pub verses: Vec<V2Verse>,
    pub verses_as_text: Vec<String>,
}

// Legacy API types (szentiras.eu)
#[derive(Debug, Serialize, Deserialize, Clone)]
struct LegacyLocation {
    pub gepi: String,
    pub szep: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LegacyNote {
    pub position: Option<i32>,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct LegacyVerse {
    pub szoveg: String,
    #[serde(default)]
    pub jegyzetek: Vec<LegacyNote>,
    pub hely: LegacyLocation,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyTranslation {
    pub nev: String,
    pub rov: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyAnswer {
    pub versek: Vec<LegacyVerse>,
    pub forditas: LegacyTranslation,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacySearchQuery {
    pub feladat: String,
    pub hivatkozas: String,
    pub forma: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacySearchResponse {
    pub keres: LegacySearchQuery,
    pub valasz: LegacyAnswer,
}

// Suggestion from szentiras.eu
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LegacySuggestion {
    pub cat: String,
    pub label: String,
    pub link: String,
}

fn map_suggestion_label(label: &str) -> String {
    let books = [
        ("Ter", "1Móz"),
        ("Kiv", "2Móz"),
        ("Lev", "3Móz"),
        ("Szám", "4Móz"),
        ("MTörv", "5Móz"),
    ];
    let mut result = label.to_string();
    for (from, to) in books.iter() {
        result = result.replace(from, to);
    }
    result
}

// Remove HTML heading tags from verse text
fn remove_headings(html: &str) -> String {
    let re = regex::Regex::new(r"<h[1-6][^>]*>[\s\S]*?</h[1-6]>")
        .unwrap_or_else(|_| regex::Regex::new("").unwrap());
    re.replace_all(html, "").to_string()
}

// Remove <br> tags from verse text
fn remove_breaks(html: &str) -> String {
    let re =
        regex::Regex::new(r"<br\s*/?>(\s*)?").unwrap_or_else(|_| regex::Regex::new("").unwrap());
    re.replace_all(html, "").to_string()
}

// Clean HTML from verse text
fn clean_verse_text(text: &str) -> String {
    let cleaned = remove_headings(text);
    let cleaned = remove_breaks(&cleaned);
    // Remove any remaining HTML tags
    let re = regex::Regex::new(r"<[^>]*>").unwrap_or_else(|_| regex::Regex::new("").unwrap());
    re.replace_all(&cleaned, "").to_string()
}

/// Fetches a passage from whichever upstream API the translation belongs to.
/// `*_v2` translations use the V2 API; everything else uses the legacy API.
pub async fn fetch_passage(
    reference: &str,
    translation: &str,
    szentiras_api_key: Option<&str>,
) -> Result<BiblePassage, String> {
    match translation.strip_suffix("_v2") {
        Some(v2_translation) => fetch_v2(reference, v2_translation).await,
        None => fetch_legacy(reference, translation, szentiras_api_key).await,
    }
}

async fn get_json<T: serde::de::DeserializeOwned>(
    url: &str,
    api_key: Option<&str>,
) -> Result<T, String> {
    let mut request = reqwest::Client::new().get(url);
    if let Some(key) = api_key {
        request = request.header("X-API-Key", key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 => "szentiras.eu rejected the API key. Set a valid key in the Szentírás connector settings (free at https://szentiras.eu/profile/api-keys).".to_string(),
            403 => "The szentiras.eu API key is disabled.".to_string(),
            429 => "szentiras.eu rate limit reached (60 requests/minute by default). Try again shortly.".to_string(),
            _ => format!("API error: {status}"),
        });
    }

    response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))
}

async fn fetch_v2(reference: &str, translation: &str) -> Result<BiblePassage, String> {
    let url = format!(
        "{}/suggest/{}/{}",
        v2_api_url(),
        urlencoding::encode(reference),
        translation
    );
    let data: V2SuggestResponse = get_json(&url, None).await?;

    let label = if data.hungarian_label.is_empty() {
        reference.to_string()
    } else {
        data.hungarian_label
    };

    Ok(BiblePassage {
        label,
        verses: data
            .verses
            .into_iter()
            .map(|v| BibleVerse {
                chapter: v.chapter,
                verse: v.verse,
                text: clean_verse_text(&v.text),
            })
            .collect(),
    })
}

async fn fetch_legacy(
    reference: &str,
    translation: &str,
    api_key: Option<&str>,
) -> Result<BiblePassage, String> {
    // Strip the leading slash suggestions carry, and encode only spaces so the
    // commas of Hungarian verse notation survive.
    let clean_ref = reference.trim_start_matches('/').replace(' ', "%20");
    let url = format!(
        "{}/api/idezet/{}/{}",
        legacy_api_url(),
        clean_ref,
        translation
    );
    let data: LegacySearchResponse = get_json(&url, api_key).await?;

    Ok(BiblePassage {
        label: data.keres.hivatkozas,
        verses: data
            .valasz
            .versek
            .into_iter()
            .enumerate()
            .map(|(index, v)| {
                let (chapter, verse) = parse_gepi(&v.hely.gepi, index);
                BibleVerse {
                    chapter,
                    verse,
                    text: clean_verse_text(&v.szoveg),
                }
            })
            .collect(),
    })
}

/// szentiras.eu calls this the "machine code". Current responses use the USX book
/// code with chapter and verse, e.g. `JHN_3_16`; older ones packed it as
/// book(3) + chapter(3) + verse(3) digits, e.g. `001001016`.
fn parse_gepi(gepi: &str, index: usize) -> (i32, i32) {
    let fallback = (1, index as i32 + 1);

    let mut parts = gepi.rsplit('_');
    if let (Some(verse), Some(chapter)) = (parts.next(), parts.next())
        && let (Ok(verse), Ok(chapter)) = (verse.parse(), chapter.parse())
    {
        return (chapter, verse);
    }

    if gepi.len() < 6 {
        return fallback;
    }
    let tail = &gepi[gepi.len() - 6..];
    (
        tail[..3].parse().unwrap_or(fallback.0),
        tail[3..].parse().unwrap_or(fallback.1),
    )
}

/// Autocomplete suggestions from the legacy API, filtered to references.
pub async fn fetch_suggestions(term: &str) -> Result<Vec<LegacySuggestion>, String> {
    if term.chars().count() < 2 {
        return Ok(Vec::new());
    }

    let url = format!(
        "{}/kereses/suggest?term={}",
        legacy_api_url(),
        urlencoding::encode(term)
    );
    // Autocomplete is public — no key needed.
    let suggestions: Vec<LegacySuggestion> = get_json(&url, None).await?;

    Ok(suggestions
        .into_iter()
        .filter(|s| s.cat == "ref")
        .map(|s| LegacySuggestion {
            label: map_suggestion_label(&s.label),
            link: map_suggestion_label(&s.link),
            cat: s.cat,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gepi_locations() {
        // Current USX form.
        assert_eq!(parse_gepi("JHN_3_16", 0), (3, 16));
        assert_eq!(parse_gepi("1CO_13_10", 2), (13, 10));
        // Legacy all-digit form.
        assert_eq!(parse_gepi("001001016", 0), (1, 16));
        assert_eq!(parse_gepi("043003016", 7), (3, 16));
        // Too short, or non-numeric: fall back to 1 and the verse's position.
        assert_eq!(parse_gepi("12", 4), (1, 5));
        assert_eq!(parse_gepi("00100xxxx", 2), (1, 3));
        assert_eq!(parse_gepi("JHN_x_y", 1), (1, 2));
    }

    #[test]
    fn strips_markup_from_verse_text() {
        assert_eq!(
            clean_verse_text("<h2>Heading</h2>In the <i>beginning</i><br/>God"),
            "In the beginningGod"
        );
    }
}
