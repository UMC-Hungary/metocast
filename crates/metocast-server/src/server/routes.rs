use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use uuid::Uuid;

use crate::connectors::{
    AtemConfig, BlackmagicCameraConfig, BroadlinkConfig, DiscordConfig, FacebookConfig,
    MiddlecontrolConfig, ObsConfig, RodecasterConfig, SzentirasConfig, VmixConfig, YouTubeConfig,
    atem, blackmagic_camera, facebook, middlecontrol, youtube,
};
use crate::connectors::{ConnectorConfig, ConnectorStatus};
use crate::database::settings;
use crate::models::{
    activity::{self, CreateEventActivity},
    cron_job::{self, CreateCronJob, UpdateCronJob},
    event::{
        CreateBibleReference, CreateEvent, SlideFolder, TitleTemplate, UpdateEvent, fetch_event,
        fetch_event_summaries,
    },
    recording::{CreateRecording, FlagUploadRequest, Recording, RecordingUpload},
    untracked_recording,
};
use crate::server::AppState;
use crate::server::OAUTH_REDIRECT_URI;
use crate::server::presenter::{self, BibleReferenceType, BibleVerseContent, PresenterState};
use crate::server::websocket::{
    broadcast_event_changed, broadcast_untracked_removed, spawn_scheduling_tasks,
};

const OAUTH_SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Authentication Successful</title>
  <style>
    body { font-family: system-ui, sans-serif; display: flex; align-items: center;
           justify-content: center; height: 100vh; margin: 0; background: #f9fafb; }
    .card { text-align: center; padding: 2rem 3rem; background: #fff;
            border-radius: 0.75rem; box-shadow: 0 2px 8px rgba(0,0,0,.08); }
    h1 { color: #065f46; margin-bottom: 0.5rem; font-size: 1.5rem; }
    p { color: #6b7280; margin: 0; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Authentication Successful</h1>
    <p>You can close this tab and return to the app.</p>
  </div>
  <script>setTimeout(() => window.close(), 3000);</script>
</body>
</html>"#;

const BIBLE_SLIDE_FOLDER_KEY: &str = "slide_folder";
const SONG_SLIDE_FOLDER_KEY: &str = "song_slide_folder";

const OAUTH_ERROR_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Authentication Failed</title>
  <style>
    body { font-family: system-ui, sans-serif; display: flex; align-items: center;
           justify-content: center; height: 100vh; margin: 0; background: #f9fafb; }
    .card { text-align: center; padding: 2rem 3rem; background: #fff;
            border-radius: 0.75rem; box-shadow: 0 2px 8px rgba(0,0,0,.08); }
    h1 { color: #991b1b; margin-bottom: 0.5rem; font-size: 1.5rem; }
    p { color: #6b7280; margin: 0; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Authentication Failed</h1>
    <p>An error occurred. Please close this tab and try again.</p>
  </div>
</body>
</html>"#;

// ── Connector statuses ────────────────────────────────────────────────────────

pub async fn get_connector_state(State(state): State<AppState>) -> impl IntoResponse {
    let obs_state = state.obs_connector.get_output_state().await;
    let camera_state = state.blackmagic_camera_connector.get_state().await;
    let middlecontrol_state = state.middlecontrol_connector.get_state().await;
    Json(json!({
        "obs": obs_state.map(|s| json!({"isStreaming": s.is_streaming, "isRecording": s.is_recording})),
        "blackmagic-camera": camera_state.map(|s| json!({"isStreaming": s.is_streaming(), "streamStatus": s.stream_status, "isRecording": s.is_recording})),
        "middlecontrol": middlecontrol_state,
        "atem": state.atem_connector.get_state().await,
    }))
}

pub async fn get_connector_statuses(State(state): State<AppState>) -> impl IntoResponse {
    let obs = state.obs_connector.get_status().await;
    let vmix = state.vmix_connector.get_status();
    let yt = state.youtube_connector.get_status().await;
    let fb = state.facebook_connector.get_status().await;
    let broadlink = state.broadlink_connector.get_status().await;
    let blackmagic_camera = state.blackmagic_camera_connector.get_status().await;
    let middlecontrol = state.middlecontrol_connector.get_status().await;
    let rodecaster = state.rodecaster_connector.get_status();
    let atem = state.atem_connector.get_status().await;
    // Discord and Szentírás have no connector worker: they report Connected
    // when configured, so a UI can render them without a separate config round-trip.
    let configured = |ok: bool| {
        if ok {
            ConnectorStatus::Connected
        } else {
            ConnectorStatus::Disconnected
        }
    };
    let discord = configured(
        settings::get_json::<DiscordConfig>(&state.pool, "discord_config")
            .await
            .is_configured(),
    );
    let szentiras = configured(
        settings::get_json::<SzentirasConfig>(&state.pool, "szentiras_config")
            .await
            .is_configured(),
    );
    Json(json!({
        "obs": obs,
        "blackmagic-camera": blackmagic_camera,
        "middlecontrol": middlecontrol,
        "rodecaster": rodecaster,
        "vmix": vmix,
        "atem": atem,
        "broadlink": broadlink,
        "youtube": yt,
        "facebook": fb,
        "discord": discord,
        "szentiras": szentiras,
    }))
}

// ── Bible lookups ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BiblePassageQuery {
    pub reference: String,
    pub translation: String,
}

fn upstream_error(e: String) -> axum::response::Response {
    (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))).into_response()
}

/// Looks a passage up through the core so no UI needs its own CORS workaround.
/// The szentiras.eu API key comes from the `szentiras` connector config.
pub async fn get_bible_passage(
    State(state): State<AppState>,
    Query(query): Query<BiblePassageQuery>,
) -> impl IntoResponse {
    let szentiras: SzentirasConfig = settings::get_json(&state.pool, "szentiras_config").await;
    let api_key = Some(szentiras.api_key.as_str()).filter(|key| !key.is_empty());

    match crate::bible::fetch_passage(&query.reference, &query.translation, api_key).await {
        Ok(passage) => Json(passage).into_response(),
        Err(e) => upstream_error(e),
    }
}

#[derive(Deserialize)]
pub struct BibleSuggestQuery {
    pub term: String,
}

/// Autocomplete suggestions; terms shorter than 2 characters return an empty list.
pub async fn get_bible_suggestions(Query(query): Query<BibleSuggestQuery>) -> impl IntoResponse {
    match crate::bible::fetch_suggestions(&query.term).await {
        Ok(suggestions) => Json(suggestions).into_response(),
        Err(e) => upstream_error(e),
    }
}

// ── Connector configuration ───────────────────────────────────────────────────

/// Credential fields that never leave the server. Reading a config blanks them
/// and reports only whether one is stored; writing an empty one keeps what is
/// already there, so a client can save a config it was never allowed to read.
const SECRET_FIELDS: [&str; 5] = [
    "password",
    "clientSecret",
    "appSecret",
    "apiKey",
    "webhookUrl",
];

/// Blanks every secret and adds a `<field>Set` boolean beside it.
fn redact_secrets(config: &mut serde_json::Value) {
    let Some(object) = config.as_object_mut() else {
        return;
    };
    for field in SECRET_FIELDS {
        let Some(value) = object.get_mut(field) else {
            continue;
        };
        let is_set = !matches!(value.as_str(), None | Some(""));
        *value = serde_json::Value::String(String::new());
        object.insert(format!("{field}Set"), serde_json::Value::Bool(is_set));
    }
}

/// Restores secrets the client left blank from what is already stored, so saving
/// a redacted config does not wipe the credential. Sending `"<field>Set": false`
/// is the explicit "clear this secret" signal.
fn restore_omitted_secrets(incoming: &mut serde_json::Value, stored: &serde_json::Value) {
    let (Some(incoming), Some(stored)) = (incoming.as_object_mut(), stored.as_object()) else {
        return;
    };
    for field in SECRET_FIELDS {
        // The `<field>Set` markers are instructions, never stored values.
        let clear_requested = incoming
            .remove(&format!("{field}Set"))
            .is_some_and(|marker| marker == serde_json::Value::Bool(false));

        let submitted_blank = match incoming.get(field) {
            None => true,
            Some(value) => matches!(value.as_str(), None | Some("")),
        };
        if !submitted_blank || clear_requested {
            continue;
        }

        match stored.get(field) {
            Some(previous) => {
                incoming.insert(field.to_string(), previous.clone());
            }
            None => {
                incoming.remove(field);
            }
        }
    }
}

async fn stored_config<T>(pool: &sqlx::PgPool, key: &str) -> axum::response::Response
where
    T: serde::de::DeserializeOwned + Default + Serialize,
{
    let config = settings::get_json::<T>(pool, key).await;
    match serde_json::to_value(&config) {
        Ok(mut value) => {
            redact_secrets(&mut value);
            Json(value).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn save_config<T>(
    pool: &sqlx::PgPool,
    key: &str,
    mut body: serde_json::Value,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned + Default + Serialize,
{
    let stored = serde_json::to_value(settings::get_json::<T>(pool, key).await)
        .map_err(|e| e.to_string())?;
    restore_omitted_secrets(&mut body, &stored);

    let config: T = serde_json::from_value(body).map_err(|e| e.to_string())?;
    settings::set_json(pool, key, &config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(config)
}

fn unknown_connector(name: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("Unknown connector: {name}") })),
    )
        .into_response()
}

/// Compares without an early exit, so a wrong guess reveals nothing through timing.
fn secret_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Returns a connector config **with its secrets**, for the desktop app that is
/// hosting this server. Two gates, both required: the caller must present the
/// admin token, which only ever reaches the host webview over Tauri IPC, and the
/// request must arrive on loopback so a leaked token is useless from the network.
pub async fn reveal_connector_secrets(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(name): Path<String>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if !peer.ip().is_loopback() {
        tracing::warn!(%peer, connector = %name, "Refused off-host secret read");
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Secrets are readable only on the host running the server" })),
        )
            .into_response();
    }

    let presented = headers
        .get("x-admin-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !secret_eq(presented, &state.admin_token) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Missing or invalid admin token" })),
        )
            .into_response();
    }

    let pool = &state.pool;
    match name.as_str() {
        "obs" => Json(settings::get_json::<ObsConfig>(pool, "obs_config").await).into_response(),
        "blackmagic-camera" => Json(
            settings::get_json::<BlackmagicCameraConfig>(pool, "blackmagic_camera_config").await,
        )
        .into_response(),
        "youtube" => {
            Json(settings::get_json::<YouTubeConfig>(pool, "youtube_config").await).into_response()
        }
        "facebook" => Json(settings::get_json::<FacebookConfig>(pool, "facebook_config").await)
            .into_response(),
        "discord" => {
            Json(settings::get_json::<DiscordConfig>(pool, "discord_config").await).into_response()
        }
        "szentiras" => Json(settings::get_json::<SzentirasConfig>(pool, "szentiras_config").await)
            .into_response(),
        // vmix, atem, middlecontrol, broadlink and rodecaster hold no credentials.
        "vmix" | "atem" | "middlecontrol" | "broadlink" | "rodecaster" => (
            StatusCode::NO_CONTENT,
            Json(json!({ "error": "This connector stores no secrets" })),
        )
            .into_response(),
        _ => unknown_connector(&name),
    }
}

