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

/// A part of the slide where text sits, as fractions of its width and height: (x0, y0, x1, y1).
/// Each zone's darkening is measured on its own, so the photo stays vivid where nothing is
/// written, and each gradient below is at full strength across its whole zone.
type Zone = (f64, f64, f64, f64);
/// The church name and the logo.
const TOP: Zone = (0.0, 0.0, 1.0, 0.14);
/// The greeting and the blessing.
const CENTRE: Zone = (0.12, 0.36, 0.88, 0.68);
/// The date and the year.
const BOTTOM: Zone = (0.0, 0.8, 1.0, 1.0);

/// How dark the overlay over `zone` must be (0–1) for white text to reach `TARGET_CONTRAST`
/// against the photo there. A black overlay of opacity `a` turns luminance `L` into `(1 − a)·L`,
/// and white on `L` has contrast `1.05 / (L + 0.05)`.
fn overlay_opacity(photo: &image::RgbImage, zone: Zone) -> f64 {
    let (w, h) = photo.dimensions();
    let (fw, fh) = (f64::from(w), f64::from(h));
    let (x0, y0, x1, y1) = (
        (zone.0 * fw) as u32,
        (zone.1 * fh) as u32,
        ((zone.2 * fw) as u32).min(w),
        ((zone.3 * fh) as u32).min(h),
    );
    let mut lums: Vec<f64> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .map(|(x, y)| luminance(photo.get_pixel(x, y)))
        .collect();
    if lums.is_empty() {
        return 0.5;
    }
    lums.sort_unstable_by(f64::total_cmp);
    let bright = lums[((lums.len() - 1) as f64 * BRIGHT_PERCENTILE) as usize];
    let allowed = 1.05 / TARGET_CONTRAST - 0.05;
    // A touch of shade even on a dark photo keeps the text calm; never black it out completely.
    (1.0 - allowed / bright.max(allowed)).clamp(0.12, 0.9)
}

/// The photo cover-cropped to the slide, as a JPEG data URI, with the overlay each zone needs:
/// (top, centre, bottom).
fn prepare_photo(bytes: &[u8]) -> Result<(String, [f64; 3]), String> {
    let photo = image::load_from_memory(bytes)
        .map_err(|e| format!("Couldn't read the photo: {e}"))?
        .resize_to_fill(WIDTH, HEIGHT, image::imageops::FilterType::CatmullRom)
        .to_rgb8();
    let shade = [TOP, CENTRE, BOTTOM].map(|zone| overlay_opacity(&photo, zone));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 82)
        .encode_image(&photo)
        .map_err(|e| format!("Couldn't encode the photo: {e}"))?;
    let uri = format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    );
    Ok((uri, shade))
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

/// One line of white text at `(x, y)` with a faint shadow for crisp edges; returns the font size
/// it was set at.
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
    let shadow = (size * 0.03).max(1.0);
    for (dx, fill) in [
        (shadow, "fill=\"#000\" fill-opacity=\"0.35\""),
        (0.0, "fill=\"#fff\""),
    ] {
        svg.push_str(&format!(
            "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"Montserrat\" font-size=\"{size:.1}\" font-weight=\"{weight}\" letter-spacing=\"{ls:.2}\" text-anchor=\"{anchor}\" {fill}>{value}</text>\n",
            x = x + dx,
            y = y + dx,
            ls = tracking * size,
        ));
    }
    size
}

/// The date as the slide shows it: the day as a big number, the month and weekday beside it,
/// and the year on the right.
struct DateParts {
    day: String,
    month: String,
    weekday: String,
    year: String,
}

fn date_parts(date: chrono::NaiveDate, language: &str) -> DateParts {
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
    let (month, weekday) = if language == "en" {
        (date.format("%B").to_string(), date.format("%A").to_string())
    } else {
        (
            HU_MONTHS[date.month0() as usize].to_string(),
            HU_DAYS[date.weekday().num_days_from_monday() as usize].to_string(),
        )
    };
    DateParts {
        day: date.day().to_string(),
        month,
        weekday,
        year: date.year().to_string(),
    }
}

