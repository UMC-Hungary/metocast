//! The welcome screen: a photo, the logo and the greeting as one SVG slide, which every screen
//! already draws (web presenter, the apps, the receiver), in Montserrat. Each month has its own
//! photo. The text is white, and a dark overlay keeps it readable on any photo: its strength is
//! worked out from the photo's own brightness behind the text.

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use base64::Engine;
use chrono::{Datelike, Local};
use serde_json::json;
use sqlx::PgPool;

pub use metocast_core::slides::WelcomeSettings;

use crate::database::settings;
use std::sync::OnceLock;

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
/// Where the top band, with the logo and church name, ends (fraction of the height).
const TOP_BAND: f64 = 0.2;
/// Where the lower third, with the greeting, blessing and date, begins.
const LOWER_THIRD: f64 = 0.66;

/// The church's logo (the cross and flame with its name), with the cross and lettering in white
/// so it reads on a photo. Replaceable in the settings.
static DEFAULT_LOGO: &[u8] = include_bytes!("../../assets/welcome/logo.svg");

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

/// The brightest a pixel behind white text may be for `TARGET_CONTRAST`: white on luminance `L`
/// has contrast `1.05 / (L + 0.05)`. Aimed a little higher, so rounding to 8-bit colour can't
/// take a pixel past it.
fn max_luminance() -> f64 {
    1.05 / (TARGET_CONTRAST + 0.1) - 0.05
}

/// Brings a luminance under `cap` with a soft knee: dark and middle tones stay exactly as they
/// are, and only highlights are compressed, never clipped flat.
fn soft_cap(l: f64, cap: f64) -> f64 {
    let knee = cap * 0.55;
    if l <= knee {
        l
    } else {
        knee + (cap - knee) * (1.0 - (-(l - knee) / (cap - knee)).exp())
    }
}

fn svg_open() -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" width=\"{WIDTH}\" height=\"{HEIGHT}\">\n"
    )
}

fn to_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f64) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much toning the point (x, y) gets (0–1, each 0 at the top or left): all of it under the
/// logo in the top-left corner (and the top-right corner when a church name is written there)
/// and across the lower third, fading out over long gradients, so there is never a visible edge.
fn toning(x: f64, y: f64, top_right: bool) -> f64 {
    let top = 1.0 - smoothstep(TOP_BAND, TOP_BAND + 0.16, y);
    let left = 1.0 - smoothstep(0.42, 0.62, x);
    let right = if top_right {
        smoothstep(0.38, 0.58, x)
    } else {
        0.0
    };
    (top * left.max(right)).max(smoothstep(LOWER_THIRD - 0.26, LOWER_THIRD, y))
}

/// How much of the blurred photo the row at height `y` shows, 0–1: none above the middle,
/// ramping to all of it across the lower third (a progressive blur).
fn blurring(y: f64) -> f64 {
    smoothstep(0.45, 0.74, y)
}

/// Tones down highlights so white text reaches `TARGET_CONTRAST` against every pixel where text
/// sits, keeping each pixel's colour and leaving dark and middle tones as they are.
fn tone(pixel: &mut image::Rgb<u8>, weight: f64) {
    let cap = max_luminance();
    let linear = pixel.0.map(to_linear);
    let l = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    if weight == 0.0 || l <= cap * 0.55 {
        return;
    }
    let target = l + (soft_cap(l, cap) - l) * weight;
    pixel.0 = linear.map(|c| to_srgb(c * target / l));
}

/// The photo cover-cropped to the slide: sharp and untouched across the middle, blurred
/// progressively into the lower third, and toned under the top band and the lower third.
fn treat_photo(photo: &mut image::RgbImage, top_right: bool) {
    let blurred = image::imageops::fast_blur(photo, 18.0);
    let (w, h) = (f64::from(photo.width()), f64::from(photo.height()));
    for (x, y, pixel) in photo.enumerate_pixels_mut() {
        let row = (f64::from(y) + 0.5) / h;
        let column = (f64::from(x) + 0.5) / w;
        let blur = blurring(row);
        if blur > 0.0 {
            let soft = blurred.get_pixel(x, y).0;
            pixel.0 = std::array::from_fn(|c| {
                (f64::from(pixel.0[c]) * (1.0 - blur) + f64::from(soft[c]) * blur).round() as u8
            });
        }
        tone(pixel, toning(column, row, top_right));
    }
}

/// The photo prepared for the slide, as a JPEG data URI.
fn prepare_photo(bytes: &[u8], top_right: bool) -> Result<String, String> {
    let mut photo = image::load_from_memory(bytes)
        .map_err(|e| format!("Couldn't read the photo: {e}"))?
        .resize_to_fill(WIDTH, HEIGHT, image::imageops::FilterType::CatmullRom)
        .to_rgb8();
    treat_photo(&mut photo, top_right);
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 84)
        .encode_image(&photo)
        .map_err(|e| format!("Couldn't encode the photo: {e}"))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    ))
}