pub async fn get_connector_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let pool = &state.pool;
    match name.as_str() {
        "obs" => stored_config::<ObsConfig>(pool, "obs_config").await,
        "blackmagic-camera" => {
            stored_config::<BlackmagicCameraConfig>(pool, "blackmagic_camera_config").await
        }
        "vmix" => stored_config::<VmixConfig>(pool, "vmix_config").await,
        "atem" => stored_config::<AtemConfig>(pool, "atem_config").await,
        "middlecontrol" => stored_config::<MiddlecontrolConfig>(pool, "middlecontrol_config").await,
        "broadlink" => stored_config::<BroadlinkConfig>(pool, "broadlink_config").await,
        "rodecaster" => stored_config::<RodecasterConfig>(pool, "rodecaster_config").await,
        "discord" => stored_config::<DiscordConfig>(pool, "discord_config").await,
        "szentiras" => stored_config::<SzentirasConfig>(pool, "szentiras_config").await,
        "youtube" => stored_config::<YouTubeConfig>(pool, "youtube_config").await,
        "facebook" => stored_config::<FacebookConfig>(pool, "facebook_config").await,
        _ => unknown_connector(&name),
    }
}

/// Persists a connector config and applies it: OBS reconnects (or disconnects),
/// YouTube/Facebook refresh the config shared with the OAuth routes and stop when
/// disabled. The remaining connectors are configuration-only today.
pub async fn put_connector_config(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let pool = &state.pool;
    let applied = match name.as_str() {
        "obs" => match save_config::<ObsConfig>(pool, "obs_config", body).await {
            Ok(config) => {
                if config.enabled {
                    state.obs_connector.start(config, state.host.clone()).await;
                } else {
                    state.obs_connector.stop().await;
                }
                Ok(())
            }
            Err(e) => Err(e),
        },
        "blackmagic-camera" => {
            match save_config::<BlackmagicCameraConfig>(pool, "blackmagic_camera_config", body)
                .await
            {
                Ok(config) => {
                    if config.enabled {
                        state
                            .blackmagic_camera_connector
                            .start(config, state.host.clone())
                            .await;
                    } else {
                        state.blackmagic_camera_connector.stop().await;
                    }
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        "middlecontrol" => {
            match save_config::<MiddlecontrolConfig>(pool, "middlecontrol_config", body).await {
                Ok(config) => {
                    if config.enabled {
                        state.middlecontrol_connector.start(config).await;
                    } else {
                        state.middlecontrol_connector.stop().await;
                    }
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        "youtube" => match save_config::<YouTubeConfig>(pool, "youtube_config", body).await {
            Ok(config) => {
                *state.youtube_config.write().await = config.clone();
                if config.enabled {
                    state
                        .youtube_connector
                        .start(pool.clone(), config, state.host.clone())
                        .await;
                } else {
                    state.youtube_connector.stop().await;
                }
                Ok(())
            }
            Err(e) => Err(e),
        },
        "facebook" => match save_config::<FacebookConfig>(pool, "facebook_config", body).await {
            Ok(config) => {
                *state.facebook_config.write().await = config.clone();
                if config.enabled {
                    state
                        .facebook_connector
                        .start(pool.clone(), config, state.host.clone())
                        .await;
                } else {
                    state.facebook_connector.stop().await;
                }
                Ok(())
            }
            Err(e) => Err(e),
        },
        "vmix" => save_config::<VmixConfig>(pool, "vmix_config", body)
            .await
            .map(|_| ()),
        "atem" => match save_config::<AtemConfig>(pool, "atem_config", body).await {
            Ok(config) => {
                state.atem_connector.start(config).await;
                Ok(())
            }
            Err(e) => Err(e),
        },
        "broadlink" => save_config::<BroadlinkConfig>(pool, "broadlink_config", body)
            .await
            .map(|_| ()),
        "rodecaster" => match serde_json::from_value::<RodecasterConfig>(body) {
            Ok(config) => {
                if config.audio_recording.enabled
                    && let Err(error) = crate::connectors::rodecaster_audio::validate_config(
                        &config.audio_recording,
                    )
                {
                    return (StatusCode::BAD_REQUEST, Json(json!({ "error": error })))
                        .into_response();
                }
                if let Err(error) = settings::set_json(pool, "rodecaster_config", &config).await {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": error.to_string() })),
                    )
                        .into_response();
                }
                if config.enabled {
                    state.rodecaster_connector.start(config);
                } else {
                    state.rodecaster_connector.stop();
                }
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        },
        "discord" => save_config::<DiscordConfig>(pool, "discord_config", body)
            .await
            .map(|_| ()),
        "szentiras" => save_config::<SzentirasConfig>(pool, "szentiras_config", body)
            .await
            .map(|_| ()),
        _ => return unknown_connector(&name),
    };

    match applied {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

// ── OBS connection control ────────────────────────────────────────────────────

pub async fn obs_connect(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("obs: manual reconnect requested");
    let config: ObsConfig = settings::get_json(&state.pool, "obs_config").await;
    state.obs_connector.start(config, state.host.clone()).await;
    StatusCode::NO_CONTENT
}

pub async fn obs_disconnect(State(state): State<AppState>) -> impl IntoResponse {
    state.obs_connector.stop().await;
    StatusCode::NO_CONTENT
}

pub async fn blackmagic_camera_connect(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("blackmagic camera: manual reconnect requested");
    let config: BlackmagicCameraConfig =
        settings::get_json(&state.pool, "blackmagic_camera_config").await;
    state
        .blackmagic_camera_connector
        .start(config, state.host.clone())
        .await;
    StatusCode::NO_CONTENT
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsStreamSettings {
    #[serde(default, skip_deserializing)]
    pub service_type: String,
    pub server: String,
    pub key: String,
}

/// Returns the RTMP destination OBS is currently configured to stream to.
pub async fn get_obs_stream_settings(State(state): State<AppState>) -> impl IntoResponse {
    let client = {
        let guard = state.obs_connector.client.lock().await;
        guard.as_ref().map(std::sync::Arc::clone)
    };
    let Some(client) = client else {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "OBS is not connected" })),
        )
            .into_response();
    };

    match client
        .config()
        .stream_service_settings::<serde_json::Value>()
        .await
    {
        Ok(settings) => {
            let field = |name: &str| {
                settings
                    .settings
                    .get(name)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            Json(ObsStreamSettings {
                service_type: settings.r#type,
                server: field("server"),
                key: field("key"),
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Applies a custom RTMP stream destination to OBS.
pub async fn set_obs_stream_settings(
    State(state): State<AppState>,
    Json(body): Json<ObsStreamSettings>,
) -> impl IntoResponse {
    let client = {
        let guard = state.obs_connector.client.lock().await;
        guard.as_ref().map(std::sync::Arc::clone)
    };
    let Some(client) = client else {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "OBS is not connected" })),
        )
            .into_response();
    };

    match client
        .config()
        .set_stream_service_settings(
            "rtmp_custom",
            &json!({ "server": body.server, "key": body.key }),
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

// ── Blackmagic camera discovery ───────────────────────────────────────────────

/// Scans the LAN, tells every connected client what turned up, and adopts the
/// first camera found when none is configured yet — a scan is what gets a camera
/// connected, the same way a Broadlink scan is.
///
/// An empty result is normal, not an error: mDNS does not cross VLANs, and on
/// macOS it needs Local Network permission. Adding a camera by host still works.
pub(crate) async fn discover_cameras(
    state: &AppState,
    timeout: std::time::Duration,
) -> Vec<serde_json::Value> {
    let found = match blackmagic_camera::discover(timeout).await {
        Ok(found) => found,
        Err(e) => {
            tracing::error!("blackmagic-camera discover: {e}");
            Vec::new()
        }
    };

    let cameras: Vec<serde_json::Value> = found
        .iter()
        .map(|camera| {
            json!({
                "host": camera.host(),
                "hostname": camera.hostname,
                "addresses": camera.addresses,
                "port": camera.port,
                "deviceName": camera.device_name,
                "productName": camera.product_name,
                "uniqueId": camera.unique_id,
                "softwareVersion": camera.software_version,
            })
        })
        .collect();

    let msg = json!({ "type": "blackmagic-camera.discovered", "cameras": cameras }).to_string();
    for tx in state.ws_clients.read().await.values() {
        let _ = tx.send(axum::extract::ws::Message::Text(msg.clone().into()));
    }

    adopt_camera(state, found.first()).await;
    cameras
}

/// Stores the discovered camera and connects to it, unless one is already
/// configured — a scan must never repoint an operator's chosen camera.
async fn adopt_camera(state: &AppState, found: Option<&blackmagic_camera::Discovered>) {
    let Some(camera) = found else { return };
    let stored: BlackmagicCameraConfig =
        settings::get_json(&state.pool, "blackmagic_camera_config").await;
    if !stored.host.is_empty() {
        return;
    }

    let config = BlackmagicCameraConfig {
        enabled: true,
        host: camera.host(),
        ..stored
    };
    if let Err(e) = settings::set_json(&state.pool, "blackmagic_camera_config", &config).await {
        tracing::error!("adopt_camera: {e}");
        return;
    }
    tracing::info!("Adopted Blackmagic camera {}", config.host);
    state
        .blackmagic_camera_connector
        .start(config, state.host.clone())
        .await;
}

/// Copies the channel's RTMP ingestion address and stream key into the camera's
/// livestream settings, and reports what the camera will stream to. Setting the
/// destination is all this does — going live is a separate action.
pub(crate) async fn push_youtube_to_camera(
    pool: &sqlx::PgPool,
    camera: &blackmagic_camera::Camera,
) -> Result<serde_json::Value, String> {
    let token = youtube::load_tokens(pool)
        .await
        .ok_or_else(|| "not_authenticated".to_string())?;
    let ingestion = youtube::live_stream_ingestion(&token.access_token)
        .await
        .map_err(|e| e.to_string())?;
    let platform = blackmagic_camera::push_youtube(camera, &ingestion.address, &ingestion.key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "rtmpUrl": ingestion.rtmp_url(),
        "platform": platform.platform,
        "server": platform.server,
        "quality": platform.quality,
        "url": platform.url,
    }))
}

pub(crate) async fn push_youtube_to_atem(state: &AppState) -> Result<serde_json::Value, String> {
    if state.atem_connector.get_state().await.is_none() {
        return Err("atem_not_connected".into());
    }
    let token = youtube::load_tokens(&state.pool)
        .await
        .ok_or_else(|| "not_authenticated".to_string())?;
    let ingestion = youtube::live_stream_ingestion(&token.access_token)
        .await
        .map_err(|e| e.to_string())?;
    let service = "YouTube RTMP";
    state
        .atem_connector
        .send(atem::AtemCommand::StreamDestination {
            service: service.into(),
            url: ingestion.address.clone(),
            key: ingestion.key,
        })
        .await?;
    Ok(json!({ "service": service, "url": ingestion.address }))
}

pub async fn atem_push_youtube(State(state): State<AppState>) -> impl IntoResponse {
    match push_youtube_to_atem(&state).await {
        Ok(payload) => Json(payload).into_response(),
        Err(e) if e == "atem_not_connected" => {
            (StatusCode::CONFLICT, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => upstream_error(e),
    }
}

/// The connected camera, or the 409 every camera route answers without one.
async fn connected_camera(state: &AppState) -> Option<std::sync::Arc<blackmagic_camera::Camera>> {
    state.blackmagic_camera_connector.client().await
}

fn camera_not_connected() -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({ "error": "Blackmagic camera is not connected" })),
    )
        .into_response()
}

pub async fn blackmagic_camera_push_youtube(State(state): State<AppState>) -> impl IntoResponse {
    let Some(camera) = connected_camera(&state).await else {
        return camera_not_connected();
    };
    match push_youtube_to_camera(&state.pool, &camera).await {
        Ok(payload) => Json(payload).into_response(),
        Err(e) => upstream_error(e),
    }
}

/// Storage, record format and livestream settings the camera control screen shows.
pub async fn blackmagic_camera_settings(State(state): State<AppState>) -> impl IntoResponse {
    let Some(camera) = connected_camera(&state).await else {
        return camera_not_connected();
    };
    match blackmagic_camera::settings(&camera).await {
        Ok(payload) => Json(payload).into_response(),
        Err(e) => upstream_error(e.to_string()),
    }
}

/// Writes the record format, the livestream platform, or both.
pub async fn blackmagic_camera_apply_settings(
    State(state): State<AppState>,
    Json(update): Json<blackmagic_camera::SettingsUpdate>,
) -> impl IntoResponse {
    let Some(camera) = connected_camera(&state).await else {
        return camera_not_connected();
    };
    match blackmagic_camera::apply_settings(&camera, &update).await {
        Ok(()) => match blackmagic_camera::settings(&camera).await {
            Ok(payload) => Json(payload).into_response(),
            Err(e) => upstream_error(e.to_string()),
        },
        Err(e) => upstream_error(e.to_string()),
    }
}

pub async fn blackmagic_camera_discover(State(state): State<AppState>) -> impl IntoResponse {
    let cameras = discover_cameras(&state, std::time::Duration::from_secs(5)).await;
    Json(json!({ "cameras": cameras }))
}

pub async fn middlecontrol_discover() -> impl IntoResponse {
    Json(json!({ "devices": middlecontrol::discover().await }))
}

pub async fn atem_discover() -> impl IntoResponse {
    match atem::discover(std::time::Duration::from_secs(3)).await {
        Ok(devices) => Json(json!({ "devices": devices })).into_response(),
        Err(e) => upstream_error(e),
    }
}

// ── YouTube OAuth ─────────────────────────────────────────────────────────────

pub async fn youtube_auth_url(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.youtube_config.read().await.clone();
    if config.client_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "YouTube not configured"})),
        )
            .into_response();
    }

    let state_token = Uuid::new_v4().to_string();
    {
        let mut states = state.oauth_states.write().await;
        states.insert(
            state_token.clone(),
            ("youtube".to_string(), std::time::Instant::now()),
        );
    }

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=https://www.googleapis.com/auth/youtube&access_type=offline&prompt=consent&state={}",
        urlencoding::encode(&config.client_id),
        urlencoding::encode(OAUTH_REDIRECT_URI),
        urlencoding::encode(&state_token),
    );
    Json(json!({ "url": url })).into_response()
}

