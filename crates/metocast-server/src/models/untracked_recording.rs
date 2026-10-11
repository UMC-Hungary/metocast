use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::recording::Recording;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedRecording {
    pub id: Uuid,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub duration_seconds: f64,
    pub detected_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub async fn list_untracked(pool: &PgPool) -> anyhow::Result<Vec<UntrackedRecording>> {
    let recordings = sqlx::query_as::<_, UntrackedRecording>(
        "SELECT * FROM untracked_recordings ORDER BY detected_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(recordings)
}

/// Files an untracked recording under an event. Its provenance travels with it: a render's
/// timeline facts, and the `source` and `mediaKind` a RØDECaster audio recording started with.
///
/// # Errors
/// `NOT_FOUND` or `EVENT_NOT_FOUND` as the message, or the database's own error.
pub async fn assign(pool: &PgPool, id: Uuid, event_id: Uuid) -> anyhow::Result<Recording> {
    let untracked =
        sqlx::query_as::<_, UntrackedRecording>("SELECT * FROM untracked_recordings WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("NOT_FOUND"))?;
    let event_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
            .bind(event_id)
            .fetch_one(pool)
            .await?;
    if !event_exists {
        return Err(anyhow::anyhow!("EVENT_NOT_FOUND"));
    }
    let metadata: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM untracked_recordings WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await?;

    let mut tx = pool.begin().await?;
    let recording = sqlx::query_as::<_, Recording>(
        r"INSERT INTO recordings (event_id, file_path, file_name, file_size, duration_seconds,
                                  detected_at, media_kind, source, metadata)
          VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7->>'mediaKind', 'video'),
                  COALESCE($7->>'source', 'obs'), $7) RETURNING *",
    )
    .bind(event_id)
    .bind(&untracked.file_path)
    .bind(&untracked.file_name)
    .bind(untracked.file_size)
    .bind(untracked.duration_seconds)
    .bind(untracked.detected_at)
    .bind(&metadata)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM untracked_recordings WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(recording)
}
