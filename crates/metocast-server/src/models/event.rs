use chrono::{DateTime, Local, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

pub use metocast_core::events::{
    BibleReference, CreateBibleReference, CreateConnection, CreateEvent, Event, EventConnection,
    EventSummary, SlideFolder, TitleTemplate, UpdateEvent, current_event,
};

/// Raw DB row for the `events` table — no platform fields, no connections, no bible refs.
/// Used only with sqlx::FromRow inside [`fetch_event`]; never serialized directly.
#[derive(Debug, FromRow)]
struct EventRow {
    pub id: Uuid,
    pub title: String,
    pub computed_title: String,
    pub date_time: DateTime<Utc>,
    pub speaker: String,
    pub description: String,
    pub auto_upload_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct EventConnectionRow {
    platform: String,
    external_id: Option<String>,
    stream_url: Option<String>,
    event_url: Option<String>,
    schedule_status: String,
    privacy_status: Option<String>,
    extra: Option<serde_json::Value>,
}

impl From<EventConnectionRow> for EventConnection {
    fn from(row: EventConnectionRow) -> Self {
        Self {
            platform: row.platform,
            external_id: row.external_id,
            stream_url: row.stream_url,
            event_url: row.event_url,
            schedule_status: row.schedule_status,
            privacy_status: row.privacy_status,
            extra: row.extra,
        }
    }
}

#[derive(Debug, FromRow)]
struct BibleReferenceRow {
    r#type: String,
    reference: String,
    translation: String,
    verses: serde_json::Value,
}

impl From<BibleReferenceRow> for BibleReference {
    fn from(row: BibleReferenceRow) -> Self {
        Self {
            r#type: row.r#type,
            reference: row.reference,
            translation: row.translation,
            verses: row.verses,
        }
    }
}

/// Fetch a single event with its connections and bible references. Returns `None` if not found.
pub async fn fetch_event(id: Uuid, pool: &PgPool) -> anyhow::Result<Option<Event>> {
    let row = sqlx::query_as::<_, EventRow>("SELECT * FROM events WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let connections = sqlx::query_as::<_, EventConnectionRow>(
        "SELECT platform, external_id, stream_url, event_url, \
         schedule_status, privacy_status, extra \
         FROM event_connections WHERE event_id = $1 ORDER BY platform",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let bible_references = sqlx::query_as::<_, BibleReferenceRow>(
        "SELECT type, reference, translation, verses \
         FROM event_bible_references WHERE event_id = $1 ORDER BY type",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(Some(Event {
        id: row.id,
        title: row.title,
        computed_title: row.computed_title,
        date_time: row.date_time,
        speaker: row.speaker,
        description: row.description,
        auto_upload_enabled: row.auto_upload_enabled,
        created_at: row.created_at,
        updated_at: row.updated_at,
        connections: connections.into_iter().map(Into::into).collect(),
        bible_references: bible_references.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Debug, FromRow)]
struct EventSummaryRow {
    id: Uuid,
    title: String,
    computed_title: String,
    date_time: DateTime<Utc>,
    speaker: String,
    recording_count: i64,
    is_completed: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<EventSummaryRow> for EventSummary {
    fn from(row: EventSummaryRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            computed_title: row.computed_title,
            date_time: row.date_time,
            speaker: row.speaker,
            recording_count: row.recording_count,
            is_completed: row.is_completed,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn fetch_event_summaries(pool: &PgPool) -> Result<Vec<EventSummary>, sqlx::Error> {
    sqlx::query_as::<_, EventSummaryRow>(
        r#"SELECT e.id, e.title, e.computed_title, e.date_time, e.speaker,
                  e.created_at, e.updated_at,
                  COUNT(r.id) AS recording_count,
                  EXISTS (
                      SELECT 1 FROM event_activities ea
                      WHERE ea.event_id = e.id AND ea.activity_type = 'completed'
                  ) AS is_completed
           FROM events e
           LEFT JOIN recordings r ON r.event_id = e.id
           GROUP BY e.id
           ORDER BY e.date_time DESC"#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Into::into).collect())
}

/// The current event when it falls on today's local date — what an automatically
/// detected recording is attached to. `None` leaves that recording untracked
/// rather than filing it under an event on another day.
pub async fn find_current_event(pool: &PgPool) -> anyhow::Result<Option<EventSummary>> {
    let now = Utc::now();
    let today = now.with_timezone(&Local).date_naive();
    let events = fetch_event_summaries(pool).await?;
    Ok(current_event(&events, now)
        .filter(|event| event.date_time.with_timezone(&Local).date_naive() == today)
        .cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn local_time(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
    }

    fn event(id: Uuid, date_time: DateTime<Utc>, is_completed: bool) -> EventSummary {
        EventSummary {
            id,
            title: id.to_string(),
            computed_title: String::new(),
            date_time,
            speaker: String::new(),
            recording_count: 0,
            is_completed,
            created_at: date_time,
            updated_at: date_time,
        }
    }

    fn selected(events: &[EventSummary], now: DateTime<Utc>) -> Option<Uuid> {
        current_event(events, now).map(|event| event.id)
    }

    #[test]
    fn selects_recent_same_day_past_event_before_later_future_event() {
        let now = local_time(2026, 1, 11, 11, 0);
        let service_id = Uuid::new_v4();
        let later_id = Uuid::new_v4();
        let events = vec![
            event(service_id, local_time(2026, 1, 11, 10, 0), false),
            event(later_id, local_time(2026, 1, 11, 18, 0), false),
        ];

        assert_eq!(selected(&events, now), Some(service_id));
    }

    #[test]
    fn ignores_recent_past_event_after_current_window() {
        let now = local_time(2026, 1, 11, 15, 1);
        let old_service_id = Uuid::new_v4();
        let later_id = Uuid::new_v4();
        let events = vec![
            event(old_service_id, local_time(2026, 1, 11, 10, 0), false),
            event(later_id, local_time(2026, 1, 11, 18, 0), false),
        ];

        assert_eq!(selected(&events, now), Some(later_id));
    }

    #[test]
    fn ignores_completed_recent_past_event() {
        let now = local_time(2026, 1, 11, 11, 0);
        let completed_id = Uuid::new_v4();
        let later_id = Uuid::new_v4();
        let events = vec![
            event(completed_id, local_time(2026, 1, 11, 10, 0), true),
            event(later_id, local_time(2026, 1, 11, 18, 0), false),
        ];

        assert_eq!(selected(&events, now), Some(later_id));
    }

    /// The morning service is rarely marked completed by hand, so the evening
    /// recording used to be filed under it.
    #[test]
    fn second_service_of_the_day_wins_once_it_has_started() {
        let now = local_time(2026, 1, 11, 18, 30);
        let morning_id = Uuid::new_v4();
        let evening_id = Uuid::new_v4();
        let events = vec![
            event(morning_id, local_time(2026, 1, 11, 10, 0), false),
            event(evening_id, local_time(2026, 1, 11, 18, 0), false),
        ];

        assert_eq!(selected(&events, now), Some(evening_id));
    }

    #[test]
    fn uses_the_local_date_not_the_utc_date() {
        let now = local_time(2026, 1, 11, 0, 45);
        let after_midnight_id = Uuid::new_v4();
        let events = vec![event(
            after_midnight_id,
            local_time(2026, 1, 11, 0, 30),
            false,
        )];

        assert_eq!(selected(&events, now), Some(after_midnight_id));
    }

    #[test]
    fn returns_nothing_when_every_event_is_past() {
        let now = local_time(2026, 1, 13, 9, 0);
        let events = vec![event(Uuid::new_v4(), local_time(2026, 1, 11, 10, 0), false)];

        assert_eq!(selected(&events, now), None);
    }
}
