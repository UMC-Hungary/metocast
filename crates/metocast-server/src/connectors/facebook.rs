use std::sync::Arc;

use crate::HostHandle;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::{Mutex, RwLock, broadcast, watch};
use tokio::time::Duration;

use super::{ConnectorConfig, ConnectorStatus, FacebookConfig};

// ── Token types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StoredToken {
    pub access_token: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct LongLivedTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

// ── API response types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacebookScheduleResult {
    pub event_id: String,
    pub stream_id: String,
    pub event_url: String,
}

#[derive(Debug, Deserialize)]
struct FbCreateResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct FbLiveVideoResponse {
    id: String,
}

// ── Connector ────────────────────────────────────────────────────────────────

pub struct FacebookConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    stop_tx: Mutex<Option<watch::Sender<bool>>>,
    host: Mutex<Option<HostHandle>>,
}

impl FacebookConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        Self {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            status_tx,
            stop_tx: Mutex::new(None),
            host: Mutex::new(None),
        }
    }

    pub async fn start(&self, pool: PgPool, config: FacebookConfig, host: Option<HostHandle>) {
        *self.host.lock().await = host.clone();
        self.stop_internal().await;

        let (stop_tx, stop_rx) = watch::channel(false);
        *self.stop_tx.lock().await = Some(stop_tx);

        let status = Arc::clone(&self.status);
        let status_tx = self.status_tx.clone();
        tokio::spawn(async move {
            run_token_loop(pool, config, status, status_tx, stop_rx, host).await;
        });
    }

    pub async fn stop(&self) {
        self.stop_internal().await;
        let guard = self.host.lock().await;
        set_status(
            &self.status,
            &self.status_tx,
            guard.as_ref(),
            ConnectorStatus::Disconnected,
        )
        .await;
    }

    async fn stop_internal(&self) {
        let mut guard = self.stop_tx.lock().await;
        if let Some(tx) = guard.take() {
            let _ = tx.send(true);
        }
    }

    pub async fn get_status(&self) -> ConnectorStatus {
        self.status.read().await.clone()
    }
}

impl Default for FacebookConnector {
    fn default() -> Self {
        Self::new()
    }
}

// ── Status helpers ────────────────────────────────────────────────────────────

async fn set_status(
    status: &Arc<RwLock<ConnectorStatus>>,
    status_tx: &broadcast::Sender<ConnectorStatus>,
    host: Option<&HostHandle>,
    new_status: ConnectorStatus,
) {
    tracing::info!(connector = "facebook", status = ?new_status, "connector status");
    *status.write().await = new_status;
    let current = status.read().await.clone();
    let _ = status_tx.send(current.clone());
    if let Some(host) = host
        && let Err(e) = crate::emit(host, "connector://facebook-status", &current)
    {
        tracing::warn!("Failed to emit Facebook status: {e}");
    }
}

// ── Token management ──────────────────────────────────────────────────────────

pub async fn load_tokens(pool: &PgPool) -> Option<StoredToken> {
    #[derive(sqlx::FromRow)]
    struct Row {
        access_token: String,
        expires_at: Option<DateTime<Utc>>,
    }

    let row = sqlx::query_as::<_, Row>(
        "SELECT access_token, expires_at FROM connector_tokens WHERE connector = 'facebook'",
    )
    .fetch_optional(pool)
    .await
    .ok()??;

    Some(StoredToken {
        access_token: row.access_token,
        expires_at: row.expires_at,
    })
}

pub async fn save_tokens(pool: &PgPool, token: &StoredToken) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO connector_tokens (connector, access_token, expires_at, updated_at)
        VALUES ('facebook', $1, $2, NOW())
        ON CONFLICT (connector) DO UPDATE SET
            access_token = EXCLUDED.access_token,
            expires_at   = EXCLUDED.expires_at,
            updated_at   = NOW()
        "#,
    )
    .bind(&token.access_token)
    .bind(token.expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_tokens(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM connector_tokens WHERE connector = 'facebook'")
        .execute(pool)
        .await?;
    Ok(())
}

/// Exchange a short-lived token for a long-lived one (60-day Facebook token).
pub async fn exchange_long_lived_token(
    pool: &PgPool,
    config: &FacebookConfig,
    short_lived_token: &str,
) -> anyhow::Result<StoredToken> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://graph.facebook.com/oauth/access_token")
        .query(&[
            ("grant_type", "fb_exchange_token"),
            ("client_id", config.app_id.as_str()),
            ("client_secret", config.app_secret.as_str()),
            ("fb_exchange_token", short_lived_token),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<LongLivedTokenResponse>()
        .await?;

    let expires_at = resp
        .expires_in
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));

    let token = StoredToken {
        access_token: resp.access_token,
        expires_at,
    };
    save_tokens(pool, &token).await?;
    Ok(token)
}

