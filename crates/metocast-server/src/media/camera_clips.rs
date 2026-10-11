//! A log of the clips the Blackmagic camera records, timed by the server's clock: the anchor
//! that lines a camera clip up with the RØDECaster recordings later, since the camera's own
//! clock can be days off.
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::models::event::find_current_event;
use crate::server::AppState;

/// The first string among `keys` in a camera REST reply.
fn first_text(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// Clip name and timecode from the camera, if it says. Both are best effort: the REST
/// reference doesn't promise either for every firmware, and the log is useful without them.
async fn clip_details(state: &AppState) -> (Option<String>, Option<String>) {
    let Some(camera) = state.blackmagic_camera_connector.client().await else {
        return (None, None);
    };
    let record = camera
        .get::<Value>("/transports/0/record")
        .await
        .unwrap_or(Value::Null);
    let timecode = camera
        .get::<Value>("/transports/0/timecode")
        .await
        .unwrap_or(Value::Null);
    (
        first_text(&record, &["clipName", "clip"]),
        first_text(&timecode, &["timecode", "display"]),
    )
}

async fn clip_started(state: &AppState) -> anyhow::Result<Uuid> {
    let started_at = Utc::now();
    let event_id = find_current_event(&state.pool).await?.map(|event| event.id);
    let (clip_name, start_timecode) = clip_details(state).await;
    let id = sqlx::query_scalar(
        "INSERT INTO camera_clips (event_id, clip_name, start_timecode, started_at) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(event_id)
    .bind(clip_name)
    .bind(start_timecode)
    .bind(started_at)
    .fetch_one(&state.pool)
    .await?;
    Ok(id)
}

/// Watches the camera's record state for as long as the server runs.
pub fn spawn_camera_clip_log(state: AppState) {
    let mut changes = state.blackmagic_camera_connector.state_tx.subscribe();
    tokio::spawn(async move {
        let mut open: Option<Uuid> = None;
        loop {
            let recording = match changes.recv().await {
                Ok(camera) => camera.is_recording,
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            };
            match (recording, open) {
                (true, None) => match clip_started(&state).await {
                    Ok(id) => open = Some(id),
                    Err(e) => tracing::error!("camera clip log: {e}"),
                },
                (false, Some(id)) => {
                    open = None;
                    if let Err(e) =
                        sqlx::query("UPDATE camera_clips SET stopped_at = now() WHERE id = $1")
                            .bind(id)
                            .execute(&state.pool)
                            .await
                    {
                        tracing::error!("camera clip log: {e}");
                    }
                }
                _ => {}
            }
        }
    });
}

/// When Metocast saw the camera start a clip: matched by clip name, or else by its start
/// timecode to the second (the REST read and the first frame can differ by a frame or two).
pub async fn started_at(
    pool: &PgPool,
    clip_name: Option<&str>,
    start_timecode: Option<&str>,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    if clip_name.is_none() && start_timecode.is_none() {
        return Ok(None);
    }
    sqlx::query_scalar(
        "SELECT started_at FROM camera_clips \
         WHERE clip_name = $1 OR left(start_timecode, 8) = left($2, 8) \
         ORDER BY (clip_name = $1) DESC NULLS LAST, started_at DESC LIMIT 1",
    )
    .bind(clip_name)
    .bind(start_timecode)
    .fetch_optional(pool)
    .await
}
