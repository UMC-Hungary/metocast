//! The welcome screen: a photo, the logo and the greeting as one SVG slide, which every screen
//! already draws (web presenter, the apps, the receiver). Twelve photos rotate while it is on
//! screen. The text is white, and a dark overlay keeps it readable on any photo: its strength is
//! worked out from the photo's own brightness behind the text.

use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use base64::Engine;
use chrono::{Datelike, Local};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::database::settings;
use crate::server::AppState;
use crate::server::presenter::presenter_svg;
use crate::server::presenter::{
    ParagraphContent, PresenterRenderMode, PresenterState, SlideContent, SvgSlideContent,
};
use crate::server::websocket;

pub const PHOTO_COUNT: usize = 12;
const SETTINGS_KEY: &str = "welcome_screen";
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
/// WCAG AAA for large text. Every line here is large (the smallest is 5% of the slide height),
/// and this is what the overlay guarantees against nearly all of the photo behind the text.
const TARGET_CONTRAST: f64 = 4.5;
/// The photo's brightness behind the text is taken at this percentile, so a few bright specks
/// don't darken the whole photo but a bright sky does.
const BRIGHT_PERCENTILE: f64 = 0.95;

static DEFAULT_PHOTOS: [&[u8]; PHOTO_COUNT] = [
    include_bytes!("../../assets/welcome/01.jpg"),
    include_bytes!("../../assets/welcome/02.jpg"),
    include_bytes!("../../assets/welcome/03.jpg"),
    include_bytes!("../../assets/welcome/04.jpg"),
    include_bytes!("../../assets/welcome/05.jpg"),
    include_bytes!("../../assets/welcome/06.jpg"),
    include_bytes!("../../assets/welcome/07.jpg"),
    include_bytes!("../../assets/welcome/08.jpg"),
    include_bytes!("../../assets/welcome/09.jpg"),
    include_bytes!("../../assets/welcome/10.jpg"),
    include_bytes!("../../assets/welcome/11.jpg"),
    include_bytes!("../../assets/welcome/12.jpg"),
];

/// Bumped on every show, so only the newest rotation keeps going.
static ROTATION: AtomicU64 = AtomicU64::new(0);

/// `GET`/`PUT /api/presenter/welcome`. `hasLogo` and `replacedPhotos` are read-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WelcomeSettings {
    pub title: String,
    pub subtitle: String,
    pub church_name: String,
    pub interval_seconds: u32,
    #[serde(skip_deserializing)]
    pub has_logo: bool,
    /// Which photos (1–12) were replaced, so a client can offer to reset them.
    #[serde(skip_deserializing)]
    pub replaced_photos: Vec<u32>,
}

impl Default for WelcomeSettings {
    fn default() -> Self {
        Self {
            title: "Isten hozott!".to_string(),
            subtitle: "Az ÚR áldása legyen életeden!".to_string(),
            church_name: String::new(),
            interval_seconds: 20,
            has_logo: false,
            replaced_photos: Vec::new(),
        }
    }
}

