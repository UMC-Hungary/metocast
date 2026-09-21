//! OBS Caption browser source handler.
//!
//! Provides two unauthenticated endpoints:
//! - `GET /caption?...`       — returns HTML for OBS browser source
//! - `GET /caption/logo`      — returns the SVG logo from caption-settings.json

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse},
};
use serde::Deserialize;

use crate::server::AppState;

#[derive(Deserialize)]
pub struct CaptionQuery {
    #[serde(rename = "type", default = "default_caption_type")]
    caption_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    bold: String,
    #[serde(default)]
    light: String,
    #[serde(default = "default_color")]
    color: String,
    #[serde(rename = "showLogo", default = "default_show_logo")]
    show_logo: String,
    #[serde(default = "default_resolution")]
    resolution: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

fn default_caption_type() -> String {
    "caption".to_string()
}

fn default_color() -> String {
    "black".to_string()
}

fn default_show_logo() -> String {
    "visible".to_string()
}

fn default_resolution() -> String {
    "1080p".to_string()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub async fn caption_handler(Query(params): Query<CaptionQuery>) -> Html<String> {
    // Resolution-based base dimensions
    let (base_width, base_height) = match params.resolution.as_str() {
        "4k" => (3840u32, 2160u32),
        _ => (1920u32, 1080u32),
    };

    // Calculate final dimensions
    let (width, height) = if let (Some(w), Some(h)) = (params.width, params.height) {
        (w, h)
    } else if params.caption_type == "full" || params.caption_type == "preview" {
        (base_width, base_height)
    } else {
        // Caption bar: 150px at 1080p, 300px at 4K
        let caption_height = if params.resolution == "4k" {
            300u32
        } else {
            150u32
        };
        (base_width, caption_height)
    };

    let text_color = match params.color.as_str() {
        "red" => "#EA0029",
        _ => "#000000",
    };

    let show_logo = params.show_logo == "visible" || params.show_logo == "true";

    // Scale factor: 1 for 1080p, 2 for 4K
    let scale: u32 = if params.resolution == "4k" { 2 } else { 1 };

    let html = if params.caption_type == "full" || params.caption_type == "preview" {
        // Preview / full-screen layout
        let title_html = if !params.title.is_empty() {
            format!(
                r#"<h1 class="title" id="title">{}</h1>"#,
                html_escape(&params.title)
            )
        } else {
            String::new()
        };

        let service_info = {
            let mut parts: Vec<String> = Vec::new();
            if !params.bold.is_empty() {
                parts.push(format!(
                    r#"<span class="caption" id="text-bold">{}</span>"#,
                    html_escape(&params.bold)
                ));
            }
            if !params.bold.is_empty() && !params.light.is_empty() {
                parts.push(r#"<span class="caption" id="text-divider"></span>"#.to_string());
            }
            if !params.light.is_empty() {
                parts.push(format!(
                    r#"<span class="caption" id="text-light">{}</span>"#,
                    html_escape(&params.light)
                ));
            }
            parts.join("")
        };

        let logo_html = if show_logo {
            r#"<div class="logo"><img src="/caption/logo" alt="Logo"></div>"#.to_string()
        } else {
            String::new()
        };

        let title_size = 200 * scale;
        let title_margin = 50 * scale;
        let dot_size = 15 * scale;
        let dot_margin = 16 * scale;
        let logo_width = 300 * scale;

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>OBS Caption</title>
    <link href="https://fonts.googleapis.com/css2?family=Oswald:wght@300;600&display=swap" rel="stylesheet">
    <style>
        :root {{
            --text-color: {text_color};
        }}

        html, body {{
            height: 100%;
        }}

        body {{
            font-family: 'Oswald', sans-serif;
            display: flex;
            flex-wrap: wrap;
            margin: 0;
            padding-left: 8%;
            padding-bottom: 15%;
            align-items: center;
            box-sizing: border-box;
            width: {width}px;
            height: {height}px;
            overflow: hidden;
        }}

        .title {{
            font-weight: 600;
            font-size: {title_size}px;
            line-height: 1.2;
            margin: 0 0 {title_margin}px;
            flex: 0 0 100%;
            max-width: 100%;
            color: var(--text-color);
        }}

        .text {{
            display: flex;
            align-items: center;
            flex: 0 0 100%;
            max-width: 100%;
            flex-wrap: wrap;
        }}

        .caption {{
            font-size: 26.667vh;
            text-transform: uppercase;
            color: var(--text-color);
        }}

        #text-divider {{
            display: inline-block;
            width: {dot_size}px;
            height: {dot_size}px;
            margin: 0 {dot_margin}px;
            background-color: var(--text-color);
            border-radius: {dot_size}px;
        }}

        #text-bold {{
            font-weight: 600;
        }}

