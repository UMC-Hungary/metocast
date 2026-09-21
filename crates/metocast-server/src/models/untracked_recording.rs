use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

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
