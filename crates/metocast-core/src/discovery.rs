//! Read models for the existing device discovery API.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDevice {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub usb: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDevice {
    pub host: String,
    pub device_name: String,
    pub product_name: String,
    pub unique_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadlinkDevice {
    pub id: String,
    pub name: String,
    pub host: String,
    pub mac: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsDevice {
    pub item_name: String,
    pub item_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsDevices {
    pub displays: Vec<ObsDevice>,
    pub audio_inputs: Vec<ObsDevice>,
    pub audio_outputs: Vec<ObsDevice>,
    pub video_inputs: Vec<ObsDevice>,
    pub capture_cards: Vec<ObsDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSource {
    pub label: String,
    pub host_channels: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEndpoint {
    pub id: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub sources: Vec<AudioSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDiscovery {
    pub endpoint: Option<AudioEndpoint>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDeviceConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}
