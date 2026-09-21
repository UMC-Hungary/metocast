//! Blackmagic Camera Control API client.
//!
//! One [`Camera`] == one camera. Multi-camera is a `HashMap<Id, Camera>` in the
//! caller — there is deliberately no registry in here.
//!
//! Two layers:
//!   * [`Camera::get`] / [`Camera::put`] / [`Camera::post`] — the whole REST API,
//!     any path, typed into whatever you ask for.
//!   * Named methods for the endpoints the UI actually drives today.
//!
//! Anything not named below is one `get`/`put` call away; add a named method when
//! a caller earns it.

pub mod discovery;
pub mod notify;
mod tls;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use tls::{Trust, fingerprint_of};

const API: &str = "/control/api/v1";

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Camera HTTP status conventions, kept distinct so callers can tell "this model
/// doesn't have that feature" from "that failed".
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 501 — the endpoint exists in the API but not on this camera model.
    #[error("not supported by this camera: {0}")]
    NotSupported(String),
    /// 400
    #[error("bad request: {0}")]
    BadRequest(String),
    /// 403 — e.g. setting shutter while auto-exposure owns it.
    #[error("forbidden (likely controlled by another setting): {0}")]
    Forbidden(String),
    /// 404
    #[error("not found: {0}")]
    NotFound(String),
    /// 409 — not valid in the camera's current state.
    #[error("conflict — not valid in the camera's current state: {0}")]
    Conflict(String),
    /// 422
    #[error("unprocessable: {0}")]
    Unprocessable(String),
    #[error("authentication failed")]
    AuthFailed,
    #[error("certificate not trusted (camera presented {0})")]
    CertUntrusted(String),
    #[error("unreachable: {0}")]
    Unreachable(String),
    #[error("http {0} on {1}")]
    Http(u16, String),
    #[error("bad response body: {0}")]
    Body(String),
    /// The camera answered fine, but not in a way this operation can use.
    #[error("{0}")]
    Unexpected(String),
}

impl Error {
    /// `body` is the camera's own response text, when it sent one — folded in here
    /// rather than discarded, since a bare status code rarely says why a 403 happened
    /// (e.g. genuinely different reasons for the same 403 on `/livestreams/0/stop`).
    fn from_status(status: u16, path: &str, body: &str) -> Self {
        let context = if body.trim().is_empty() {
            path.to_string()
        } else {
            format!("{path}: {}", body.trim())
        };
        match status {
            400 => Error::BadRequest(context),
            401 => Error::AuthFailed,
            403 => Error::Forbidden(context),
            404 => Error::NotFound(context),
            409 => Error::Conflict(context),
            422 => Error::Unprocessable(context),
            501 => Error::NotSupported(context),
            other => Error::Http(other, context),
        }
    }

    /// True when the camera answered fine but doesn't implement this endpoint —
    /// callers should hide the feature, not report a failure.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Error::NotSupported(_))
    }
}

pub struct Camera {
    http: reqwest::Client,
    /// `https://host` or `http://host`, no trailing slash.
    origin: String,
    auth: Option<(String, String)>,
    seen_fingerprint: Arc<Mutex<Option<String>>>,
}

impl Camera {
    /// `host` may be `cam.local`, `10.0.0.5`, `cam.local:8080`, or an explicit
    /// `http://cam.local` to skip TLS entirely.
    pub fn connect(host: &str, auth: Option<(&str, &str)>, trust: Trust) -> Result<Self> {
        let (origin, plain_http) = match host.strip_prefix("http://") {
            Some(rest) => (format!("http://{}", rest.trim_end_matches('/')), true),
            None => {
                let bare = host
                    .strip_prefix("https://")
                    .unwrap_or(host)
                    .trim_end_matches('/');
                (format!("https://{bare}"), false)
            }
        };

        let mut builder = reqwest::Client::builder();
        let seen_fingerprint = if plain_http {
            Arc::new(Mutex::new(None))
        } else {
            let (config, seen) = tls::client_config(trust);
            builder = builder.use_preconfigured_tls(config);
            seen
        };

        let http = builder
            .build()
            .map_err(|e| Error::Unreachable(e.to_string()))?;

        Ok(Self {
            http,
            origin,
            auth: auth.map(|(u, p)| (u.to_string(), p.to_string())),
            seen_fingerprint,
        })
    }

    /// The fingerprint the camera presented on the last connection attempt —
    /// what you show the operator to accept. `None` before any request, or on plain HTTP.
    pub fn presented_fingerprint(&self) -> Option<String> {
        self.seen_fingerprint
            .lock()
            .expect("fingerprint lock")
            .clone()
    }