#[derive(Deserialize)]
pub struct OAuthCallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Unified OAuth callback — handles both YouTube and Facebook.
/// Google/Facebook redirect to http://127.0.0.1:8766/callback; the connector
/// is identified by looking up the state token in oauth_states.
pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(params): Query<OAuthCallbackParams>,
) -> impl IntoResponse {
    if params.error.is_some() {
        return Html(OAUTH_ERROR_HTML).into_response();
    }
    let (code, state_token) = match (params.code, params.state) {
        (Some(c), Some(s)) => (c, s),
        _ => return Html(OAUTH_ERROR_HTML).into_response(),
    };

    let connector = {
        let mut states = state.oauth_states.write().await;
        match states.remove(&state_token) {
            Some((name, _)) => name,
            None => return Html(OAUTH_ERROR_HTML).into_response(),
        }
    };

    match connector.as_str() {
        "youtube" => {
            let config = state.youtube_config.read().await.clone();
            match youtube::exchange_code(&state.pool, &config, &code, OAUTH_REDIRECT_URI).await {
                Ok(_) => {
                    state
                        .youtube_connector
                        .start(state.pool.clone(), config, state.host.clone())
                        .await;
                    Html(OAUTH_SUCCESS_HTML).into_response()
                }
                Err(e) => {
                    tracing::error!("YouTube token exchange failed: {e}");
                    Html(OAUTH_ERROR_HTML).into_response()
                }
            }
        }
        "facebook" => {
            let config = state.facebook_config.read().await.clone();
            match facebook::exchange_code(&state.pool, &config, &code, OAUTH_REDIRECT_URI).await {
                Ok(_) => {
                    state
                        .facebook_connector
                        .start(state.pool.clone(), config, state.host.clone())
                        .await;
                    Html(OAUTH_SUCCESS_HTML).into_response()
                }
                Err(e) => {
                    tracing::error!("Facebook token exchange failed: {e}");
                    Html(OAUTH_ERROR_HTML).into_response()
                }
            }
        }
        _ => Html(OAUTH_ERROR_HTML).into_response(),
    }
}

pub async fn youtube_logout(State(state): State<AppState>) -> impl IntoResponse {
    if let Err(e) = youtube::delete_tokens(&state.pool).await {
        tracing::error!("YouTube logout: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    state.youtube_connector.stop().await;
    StatusCode::NO_CONTENT.into_response()
}

// ── Facebook OAuth ────────────────────────────────────────────────────────────

pub async fn facebook_auth_url(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.facebook_config.read().await.clone();
    if config.app_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Facebook not configured"})),
        )
            .into_response();
    }

    let state_token = Uuid::new_v4().to_string();
    {
        let mut states = state.oauth_states.write().await;
        states.insert(
            state_token.clone(),
            ("facebook".to_string(), std::time::Instant::now()),
        );
    }

    let url = format!(
        "https://www.facebook.com/v19.0/dialog/oauth?client_id={}&redirect_uri={}&scope=pages_manage_posts,pages_read_engagement,publish_video&state={}",
        urlencoding::encode(&config.app_id),
        urlencoding::encode(OAUTH_REDIRECT_URI),
        urlencoding::encode(&state_token),
    );
    Json(json!({ "url": url })).into_response()
}