/// The greeting a new event's welcome step is named after.
pub async fn greeting(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> sqlx::Result<String> {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1")
        .bind(SETTINGS_KEY)
        .fetch_optional(&mut **tx)
        .await?;
    let stored: WelcomeSettings = raw
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    Ok(stored.title)
}

async fn asset(pool: &PgPool, name: &str) -> sqlx::Result<Option<(String, Vec<u8>)>> {
    sqlx::query_as("SELECT mime, bytes FROM welcome_assets WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await
}

fn photo_name(number: usize) -> String {
    format!("{number:02}")
}

/// Photo `number` (1–12): the replacement if one was uploaded, else the default.
async fn photo(pool: &PgPool, number: usize) -> sqlx::Result<Vec<u8>> {
    Ok(match asset(pool, &photo_name(number)).await? {
        Some((_, bytes)) => bytes,
        None => DEFAULT_PHOTOS[number - 1].to_vec(),
    })
}

async fn load_settings(pool: &PgPool) -> sqlx::Result<WelcomeSettings> {
    let mut stored: WelcomeSettings = settings::get_json(pool, SETTINGS_KEY).await;
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM welcome_assets")
        .fetch_all(pool)
        .await?;
    stored.has_logo = names.iter().any(|n| n == "logo");
    stored.replaced_photos = names.iter().filter_map(|n| n.parse().ok()).collect();
    stored.replaced_photos.sort_unstable();
    Ok(stored)
}

/// Relative luminance of an sRGB pixel, as WCAG defines it.
fn luminance(pixel: &image::Rgb<u8>) -> f64 {
    let channel = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(pixel[0]) + 0.7152 * channel(pixel[1]) + 0.0722 * channel(pixel[2])
}

/// How dark the overlay behind the text must be (0–1) for white text to reach
/// `TARGET_CONTRAST` against the photo there. A black overlay of opacity `a` turns luminance `L`
/// into `(1 − a)·L`, and white on `L` has contrast `1.05 / (L + 0.05)`.
fn overlay_opacity(photo: &image::RgbImage) -> f64 {
    let (w, h) = photo.dimensions();
    let mut lums: Vec<f64> = (h * 30 / 100..h * 95 / 100)
        .flat_map(|y| (w / 10..w * 9 / 10).map(move |x| (x, y)))
        .map(|(x, y)| luminance(photo.get_pixel(x, y)))
        .collect();
    if lums.is_empty() {
        return 0.5;
    }
    lums.sort_unstable_by(f64::total_cmp);
    let bright = lums[((lums.len() - 1) as f64 * BRIGHT_PERCENTILE) as usize];
    let allowed = 1.05 / TARGET_CONTRAST - 0.05;
    // A little overlay even on a dark photo keeps the text calm; never black it out completely.
    (1.0 - allowed / bright.max(allowed)).clamp(0.15, 0.9)
}

/// The photo cover-cropped to the slide, as a JPEG data URI, with the overlay it needs.
fn prepare_photo(bytes: &[u8]) -> Result<(String, f64), String> {
    let photo = image::load_from_memory(bytes)
        .map_err(|e| format!("Couldn't read the photo: {e}"))?
        .resize_to_fill(WIDTH, HEIGHT, image::imageops::FilterType::CatmullRom)
        .to_rgb8();
    let opacity = overlay_opacity(&photo);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 82)
        .encode_image(&photo)
        .map_err(|e| format!("Couldn't encode the photo: {e}"))?;
    let uri = format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    );
    Ok((uri, opacity))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One centred line in white with a soft dark copy under it, shrunk to fit the slide's width.
fn line(svg: &mut String, text: &str, y: f64, size: f64, weight: u32) {
    // Helvetica-like glyphs average about 0.55 em; keep a margin either side.
    let max = f64::from(WIDTH) * 0.84;
    let size = size.min(max / (text.chars().count().max(1) as f64 * 0.55));
    let text = escape(text);
    for (dy, fill) in [
        (size * 0.05, "fill=\"#000\" fill-opacity=\"0.55\""),
        (0.0, "fill=\"#fff\""),
    ] {
        svg.push_str(&format!(
            "<text x=\"{x}\" y=\"{y}\" font-family=\"Helvetica\" font-size=\"{size:.1}\" font-weight=\"{weight}\" text-anchor=\"middle\" {fill}>{text}</text>\n",
            x = f64::from(WIDTH) / 2.0 + dy,
            y = y + dy,
        ));
    }
}

/// The welcome slide as SVG: photo, overlay, logo top right, title, lines and footer.
fn slide_svg(
    photo: &[u8],
    logo: Option<(&str, &[u8])>,
    title: &str,
    lines: &[String],
    footer: &[String],
) -> Result<String, String> {
    let (photo, opacity) = prepare_photo(photo)?;
    let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" width=\"{WIDTH}\" height=\"{HEIGHT}\">\n\
         <defs><linearGradient id=\"scrim\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\
         <stop offset=\"0\" stop-color=\"#000\" stop-opacity=\"{top:.3}\"/>\
         <stop offset=\"0.3\" stop-color=\"#000\" stop-opacity=\"{opacity:.3}\"/>\
         <stop offset=\"1\" stop-color=\"#000\" stop-opacity=\"{opacity:.3}\"/></linearGradient></defs>\n\
         <image x=\"0\" y=\"0\" width=\"{WIDTH}\" height=\"{HEIGHT}\" preserveAspectRatio=\"xMidYMid slice\" xlink:href=\"{photo}\"/>\n\
         <rect x=\"0\" y=\"0\" width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#scrim)\"/>\n",
        top = opacity * 0.35,
    );
    if let Some((mime, bytes)) = logo {
        let uri = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        svg.push_str(&format!(
            "<image x=\"{x}\" y=\"{y}\" width=\"{s}\" height=\"{s}\" preserveAspectRatio=\"xMaxYMin meet\" xlink:href=\"{uri}\"/>\n",
            s = h * 0.2,
            x = w - h * 0.2 - h * 0.05,
            y = h * 0.05,
        ));
    }
    line(&mut svg, title, h * 0.47, h * 0.11, 700);
    let mut y = h * 0.6;
    for text in lines {
        line(&mut svg, text, y, h * 0.055, 700);
        y += h * 0.075;
    }
    let mut y = h * (0.88 - 0.065 * footer.len().saturating_sub(1) as f64);
    for text in footer {
        line(&mut svg, text, y, h * 0.05, 600);
        y += h * 0.065;
    }
    svg.push_str("</svg>\n");
    // Outlines instead of text, so no screen needs the font.
    Ok(presenter_svg::flatten_text_to_paths(
        &svg,
        &presenter_svg::build_flatten_options(),
    ))
}