    pub fn websocket_url(&self) -> String {
        format!(
            "{}{API}/event/websocket",
            self.origin.replacen("http", "ws", 1)
        )
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let req = self
            .http
            .request(method, format!("{}{API}{path}", self.origin));
        match &self.auth {
            Some((u, p)) => req.basic_auth(u, Some(p)),
            None => req,
        }
    }

    async fn send(&self, req: reqwest::RequestBuilder, path: &str) -> Result<reqwest::Response> {
        let res = req.send().await.map_err(|e| self.transport_error(e))?;
        let status = res.status().as_u16();
        if status >= 400 {
            let body = res.text().await.unwrap_or_default();
            return Err(Error::from_status(status, path, &body));
        }
        Ok(res)
    }

    /// A failed TLS handshake is reported as `CertUntrusted` with the fingerprint the
    /// camera presented, so the operator has something to accept.
    fn transport_error(&self, e: reqwest::Error) -> Error {
        let chain = format!("{e:?}");
        if (chain.contains("fingerprint mismatch") || chain.contains("certificate"))
            && let Some(fp) = self.presented_fingerprint()
        {
            return Error::CertUntrusted(fp);
        }
        Error::Unreachable(e.to_string())
    }

    // ── Generic REST — the whole API surface ────────────────────────────────

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let res = self
            .send(self.request(reqwest::Method::GET, path), path)
            .await?;
        res.json().await.map_err(|e| Error::Body(e.to_string()))
    }

    /// Most setters answer `204 No Content`.
    pub async fn put<B: Serialize + ?Sized>(&self, path: &str, body: &B) -> Result<()> {
        self.send(self.request(reqwest::Method::PUT, path).json(body), path)
            .await
            .map(drop)
    }

    pub async fn post<B: Serialize + ?Sized>(&self, path: &str, body: &B) -> Result<()> {
        self.send(self.request(reqwest::Method::POST, path).json(body), path)
            .await
            .map(drop)
    }

    /// Several action endpoints take no parameters at all.
    pub async fn put_empty(&self, path: &str) -> Result<()> {
        self.send(self.request(reqwest::Method::PUT, path), path)
            .await
            .map(drop)
    }

    pub async fn post_empty(&self, path: &str) -> Result<()> {
        self.send(self.request(reqwest::Method::POST, path), path)
            .await
            .map(drop)
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        self.send(self.request(reqwest::Method::DELETE, path), path)
            .await
            .map(drop)
    }

    /// For endpoints that answer with something other than JSON — custom platform
    /// files come back as Blackmagic streaming XML.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        let res = self
            .send(self.request(reqwest::Method::GET, path), path)
            .await?;
        res.text().await.map_err(|e| Error::Body(e.to_string()))
    }

    /// Raw bytes in, for the file-upload endpoints (custom livestream platforms, LUTs, presets).
    pub async fn put_bytes(&self, path: &str, body: Vec<u8>) -> Result<()> {
        self.send(self.request(reqwest::Method::PUT, path).body(body), path)
            .await
            .map(drop)
    }

    // ── Identity ───────────────────────────────────────────────────────────

    /// Also the reachability/auth check: this is what confirms a host is really a camera.
    pub async fn product(&self) -> Result<Product> {
        self.get("/system/product").await
    }

    // ── Transport / record ─────────────────────────────────────────────────

    pub async fn transport_mode(&self) -> Result<TransportMode> {
        self.get("/transports/0").await
    }

    pub async fn set_transport_mode(&self, mode: &str) -> Result<()> {
        self.put("/transports/0", &TransportMode { mode: mode.into() })
            .await
    }

    pub async fn record_state(&self) -> Result<RecordState> {
        self.get("/transports/0/record").await
    }

    /// `POST`, not `PUT` — the manual marks `PUT /transports/0/record` deprecated.
    pub async fn start_recording(&self, clip_name: Option<&str>) -> Result<()> {
        match clip_name {
            Some(name) => {
                self.post(
                    "/transports/0/record",
                    &serde_json::json!({ "clipName": name }),
                )
                .await
            }
            None => self.post_empty("/transports/0/record").await,
        }
    }

    /// Recording stops via the transport's stop action; there is no "record false".
    pub async fn stop_recording(&self) -> Result<()> {
        self.post_empty("/transports/0/stop").await
    }

    // ── Exposure ───────────────────────────────────────────────────────────

    pub async fn iso(&self) -> Result<Iso> {
        self.get("/video/iso").await
    }

    pub async fn set_iso(&self, iso: i32) -> Result<()> {
        self.put("/video/iso", &Iso { iso }).await
    }

    pub async fn white_balance(&self) -> Result<WhiteBalance> {
        self.get("/video/whiteBalance").await
    }

    pub async fn set_white_balance(&self, kelvin: i32) -> Result<()> {
        self.put(
            "/video/whiteBalance",
            &WhiteBalance {
                white_balance: kelvin,
            },
        )
        .await
    }

    pub async fn shutter(&self) -> Result<Shutter> {
        self.get("/video/shutter").await
    }

    pub async fn set_shutter_speed(&self, speed: i32) -> Result<()> {
        self.put(
            "/video/shutter",
            &Shutter {
                shutter_speed: Some(speed),
                shutter_angle: None,
            },
        )
        .await
    }

    pub async fn set_auto_exposure(&self, mode: &str) -> Result<()> {
        self.put("/video/autoExposure", &AutoExposure { mode: mode.into() })
            .await
    }

    // ── Lens ───────────────────────────────────────────────────────────────

    /// Normalised 0.0–1.0, per the API's `/lens/iris` `normalised` field.
    pub async fn iris(&self) -> Result<Iris> {
        self.get("/lens/iris").await
    }

    pub async fn set_iris(&self, normalised: f64) -> Result<()> {
        self.put(
            "/lens/iris",
            &Iris {
                normalised: Some(normalised),
                aperture_stop: None,
            },
        )
        .await
    }

    pub async fn autofocus(&self) -> Result<()> {
        self.put("/lens/focus/doAutoFocus", &serde_json::json!({}))
            .await
    }

    // ── Livestream ─────────────────────────────────────────────────────────
    //
    // Paths verified against the Cinema Camera 6K manual (Livestream Control API).
    // Note the plural, indexed `/livestreams/0` — and that start/stop are PUT.

    pub async fn livestream_status(&self) -> Result<LivestreamStatus> {
        self.get("/livestreams/0").await
    }

    /// Whether livestreaming can start right now, and if not, why.
    pub async fn livestream_available(&self) -> Result<LivestreamAvailability> {
        self.get("/livestreams/0/available").await
    }

    pub async fn livestream_start(&self) -> Result<()> {
        self.put_empty("/livestreams/0/start").await
    }

    pub async fn livestream_stop(&self) -> Result<()> {
        self.put_empty("/livestreams/0/stop").await
    }

    pub async fn livestream_active_platform(&self) -> Result<LivestreamPlatform> {
        self.get("/livestreams/0/activePlatform").await
    }

    pub async fn set_livestream_active_platform(
        &self,
        platform: &LivestreamPlatform,
    ) -> Result<()> {
        self.put("/livestreams/0/activePlatform", platform).await
    }

    /// Names of every platform the camera knows about (YouTube, Twitch, …).
    pub async fn livestream_platforms(&self) -> Result<Vec<String>> {
        self.get("/livestreams/platforms").await
    }

    /// Servers, quality profiles and credentials for one platform. The full response is
    /// larger than [`PlatformConfig`]; use `get` with your own type if you need the rest.
    pub async fn livestream_platform_config(&self, name: &str) -> Result<PlatformConfig> {
        self.get(&format!("/livestreams/platforms/{name}")).await
    }

    /// Point the stream at an arbitrary RTMP/SRT destination — e.g. a receiver on the
    /// LAN, to get a live preview feed off the camera without a streaming platform.
    ///
    /// Finds a platform whose URL the camera allows overriding and selects it with
    /// `server: "Custom"`. Sets the destination only; call [`Camera::livestream_start`]
    /// to actually go live.
    pub async fn stream_to(&self, url: &str, quality: Option<&str>) -> Result<LivestreamPlatform> {
        for name in self.livestream_platforms().await? {
            let config = self.livestream_platform_config(&name).await?;
            if !config.customizable_url_enabled {
                continue;
            }
            let quality = quality
                .map(str::to_string)
                .or(config.default_profile)
                .or_else(|| config.profiles.first().map(|p| p.profile.clone()))
                .ok_or_else(|| {
                    Error::Unexpected(format!("platform {name} lists no quality profiles"))
                })?;

            let platform = LivestreamPlatform {
                platform: config.platform,
                server: "Custom".to_string(),
                key: None,
                passphrase: None,
                quality,
                url: Some(url.to_string()),
            };
            self.set_livestream_active_platform(&platform).await?;
            return Ok(platform);
        }
        Err(Error::Unexpected(
            "no streaming platform on this camera accepts a custom URL".into(),
        ))
    }

    pub async fn livestream_custom_platforms(&self) -> Result<Vec<String>> {
        self.get("/livestreams/customPlatforms").await
    }

    /// Custom platform files are Blackmagic streaming **XML**, not JSON.
    pub async fn upload_custom_platform(&self, filename: &str, xml: Vec<u8>) -> Result<()> {
        self.put_bytes(&format!("/livestreams/customPlatforms/{filename}"), xml)
            .await
    }

    pub async fn delete_custom_platform(&self, filename: &str) -> Result<()> {
        self.delete(&format!("/livestreams/customPlatforms/{filename}"))
            .await
    }

    pub async fn delete_all_custom_platforms(&self) -> Result<()> {
        self.delete("/livestreams/customPlatforms").await
    }
}