pub async fn facebook_logout(State(state): State<AppState>) -> impl IntoResponse {
    if let Err(e) = facebook::delete_tokens(&state.pool).await {
        tracing::error!("Facebook logout: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    state.facebook_connector.stop().await;
    StatusCode::NO_CONTENT.into_response()
}

// ── Manual schedule triggers ──────────────────────────────────────────────────

pub async fn trigger_youtube_schedule(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> impl IntoResponse {
    let event = match fetch_event(event_id, &state.pool).await {
        Ok(Some(e)) => e,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("trigger_youtube_schedule fetch: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let token = match youtube::load_tokens(&state.pool).await {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
                .into_response();
        }
    };

    let yt_conn = event.connection("youtube");
    let existing_id = yt_conn.and_then(|c| c.external_id.as_deref());
    let privacy_status = yt_conn
        .and_then(|c| c.privacy_status.as_deref())
        .unwrap_or("private");

    match youtube::schedule_event(
        &event.id.to_string(),
        event.published_title(),
        &event.date_time,
        &token.access_token,
        existing_id,
        privacy_status,
    )
    .await
    {
        Ok(result) => {
            let write_result = async {
                let mut tx = state.pool.begin().await?;
                sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    r#"INSERT INTO event_connections (event_id, platform, external_id, stream_url, schedule_status)
                       VALUES ($1, 'youtube', $2, $3, 'scheduled')
                       ON CONFLICT (event_id, platform) DO UPDATE SET
                           external_id     = EXCLUDED.external_id,
                           stream_url      = EXCLUDED.stream_url,
                           schedule_status = 'scheduled',
                           updated_at      = NOW()"#,
                )
                .bind(event_id)
                .bind(&result.broadcast_id)
                .bind(&result.watch_url)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
                    .bind(event_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                anyhow::Ok(())
            }
            .await;
            if let Err(e) = write_result {
                tracing::error!("Failed to persist YouTube broadcast result: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(result).into_response()
        }
        Err(e) => {
            tracing::error!("YouTube schedule_event failed: {e}");
            let _ = async {
                let mut tx = state.pool.begin().await?;
                sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    r#"INSERT INTO event_connections (event_id, platform, schedule_status)
                       VALUES ($1, 'youtube', 'failed')
                       ON CONFLICT (event_id, platform) DO UPDATE SET
                           schedule_status = 'failed',
                           updated_at      = NOW()"#,
                )
                .bind(event_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
                    .bind(event_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                anyhow::Ok(())
            }
            .await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

// ── YouTube channel content ───────────────────────────────────────────────────

pub async fn get_youtube_content(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.youtube_config.read().await.clone();
    match youtube::fetch_channel_content(&state.pool, &config).await {
        Ok(content) => Json(content).into_response(),
        Err(e) => {
            tracing::error!("fetch_channel_content failed: {e}");
            if e.is::<youtube::AuthRequired>() {
                // Tokens were already deleted by fetch_channel_content; restart the
                // loop so it reports the missing login as an error the UI raises.
                state
                    .youtube_connector
                    .start(state.pool.clone(), config, state.host.clone())
                    .await;
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "auth_required", "message": "Re-login required" })),
                )
                    .into_response();
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

pub async fn trigger_facebook_schedule(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> impl IntoResponse {
    let event = match fetch_event(event_id, &state.pool).await {
        Ok(Some(e)) => e,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("trigger_facebook_schedule fetch: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let token = match facebook::load_tokens(&state.pool).await {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
                .into_response();
        }
    };

    let config = state.facebook_config.read().await.clone();
    if config.page_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Facebook page_id not configured"})),
        )
            .into_response();
    }

    let fb_conn = event.connection("facebook");
    let privacy_status = fb_conn
        .and_then(|c| c.privacy_status.as_deref())
        .unwrap_or("EVERYONE");

    match facebook::schedule_event(
        &event.title,
        &event.date_time,
        &token.access_token,
        &config.page_id,
        privacy_status,
    )
    .await
    {
        Ok(result) => {
            let extra = serde_json::json!({"stream_id": result.stream_id});
            let write_result = async {
                let mut tx = state.pool.begin().await?;
                sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    r#"INSERT INTO event_connections (event_id, platform, external_id, event_url, schedule_status, extra)
                       VALUES ($1, 'facebook', $2, $3, 'scheduled', $4)
                       ON CONFLICT (event_id, platform) DO UPDATE SET
                           external_id     = EXCLUDED.external_id,
                           event_url       = EXCLUDED.event_url,
                           schedule_status = 'scheduled',
                           extra           = EXCLUDED.extra,
                           updated_at      = NOW()"#,
                )
                .bind(event_id)
                .bind(&result.event_id)
                .bind(&result.event_url)
                .bind(extra)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
                    .bind(event_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                anyhow::Ok(())
            }
            .await;
            if let Err(e) = write_result {
                tracing::error!("Failed to persist Facebook schedule result: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(result).into_response()
        }
        Err(e) => {
            tracing::error!("Facebook schedule_event failed: {e}");
            let _ = async {
                let mut tx = state.pool.begin().await?;
                sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    r#"INSERT INTO event_connections (event_id, platform, schedule_status)
                       VALUES ($1, 'facebook', 'failed')
                       ON CONFLICT (event_id, platform) DO UPDATE SET
                           schedule_status = 'failed',
                           updated_at      = NOW()"#,
                )
                .bind(event_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
                    .bind(event_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                anyhow::Ok(())
            }
            .await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

// ── Multi-stream relay: stream key fetch ──────────────────────────────────────

/// Fetch the default ingestion (stream) key for the authenticated YouTube channel.
/// Returns `{ rtmpUrl: "rtmp://a.rtmp.youtube.com/live2/STREAM_KEY" }`.
pub async fn get_youtube_stream_key(State(state): State<AppState>) -> impl IntoResponse {
    let token = match youtube::load_tokens(&state.pool).await {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated with YouTube"})),
            )
                .into_response();
        }
    };

    #[derive(Deserialize)]
    struct IngestionInfo {
        #[serde(rename = "ingestionAddress")]
        ingestion_address: String,
        #[serde(rename = "streamName")]
        stream_name: String,
    }

    #[derive(Deserialize)]
    struct Cdn {
        #[serde(rename = "ingestionInfo")]
        ingestion_info: IngestionInfo,
    }

    #[derive(Deserialize)]
    struct StreamItem {
        cdn: Cdn,
    }

    #[derive(Deserialize)]
    struct StreamList {
        items: Option<Vec<StreamItem>>,
    }

    let client = reqwest::Client::new();
    let resp = client
        .get("https://www.googleapis.com/youtube/v3/liveStreams")
        .query(&[("part", "cdn"), ("mine", "true")])
        .bearer_auth(&token.access_token)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => match r.json::<StreamList>().await {
            Ok(list) => match list.items.and_then(|items| items.into_iter().next()) {
                Some(item) => {
                    let rtmp_url = format!(
                        "{}/{}",
                        item.cdn.ingestion_info.ingestion_address,
                        item.cdn.ingestion_info.stream_name
                    );
                    Json(json!({ "rtmpUrl": rtmp_url })).into_response()
                }
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "No YouTube live stream found for this account. Make sure you have a live stream set up in YouTube Studio."})),
                )
                    .into_response(),
            },
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response(),
        },
        Ok(r) => {
            let status = r.status();
            let detail = r.text().await.unwrap_or_default();
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("YouTube API {status}: {detail}")})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// Fetch the RTMP stream URL for the first scheduled Facebook live video on the page.
/// Returns `{ rtmpUrl: "rtmps://live-api-s.facebook.com:443/rtmp/STREAM_KEY" }`.
pub async fn get_facebook_stream_key(State(state): State<AppState>) -> impl IntoResponse {
    let token = match facebook::load_tokens(&state.pool).await {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated with Facebook"})),
            )
                .into_response();
        }
    };

    let config = state.facebook_config.read().await.clone();
    if config.page_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Facebook page_id not configured"})),
        )
            .into_response();
    }

    #[derive(Deserialize)]
    struct LiveVideo {
        secure_stream_url: Option<String>,
    }

    #[derive(Deserialize)]
    struct LiveVideoList {
        data: Vec<LiveVideo>,
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://graph.facebook.com/v19.0/{}/live_videos",
            config.page_id
        ))
        .query(&[
            ("fields", "secure_stream_url"),
            ("status", "SCHEDULED_UNPUBLISHED"),
            ("access_token", token.access_token.as_str()),
        ])
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => match r.json::<LiveVideoList>().await {
            Ok(list) => match list.data.into_iter().next().and_then(|v| v.secure_stream_url) {
                Some(url) => Json(json!({ "rtmpUrl": url })).into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "No scheduled Facebook live video found. Create a live event in the app first."})),
                )
                    .into_response(),
            },
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
                .into_response(),
        },
        Ok(r) => {
            let status = r.status();
            let detail = r.text().await.unwrap_or_default();
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Facebook API {status}: {detail}")})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// The template the editor renders `computed_title` from. Unset falls back to
/// `TitleTemplate::default()`, so this never 404s.
pub async fn get_title_template(State(state): State<AppState>) -> impl IntoResponse {
    Json(settings::get_json::<TitleTemplate>(&state.pool, "title_template").await)
}

pub async fn set_title_template(
    State(state): State<AppState>,
    Json(body): Json<TitleTemplate>,
) -> impl IntoResponse {
    match settings::set_json(&state.pool, "title_template", &body).await {
        Ok(()) => (StatusCode::OK, Json(body)).into_response(),
        Err(e) => {
            tracing::error!("set_title_template: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The folder generated Bible slide decks are written into. An unset key returns
/// an empty path, so this never 404s.
pub async fn get_slide_folder(State(state): State<AppState>) -> impl IntoResponse {
    Json(settings::get_json::<SlideFolder>(&state.pool, BIBLE_SLIDE_FOLDER_KEY).await)
}

/// Every window sends a plain string — the desktop shell that *is* the core fills
/// it from a native picker, everyone else types it — so the core is what checks
/// the path actually exists on the machine that will do the writing.
pub async fn set_slide_folder(
    State(state): State<AppState>,
    Json(body): Json<SlideFolder>,
) -> impl IntoResponse {
    let folder = SlideFolder {
        path: body.path.trim().to_string(),
    };
    if !folder.path.is_empty() && !std::path::Path::new(&folder.path).is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("No such folder: {}", folder.path)})),
        )
            .into_response();
    }
    match settings::set_json(&state.pool, BIBLE_SLIDE_FOLDER_KEY, &folder).await {
        Ok(()) => (StatusCode::OK, Json(folder)).into_response(),
        Err(e) => {
            tracing::error!("set_slide_folder: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The folder generated song slide decks are written into. An unset key returns
/// an empty path, so this never 404s.
pub async fn get_song_slide_folder(State(state): State<AppState>) -> impl IntoResponse {
    Json(settings::get_json::<SlideFolder>(&state.pool, SONG_SLIDE_FOLDER_KEY).await)
}

pub async fn set_song_slide_folder(
    State(state): State<AppState>,
    Json(body): Json<SlideFolder>,
) -> impl IntoResponse {
    let folder = SlideFolder {
        path: body.path.trim().to_string(),
    };
    if !folder.path.is_empty() && !std::path::Path::new(&folder.path).is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("No such folder: {}", folder.path)})),
        )
            .into_response();
    }
    match settings::set_json(&state.pool, SONG_SLIDE_FOLDER_KEY, &folder).await {
        Ok(()) => (StatusCode::OK, Json(folder)).into_response(),
        Err(e) => {
            tracing::error!("set_song_slide_folder: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSongSlidesBody {
    title: String,
    lyrics: String,
}

/// Turns pasted lyrics into a PowerPoint in the configured song output folder.
pub async fn create_song_slides(
    State(state): State<AppState>,
    Json(body): Json<CreateSongSlidesBody>,
) -> impl IntoResponse {
    let title = body.title.trim();
    let lyrics = body.lyrics.trim();
    if title.is_empty()
        || lyrics.is_empty()
        || title.chars().count() > 200
        || lyrics.chars().count() > 50_000
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Enter a title and lyrics"})),
        )
            .into_response();
    }

    let folder = settings::get_json::<SlideFolder>(&state.pool, SONG_SLIDE_FOLDER_KEY).await;
    let dir = std::path::PathBuf::from(&folder.path);
    if folder.path.is_empty() || !dir.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "No song slide folder configured"})),
        )
            .into_response();
    }

    let Some(file_name) = presenter::song_file_name(title) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "The title must contain letters or numbers"})),
        )
            .into_response();
    };
    let slides = presenter::song_slides(title, lyrics);
    let slide_count = slides.len();
    let path = dir.join(file_name);
    let theme = *state.presenter_theme.read().await;
    if let Err(error) = presenter::write_pptx(&path, &slides, 12_192_000, 6_858_000, theme) {
        tracing::error!("create_song_slides: {error}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": error})),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "filePath": path.to_string_lossy(),
            "slideCount": slide_count,
        })),
    )
        .into_response()
}