        #text-light {{
            font-weight: 300;
        }}

        .logo {{
            position: absolute;
            left: 8%;
            bottom: 5%;
            width: {logo_width}px;
        }}

        .logo img {{
            width: 100%;
            height: auto;
        }}
    </style>
</head>
<body>
    {title_html}
    <div class="text">
        {service_info}
    </div>
    {logo_html}
</body>
</html>"#
        )
    } else {
        // Caption bar layout
        let logo_visibility_class = if show_logo {
            "logo-visibility--visible"
        } else {
            "logo-visibility--hidden"
        };

        let logo_html = if show_logo {
            r#"<img id="logo" src="/caption/logo" alt="Logo">"#
        } else {
            ""
        };

        let bold_html = if !params.bold.is_empty() {
            format!(
                r#"<span class="caption" id="text-bold">{}</span>"#,
                html_escape(&params.bold)
            )
        } else {
            String::new()
        };

        let divider_html = if !params.bold.is_empty() && !params.light.is_empty() {
            r#"<span class="caption" id="text-divider"></span>"#.to_string()
        } else {
            String::new()
        };

        let light_html = if !params.light.is_empty() {
            format!(
                r#"<span class="caption" id="text-light">{}</span>"#,
                html_escape(&params.light)
            )
        } else {
            String::new()
        };

        let padding_y = 2 * scale;
        let padding_x = 3 * scale;
        let divider_border = 5 * scale;
        let bar_dot_size = 15 * scale;
        let bar_dot_margin = 16 * scale;

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>OBS Caption</title>
    <link href="https://fonts.googleapis.com/css2?family=Oswald:wght@300;600&display=swap" rel="stylesheet">
    <style>
        :root {{
            --text-color: {text_color};
        }}

        html, body {{
            height: 100%;
        }}

        body {{
            font-family: 'Oswald', sans-serif;
            display: flex;
            margin: 0;
            padding: {padding_y}rem {padding_x}rem;
            align-items: center;
            box-sizing: border-box;
            width: {width}px;
            height: {height}px;
            overflow: hidden;
        }}

        #logo {{
            flex: 0 0 133.3334vh;
            height: 100%;
            object-fit: contain;
        }}

        .divider {{
            height: 50vh;
            border-right: {divider_border}px solid var(--text-color);
            margin: 0 21.3334vh;
        }}

        .text {{
            display: flex;
            align-items: center;
            flex-grow: 1;
        }}

        .caption {{
            font-size: 26.667vh;
            text-transform: uppercase;
            color: var(--text-color);
        }}

        #text-divider {{
            display: inline-block;
            width: {bar_dot_size}px;
            height: {bar_dot_size}px;
            margin: 0 {bar_dot_margin}px;
            background-color: var(--text-color);
            border-radius: {bar_dot_size}px;
        }}

        #text-bold {{
            font-weight: 600;
        }}

        #text-light {{
            font-weight: 300;
        }}

        body.logo-visibility--hidden #logo,
        body.logo-visibility--hidden .divider {{
            display: none;
        }}
    </style>
</head>
<body class="caption {logo_visibility_class}">
    {logo_html}

    <div class="divider"></div>

    <div class="text">
        {bold_html}
        {divider_html}
        {light_html}
    </div>
</body>
</html>"#
        )
    };

    Html(html)
}

pub async fn caption_logo_handler(State(state): State<AppState>) -> impl IntoResponse {
    let svg = state
        .host
        .as_ref()
        .and_then(|host| host.caption_logo().ok().flatten())
        .unwrap_or_default();

    if svg.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            "No logo configured".to_string(),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        svg,
    )
        .into_response()
}
