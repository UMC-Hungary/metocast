//! Read models for an event's recorded files.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How one platform's upload of a recording is going.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingUpload {
    pub platform: String,
    pub state: String,
    #[serde(default)]
    pub progress_bytes: i64,
    #[serde(default)]
    pub total_bytes: i64,
    pub video_url: Option<String>,
    pub error: Option<String>,
}

/// One recorded file, from `GET /api/events/{id}/recordings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub id: Uuid,
    pub file_name: String,
    #[serde(default)]
    pub file_size: i64,
    #[serde(default)]
    pub duration_seconds: f64,
    #[serde(default)]
    pub media_kind: String,
    #[serde(default)]
    pub source: String,
    pub detected_at: DateTime<Utc>,
    /// Flagged to be uploaded.
    #[serde(default)]
    pub uploadable: bool,
    #[serde(default)]
    pub uploaded: bool,
    pub video_url: Option<String>,
    #[serde(default)]
    pub uploads: Vec<RecordingUpload>,
}

/// `POST /api/events/{id}/recordings/flag-upload` body.
#[derive(Debug, Clone, Serialize)]
pub struct FlagUpload {
    pub recordings: Vec<FlagUploadItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct FlagUploadItem {
    pub recording_id: Uuid,
    pub platforms: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recordings_decode_the_server_shape() {
        // Shape from `models::recording::Recording`, which serializes camelCase.
        let json = r#"[{"id":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","eventId":"0d9c4b7a-1e2f-4a3b-8c5d-6e7f8a9b0c1d",
            "filePath":"/Movies/service.mkv","fileName":"service.mkv","fileSize":10485760,
            "durationSeconds":3600.5,"mediaKind":"video","source":"obs","metadata":{},
            "detectedAt":"2026-09-20T08:00:00Z","whitelisted":false,"uploaded":false,"uploadedAt":null,
            "videoId":null,"videoUrl":null,"customTitle":null,"uploadable":true,"customDescription":null,
            "createdAt":"2026-09-20T08:00:00Z","updatedAt":"2026-09-20T08:00:00Z",
            "uploads":[{"recordingId":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","platform":"youtube",
            "state":"uploading","progressBytes":1024,"totalBytes":10485760,"visibility":"unlisted",
            "videoId":null,"videoUrl":null,"error":null,"startedAt":null,"completedAt":null,
            "updatedAt":"2026-09-20T08:00:00Z"}]}]"#;
        let recordings: Vec<Recording> = serde_json::from_str(json).unwrap();
        let recording = &recordings[0];
        assert_eq!(recording.file_name, "service.mkv");
        assert!(recording.uploadable && !recording.uploaded);
        assert_eq!(recording.uploads[0].platform, "youtube");
        assert_eq!(recording.uploads[0].progress_bytes, 1024);
    }

    #[test]
    fn flagging_an_upload_matches_the_server_body() {
        let id = Uuid::nil();
        assert_eq!(
            serde_json::to_value(FlagUpload {
                recordings: vec![FlagUploadItem {
                    recording_id: id,
                    platforms: vec!["youtube".to_string()],
                }],
            })
            .unwrap(),
            serde_json::json!({"recordings": [{"recording_id": id, "platforms": ["youtube"]}]})
        );
    }
}
