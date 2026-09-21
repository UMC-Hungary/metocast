use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EventActivity {
    pub id: Uuid,
    pub event_id: Uuid,
    pub activity_type: String,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEventActivity {
    pub activity_type: String,
    pub message: Option<String>,
}

pub async fn list_activities(event_id: Uuid, pool: &PgPool) -> anyhow::Result<Vec<EventActivity>> {
    let activities = sqlx::query_as::<_, EventActivity>(
        "SELECT * FROM event_activities WHERE event_id = $1 ORDER BY created_at ASC",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;
    Ok(activities)
}