/// The welcome slide as SVG, in the Editorial layout: the photo with its shade, the church name
/// top left, the logo top right, the greeting and blessing in the middle, the date bottom left
/// and the year bottom right. The photo zooms in slowly where the screen animates SVG.
fn slide_svg(
    photo: &[u8],
    logo: Option<(&str, &[u8])>,
    church: &str,
    title: &str,
    lines: &[String],
    date: &DateParts,
) -> Result<String, String> {
    let (photo, [top, centre, bottom]) = prepare_photo(photo)?;
    // An even shade sized for the greeting, then only the extra the top or bottom still needs:
    // a shade `even` and a gradient `extra` together darken by 1 − (1 − even)(1 − extra).
    let even = centre;
    let extra = |zone: f64| (1.0 - (1.0 - zone) / (1.0 - even)).max(0.0);
    let (top, bottom) = (extra(top), extra(bottom));
    let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));
    let margin = 48.0;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {WIDTH} {HEIGHT}\" width=\"{WIDTH}\" height=\"{HEIGHT}\">\n\
         <defs>\
         <linearGradient id=\"top\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"#000\" stop-opacity=\"{top:.3}\"/><stop offset=\"{t1}\" stop-color=\"#000\" stop-opacity=\"{top:.3}\"/><stop offset=\"0.22\" stop-color=\"#000\" stop-opacity=\"{top_mid:.3}\"/><stop offset=\"0.31\" stop-color=\"#000\" stop-opacity=\"{top_low:.3}\"/><stop offset=\"0.42\" stop-color=\"#000\" stop-opacity=\"0\"/></linearGradient>\
         <linearGradient id=\"bottom\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0.48\" stop-color=\"#000\" stop-opacity=\"0\"/><stop offset=\"0.6\" stop-color=\"#000\" stop-opacity=\"{bottom_low:.3}\"/><stop offset=\"0.7\" stop-color=\"#000\" stop-opacity=\"{bottom_mid:.3}\"/><stop offset=\"{b0}\" stop-color=\"#000\" stop-opacity=\"{bottom:.3}\"/><stop offset=\"1\" stop-color=\"#000\" stop-opacity=\"{bottom:.3}\"/></linearGradient>\
         </defs>\n\
         <image x=\"0\" y=\"0\" width=\"{WIDTH}\" height=\"{HEIGHT}\" preserveAspectRatio=\"xMidYMid slice\" xlink:href=\"{photo}\"/>\n\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"#000\" fill-opacity=\"{even:.3}\"/>\n\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#top)\"/>\n\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#bottom)\"/>\n",
        t1 = TOP.3,
        b0 = BOTTOM.1,
        // Eased fades, so the shade never ends in a visible band.
        top_mid = top * 0.5,
        top_low = top * 0.18,
        bottom_low = bottom * 0.18,
        bottom_mid = bottom * 0.5,
    );
    let logo_size = 84.0;
    if let Some((mime, bytes)) = logo {
        let uri = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        svg.push_str(&format!(
            "<image x=\"{x}\" y=\"{y}\" width=\"{logo_size}\" height=\"{logo_size}\" preserveAspectRatio=\"xMaxYMin meet\" xlink:href=\"{uri}\"/>\n",
            x = w - margin - logo_size,
            y = margin - 14.0,
        ));
    }
    // Nothing is smaller than 4.2% of the height (30 px here): readable from the back.
    let small = h * 0.042;
    if !church.is_empty() {
        text(
            &mut svg,
            church,
            (margin, margin + small * 0.8),
            Style {
                size: small,
                weight: 600,
                anchor: "start",
                tracking: 0.0,
                max_width: w - 3.0 * margin - logo_size,
            },
        );
    }
    let title_baseline = h * 0.52;
    let title_size = text(
        &mut svg,
        title,
        (w / 2.0, title_baseline),
        Style {
            size: h * 0.15,
            weight: 700,
            anchor: "middle",
            tracking: -0.025,
            max_width: w * 0.86,
        },
    );
    let mut y = title_baseline + title_size * 0.2 + h * 0.07;
    for value in lines {
        text(
            &mut svg,
            value,
            (w / 2.0, y),
            Style {
                size: h * 0.047,
                weight: 500,
                anchor: "middle",
                tracking: 0.0,
                max_width: w * 0.8,
            },
        );
        y += h * 0.064;
    }
    let baseline = h - margin;
    let day_size = text(
        &mut svg,
        &date.day,
        (margin, baseline),
        Style {
            size: h * 0.1,
            weight: 700,
            anchor: "start",
            tracking: -0.03,
            max_width: w * 0.2,
        },
    );
    let beside = margin + text_width(&date.day, day_size) * 0.95 + 16.0;
    text(
        &mut svg,
        &date.month,
        (beside, baseline - small * 1.2),
        Style {
            size: small,
            weight: 600,
            anchor: "start",
            tracking: 0.0,
            max_width: w * 0.4,
        },
    );
    text(
        &mut svg,
        &date.weekday,
        (beside, baseline),
        Style {
            size: small,
            weight: 600,
            anchor: "start",
            tracking: 0.0,
            max_width: w * 0.4,
        },
    );
    text(
        &mut svg,
        &date.year,
        (w - margin, baseline),
        Style {
            size: small,
            weight: 600,
            anchor: "end",
            tracking: 0.0,
            max_width: w * 0.2,
        },
    );
    svg.push_str("</svg>\n");
    // Outlines instead of text, so no screen needs the font.
    let flat = presenter_svg::flatten_text_to_paths(&svg, text_options());
    Ok(with_slow_zoom(&flat))
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
    let parts = date_parts(date, &stored.language);
    let photo = photo(pool, date.month() as usize)
        .await
        .map_err(|e| e.to_string())?;
    let logo = asset(pool, "logo").await.map_err(|e| e.to_string())?;
    let svg = slide_svg(
        &photo,
        logo.as_ref()
            .map(|(mime, bytes)| (mime.as_str(), bytes.as_slice())),
        stored.church_name.trim(),
        title.trim(),
        &lines,
        &parts,
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

    /// Every default photo, with the shade each zone gets, keeps white text at the target
    /// contrast against all but the brightest specks behind it, in all three zones.
    #[test]
    fn white_text_is_readable_on_every_default_photo() {
        for (index, bytes) in DEFAULT_PHOTOS.iter().enumerate() {
            let photo = image::load_from_memory(bytes)
                .expect("default photo decodes")
                .resize_to_fill(WIDTH, HEIGHT, image::imageops::FilterType::Triangle)
                .to_rgb8();
            for (name, zone) in [("top", TOP), ("centre", CENTRE), ("bottom", BOTTOM)] {
                let opacity = overlay_opacity(&photo, zone);
                let (w, h) = (f64::from(WIDTH), f64::from(HEIGHT));
                let mut lums: Vec<f64> = ((zone.1 * h) as u32..(zone.3 * h) as u32)
                    .flat_map(|y| ((zone.0 * w) as u32..(zone.2 * w) as u32).map(move |x| (x, y)))
                    .map(|(x, y)| (1.0 - opacity) * luminance(photo.get_pixel(x, y)))
                    .collect();
                lums.sort_unstable_by(f64::total_cmp);
                let bright = lums[((lums.len() - 1) as f64 * BRIGHT_PERCENTILE) as usize];
                let contrast = 1.05 / (bright + 0.05);
                assert!(
                    contrast >= TARGET_CONTRAST - 0.01,
                    "photo {:02}, {name}: contrast {contrast:.2} with shade {opacity:.2}",
                    index + 1
                );
            }
        }
    }

    #[test]
    fn the_date_reads_as_each_language_writes_it() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 25).expect("valid date");
        assert_eq!(service_date(date, "hu"), "2026. október 25. vasárnap");
        assert_eq!(service_date(date, "en"), "Sunday, October 25, 2026");
        let hu = date_parts(date, "hu");
        assert_eq!(
            [hu.day, hu.month, hu.weekday, hu.year],
            ["25", "október", "vasárnap", "2026"]
        );
        let en = date_parts(date, "en");
        assert_eq!([en.month, en.weekday], ["October", "Sunday"]);
    }

    #[test]
    fn a_dark_photo_gets_little_shade_and_a_white_one_a_lot() {
        let dark = image::RgbImage::from_pixel(64, 36, image::Rgb([20, 20, 20]));
        let white = image::RgbImage::from_pixel(64, 36, image::Rgb([255, 255, 255]));
        assert!(overlay_opacity(&dark, CENTRE) <= 0.2);
        assert!(overlay_opacity(&white, CENTRE) >= 0.8);
    }

    #[test]
    fn the_slide_is_an_svg_with_the_photo_zooming_and_the_text_as_outlines() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 25).expect("valid date");
        let svg = slide_svg(
            DEFAULT_PHOTOS[9],
            None,
            "Nyíregyházi Metodista Gyülekezet",
            "Isten hozott.",
            &["Az ÚR áldása legyen életeden!".to_string()],
            &date_parts(date, "hu"),
        )
        .expect("slide renders");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("data:image/jpeg;base64,"));
        assert!(svg.contains("class=\"metocast-zoom\""));
        // The built-in font is found, so no text is left for a screen to lay out.
        assert!(!svg.contains("<text"));
    }
}