/// "2026. október 25. vasárnap", as Hungarian churches write the date.
fn hungarian_date(date: chrono::NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "január",
        "február",
        "március",
        "április",
        "május",
        "június",
        "július",
        "augusztus",
        "szeptember",
        "október",
        "november",
        "december",
    ];
    const DAYS: [&str; 7] = [
        "hétfő",
        "kedd",
        "szerda",
        "csütörtök",
        "péntek",
        "szombat",
        "vasárnap",
    ];
    format!(
        "{}. {} {}. {}",
        date.year(),
        MONTHS[date.month0() as usize],
        date.day(),
        DAYS[date.weekday().num_days_from_monday() as usize]
    )
}

/// The welcome slide for a step, on photo `index` (0–11), with its text also as plain
/// paragraphs for the Editorial design and text-only views.
async fn deck(
    pool: &PgPool,
    index: usize,
    title: &str,
    notes: &str,
    event_date: chrono::DateTime<chrono::Utc>,
) -> Result<PresenterState, String> {
    let stored = load_settings(pool).await.map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = notes
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if lines.is_empty() && !stored.subtitle.trim().is_empty() {
        lines.push(stored.subtitle.trim().to_string());
    }
    let footer: Vec<String> = [
        stored.church_name.trim().to_string(),
        hungarian_date(event_date.with_timezone(&Local).date_naive()),
    ]
    .into_iter()
    .filter(|l| !l.is_empty())
    .collect();
    let photo = photo(pool, index + 1).await.map_err(|e| e.to_string())?;
    let logo = asset(pool, "logo").await.map_err(|e| e.to_string())?;
    let svg = slide_svg(
        &photo,
        logo.as_ref()
            .map(|(mime, bytes)| (mime.as_str(), bytes.as_slice())),
        title.trim(),
        &lines,
        &footer,
    )?;
    let mut text = vec![ParagraphContent {
        lines: vec![title.trim().to_string()],
        align: "center".to_string(),
        font_size_pt: 52.0,
    }];
    text.extend(
        [lines, footer]
            .into_iter()
            .filter(|group| !group.is_empty())
            .map(|lines| ParagraphContent {
                lines,
                align: "center".to_string(),
                font_size_pt: 32.0,
            }),
    );
    Ok(PresenterState {
        loaded: true,
        file_path: Some(title.trim().to_string()),
        current_slide: 1,
        total_slides: 1,
        render_mode: PresenterRenderMode::Svg,
        slides: vec![SlideContent {
            index: 1,
            paragraphs: text,
        }],
        svg_slides: vec![SvgSlideContent {
            index: 1,
            svg,
            width_px: WIDTH,
            height_px: HEIGHT,
        }],
        muted: false,
        slide_width_emu: 12192000,
        slide_height_emu: 6858000,
    })
}