/// A logo's width for a height of 1: from an SVG's size or a PNG's pixels.
fn logo_aspect(mime: &str, bytes: &[u8]) -> f64 {
    let size = if mime == "image/svg+xml" {
        usvg::Tree::from_data(bytes, &usvg::Options::default())
            .ok()
            .map(|tree| {
                (
                    f64::from(tree.size().width()),
                    f64::from(tree.size().height()),
                )
            })
    } else {
        image::load_from_memory(bytes)
            .ok()
            .map(|image| (f64::from(image.width()), f64::from(image.height())))
    };
    size.filter(|(_, h)| *h > 0.0).map_or(1.0, |(w, h)| w / h)
}

/// Montserrat (SIL Open Font License), built in so the slide looks the same wherever the server
/// runs; the text becomes outlines, so no screen needs it either.
fn text_options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = usvg::Options::default();
        let fonts = options.fontdb_mut();
        fonts.load_font_data(include_bytes!("../../assets/fonts/Montserrat-Medium.ttf").to_vec());
        fonts.load_font_data(include_bytes!("../../assets/fonts/Montserrat-SemiBold.ttf").to_vec());
        fonts.load_font_data(include_bytes!("../../assets/fonts/Montserrat-Bold.ttf").to_vec());
        options.font_family = "Montserrat".to_string();
        options
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Montserrat's glyphs average about 0.62 em.
fn text_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * size * 0.62
}

/// How a line of text is set.
struct Style {
    size: f64,
    weight: u32,
    /// `start`, `middle` or `end`.
    anchor: &'static str,
    /// Letter spacing in em.
    tracking: f64,
    /// The line shrinks to fit this width.
    max_width: f64,
}

/// One line of white text at `(x, y)`; returns the font size it was set at.
fn text(svg: &mut String, value: &str, (x, y): (f64, f64), style: Style) -> f64 {
    let Style {
        size,
        weight,
        anchor,
        tracking,
        max_width,
    } = style;
    let size = size.min(max_width / (text_width(value, 1.0) + 0.0001));
    let value = escape(value);
    svg.push_str(&format!(
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"Montserrat\" font-size=\"{size:.1}\" font-weight=\"{weight}\" letter-spacing=\"{ls:.2}\" text-anchor=\"{anchor}\" fill=\"#fff\">{value}</text>\n",
        ls = tracking * size,
    ));
    size
}

/// The welcome slide as SVG, in the lower-third layout: the photo, sharp at the top and melting
/// into a soft, toned blur at the bottom where the greeting, blessing and date sit; the logo top
/// left. No shadows: the photo itself is prepared so white text reads on it.
fn slide_svg(
    photo: &[u8],
    logo: Option<(&str, &[u8])>,
    church: &str,
    title: &str,
    lines: &[String],
    date: &str,
) -> Result<String, String> {
    // The church name goes top right beside a logo, so that corner is toned too.
    let photo = prepare_photo(photo, logo.is_some() && !church.is_empty())?;
    let layer = foreground(logo, church, title, lines, date);
    let svg = format!(
        "{}<image x=\"0\" y=\"0\" width=\"{WIDTH}\" height=\"{HEIGHT}\" preserveAspectRatio=\"xMidYMid slice\" xlink:href=\"{photo}\"/>\n{layer}</svg>\n",
        svg_open()
    );
    // Outlines instead of text, so no screen needs the font.
    let flat = presenter_svg::flatten_text_to_paths(&svg, text_options());
    Ok(with_slow_zoom(&flat))
}