// ── DTOs for the named methods above ───────────────────────────────────────
// Everything else in the API goes through `get`/`put` with the caller's own type.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub device_name: String,
    pub product_name: String,
    pub software_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportMode {
    /// `InputPreview` | `InputRecord` | `Output`
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordState {
    pub recording: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Iso {
    pub iso: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhiteBalance {
    pub white_balance: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shutter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutter_speed: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutter_angle: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoExposure {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Iris {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalised: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aperture_stop: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivestreamStatus {
    /// `Idle` | `Connecting` | `Streaming` | `Flushing` | `Interrupted`
    pub status: String,
    /// Current bitrate in bps.
    pub bitrate: i64,
    pub effective_video_format: String,
    /// Absent while idle.
    #[serde(default)]
    pub duration: Option<i64>,
    /// Stream cache usage percentage.
    #[serde(default)]
    pub cache: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivestreamAvailability {
    pub available: bool,
    /// `not-supported` | `unsupported-format` | `in-playback` |
    /// `pending-format-transition` | `unexpected-reason`. Empty when available.
    #[serde(default)]
    pub reasons: Vec<String>,
}

/// The part of `GET /livestreams/platforms/{name}` needed to build an active platform.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformConfig {
    pub platform: String,
    /// True when this platform's destination URL can be overridden — what makes
    /// streaming to your own LAN receiver possible.
    #[serde(default)]
    pub customizable_url_enabled: bool,
    #[serde(default)]
    pub servers: Vec<PlatformServer>,
    #[serde(default)]
    pub profiles: Vec<PlatformProfile>,
    #[serde(default)]
    pub default_profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformServer {
    pub server: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformProfile {
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivestreamPlatform {
    pub platform: String,
    /// The platform's server name, or `Custom` when the URL is customizable.
    pub server: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// SRT streams only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    pub quality: String,
    /// Only present when the server URL is customizable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_map_to_distinct_errors() {
        assert!(Error::from_status(501, "/lens/iris", "").is_unsupported());
        assert!(matches!(
            Error::from_status(409, "/x", ""),
            Error::Conflict(_)
        ));
        assert!(matches!(
            Error::from_status(403, "/x", ""),
            Error::Forbidden(_)
        ));
        assert!(matches!(
            Error::from_status(401, "/x", ""),
            Error::AuthFailed
        ));
        assert!(matches!(
            Error::from_status(418, "/x", ""),
            Error::Http(418, _)
        ));
        // A real failure must never be mistaken for "camera lacks the feature".
        assert!(!Error::from_status(500, "/x", "").is_unsupported());
    }

    #[test]
    fn a_response_body_is_folded_into_the_message_instead_of_discarded() {
        let with_body = Error::from_status(
            403,
            "/livestreams/0/stop",
            r#"{"error":"stream owned by Blackmagic Cloud"}"#,
        );
        assert!(
            with_body
                .to_string()
                .contains("stream owned by Blackmagic Cloud")
        );

        // A blank or whitespace-only body falls back to the path alone.
        let no_body = Error::from_status(403, "/livestreams/0/stop", "   ");
        assert_eq!(
            no_body.to_string(),
            "forbidden (likely controlled by another setting): /livestreams/0/stop"
        );
    }

    #[test]
    fn origin_and_websocket_url_follow_the_scheme() {
        let tls = Camera::connect("cam.local", None, Trust::OnFirstUse).unwrap();
        assert_eq!(tls.origin, "https://cam.local");
        assert_eq!(
            tls.websocket_url(),
            "wss://cam.local/control/api/v1/event/websocket"
        );

        let plain = Camera::connect("http://10.0.0.5:8080/", None, Trust::OnFirstUse).unwrap();
        assert_eq!(plain.origin, "http://10.0.0.5:8080");
        assert_eq!(
            plain.websocket_url(),
            "ws://10.0.0.5:8080/control/api/v1/event/websocket"
        );
    }
}
