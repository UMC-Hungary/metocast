use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    connectors::ConnectorStatus,
    events::{Event, EventSummary},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum PresenterCommand {
    #[serde(rename = "presenter.status")]
    Status,
    #[serde(rename = "presenter.next")]
    Next,
    #[serde(rename = "presenter.prev")]
    Previous,
    #[serde(rename = "presenter.first")]
    First,
    #[serde(rename = "presenter.last")]
    Last,
    #[serde(rename = "presenter.goto")]
    GoTo { slide: u32 },
    #[serde(rename = "presenter.mute")]
    Mute,
    #[serde(rename = "presenter.unmute")]
    Unmute,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PresenterRenderMode {
    #[default]
    Text,
    Svg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphContent {
    #[serde(default)]
    pub lines: Vec<String>,
    pub align: String,
    #[serde(default)]
    pub font_size_pt: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideContent {
    pub index: u32,
    pub paragraphs: Vec<ParagraphContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgSlideContent {
    pub index: u32,
    pub svg: String,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenterState {
    pub loaded: bool,
    pub file_path: Option<String>,
    pub current_slide: u32,
    pub total_slides: u32,
    #[serde(default)]
    pub render_mode: PresenterRenderMode,
    #[serde(default)]
    pub slides: Vec<SlideContent>,
    #[serde(default)]
    pub svg_slides: Vec<SvgSlideContent>,
    pub muted: bool,
    #[serde(default = "default_slide_width")]
    pub slide_width_emu: u64,
    #[serde(default = "default_slide_height")]
    pub slide_height_emu: u64,
}

const fn default_slide_width() -> u64 {
    12_192_000
}

const fn default_slide_height() -> u64 {
    6_858_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "connected")]
    Connected {
        #[serde(rename = "serverId")]
        server_id: String,
    },
    #[serde(rename = "connector.status")]
    ConnectorStatus {
        connector: String,
        status: ConnectorStatus,
    },
    #[serde(rename = "connectors.status")]
    ConnectorStatuses {
        statuses: HashMap<String, ConnectorStatus>,
    },
    #[serde(rename = "events.list")]
    EventsList { events: Vec<EventSummary> },
    #[serde(rename = "events.get")]
    Event { event: Event },
    #[serde(rename = "presenter.state")]
    PresenterState { state: PresenterState },
    #[serde(rename = "ping")]
    Ping {
        #[serde(rename = "pingId")]
        ping_id: i64,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenter_command_matches_existing_protocol() {
        assert_eq!(
            serde_json::to_string(&PresenterCommand::GoTo { slide: 3 }).unwrap(),
            r#"{"type":"presenter.goto","slide":3}"#
        );
    }
}