/// Exchange an OAuth code for a short-lived token, then upgrade to long-lived.
pub async fn exchange_code(
    pool: &PgPool,
    config: &FacebookConfig,
    code: &str,
    redirect_uri: &str,
) -> anyhow::Result<StoredToken> {
    let client = reqwest::Client::new();

    // Step 1: exchange code for short-lived token
    let resp = client
        .get("https://graph.facebook.com/v19.0/oauth/access_token")
        .query(&[
            ("client_id", config.app_id.as_str()),
            ("redirect_uri", redirect_uri),
            ("client_secret", config.app_secret.as_str()),
            ("code", code),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<LongLivedTokenResponse>()
        .await?;

    // Step 2: upgrade to long-lived token
    exchange_long_lived_token(pool, config, &resp.access_token).await
}

// ── Background token-refresh loop ─────────────────────────────────────────────

async fn run_token_loop(
    pool: PgPool,
    config: FacebookConfig,
    status: Arc<RwLock<ConnectorStatus>>,
    status_tx: broadcast::Sender<ConnectorStatus>,
    mut stop_rx: watch::Receiver<bool>,
    host: Option<HostHandle>,
) {
    // An enabled connector that cannot work is an error, not a quiet absence: the
    // UI only raises a notification on `Error`, and the operator has to be told.
    if !config.is_configured() {
        set_status(
            &status,
            &status_tx,
            host.as_ref(),
            ConnectorStatus::Error {
                message: "credentials_required".to_string(),
            },
        )
        .await;
        return;
    }

    let token = match load_tokens(&pool).await {
        Some(t) => t,
        None => {
            set_status(
                &status,
                &status_tx,
                host.as_ref(),
                ConnectorStatus::Error {
                    message: "login_required".to_string(),
                },
            )
            .await;
            return;
        }
    };

    set_status(
        &status,
        &status_tx,
        host.as_ref(),
        ConnectorStatus::Connected,
    )
    .await;

    loop {
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(300)) => {}
            result = stop_rx.changed() => {
                let _ = result;
                set_status(&status, &status_tx, host.as_ref(), ConnectorStatus::Disconnected).await;
                return;
            }
        }

        if *stop_rx.borrow() {
            set_status(
                &status,
                &status_tx,
                host.as_ref(),
                ConnectorStatus::Disconnected,
            )
            .await;
            return;
        }

        // Facebook long-lived tokens last ~60 days; warn when < 10 days remain
        let needs_renewal = token
            .expires_at
            .is_some_and(|exp| exp - Utc::now() < chrono::Duration::days(10));

        if needs_renewal {
            tracing::warn!("Facebook token expiring soon; prompting re-login");
            set_status(
                &status,
                &status_tx,
                host.as_ref(),
                ConnectorStatus::Error {
                    message: "relogin_required".to_string(),
                },
            )
            .await;
            return;
        }
    }
}

// ── Event scheduling ──────────────────────────────────────────────────────────

/// Create a Facebook Page Event and an associated Live Video stream.
/// Returns `(event_id, stream_id, event_url)` on success.
pub async fn schedule_event(
    event_title: &str,
    event_time: &DateTime<Utc>,
    access_token: &str,
    page_id: &str,
    privacy_status: &str,
) -> anyhow::Result<FacebookScheduleResult> {
    let client = reqwest::Client::new();
    let start_time = event_time.timestamp();

    // Create the Facebook Page Event
    let event_resp = client
        .post(format!("https://graph.facebook.com/v19.0/{page_id}/events"))
        .query(&[("access_token", access_token)])
        .json(&serde_json::json!({
            "name": event_title,
            "start_time": start_time,
            "privacy": privacy_status
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<FbCreateResponse>()
        .await?;

    let event_id = event_resp.id;
    let event_url = format!("https://www.facebook.com/events/{event_id}");

    // Create a Live Video linked to the page
    let live_resp = client
        .post(format!(
            "https://graph.facebook.com/v19.0/{page_id}/live_videos"
        ))
        .query(&[("access_token", access_token)])
        .json(&serde_json::json!({
            "title": event_title,
            "status": "SCHEDULED_UNPUBLISHED",
            "planned_start_time": start_time,
            "privacy": { "value": privacy_status }
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<FbLiveVideoResponse>()
        .await?;

    Ok(FacebookScheduleResult {
        event_id,
        stream_id: live_resp.id,
        event_url,
    })
}
