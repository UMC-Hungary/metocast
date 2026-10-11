//! Live Hungarian → English transcription: the `openai` connector config and the state the core
//! broadcasts as `transcription.*` WebSocket messages.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connectors::{ConnectorConfig, RodecasterAudioSource};

/// Below this balance the credit line turns red.
pub const LOW_CREDIT_USD: f64 = 5.0;
pub const OPENAI_CONFIG_KEY: &str = "openai_config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopUp {
    pub amount_usd: f64,
    pub date: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenAiConfig {
    pub enabled: bool,
    pub api_key: String,
    /// Organization admin key; only used to read the Costs API for the credit line.
    pub admin_key: String,
    pub top_ups: Vec<TopUp>,
    /// The RØDECaster source the transcriber listens to.
    pub source: RodecasterAudioSource,
    /// How far the translated text trails the speech.
    pub lag_ms: u64,
    /// Sets new YouTube broadcasts to HTTP-POST captions and posts each segment to them.
    pub youtube_live_captions: bool,
    /// Overrides the ingestion URL read from the YouTube stream.
    pub caption_ingestion_url: String,
    /// Captions every finished YouTube upload.
    pub auto_youtube_captions: bool,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
            admin_key: String::new(),
            top_ups: Vec::new(),
            source: RodecasterAudioSource::MainMix,
            lag_ms: 3_200,
            youtube_live_captions: false,
            caption_ingestion_url: String::new(),
            auto_youtube_captions: false,
        }
    }
}

impl ConnectorConfig for OpenAiConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.api_key.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TranscriptionStatus {
    NotConfigured,
    /// `reason` says why, e.g. `mixerOffline`.
    Unavailable,
    Ready,
    Connecting,
    Running,
    /// `reason` is `noCredit`, `invalidKey` or a message.
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LiveCaptionStatus {
    pub ok: bool,
    pub last_seq: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionState {
    pub status: TranscriptionStatus,
    pub reason: Option<String>,
    pub session_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    /// Present while live YouTube captions are being posted.
    pub youtube: Option<LiveCaptionStatus>,
}

impl TranscriptionState {
    #[must_use]
    pub fn idle(status: TranscriptionStatus, reason: Option<String>) -> Self {
        Self {
            status,
            reason,
            session_id: None,
            event_id: None,
            started_at: None,
            youtube: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub seq: u32,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreditStatus {
    pub balance_usd: f64,
    pub topped_up_usd: f64,
    pub spent_usd: f64,
    pub as_of: DateTime<Utc>,
    pub low: bool,
}

impl CreditStatus {
    #[must_use]
    pub fn new(topped_up_usd: f64, spent_usd: f64, as_of: DateTime<Utc>) -> Self {
        let balance_usd = topped_up_usd - spent_usd;
        Self {
            balance_usd,
            topped_up_usd,
            spent_usd,
            as_of,
            low: balance_usd < LOW_CREDIT_USD,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_fill_missing_fields() {
        let config: OpenAiConfig =
            serde_json::from_str(r#"{"enabled":true,"apiKey":"sk"}"#).unwrap();
        assert!(config.is_configured());
        assert_eq!(config.lag_ms, 3_200);
        assert_eq!(config.source, RodecasterAudioSource::MainMix);
        assert!(!OpenAiConfig::default().is_configured());
    }

    #[test]
    fn credit_is_low_below_five_dollars() {
        let now = Utc::now();
        assert!(!CreditStatus::new(20.0, 15.0, now).low);
        let low = CreditStatus::new(20.0, 15.01, now);
        assert!(low.low);
        assert!((low.balance_usd - 4.99).abs() < 1e-9);
    }
}