async fn put_on_screen(state: &AppState, deck: PresenterState) {
    *state.presenter_state.write().await = deck;
    websocket::broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
        .await;
    websocket::broadcast_presentation_status(&state.ws_clients, state).await;
}

/// Shows a welcome step in the web presenter and keeps its photo rotating until another step
/// or deck takes the screen. Each service starts on a different photo (by its week).
pub async fn show(
    state: &AppState,
    event_id: Uuid,
    item_id: Uuid,
    title: &str,
    notes: &str,
    event_date: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let first = event_date.iso_week().week() as usize % PHOTO_COUNT;
    put_on_screen(
        state,
        deck(&state.pool, first, title, notes, event_date).await?,
    )
    .await;

    let rotation = ROTATION.fetch_add(1, Ordering::SeqCst) + 1;
    let interval = load_settings(&state.pool)
        .await
        .map_err(|e| e.to_string())?
        .interval_seconds
        .clamp(5, 3600);
    let state = state.clone();
    let (title, notes) = (title.to_string(), notes.to_string());
    tokio::spawn(async move {
        let mut index = first;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(u64::from(interval))).await;
            let still_here = ROTATION.load(Ordering::SeqCst) == rotation
                && state
                    .order_position
                    .read()
                    .await
                    .is_some_and(|p| p.event_id == event_id && p.item_id == item_id)
                && state.presenter_state.read().await.file_path.as_deref() == Some(title.trim());
            if !still_here {
                return;
            }
            index = (index + 1) % PHOTO_COUNT;
            match deck(&state.pool, index, &title, &notes, event_date).await {
                Ok(deck) => put_on_screen(&state, deck).await,
                Err(error) => {
                    tracing::warn!(%error, "welcome screen: couldn't show the next photo");
                    return;
                }
            }
        }
    });
    Ok(())
}

// ── REST ──────────────────────────────────────────────────────────────────────

fn internal(error: impl std::fmt::Display) -> axum::response::Response {
    tracing::error!("welcome screen: {error}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn bad_request(message: String) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    match load_settings(&state.pool).await {
        Ok(stored) => Json(stored).into_response(),
        Err(e) => internal(e),
    }
}

pub async fn put_settings(
    State(state): State<AppState>,
    Json(body): Json<WelcomeSettings>,
) -> impl IntoResponse {
    if body.title.chars().count() > 120
        || body.subtitle.chars().count() > 200
        || body.church_name.chars().count() > 120
    {
        return bad_request("The welcome text is too long".to_string());
    }
    if !(5..=3600).contains(&body.interval_seconds) {
        return bad_request("The interval must be between 5 and 3600 seconds".to_string());
    }
    if let Err(e) = settings::set_json(&state.pool, SETTINGS_KEY, &body).await {
        return internal(e);
    }
    match load_settings(&state.pool).await {
        Ok(stored) => Json(stored).into_response(),
        Err(e) => internal(e),
    }
}

fn photo_number(number: usize) -> Option<usize> {
    (1..=PHOTO_COUNT).contains(&number).then_some(number)
}

pub async fn get_photo(
    State(state): State<AppState>,
    Path(number): Path<usize>,
) -> impl IntoResponse {
    let Some(number) = photo_number(number) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match photo(&state.pool, number).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/jpeg")], bytes).into_response(),
        Err(e) => internal(e),
    }
}

/// Stores an uploaded photo cropped to 16:9 at 1920×1080, the size the defaults have.
pub async fn put_photo(
    State(state): State<AppState>,
    Path(number): Path<usize>,
    body: Bytes,
) -> impl IntoResponse {
    let Some(number) = photo_number(number) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let photo = match image::load_from_memory(&body) {
        Ok(photo) => photo.resize_to_fill(1920, 1080, image::imageops::FilterType::CatmullRom),
        Err(e) => return bad_request(format!("That isn't a photo Metocast can read: {e}")),
    };
    let mut jpeg = Vec::new();
    if let Err(e) = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
        .encode_image(&photo.to_rgb8())
    {
        return internal(e);
    }
    store(&state.pool, &photo_name(number), "image/jpeg", jpeg).await
}