/// Writes one `.pptx` per Bible reference on the event into the configured slide
/// folder, using the same pagination the web presenter shows. The names are fixed
/// — `textus.pptx` and `lekcio.pptx` — so a regenerated deck replaces the one a
/// projector is already pointed at.
pub async fn create_event_slides(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let folder = settings::get_json::<SlideFolder>(&state.pool, BIBLE_SLIDE_FOLDER_KEY).await;
    if folder.path.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "No Bible slide folder configured"})),
        )
            .into_response();
    }
    let dir = std::path::PathBuf::from(&folder.path);
    if !dir.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("No such folder: {}", folder.path)})),
        )
            .into_response();
    }

    let event = match fetch_event(id, &state.pool).await {
        Ok(Some(event)) => event,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("create_event_slides: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let theme = *state.presenter_theme.read().await;
    let mut files: Vec<String> = Vec::new();
    for reference in &event.bible_references {
        let (kind, file_name) = match reference.r#type.as_str() {
            "textus" => (BibleReferenceType::Textus, "textus.pptx"),
            "leckio" => (BibleReferenceType::Leckio, "lekcio.pptx"),
            _ => continue,
        };
        let verses: Vec<BibleVerseContent> =
            serde_json::from_value(reference.verses.clone()).unwrap_or_default();
        if verses.is_empty() {
            continue;
        }
        let deck =
            PresenterState::from_bible_reference(&event.title, kind, &reference.reference, verses);
        let path = dir.join(file_name);
        if let Err(e) = presenter::write_pptx(
            &path,
            &deck.slides,
            deck.slide_width_emu,
            deck.slide_height_emu,
            theme,
        ) {
            tracing::error!("create_event_slides: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e}))).into_response();
        }
        files.push(path.to_string_lossy().into_owned());
    }

    if files.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Event has no Bible references with verses"})),
        )
            .into_response();
    }
    (StatusCode::OK, Json(json!({ "files": files }))).into_response()
}

