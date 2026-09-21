use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::device_listener::DeviceListener;

// ── Device kind → category mapping ───────────────────────────────────────────

/// OBS input kinds that map to each device category.
const DISPLAY_KINDS: &[&str] = &["screen_capture", "monitor_capture"];
const AUDIO_INPUT_KINDS: &[&str] = &[
    "coreaudio_input_capture",
    "wasapi_input_capture",
    "pulse_input_capture",
];
const AUDIO_OUTPUT_KINDS: &[&str] = &[
    "coreaudio_output_capture",
    "wasapi_output_capture",
    "pulse_output_capture",
];
const VIDEO_INPUT_KINDS: &[&str] = &["av_capture_input", "dshow_input"];
const CAPTURE_CARD_KINDS: &[&str] = &["blackmagic-design-capture", "decklink-input"];

/// Property name used to enumerate the device list for each input kind.
fn property_for_kind(kind: &str) -> &'static str {
    match kind {
        "screen_capture" | "monitor_capture" => "display_uuid",
        "coreaudio_input_capture"
        | "wasapi_input_capture"
        | "pulse_input_capture"
        | "coreaudio_output_capture"
        | "wasapi_output_capture"
        | "pulse_output_capture" => "device_id",
        "av_capture_input" => "device",
        "dshow_input" => "video_device_id",
        "blackmagic-design-capture" => "device_hash",
        "decklink-input" => "device_name",
        _ => "device_id",
    }
}

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsDeviceItem {
    pub item_name: String,
    pub item_value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsAvailableDevices {
    pub displays: Vec<ObsDeviceItem>,
    pub audio_inputs: Vec<ObsDeviceItem>,
    pub audio_outputs: Vec<ObsDeviceItem>,
    pub video_inputs: Vec<ObsDeviceItem>,
    pub capture_cards: Vec<ObsDeviceItem>,
    pub scanned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListenerStatus {
    pub listener_id: Uuid,
    pub available: bool,
    pub last_checked: DateTime<Utc>,
}

// ── Scanner ───────────────────────────────────────────────────────────────────

/// Query all OBS inputs once and return the available devices grouped by category.
pub async fn scan_obs_devices(client: &obws::Client) -> ObsAvailableDevices {
    let all_inputs = match client.inputs().list(None).await {
        Ok(inputs) => inputs,
        Err(e) => {
            tracing::warn!("scan_obs_devices: failed to list inputs: {e}");
            return ObsAvailableDevices {
                displays: vec![],
                audio_inputs: vec![],
                audio_outputs: vec![],
                video_inputs: vec![],
                capture_cards: vec![],
                scanned_at: Utc::now(),
            };
        }
    };

    ObsAvailableDevices {
        displays: query_category(client, &all_inputs, DISPLAY_KINDS).await,
        audio_inputs: query_category(client, &all_inputs, AUDIO_INPUT_KINDS).await,
        audio_outputs: query_category(client, &all_inputs, AUDIO_OUTPUT_KINDS).await,
        video_inputs: query_category(client, &all_inputs, VIDEO_INPUT_KINDS).await,
        capture_cards: query_category(client, &all_inputs, CAPTURE_CARD_KINDS).await,
        scanned_at: Utc::now(),
    }
}

/// For a set of input kinds, find the first matching OBS source and query its
/// device list. Returns an empty vec if no matching source exists.
async fn query_category(
    client: &obws::Client,
    all_inputs: &[obws::responses::inputs::Input],
    kinds: &[&str],
) -> Vec<ObsDeviceItem> {
    for kind in kinds {
        if let Some(input) = all_inputs.iter().find(|i| i.kind == *kind) {
            let source_name = input.id.name.clone();
            if source_name.is_empty() {
                continue;
            }
            let property = property_for_kind(kind);
            match client
                .inputs()
                .properties_list_property_items(
                    obws::requests::inputs::InputId::Name(&source_name),
                    property,
                )
                .await
            {
                Ok(items) => {
                    return items
                        .into_iter()
                        .map(|item| ObsDeviceItem {
                            item_name: item.name,
                            item_value: item.value.as_str().map(String::from).unwrap_or_default(),
                        })
                        .collect();
                }
                Err(e) => {
                    tracing::debug!(
                        "scan_obs_devices: properties_list_property_items({source_name}, {property}): {e}"
                    );
                }
            }
        }
    }
    vec![]
}

// ── Status computation ────────────────────────────────────────────────────────

/// Check whether each listener's device is present in the latest scan results.
pub fn compute_listener_statuses(
    available: &ObsAvailableDevices,
    listeners: &[DeviceListener],
) -> Vec<DeviceListenerStatus> {
    let now = Utc::now();
    listeners
        .iter()
        .map(|l| {
            let device_list = match l.category.as_str() {
                "display" => &available.displays,
                "audio_input" => &available.audio_inputs,
                "audio_output" => &available.audio_outputs,
                "video_input" => &available.video_inputs,
                "capture_card" => &available.capture_cards,
                _ => {
                    return DeviceListenerStatus {
                        listener_id: l.id,
                        available: false,
                        last_checked: now,
                    };
                }
            };
            let found = device_list
                .iter()
                .any(|d| d.item_value == l.device_item_value);
            DeviceListenerStatus {
                listener_id: l.id,
                available: found,
                last_checked: now,
            }
        })
        .collect()
}