pub async fn delete_photo(
    State(state): State<AppState>,
    Path(number): Path<usize>,
) -> impl IntoResponse {
    match photo_number(number) {
        Some(n) => remove(&state.pool, &photo_name(n)).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn get_logo(State(state): State<AppState>) -> impl IntoResponse {
    match asset(&state.pool, "logo").await {
        Ok(Some((mime, bytes))) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => internal(e),
    }
}

/// Takes a PNG (kept as sent, so transparency survives) or an SVG.
pub async fn put_logo(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let mime = if usvg::Tree::from_data(&body, &usvg::Options::default()).is_ok() {
        "image/svg+xml"
    } else if image::load_from_memory_with_format(&body, image::ImageFormat::Png).is_ok() {
        "image/png"
    } else {
        return bad_request("The logo must be a PNG or an SVG file".to_string());
    };
    store(&state.pool, "logo", mime, body.to_vec()).await
}

pub async fn delete_logo(State(state): State<AppState>) -> impl IntoResponse {
    remove(&state.pool, "logo").await
}

async fn store(pool: &PgPool, name: &str, mime: &str, bytes: Vec<u8>) -> axum::response::Response {
    let result = sqlx::query(
        "INSERT INTO welcome_assets (name, mime, bytes) VALUES ($1, $2, $3) \
         ON CONFLICT (name) DO UPDATE SET mime = EXCLUDED.mime, bytes = EXCLUDED.bytes, updated_at = now()",
    )
    .bind(name)
    .bind(mime)
    .bind(bytes)
    .execute(pool)
    .await;
    match result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

async fn remove(pool: &PgPool, name: &str) -> axum::response::Response {
    match sqlx::query("DELETE FROM welcome_assets WHERE name = $1")
        .bind(name)
        .execute(pool)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every default photo, with the overlay it gets, keeps white text at the target contrast
    /// against all but the brightest specks behind it.
    #[test]
    fn white_text_is_readable_on_every_default_photo() {
        for (index, bytes) in DEFAULT_PHOTOS.iter().enumerate() {
            let photo = image::load_from_memory(bytes)
                .expect("default photo decodes")
                .resize_to_fill(WIDTH, HEIGHT, image::imageops::FilterType::Triangle)
                .to_rgb8();
            let opacity = overlay_opacity(&photo);
            let (w, h) = photo.dimensions();
            let mut lums: Vec<f64> = (h * 30 / 100..h * 95 / 100)
                .flat_map(|y| (w / 10..w * 9 / 10).map(move |x| (x, y)))
                .map(|(x, y)| (1.0 - opacity) * luminance(photo.get_pixel(x, y)))
                .collect();
            lums.sort_unstable_by(f64::total_cmp);
            let bright = lums[((lums.len() - 1) as f64 * BRIGHT_PERCENTILE) as usize];
            let contrast = 1.05 / (bright + 0.05);
            assert!(
                contrast >= TARGET_CONTRAST - 0.01,
                "photo {:02}: contrast {contrast:.2} with overlay {opacity:.2}",
                index + 1
            );
        }
    }

    #[test]
    fn the_date_reads_as_hungarian_churches_write_it() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 25).expect("valid date");
        assert_eq!(hungarian_date(date), "2026. október 25. vasárnap");
    }

    #[test]
    fn a_dark_photo_gets_little_overlay_and_a_white_one_a_lot() {
        let dark = image::RgbImage::from_pixel(64, 36, image::Rgb([20, 20, 20]));
        let white = image::RgbImage::from_pixel(64, 36, image::Rgb([255, 255, 255]));
        assert!(overlay_opacity(&dark) <= 0.2);
        assert!(overlay_opacity(&white) >= 0.8);
    }

    #[test]
    fn the_slide_is_an_svg_with_the_photo() {
        let svg = slide_svg(
            DEFAULT_PHOTOS[0],
            None,
            "Isten hozott!",
            &["Az ÚR áldása legyen életeden!".to_string()],
            &["2026. október 25. vasárnap".to_string()],
        )
        .expect("slide renders");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("data:image/jpeg;base64,"));
    }
}