pub async fn list_events(State(state): State<AppState>) -> impl IntoResponse {
    match fetch_event_summaries(&state.pool).await {
        Ok(events) => (StatusCode::OK, Json(events)).into_response(),
        Err(e) => {
            tracing::error!("list_events: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_event(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match fetch_event(id, &state.pool).await {
        Ok(Some(event)) => (StatusCode::OK, Json(event)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Upsert (or delete) bible references for an event inside an open transaction.
/// An entry with an empty reference string is deleted; non-empty entries are upserted.
async fn upsert_bible_references(
    event_id: Uuid,
    refs: &Option<Vec<CreateBibleReference>>,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> anyhow::Result<()> {
    let Some(refs) = refs else { return Ok(()) };
    for br in refs {
        let reference = br.reference.as_deref().unwrap_or("").trim();
        if reference.is_empty() {
            sqlx::query("DELETE FROM event_bible_references WHERE event_id = $1 AND type = $2")
                .bind(event_id)
                .bind(&br.r#type)
                .execute(&mut **tx)
                .await?;
        } else {
            let translation = br.translation.as_deref().unwrap_or("UF");
            let verses = br.verses.clone().unwrap_or_else(|| serde_json::json!([]));
            sqlx::query(
                "INSERT INTO event_bible_references (event_id, type, reference, translation, verses) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (event_id, type) DO UPDATE SET \
                   reference = EXCLUDED.reference, \
                   translation = EXCLUDED.translation, \
                   verses = EXCLUDED.verses, \
                   updated_at = NOW()",
            )
            .bind(event_id)
            .bind(&br.r#type)
            .bind(reference)
            .bind(translation)
            .bind(verses)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

pub async fn create_event(
    State(state): State<AppState>,
    Json(body): Json<CreateEvent>,
) -> impl IntoResponse {
    let result: anyhow::Result<_> = async {
        let mut tx = state.pool.begin().await?;
        sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
            .execute(&mut *tx)
            .await?;

        let event_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO events (title, computed_title, date_time, speaker, description, auto_upload_enabled)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id"#,
        )
        .bind(&body.title)
        .bind(body.computed_title.unwrap_or_default())
        .bind(body.date_time)
        .bind(body.speaker.unwrap_or_default())
        .bind(body.description.unwrap_or_default())
        .bind(body.auto_upload_enabled.unwrap_or(false))
        .fetch_one(&mut *tx)
        .await?;

        // Default connections: youtube (private) and facebook (EVERYONE).
        // Override privacy if provided in the request body.
        let mut conn_map: Vec<(String, String)> = vec![
            ("youtube".to_string(), "private".to_string()),
            ("facebook".to_string(), "EVERYONE".to_string()),
        ];
        if let Some(req_conns) = &body.connections {
            for c in req_conns {
                if let Some(entry) = conn_map.iter_mut().find(|(p, _)| p == &c.platform) {
                    if let Some(ps) = &c.privacy_status {
                        entry.1 = ps.clone();
                    }
                } else {
                    conn_map.push((
                        c.platform.clone(),
                        c.privacy_status.clone().unwrap_or_default(),
                    ));
                }
            }
        }

        for (platform, privacy) in &conn_map {
            sqlx::query(
                "INSERT INTO event_connections (event_id, platform, privacy_status) VALUES ($1, $2, $3)",
            )
            .bind(event_id)
            .bind(platform)
            .bind(privacy)
            .execute(&mut *tx)
            .await?;
        }

        upsert_bible_references(event_id, &body.bible_references, &mut tx).await?;

        tx.commit().await?;

        let event = fetch_event(event_id, &state.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("event {event_id} not found after create"))?;
        anyhow::Ok(event)
    }
    .await;

    match result {
        Ok(event) => {
            broadcast_event_changed(&state, "INSERT", &event).await;
            spawn_scheduling_tasks(event.clone(), state);
            (StatusCode::CREATED, Json(event)).into_response()
        }
        Err(e) => {
            tracing::error!("create_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn update_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateEvent>,
) -> impl IntoResponse {
    let result: anyhow::Result<Option<_>> = async {
        let mut tx = state.pool.begin().await?;
        sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
            .execute(&mut *tx)
            .await?;

        let updated_id: Option<Uuid> = sqlx::query_scalar(
            r#"UPDATE events SET
                title = $1,
                computed_title = $2,
                date_time = $3,
                speaker = $4,
                description = $5,
                auto_upload_enabled = $6,
                updated_at = NOW()
            WHERE id = $7
            RETURNING id"#,
        )
        .bind(&body.title)
        .bind(body.computed_title.unwrap_or_default())
        .bind(body.date_time)
        .bind(body.speaker.unwrap_or_default())
        .bind(body.description.unwrap_or_default())
        .bind(body.auto_upload_enabled.unwrap_or(false))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        if updated_id.is_none() {
            tx.rollback().await?;
            return Ok(None);
        }

        if let Some(conns) = &body.connections {
            for conn in conns {
                if let Some(ps) = &conn.privacy_status {
                    sqlx::query(
                        "UPDATE event_connections SET privacy_status = $1, updated_at = NOW() \
                         WHERE event_id = $2 AND platform = $3",
                    )
                    .bind(ps)
                    .bind(id)
                    .bind(&conn.platform)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        upsert_bible_references(id, &body.bible_references, &mut tx).await?;

        tx.commit().await?;

        let event = fetch_event(id, &state.pool).await?;
        anyhow::Ok(event)
    }
    .await;

    match result {
        Ok(Some(event)) => {
            broadcast_event_changed(&state, "UPDATE", &event).await;
            spawn_scheduling_tasks(event.clone(), state);
            (StatusCode::OK, Json(event)).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("update_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct AllRecordingsQuery {
    filter: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingWithEvent {
    #[serde(flatten)]
    recording: Recording,
    event_title: String,
}

pub async fn list_all_recordings(
    State(state): State<AppState>,
    Query(params): Query<AllRecordingsQuery>,
) -> impl IntoResponse {
    let where_clause = match params.filter.as_deref().unwrap_or("") {
        // Never flagged and no upload history at all
        "not_flagged" => {
            "r.uploadable = false AND NOT EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id)"
        }
        // Flagged for upload but no active or completed upload yet
        "flagged" => {
            "r.uploadable = true AND NOT EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id AND state IN ('uploading','paused','completed','failed'))"
        }
        // Upload started and currently active/failed
        "in_progress" => {
            "EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id AND state IN ('uploading','paused','failed'))"
        }
        // At least one platform upload completed (r.uploaded may not be set)
        "uploaded" => {
            "EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id AND state = 'completed')"
        }
        _ => "true",
    };
    let sql = format!(
        "SELECT r.*, e.title AS _event_title \
         FROM recordings r JOIN events e ON e.id = r.event_id \
         WHERE {where_clause} \
         ORDER BY r.detected_at DESC LIMIT 100"
    );

    #[derive(sqlx::FromRow)]
    struct RecordingRow {
        #[sqlx(flatten)]
        recording: Recording,
        _event_title: String,
    }

    let rows = match sqlx::query_as::<_, RecordingRow>(&sql)
        .fetch_all(&state.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_all_recordings: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut results: Vec<RecordingWithEvent> = rows
        .into_iter()
        .map(|row| RecordingWithEvent {
            event_title: row._event_title.clone(),
            recording: row.recording,
        })
        .collect();

    if !results.is_empty() {
        let ids: Vec<Uuid> = results.iter().map(|r| r.recording.id).collect();
        let uploads = sqlx::query_as::<_, RecordingUpload>(
            "SELECT recording_id, platform, state, progress_bytes, total_bytes, \
             visibility, video_id, video_url, error, started_at, completed_at, updated_at \
             FROM recording_uploads WHERE recording_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        for item in &mut results {
            item.recording.uploads = uploads
                .iter()
                .filter(|u| u.recording_id == item.recording.id)
                .cloned()
                .collect();
        }
    }

    (StatusCode::OK, Json(results)).into_response()
}

pub async fn list_recordings(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut recordings = match sqlx::query_as::<_, Recording>(
        "SELECT * FROM recordings WHERE event_id = $1 ORDER BY detected_at DESC",
    )
    .bind(event_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("list_recordings: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !recordings.is_empty() {
        let ids: Vec<Uuid> = recordings.iter().map(|r| r.id).collect();
        let uploads = sqlx::query_as::<_, RecordingUpload>(
            "SELECT recording_id, platform, state, progress_bytes, total_bytes, \
             visibility, video_id, video_url, error, started_at, completed_at, updated_at \
             FROM recording_uploads WHERE recording_id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        for rec in &mut recordings {
            rec.uploads = uploads
                .iter()
                .filter(|u| u.recording_id == rec.id)
                .cloned()
                .collect();
        }
    }

    (StatusCode::OK, Json(recordings)).into_response()
}

pub async fn create_recording(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(body): Json<CreateRecording>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, Recording>(
        r#"
        INSERT INTO recordings (
            event_id, file_path, file_name, file_size, duration_seconds,
            custom_title, custom_description, detected_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(event_id)
    .bind(&body.file_path)
    .bind(&body.file_name)
    .bind(body.file_size.unwrap_or(0))
    .bind(body.duration_seconds.unwrap_or(0.0))
    .bind(body.custom_title.as_deref())
    .bind(body.custom_description.as_deref())
    .bind(Utc::now())
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(recording) => (StatusCode::CREATED, Json(recording)).into_response(),
        Err(e) => {
            tracing::error!("create_recording: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct DeleteRecordingParams {
    #[serde(default)]
    pub delete_file: bool,
}

pub async fn delete_recording(
    State(state): State<AppState>,
    Path((event_id, recording_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<DeleteRecordingParams>,
) -> impl IntoResponse {
    let row =
        sqlx::query_as::<_, Recording>("SELECT * FROM recordings WHERE id = $1 AND event_id = $2")
            .bind(recording_id)
            .bind(event_id)
            .fetch_optional(&state.pool)
            .await;

    match row {
        Ok(Some(rec)) => {
            let del = sqlx::query("DELETE FROM recordings WHERE id = $1")
                .bind(recording_id)
                .execute(&state.pool)
                .await;
            if let Err(e) = del {
                tracing::error!("delete_recording DB: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            if params.delete_file
                && let Err(e) = tokio::fs::remove_file(&rec.file_path).await
            {
                tracing::warn!(
                    "delete_recording: could not delete file {}: {e}",
                    rec.file_path
                );
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_recording fetch: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn delete_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let event = match fetch_event(id, &state.pool).await {
        Ok(Some(e)) => e,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_event fetch: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let result = sqlx::query("DELETE FROM events WHERE id = $1 RETURNING id")
        .bind(id)
        .fetch_optional(&state.pool)
        .await;

    match result {
        Ok(Some(_)) => {
            crate::queue::enqueue_youtube_delete(&state.pool, &event).await;
            broadcast_event_changed(&state, "DELETE", &event).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_event: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── Cron Jobs ─────────────────────────────────────────────────────────────────

pub async fn list_cron_jobs(State(state): State<AppState>) -> impl IntoResponse {
    match cron_job::list_all(&state.pool).await {
        Ok(jobs) => Json(jobs).into_response(),
        Err(e) => {
            tracing::error!("list_cron_jobs: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn create_cron_job(
    State(state): State<AppState>,
    Json(body): Json<CreateCronJob>,
) -> impl IntoResponse {
    // Validate the cron expression before inserting.
    if tokio_cron_scheduler::Job::new_async(
        body.cron_expression.as_str(),
        |_, _| Box::pin(async {}),
    )
    .is_err()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid cron expression"})),
        )
            .into_response();
    }

    let result: anyhow::Result<cron_job::CronJob> = async {
        let mut tx = state.pool.begin().await?;

        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                bool,
                chrono::DateTime<Utc>,
                chrono::DateTime<Utc>,
            ),
        >(
            "INSERT INTO cron_jobs (name, cron_expression, enabled) \
             VALUES ($1, $2, $3) \
             RETURNING id, name, cron_expression, enabled, created_at, updated_at",
        )
        .bind(&body.name)
        .bind(&body.cron_expression)
        .bind(body.enabled)
        .fetch_one(&mut *tx)
        .await?;

        cron_job::sync_features(&mut tx, row.0, body.pull_youtube, body.auto_upload).await?;
        tx.commit().await?;

        Ok(cron_job::CronJob {
            id: row.0,
            name: row.1,
            cron_expression: row.2,
            enabled: row.3,
            pull_youtube: body.pull_youtube,
            auto_upload: body.auto_upload,
            created_at: row.4,
            updated_at: row.5,
        })
    }
    .await;

    match result {
        Ok(job) => {
            let pool = state.pool.clone();
            let clients = state.ws_clients.clone();
            let yt = state.youtube_connector.clone();
            let sched = state.cron_scheduler.clone();
            let us = state.upload_service.clone();
            tokio::spawn(async move {
                sched.reload(pool, clients, yt, us).await;
            });
            (StatusCode::CREATED, Json(job)).into_response()
        }
        Err(e) => {
            tracing::error!("create_cron_job: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn update_cron_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCronJob>,
) -> impl IntoResponse {
    // Validate the cron expression before updating.
    if tokio_cron_scheduler::Job::new_async(
        body.cron_expression.as_str(),
        |_, _| Box::pin(async {}),
    )
    .is_err()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid cron expression"})),
        )
            .into_response();
    }

    let result: anyhow::Result<Option<cron_job::CronJob>> = async {
        let mut tx = state.pool.begin().await?;

        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                bool,
                chrono::DateTime<Utc>,
                chrono::DateTime<Utc>,
            ),
        >(
            "UPDATE cron_jobs \
             SET name = $1, cron_expression = $2, enabled = $3, updated_at = NOW() \
             WHERE id = $4 \
             RETURNING id, name, cron_expression, enabled, created_at, updated_at",
        )
        .bind(&body.name)
        .bind(&body.cron_expression)
        .bind(body.enabled)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(None);
        };

        cron_job::sync_features(&mut tx, row.0, body.pull_youtube, body.auto_upload).await?;
        tx.commit().await?;

        Ok(Some(cron_job::CronJob {
            id: row.0,
            name: row.1,
            cron_expression: row.2,
            enabled: row.3,
            pull_youtube: body.pull_youtube,
            auto_upload: body.auto_upload,
            created_at: row.4,
            updated_at: row.5,
        }))
    }
    .await;

    match result {
        Ok(Some(job)) => {
            let pool = state.pool.clone();
            let clients = state.ws_clients.clone();
            let yt = state.youtube_connector.clone();
            let sched = state.cron_scheduler.clone();
            let us = state.upload_service.clone();
            tokio::spawn(async move {
                sched.reload(pool, clients, yt, us).await;
            });
            Json(job).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("update_cron_job: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn delete_cron_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM cron_jobs WHERE id = $1 RETURNING id")
        .bind(id)
        .fetch_optional(&state.pool)
        .await;

    match result {
        Ok(Some(_)) => {
            let pool = state.pool.clone();
            let clients = state.ws_clients.clone();
            let yt = state.youtube_connector.clone();
            let sched = state.cron_scheduler.clone();
            let us = state.upload_service.clone();
            tokio::spawn(async move {
                sched.reload(pool, clients, yt, us).await;
            });
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_cron_job: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── Upload flag & trigger ─────────────────────────────────────────────────────

pub async fn flag_upload(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(body): Json<FlagUploadRequest>,
) -> impl IntoResponse {
    let result: anyhow::Result<()> = async {
        for item in &body.recordings {
            // Mark the recording as uploadable, optionally update custom title/description.
            sqlx::query(
                "UPDATE recordings SET uploadable = true, \
                 custom_title = COALESCE($1, custom_title), \
                 custom_description = COALESCE($2, custom_description), \
                 updated_at = NOW() \
                 WHERE id = $3 AND event_id = $4",
            )
            .bind(item.custom_title.as_deref())
            .bind(item.custom_description.as_deref())
            .bind(item.recording_id)
            .bind(event_id)
            .execute(&state.pool)
            .await?;

            // Insert/update upload rows for each requested platform.
            for platform in &item.platforms {
                let visibility = if platform == "youtube" {
                    item.youtube_visibility
                        .as_deref()
                        .unwrap_or("private")
                        .to_string()
                } else {
                    item.facebook_visibility
                        .as_deref()
                        .unwrap_or("ONLY_ME")
                        .to_string()
                };

                sqlx::query(
                    "INSERT INTO recording_uploads (recording_id, platform, state, visibility, updated_at) \
                     VALUES ($1, $2, 'pending', $3, NOW()) \
                     ON CONFLICT (recording_id, platform) DO UPDATE SET \
                         state = CASE WHEN recording_uploads.state = 'completed' \
                                      THEN 'completed' ELSE 'pending' END, \
                         visibility = EXCLUDED.visibility, \
                         updated_at = NOW()",
                )
                .bind(item.recording_id)
                .bind(platform)
                .bind(&visibility)
                .execute(&state.pool)
                .await?;
            }
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("flag_upload: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn trigger_upload_cycle(State(state): State<AppState>) -> impl IntoResponse {
    let us = state.upload_service.clone();
    tokio::spawn(async move {
        us.run_cycle().await;
    });
    StatusCode::NO_CONTENT.into_response()
}

// ── Untracked recordings ───────────────────────────────────────────────────────

pub async fn list_untracked_recordings(State(state): State<AppState>) -> impl IntoResponse {
    match untracked_recording::list_untracked(&state.pool).await {
        Ok(recordings) => (StatusCode::OK, Json(recordings)).into_response(),
        Err(e) => {
            tracing::error!("list_untracked_recordings: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct AssignRecordingBody {
    pub event_id: Uuid,
}

pub async fn assign_untracked_recording(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignRecordingBody>,
) -> impl IntoResponse {
    let result: anyhow::Result<Recording> = async {
        // Fetch the untracked row (404 if missing)
        let untracked = sqlx::query_as::<_, untracked_recording::UntrackedRecording>(
            "SELECT * FROM untracked_recordings WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("NOT_FOUND"))?;

        // Verify event exists
        let event_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
                .bind(body.event_id)
                .fetch_one(&state.pool)
                .await?;
        if !event_exists {
            return Err(anyhow::anyhow!("EVENT_NOT_FOUND"));
        }

        let mut tx = state.pool.begin().await?;
        let recording = sqlx::query_as::<_, Recording>(
            r#"INSERT INTO recordings (event_id, file_path, file_name, file_size, duration_seconds, detected_at)
               VALUES ($1, $2, $3, $4, $5, $6) RETURNING *"#,
        )
        .bind(body.event_id)
        .bind(&untracked.file_path)
        .bind(&untracked.file_name)
        .bind(untracked.file_size)
        .bind(untracked.duration_seconds)
        .bind(untracked.detected_at)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM untracked_recordings WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(recording)
    }
    .await;

    match result {
        Ok(recording) => {
            // Broadcast the removal of the untracked recording
            let clients = state.ws_clients.clone();
            tokio::spawn(async move {
                broadcast_untracked_removed(&clients, id).await;
            });
            (StatusCode::CREATED, Json(recording)).into_response()
        }
        Err(e) if e.to_string() == "NOT_FOUND" => StatusCode::NOT_FOUND.into_response(),
        Err(e) if e.to_string() == "EVENT_NOT_FOUND" => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("assign_untracked_recording: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct DeleteUntrackedParams {
    #[serde(default)]
    pub delete_file: bool,
}

pub async fn delete_untracked_recording(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<DeleteUntrackedParams>,
) -> impl IntoResponse {
    let row = sqlx::query_as::<_, untracked_recording::UntrackedRecording>(
        "SELECT * FROM untracked_recordings WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some(rec)) => {
            let del = sqlx::query("DELETE FROM untracked_recordings WHERE id = $1")
                .bind(id)
                .execute(&state.pool)
                .await;
            if let Err(e) = del {
                tracing::error!("delete_untracked_recording DB: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            if params.delete_file
                && let Err(e) = tokio::fs::remove_file(&rec.file_path).await
            {
                tracing::warn!(
                    "delete_untracked_recording: could not delete file {}: {e}",
                    rec.file_path
                );
            }
            let clients = state.ws_clients.clone();
            tokio::spawn(async move {
                broadcast_untracked_removed(&clients, id).await;
            });
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_untracked_recording fetch: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── Event activities ───────────────────────────────────────────────────────────

pub async fn list_event_activities(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
) -> impl IntoResponse {
    match activity::list_activities(event_id, &state.pool).await {
        Ok(activities) => (StatusCode::OK, Json(activities)).into_response(),
        Err(e) => {
            tracing::error!("list_event_activities: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn create_event_activity(
    State(state): State<AppState>,
    Path(event_id): Path<Uuid>,
    Json(body): Json<CreateEventActivity>,
) -> impl IntoResponse {
    // Verify event exists
    let event_exists: Result<bool, _> =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
            .bind(event_id)
            .fetch_one(&state.pool)
            .await;

    match event_exists {
        Ok(false) | Err(_) => return StatusCode::NOT_FOUND.into_response(),
        Ok(true) => {}
    }

    let result = sqlx::query_as::<_, activity::EventActivity>(
        "INSERT INTO event_activities (event_id, activity_type, message) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(event_id)
    .bind(&body.activity_type)
    .bind(&body.message)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(act) => (StatusCode::CREATED, Json(act)).into_response(),
        Err(e) => {
            tracing::error!("create_event_activity: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn delete_event_activity(
    State(state): State<AppState>,
    Path((event_id, activity_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let result =
        sqlx::query("DELETE FROM event_activities WHERE id = $1 AND event_id = $2 RETURNING id")
            .bind(activity_id)
            .bind(event_id)
            .fetch_optional(&state.pool)
            .await;

    match result {
        Ok(Some(_)) => StatusCode::NO_CONTENT.into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete_event_activity: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── Broadlink ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BroadlinkDevice {
    id: Uuid,
    name: String,
    device_type: String,
    model: Option<String>,
    host: String,
    mac: String,
    is_default: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BroadlinkCommand {
    id: Uuid,
    device_id: Option<Uuid>,
    name: String,
    slug: String,
    code: String,
    code_type: String,
    category: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDeviceBody {
    name: String,
    host: String,
    mac: String,
    device_type: String,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCommandBody {
    device_id: Option<Uuid>,
    name: String,
    slug: String,
    code: String,
    code_type: String,
    category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCommandBody {
    name: Option<String>,
    slug: Option<String>,
    code: Option<String>,
    code_type: Option<String>,
    category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnBody {
    signal_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommandsQuery {
    device_id: Option<Uuid>,
    category: Option<String>,
}

pub async fn broadlink_get_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.broadlink_connector.get_status().await;
    Json(json!({ "status": status }))
}

pub async fn broadlink_list_devices(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String, String, bool)>(
        "SELECT id, name, device_type, model, host, mac, is_default FROM broadlink_devices ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(devices) => {
            let list: Vec<BroadlinkDevice> = devices
                .into_iter()
                .map(
                    |(id, name, device_type, model, host, mac, is_default)| BroadlinkDevice {
                        id,
                        name,
                        device_type,
                        model,
                        host,
                        mac,
                        is_default,
                    },
                )
                .collect();
            Json(list).into_response()
        }
        Err(e) => {
            tracing::error!("broadlink_list_devices: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_add_device(
    State(state): State<AppState>,
    Json(body): Json<AddDeviceBody>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO broadlink_devices (name, device_type, model, host, mac) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&body.name)
    .bind(&body.device_type)
    .bind(&body.model)
    .bind(&body.host)
    .bind(&body.mac)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok((id,)) => {
            // Update connector status: now we have at least one device
            state
                .broadlink_connector
                .set_status(crate::connectors::ConnectorStatus::Connected)
                .await;
            let device = BroadlinkDevice {
                id,
                name: body.name,
                device_type: body.device_type,
                model: body.model,
                host: body.host,
                mac: body.mac,
                is_default: false,
            };
            (StatusCode::CREATED, Json(device)).into_response()
        }
        Err(e) => {
            tracing::error!("broadlink_add_device: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_remove_device(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM broadlink_devices WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => StatusCode::NOT_FOUND.into_response(),
        Ok(_) => {
            // Check if any devices remain; update status accordingly
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM broadlink_devices")
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0);
            let new_status = if count > 0 {
                crate::connectors::ConnectorStatus::Connected
            } else {
                crate::connectors::ConnectorStatus::Disconnected
            };
            state.broadlink_connector.set_status(new_status).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!("broadlink_remove_device: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_discover(State(state): State<AppState>) -> impl IntoResponse {
    let clients = state.ws_clients.clone();
    let pool = state.pool.clone();
    let connector = state.broadlink_connector.clone();

    tokio::spawn(async move {
        match crate::broadlink::discover_devices(5).await {
            Ok(devices) => {
                for dev in devices {
                    let msg = json!({
                        "type": "broadlink.device.discovered",
                        "device": {
                            "name": dev.name,
                            "host": dev.host,
                            "mac": dev.mac,
                            "deviceType": dev.device_type,
                            "model": dev.model,
                        }
                    })
                    .to_string();
                    let guard = clients.read().await;
                    for tx in guard.values() {
                        let _ = tx.send(axum::extract::ws::Message::Text(msg.clone().into()));
                    }
                    drop(guard);

                    // Upsert discovered device into DB
                    let _ = sqlx::query(
                        "INSERT INTO broadlink_devices (name, device_type, model, host, mac, last_seen_at) \
                         VALUES ($1, $2, $3, $4, $5, NOW()) \
                         ON CONFLICT (mac) DO UPDATE SET host = EXCLUDED.host, last_seen_at = NOW()",
                    )
                    .bind(&dev.name)
                    .bind(&dev.device_type)
                    .bind(&dev.model)
                    .bind(&dev.host)
                    .bind(&dev.mac)
                    .execute(&pool)
                    .await;
                }

                // Refresh status after upserts
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM broadlink_devices")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0);
                let new_status = if count > 0 {
                    crate::connectors::ConnectorStatus::Connected
                } else {
                    crate::connectors::ConnectorStatus::Disconnected
                };
                connector.set_status(new_status).await;
            }
            Err(e) => tracing::error!("broadlink_discover: {e}"),
        }
    });

    StatusCode::ACCEPTED.into_response()
}

pub async fn broadlink_list_commands(
    State(state): State<AppState>,
    Query(q): Query<CommandsQuery>,
) -> impl IntoResponse {
    let rows = if let Some(device_id) = q.device_id {
        if let Some(cat) = q.category {
            sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String)>(
                "SELECT id, device_id, name, slug, code, code_type, category \
                 FROM broadlink_commands WHERE device_id = $1 AND category = $2 ORDER BY created_at",
            )
            .bind(device_id)
            .bind(cat)
            .fetch_all(&state.pool)
            .await
        } else {
            sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String)>(
                "SELECT id, device_id, name, slug, code, code_type, category \
                 FROM broadlink_commands WHERE device_id = $1 ORDER BY created_at",
            )
            .bind(device_id)
            .fetch_all(&state.pool)
            .await
        }
    } else {
        sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String)>(
            "SELECT id, device_id, name, slug, code, code_type, category \
             FROM broadlink_commands ORDER BY created_at",
        )
        .fetch_all(&state.pool)
        .await
    };

    match rows {
        Ok(commands) => {
            let list: Vec<BroadlinkCommand> = commands
                .into_iter()
                .map(
                    |(id, device_id, name, slug, code, code_type, category)| BroadlinkCommand {
                        id,
                        device_id,
                        name,
                        slug,
                        code,
                        code_type,
                        category,
                    },
                )
                .collect();
            Json(list).into_response()
        }
        Err(e) => {
            tracing::error!("broadlink_list_commands: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_add_command(
    State(state): State<AppState>,
    Json(body): Json<AddCommandBody>,
) -> impl IntoResponse {
    let category = body.category.unwrap_or_else(|| "other".to_string());
    let result = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO broadlink_commands (device_id, name, slug, code, code_type, category) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(body.device_id)
    .bind(&body.name)
    .bind(&body.slug)
    .bind(&body.code)
    .bind(&body.code_type)
    .bind(&category)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok((id,)) => {
            let cmd = BroadlinkCommand {
                id,
                device_id: body.device_id,
                name: body.name,
                slug: body.slug,
                code: body.code,
                code_type: body.code_type,
                category,
            };
            (StatusCode::CREATED, Json(cmd)).into_response()
        }
        Err(e) => {
            tracing::error!("broadlink_add_command: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_update_command(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCommandBody>,
) -> impl IntoResponse {
    let result = sqlx::query(
        "UPDATE broadlink_commands SET \
         name = COALESCE($2, name), \
         slug = COALESCE($3, slug), \
         code = COALESCE($4, code), \
         code_type = COALESCE($5, code_type), \
         category = COALESCE($6, category), \
         updated_at = NOW() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(&body.name)
    .bind(&body.slug)
    .bind(&body.code)
    .bind(&body.code_type)
    .bind(&body.category)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => StatusCode::NOT_FOUND.into_response(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("broadlink_update_command: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_remove_command(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let result = sqlx::query("DELETE FROM broadlink_commands WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => StatusCode::NOT_FOUND.into_response(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("broadlink_remove_command: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn broadlink_start_learn(
    State(state): State<AppState>,
    Path(device_id): Path<Uuid>,
    Json(body): Json<Option<LearnBody>>,
) -> impl IntoResponse {
    // Prevent concurrent learns
    if state
        .broadlink_learn_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "Learning already in progress" })),
        )
            .into_response();
    }

    let signal_type = body
        .and_then(|b| b.signal_type)
        .unwrap_or_else(|| "ir".to_string());

    // Fetch device info
    let device = sqlx::query_as::<_, (String, String, String)>(
        "SELECT host, mac, device_type FROM broadlink_devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(&state.pool)
    .await;

    let (host, mac, devtype) = match device {
        Ok(Some(row)) => row,
        Ok(None) => {
            state.broadlink_learn_active.store(false, Ordering::SeqCst);
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Device not found" })),
            )
                .into_response();
        }
        Err(e) => {
            state.broadlink_learn_active.store(false, Ordering::SeqCst);
            tracing::error!("broadlink_start_learn fetch device: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let learn_active = state.broadlink_learn_active.clone();
    let learn_tx = state.broadlink_connector.learn_tx.clone();

    tokio::spawn(async move {
        let result = crate::broadlink::learn_code(&host, &mac, &devtype, &signal_type).await;
        let event = match result {
            Ok(lr) => crate::connectors::broadlink::BroadlinkLearnEvent {
                code: lr.code,
                error: lr.error,
            },
            Err(e) => crate::connectors::broadlink::BroadlinkLearnEvent {
                code: None,
                error: Some(e),
            },
        };
        let _ = learn_tx.send(event);
        learn_active.store(false, Ordering::SeqCst);
    });

    StatusCode::ACCEPTED.into_response()
}

pub async fn broadlink_cancel_learn(State(state): State<AppState>) -> impl IntoResponse {
    crate::broadlink::cancel_learn().await;
    state.broadlink_learn_active.store(false, Ordering::SeqCst);
    StatusCode::NO_CONTENT.into_response()
}

pub async fn broadlink_send_command(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT bc.code, bd.host, bd.mac, bd.device_type \
         FROM broadlink_commands bc \
         JOIN broadlink_devices bd ON bc.device_id = bd.id \
         WHERE bc.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await;

    let (code, host, mac, devtype) = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Command not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("broadlink_send_command fetch: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match crate::broadlink::send_code(&host, &mac, &devtype, &code).await {
        Ok(r) if r.success => StatusCode::NO_CONTENT.into_response(),
        Ok(r) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": r.error.unwrap_or_default() })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("broadlink_send_command send: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The core's own application log, so a client device can read the server's log
/// rather than its own. Reads the same file the desktop log commands do.
pub async fn get_application_log(State(state): State<AppState>) -> impl IntoResponse {
    let Some(host) = state.host.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "Application log is unavailable on this core"})),
        )
            .into_response();
    };
    match host.application_log() {
        Ok(log) => Json(json!({
            "path": log.path,
            "content": log.content,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn clear_application_log(State(state): State<AppState>) -> impl IntoResponse {
    let Some(host) = state.host.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "Application log is unavailable on this core"})),
        )
            .into_response();
    };
    match host.clear_application_log() {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_and_bible_slides_use_different_folder_settings() {
        assert_ne!(SONG_SLIDE_FOLDER_KEY, BIBLE_SLIDE_FOLDER_KEY);
    }

    #[test]
    fn secret_comparison_rejects_wrong_and_shorter_values() {
        assert!(secret_eq("s3cret", "s3cret"));
        assert!(!secret_eq("s3cret", "s3crev"));
        assert!(!secret_eq("s3cret", "s3cre"));
        assert!(!secret_eq("", "s3cret"));
        // An absent header arrives as "" — it must never match a real token.
        assert!(!secret_eq("", "any-token"));
        assert!(secret_eq("", ""));
    }

    #[test]
    fn reading_a_config_blanks_secrets_and_reports_whether_they_are_set() {
        let mut config = json!({
            "enabled": true,
            "host": "localhost",
            "password": "hunter2",
            "apiKey": "",
        });
        redact_secrets(&mut config);

        assert_eq!(config["host"], "localhost");
        assert_eq!(config["password"], "");
        assert_eq!(config["passwordSet"], true);
        assert_eq!(config["apiKey"], "");
        assert_eq!(config["apiKeySet"], false);
    }

    #[test]
    fn saving_a_blank_secret_keeps_the_stored_one() {
        let stored = json!({ "enabled": true, "apiKey": "stored-key" });
        let mut incoming = json!({ "enabled": false, "apiKey": "", "apiKeySet": true });
        restore_omitted_secrets(&mut incoming, &stored);

        assert_eq!(incoming["enabled"], false);
        assert_eq!(incoming["apiKey"], "stored-key");
        // The read-only marker must never be persisted.
        assert!(incoming.get("apiKeySet").is_none());
    }

    #[test]
    fn an_explicit_set_false_clears_the_stored_secret() {
        let stored = json!({ "apiKey": "stored-key" });
        let mut incoming = json!({ "apiKey": "", "apiKeySet": false });
        restore_omitted_secrets(&mut incoming, &stored);

        assert_eq!(incoming["apiKey"], "");
        assert!(incoming.get("apiKeySet").is_none());
    }

    #[test]
    fn saving_a_new_secret_replaces_the_stored_one() {
        let stored = json!({ "apiKey": "stored-key" });
        let mut incoming = json!({ "apiKey": "fresh-key" });
        restore_omitted_secrets(&mut incoming, &stored);

        assert_eq!(incoming["apiKey"], "fresh-key");
    }

    #[test]
    fn an_omitted_secret_falls_back_to_the_stored_value() {
        let stored = json!({ "password": "hunter2" });
        let mut incoming = json!({ "enabled": true });
        restore_omitted_secrets(&mut incoming, &stored);

        assert_eq!(incoming["password"], "hunter2");
    }

    #[test]
    fn a_null_secret_is_treated_as_blank() {
        // ObsConfig.password is Option<String>, so null arrives for "unchanged".
        let stored = json!({ "password": "hunter2" });
        let mut incoming = json!({ "password": null });
        restore_omitted_secrets(&mut incoming, &stored);

        assert_eq!(incoming["password"], "hunter2");
    }
}
