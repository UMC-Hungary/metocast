use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ── Upload row (maps to recording_uploads table) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecordingUpload {
    pub recording_id: Uuid,
    pub platform: String,
    pub state: String,
    pub progress_bytes: i64,
    pub total_bytes: i64,
    pub visibility: String,
    pub video_id: Option<String>,
    pub video_url: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

// ── Main recording row ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct Recording {
    pub id: Uuid,
    pub event_id: Uuid,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub duration_seconds: f64,
    pub media_kind: String,
    pub source: String,
    pub metadata: serde_json::Value,
    pub detected_at: DateTime<Utc>,
    pub whitelisted: bool,
    pub uploaded: bool,
    pub uploaded_at: Option<DateTime<Utc>>,
    pub video_id: Option<String>,
    pub video_url: Option<String>,
    pub custom_title: Option<String>,
    pub uploadable: bool,
    pub custom_description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Populated manually after a JOIN — not a DB column on recordings.
    #[sqlx(skip)]
    #[serde(default)]
    pub uploads: Vec<RecordingUpload>,
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Received from frontend — stays snake_case to match JSON body
#[derive(Debug, Deserialize)]
pub struct CreateRecording {
    pub file_path: String,
    pub file_name: String,
    pub file_size: Option<i64>,
    pub duration_seconds: Option<f64>,
    pub custom_title: Option<String>,
    pub custom_description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FlagUploadRequest {
    pub recordings: Vec<FlagUploadItem>,
}

#[derive(Debug, Deserialize)]
pub struct FlagUploadItem {
    pub recording_id: Uuid,
    pub custom_title: Option<String>,
    pub custom_description: Option<String>,
    pub youtube_visibility: Option<String>,
    pub facebook_visibility: Option<String>,
    pub platforms: Vec<String>,
}
