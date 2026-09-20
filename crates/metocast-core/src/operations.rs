//! Read models for the housekeeping endpoints: scheduled jobs, the upload queues, saved
//! Broadlink commands, OBS device alerts and recordings that belong to no event yet.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A scheduled job from `GET /api/cron-jobs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: Uuid,
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    #[serde(default)]
    pub pull_youtube: bool,
    #[serde(default)]
    pub auto_upload: bool,
}

/// The body for creating or updating one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJobDraft {
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub pull_youtube: bool,
    pub auto_upload: bool,
}

/// One queue's counts from `GET /api/queues`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSummary {
    pub queue: String,
    #[serde(default)]
    pub pending: i64,
    #[serde(default)]
    pub processing: i64,
    #[serde(default)]
    pub succeeded: i64,
    #[serde(default)]
    pub dead: i64,
}

/// One job from `GET /api/queues/{queue}/jobs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueJob {
    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    pub status: String,
    #[serde(default)]
    pub attempts: i32,
    #[serde(default)]
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// A saved RF/IR command from `GET /api/connectors/broadlink/commands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadlinkCommand {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub category: String,
    pub device_id: Option<Uuid>,
}

/// An OBS source the server watches, from the `obs.listeners.list` message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListener {
    pub id: Uuid,
    pub connector_type: String,
    pub category: String,
    pub device_item_value: String,
    pub device_item_name: String,
    pub friendly_name: String,
}

/// A recorded file that belongs to no event, from `GET /api/recordings/untracked`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedRecording {
    pub id: Uuid,
    pub file_name: String,
    #[serde(default)]
    pub file_size: i64,
    #[serde(default)]
    pub duration_seconds: f64,
    pub detected_at: DateTime<Utc>,
}

/// One autocomplete hit from `GET /api/bible/suggest`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibleSuggestion {
    pub cat: String,
    pub label: String,
    pub link: String,
}

/// `GET /api/logs`, which only a core with a desktop host can answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationLog {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn housekeeping_models_decode_the_server_shapes() {
        let jobs: Vec<CronJob> = serde_json::from_str(
            r#"[{"id":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","name":"Sunday pull",
                "cronExpression":"0 9 * * 0","enabled":true,"pullYoutube":true,"autoUpload":false,
                "createdAt":"2026-09-20T08:00:00Z","updatedAt":"2026-09-20T08:00:00Z"}]"#,
        )
        .unwrap();
        assert_eq!(
            (jobs[0].cron_expression.as_str(), jobs[0].pull_youtube),
            ("0 9 * * 0", true)
        );

        let queues: Vec<QueueSummary> = serde_json::from_str(
            r#"[{"queue":"uploads","pending":2,"processing":1,"succeeded":9,"dead":0,
                "oldestAvailableAt":"2026-09-20T08:00:00Z"}]"#,
        )
        .unwrap();
        assert_eq!(
            (queues[0].queue.as_str(), queues[0].pending),
            ("uploads", 2)
        );

        let queue_jobs: Vec<QueueJob> = serde_json::from_str(
            r#"[{"id":"0d9c4b7a-1e2f-4a3b-8c5d-6e7f8a9b0c1d","queue":"uploads","jobType":"youtube",
                "dedupKey":null,"payload":{},"status":"dead","attempts":3,"maxAttempts":3,
                "availableAt":"2026-09-20T08:00:00Z","lockedAt":null,"lockedBy":null,
                "lastError":"quota exceeded","createdAt":"2026-09-20T08:00:00Z",
                "updatedAt":"2026-09-20T09:00:00Z"}]"#,
        )
        .unwrap();
        assert_eq!(queue_jobs[0].last_error.as_deref(), Some("quota exceeded"));

        let commands: Vec<BroadlinkCommand> = serde_json::from_str(
            r#"[{"id":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","deviceId":null,"name":"Projector on",
                "slug":"projector-on","code":"JgB...","codeType":"ir","category":"projector"}]"#,
        )
        .unwrap();
        assert_eq!(commands[0].slug, "projector-on");

        let listeners: Vec<DeviceListener> = serde_json::from_str(
            r#"[{"id":"0d9c4b7a-1e2f-4a3b-8c5d-6e7f8a9b0c1d","connectorType":"obs",
                "category":"audio_input","deviceItemValue":"mic-1","deviceItemName":"Mic",
                "friendlyName":"Pulpit mic","createdAt":"2026-09-20T08:00:00Z",
                "updatedAt":"2026-09-20T08:00:00Z"}]"#,
        )
        .unwrap();
        assert_eq!(listeners[0].friendly_name, "Pulpit mic");

        let untracked: Vec<UntrackedRecording> = serde_json::from_str(
            r#"[{"id":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","filePath":"/Movies/stray.mkv",
                "fileName":"stray.mkv","fileSize":2048,"durationSeconds":61.5,
                "detectedAt":"2026-09-20T08:00:00Z","createdAt":"2026-09-20T08:00:00Z"}]"#,
        )
        .unwrap();
        assert_eq!(untracked[0].file_name, "stray.mkv");

        let suggestions: Vec<BibleSuggestion> =
            serde_json::from_str(r#"[{"cat":"book","label":"János 3","link":"/Jn3"}]"#).unwrap();
        assert_eq!(suggestions[0].label, "János 3");

        let log: ApplicationLog =
            serde_json::from_str(r#"{"path":"/tmp/metocast.log","content":"started"}"#).unwrap();
        assert_eq!(log.content, "started");
    }

    #[test]
    fn a_cron_draft_matches_the_server_body() {
        assert_eq!(
            serde_json::to_value(CronJobDraft {
                name: "Sunday pull".to_string(),
                cron_expression: "0 9 * * 0".to_string(),
                enabled: true,
                pull_youtube: true,
                auto_upload: false,
            })
            .unwrap(),
            serde_json::json!({"name": "Sunday pull", "cronExpression": "0 9 * * 0",
                               "enabled": true, "pullYoutube": true, "autoUpload": false})
        );
    }
}
