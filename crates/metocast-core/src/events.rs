use chrono::{DateTime, Duration, Local, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const CURRENT_EVENT_WINDOW_MINUTES: i64 = 4 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventConnection {
    pub platform: String,
    #[serde(alias = "external_id")]
    pub external_id: Option<String>,
    #[serde(alias = "stream_url")]
    pub stream_url: Option<String>,
    #[serde(alias = "event_url")]
    pub event_url: Option<String>,
    #[serde(alias = "schedule_status")]
    pub schedule_status: String,
    #[serde(alias = "privacy_status")]
    pub privacy_status: Option<String>,
    pub extra: Option<serde_json::Value>,
}

/// One verse, normalised across the upstream Bible APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibleVerse {
    pub chapter: i32,
    pub verse: i32,
    pub text: String,
}

/// `GET /api/bible/verses`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiblePassage {
    pub label: String,
    pub verses: Vec<BibleVerse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleReference {
    pub r#type: String,
    pub reference: String,
    pub translation: String,
    pub verses: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: Uuid,
    pub title: String,
    #[serde(default)]
    #[serde(alias = "computed_title")]
    pub computed_title: String,
    #[serde(alias = "date_time")]
    pub date_time: DateTime<Utc>,
    pub speaker: String,
    pub description: String,
    #[serde(alias = "auto_upload_enabled")]
    pub auto_upload_enabled: bool,
    #[serde(alias = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(alias = "updated_at")]
    pub updated_at: DateTime<Utc>,
    pub connections: Vec<EventConnection>,
    #[serde(alias = "bible_references")]
    pub bible_references: Vec<BibleReference>,
}

impl Event {
    pub fn connection(&self, platform: &str) -> Option<&EventConnection> {
        self.connections
            .iter()
            .find(|connection| connection.platform == platform)
    }

    pub fn published_title(&self) -> &str {
        if self.computed_title.is_empty() {
            &self.title
        } else {
            &self.computed_title
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSummary {
    pub id: Uuid,
    pub title: String,
    #[serde(alias = "computed_title")]
    pub computed_title: String,
    #[serde(alias = "date_time")]
    pub date_time: DateTime<Utc>,
    pub speaker: String,
    #[serde(alias = "recording_count")]
    pub recording_count: i64,
    #[serde(alias = "is_completed")]
    pub is_completed: bool,
    #[serde(alias = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(alias = "updated_at")]
    pub updated_at: DateTime<Utc>,
}

pub fn current_event(events: &[EventSummary], now: DateTime<Utc>) -> Option<&EventSummary> {
    let today = now.with_timezone(&Local).date_naive();
    let window = Duration::minutes(CURRENT_EVENT_WINDOW_MINUTES);
    events
        .iter()
        .filter(|event| {
            !event.is_completed
                && event.date_time.with_timezone(&Local).date_naive() == today
                && event.date_time <= now
                && now - event.date_time <= window
        })
        .max_by_key(|event| event.date_time)
        .or_else(|| {
            events
                .iter()
                .filter(|event| !event.is_completed && event.date_time >= now)
                .min_by_key(|event| event.date_time)
        })
}

pub const DEFAULT_TITLE_TEMPLATE: &str =
    "{date|YYYY.MM.DD.} {title}[ | Textus: {textus}][ Lekció: {leckio}][ | {speaker}]";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleTemplate {
    pub template: String,
}

impl Default for TitleTemplate {
    fn default() -> Self {
        Self {
            template: DEFAULT_TITLE_TEMPLATE.to_string(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SlideFolder {
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConnection {
    pub platform: String,
    pub privacy_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBibleReference {
    pub r#type: String,
    pub reference: Option<String>,
    pub translation: Option<String>,
    pub verses: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvent {
    pub title: String,
    pub computed_title: Option<String>,
    pub date_time: DateTime<Utc>,
    pub speaker: Option<String>,
    pub description: Option<String>,
    pub auto_upload_enabled: Option<bool>,
    pub connections: Option<Vec<CreateConnection>>,
    pub bible_references: Option<Vec<CreateBibleReference>>,
}

pub type UpdateEvent = CreateEvent;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_wire_format_stays_camel_case() {
        let timestamp = "2026-01-11T10:00:00Z".parse().unwrap();
        let event = EventSummary {
            id: Uuid::nil(),
            title: "Sunday".to_string(),
            computed_title: String::new(),
            date_time: timestamp,
            speaker: "Speaker".to_string(),
            recording_count: 0,
            is_completed: false,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(value["recordingCount"], 0);
        assert_eq!(value["isCompleted"], false);
        assert!(value.get("recording_count").is_none());
    }
}