/// The text and the logo as SVG elements. Everything written sits where `prepare_photo` has
/// toned the photo: the top band (logo, church name) and the lower third (the rest).
fn foreground(
    logo: Option<(&str, &[u8])>,
    church: &str,
    title: &str,
    lines: &[String],
    date: &str,
) -> String {
    let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));
    let margin = 64.0;
    let mut layer = String::new();
    // The logo top left at its own proportions (the default carries the church's name).
    let logo_height = h * 0.12;
    let mut logo_width = 0.0;
    if let Some((mime, bytes)) = logo {
        logo_width = (logo_height * logo_aspect(mime, bytes)).min(w * 0.45);
        let uri = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        layer.push_str(&format!(
            "<image x=\"{margin}\" y=\"{y}\" width=\"{logo_width:.1}\" height=\"{logo_height:.1}\" preserveAspectRatio=\"xMinYMin meet\" xlink:href=\"{uri}\"/>\n",
            y = margin - 20.0,
        ));
    }
    // Nothing is smaller than 4.2% of the height (30 px here): readable from the back.
    let small = h * 0.042;
    if !church.is_empty() {
        // Top right beside a logo, top left without one.
        let (x, anchor) = if logo_width > 0.0 {
            (w - margin, "end")
        } else {
            (margin, "start")
        };
        text(
            &mut layer,
            church,
            (x, margin + small * 0.6),
            Style {
                size: small,
                weight: 600,
                anchor,
                tracking: 0.0,
                max_width: w - 3.0 * margin - logo_width,
            },
        );
    }
    // The lower third: the greeting big and left-aligned, the blessing under it, the date on
    // the right of the blessing's line.
    let baseline = h - 56.0;
    let blessing_lines = lines.len().max(1) as f64;
    let title_baseline = baseline - small * 1.5 * blessing_lines - h * 0.035;
    text(
        &mut layer,
        title,
        (margin, title_baseline),
        Style {
            size: h * 0.14,
            weight: 700,
            anchor: "start",
            tracking: -0.025,
            max_width: w - 2.0 * margin,
        },
    );
    let mut y = baseline - small * 1.5 * (blessing_lines - 1.0);
    for value in lines {
        text(
            &mut layer,
            value,
            (margin, y),
            Style {
                size: small * 1.1,
                weight: 500,
                anchor: "start",
                tracking: 0.0,
                max_width: w * 0.56,
            },
        );
        y += small * 1.5;
    }
    text(
        &mut layer,
        date,
        (w - margin, baseline),
        Style {
            size: small,
            weight: 600,
            anchor: "end",
            tracking: 0.0,
            max_width: w * 0.34,
        },
    );
    layer
}

/// Wraps the photo (the first `<image>`) in a group that zooms in by 8% over 20 seconds and back,
/// for the screens that play CSS animation in SVG (the web presenter and the apps). Others, such
/// as the Raspberry Pi receiver, ignore it and show the photo still.
fn with_slow_zoom(svg: &str) -> String {
    let Some(start) = svg.find("<image") else {
        return svg.to_string();
    };
    let Some(len) = svg[start..].find("/>") else {
        return svg.to_string();
    };
    let end = start + len + 2;
    let style = "<style>@keyframes metocast-zoom{from{transform:scale(1)}to{transform:scale(1.08)}}\
                 .metocast-zoom{transform-origin:50% 50%;transform-box:view-box;animation:metocast-zoom 20s ease-in-out infinite alternate}</style>";
    format!(
        "{}{style}<g class=\"metocast-zoom\">{}</g>{}",
        &svg[..start],
        &svg[start..end],
        &svg[end..]
    )
}

/// The service date in the welcome screen's language: "2026. október 25. vasárnap" as Hungarian
/// churches write it, or "Sunday, October 25, 2026".
fn service_date(date: chrono::NaiveDate, language: &str) -> String {
    const HU_MONTHS: [&str; 12] = [
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
    const HU_DAYS: [&str; 7] = [
        "hétfő",
        "kedd",
        "szerda",
        "csütörtök",
        "péntek",
        "szombat",
        "vasárnap",
    ];
    if language == "en" {
        return date.format("%A, %B %-d, %Y").to_string();
    }
    format!(
        "{}. {} {}. {}",
        date.year(),
        HU_MONTHS[date.month0() as usize],
        date.day(),
        HU_DAYS[date.weekday().num_days_from_monday() as usize]
    )
}

/// The welcome slide's SVG for a service on `date`: that month's photo, the logo, `title`, the
/// step's `notes` (or the blessing) and the church name with the date. Also returns the lines.
async fn render(
    pool: &PgPool,
    title: &str,
    notes: &str,
    date: chrono::NaiveDate,
) -> Result<(String, Vec<String>, Vec<String>), String> {
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
        service_date(date, &stored.language),
    ]
    .into_iter()
    .filter(|l| !l.is_empty())
    .collect();
    let photo = photo(pool, date.month() as usize)
        .await
        .map_err(|e| e.to_string())?;
    let logo = asset(pool, "logo")
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| ("image/svg+xml".to_string(), DEFAULT_LOGO.to_vec()));
    let svg = slide_svg(
        &photo,
        Some((logo.0.as_str(), logo.1.as_slice())),
        stored.church_name.trim(),
        title.trim(),
        &lines,
        &service_date(date, &stored.language),
    )?;
    Ok((svg, lines, footer))
}

