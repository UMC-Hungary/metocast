//! Going live and ending an event's YouTube broadcast or Facebook live video from Metocast, and
//! keeping their state in `event_connections.schedule_status` fresh while a service is near.
//! Every change reaches the clients as `event.changed`.

use std::time::Duration;

use serde::Deserialize;
use uuid::Uuid;

use crate::connectors::{facebook, youtube};
use crate::models::event::fetch_event;
use crate::server::AppState;
use crate::server::websocket::broadcast_event_changed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Youtube,
    Facebook,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Youtube => "youtube",
            Self::Facebook => "facebook",
        }
    }
}

/// Goes live (`start`) or ends the broadcast. Errors are short codes the clients translate:
/// `not_found`, `not_scheduled`, `not_authenticated`, `already_live`, `not_live`,
/// `broadcast_ended`, `encoder_not_streaming`, or `youtube_api_…` / `facebook_api_…` with
/// the platform's message.
pub async fn control(
    state: &AppState,
    event_id: Uuid,
    platform: Platform,
    start: bool,
) -> Result<(), String> {
    let event = fetch_event(event_id, &state.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("not_found")?;
    let conn = event.connection(platform.as_str());
    let before = conn
        .map_or("not_scheduled", |c| c.schedule_status.as_str())
        .to_string();
    let id = match platform {
        Platform::Youtube => conn.and_then(|c| c.external_id.clone()),
        Platform::Facebook => conn
            .and_then(|c| c.extra.as_ref())
            .and_then(|extra| extra["stream_id"].as_str())
            .map(str::to_string),
    }
    .ok_or("not_scheduled")?;
    match (start, before.as_str()) {
        (true, "live") => return Err("already_live".into()),
        (true, "ended") => return Err("broadcast_ended".into()),
        (false, status) if !matches!(status, "starting" | "live") => {
            return Err("not_live".into());
        }
        _ => {}
    }
    let token = match platform {
        Platform::Youtube => youtube::load_tokens(&state.pool)
            .await
            .map(|t| t.access_token),
        Platform::Facebook => facebook::load_tokens(&state.pool)
            .await
            .map(|t| t.access_token),
    }
    .ok_or("not_authenticated")?;

    if start {
        write_status(state, event_id, platform, "starting").await;
    }
    let result = match (platform, start) {
        (Platform::Youtube, true) => youtube::go_live(&id, &token).await.map(|()| "live"),
        (Platform::Youtube, false) => youtube::end_broadcast(&id, &token).await.map(|()| "ended"),
        (Platform::Facebook, true) => match facebook::go_live(&id, &token).await {
            // Facebook shows it once video arrives; until then it stays `starting`.
            Ok(()) => Ok(match facebook::live_video_status(&id, &token).await {
                Ok(Some("live")) => "live",
                Ok(_) => "starting",
                Err(error) => {
                    tracing::warn!(%error, "facebook: cannot read the live video after going live");
                    "starting"
                }
            }),
            Err(e) => Err(e),
        },
        (Platform::Facebook, false) => facebook::end_live_video(&id, &token)
            .await
            .map(|()| "ended"),
    };
    match result {
        Ok(status) => {
            write_status(state, event_id, platform, status).await;
            Ok(())
        }
        Err(error) => {
            if start {
                write_status(state, event_id, platform, &before).await;
            }
            Err(error.to_string())
        }
    }
}

/// Stores a connection's state and tells every client, when it changed.
async fn write_status(state: &AppState, event_id: Uuid, platform: Platform, status: &str) {
    let result = async {
        let mut tx = state.pool.begin().await?;
        // Not an edit of the event: don't re-run the YouTube auto-scheduler.
        sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
            .execute(&mut *tx)
            .await?;
        let changed = sqlx::query(
            "UPDATE event_connections SET schedule_status = $3, updated_at = NOW() \
             WHERE event_id = $1 AND platform = $2 AND schedule_status <> $3",
        )
        .bind(event_id)
        .bind(platform.as_str())
        .bind(status)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed > 0 {
            sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
                .bind(event_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        anyhow::Ok(changed > 0)
    }
    .await;
    match result {
        Ok(false) => {}
        Ok(true) => match fetch_event(event_id, &state.pool).await {
            Ok(Some(event)) => broadcast_event_changed(state, "UPDATE", &event).await,
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "broadcast status: cannot reread the event"),
        },
        Err(error) => tracing::warn!(%error, %event_id, status, "broadcast status: cannot store"),
    }
}

#[derive(sqlx::FromRow)]
struct Watched {
    event_id: Uuid,
    platform: String,
    id: String,
}

/// Every minute, re-reads the platform state of broadcasts that may change soon: scheduled,
/// starting or live, on events within 12 hours of now. With none (any day without a service)
/// it calls nothing. One YouTube call covers up to 50 broadcasts (1 quota unit).
pub fn spawn_watcher(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(error) = refresh(&state).await {
                tracing::warn!(%error, "broadcast status refresh failed");
            }
        }
    });
}

async fn refresh(state: &AppState) -> anyhow::Result<()> {
    let watched = sqlx::query_as::<_, Watched>(
        "SELECT c.event_id, c.platform, \
                CASE c.platform WHEN 'facebook' THEN c.extra->>'stream_id' ELSE c.external_id END AS id \
         FROM event_connections c JOIN events e ON e.id = c.event_id \
         WHERE c.schedule_status IN ('scheduled', 'starting', 'live') \
           AND e.date_time BETWEEN NOW() - INTERVAL '12 hours' AND NOW() + INTERVAL '12 hours' \
           AND (CASE c.platform WHEN 'facebook' THEN c.extra->>'stream_id' ELSE c.external_id END) IS NOT NULL",
    )
    .fetch_all(&state.pool)
    .await?;

    let youtube_rows: Vec<&Watched> = watched
        .iter()
        .filter(|w| w.platform == "youtube")
        .take(50)
        .collect();
    if !youtube_rows.is_empty()
        && let Some(token) = youtube::load_tokens(&state.pool).await
    {
        let ids: Vec<String> = youtube_rows.iter().map(|w| w.id.clone()).collect();
        let statuses = youtube::broadcast_statuses(&ids, &token.access_token).await?;
        for row in youtube_rows {
            if let Some(status) = statuses.get(&row.id) {
                write_status(state, row.event_id, Platform::Youtube, status).await;
            }
        }
    }

    let facebook_rows = watched.iter().filter(|w| w.platform == "facebook");
    if let Some(token) = facebook::load_tokens(&state.pool).await {
        for row in facebook_rows {
            match facebook::live_video_status(&row.id, &token.access_token).await {
                Ok(Some(status)) => {
                    write_status(state, row.event_id, Platform::Facebook, status).await
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, id = row.id, "facebook live video status"),
            }
        }
    }
    Ok(())
}