/// The welcome slide for a step, with its text also as plain paragraphs for the Editorial design
/// and text-only views.
async fn deck(
    pool: &PgPool,
    title: &str,
    notes: &str,
    event_date: chrono::DateTime<chrono::Utc>,
) -> Result<PresenterState, String> {
    let (svg, lines, footer) = render(
        pool,
        title,
        notes,
        event_date.with_timezone(&Local).date_naive(),
    )
    .await?;
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

/// Shows a welcome step in the web presenter, on the photo of the event's month.
pub async fn show(
    state: &AppState,
    title: &str,
    notes: &str,
    event_date: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let deck = deck(&state.pool, title, notes, event_date).await?;
    *state.presenter_state.write().await = deck;
    websocket::broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
        .await;
    websocket::broadcast_presentation_status(&state.ws_clients, state).await;
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
    if !matches!(body.language.as_str(), "hu" | "en") {
        return bad_request("The language must be hu or en".to_string());
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

/// The welcome slide with photo `number` (month 1–12) and the saved text, dated the first
/// Sunday of that month, so the settings can show what each month will look like.
pub async fn get_preview(
    State(state): State<AppState>,
    Path(number): Path<usize>,
) -> impl IntoResponse {
    let Some(number) = photo_number(number) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let stored = match load_settings(&state.pool).await {
        Ok(stored) => stored,
        Err(e) => return internal(e),
    };
    let today = Local::now().date_naive();
    let year = if number as u32 >= today.month() {
        today.year()
    } else {
        today.year() + 1
    };
    let Some(first_sunday) =
        chrono::NaiveDate::from_weekday_of_month_opt(year, number as u32, chrono::Weekday::Sun, 1)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match render(&state.pool, &stored.title, "", first_sunday).await {
        Ok((svg, _, _)) => ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response(),
        Err(e) => internal(e),
    }
}

pub async fn get_logo(State(state): State<AppState>) -> impl IntoResponse {
    match asset(&state.pool, "logo").await {
        Ok(Some((mime, bytes))) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        // The built-in logo until one is uploaded.
        Ok(None) => ([(header::CONTENT_TYPE, "image/svg+xml")], DEFAULT_LOGO).into_response(),
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

    /// Relative luminance of an sRGB pixel, as WCAG defines it.
    fn luminance(pixel: &image::Rgb<u8>) -> f64 {
        let [r, g, b] = pixel.0.map(to_linear);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// On every default photo, once treated, no pixel in the top band or the lower third (where
    /// all the text sits) is bright enough to bring white text under the target contrast.
    #[test]
    fn white_text_is_readable_on_every_default_photo() {
        for (index, bytes) in DEFAULT_PHOTOS.iter().enumerate() {
            let mut photo = image::load_from_memory(bytes)
                .expect("default photo decodes")
                .resize_to_fill(WIDTH, HEIGHT, image::imageops::FilterType::Triangle)
                .to_rgb8();
            treat_photo(&mut photo, true);
            let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));
            // The logo's corner, the church name's corner and the lower third.
            let brightest = photo
                .enumerate_pixels()
                .filter(|(x, y, _)| {
                    let (column, row) = ((f64::from(*x) + 0.5) / w, (f64::from(*y) + 0.5) / h);
                    (row <= TOP_BAND && !(0.42..=0.58).contains(&column)) || row >= LOWER_THIRD
                })
                .map(|(_, _, p)| luminance(p))
                .fold(0.0, f64::max);
            let contrast = 1.05 / (brightest + 0.05);
            assert!(
                contrast >= TARGET_CONTRAST - 0.01,
                "photo {:02}: contrast {contrast:.2}",
                index + 1
            );
        }
    }

    #[test]
    fn the_middle_of_the_photo_is_left_sharp_and_bright() {
        let mut photo = image::RgbImage::from_fn(WIDTH, HEIGHT, |x, _| {
            image::Rgb(if x % 2 == 0 {
                [255, 255, 255]
            } else {
                [30, 30, 30]
            })
        });
        treat_photo(&mut photo, true);
        let middle = HEIGHT * 2 / 5;
        assert_eq!(photo.get_pixel(10, middle).0, [255, 255, 255]);
        assert_eq!(photo.get_pixel(11, middle).0, [30, 30, 30]);
    }

    #[test]
    fn the_date_reads_as_each_language_writes_it() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 25).expect("valid date");
        assert_eq!(service_date(date, "hu"), "2026. október 25. vasárnap");
        assert_eq!(service_date(date, "en"), "Sunday, October 25, 2026");
    }

    #[test]
    fn the_slide_is_an_svg_with_the_photo_zooming_and_the_text_as_outlines() {
        let svg = slide_svg(
            DEFAULT_PHOTOS[9],
            Some(("image/svg+xml", DEFAULT_LOGO)),
            "",
            "Isten hozott.",
            &["Az ÚR áldása legyen életeden!".to_string()],
            "2026. október 25. vasárnap",
        )
        .expect("slide renders");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("data:image/jpeg;base64,"));
        assert!(svg.contains("class=\"metocast-zoom\""));
        // The built-in font is found, so no text is left for a screen to lay out.
        assert!(!svg.contains("<text"));
    }
}
