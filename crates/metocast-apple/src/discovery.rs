use metocast_core::discovery::{AudioDiscovery, NetworkDeviceConfig, ObsDevices};
use serde::Serialize;

use crate::{AppleClient, AppleError};

#[derive(Clone, uniffi::Record)]
pub struct DiscoveredDeviceRecord {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub host: String,
    pub port: u16,
}

#[uniffi::remote(Record)]
pub struct NetworkDeviceConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum DiscoveryTool {
    Atem,
    Middlecontrol,
    Camera,
    Broadlink,
    Obs,
    Rodecaster,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ScanCommand {
    #[serde(rename = "obs.devices.scan")]
    Obs,
    #[serde(rename = "rodecaster.audio.discover")]
    Rodecaster,
}

#[uniffi::export]
impl AppleClient {
    pub async fn discover_devices(
        &self,
        tool: DiscoveryTool,
    ) -> Result<Vec<DiscoveredDeviceRecord>, AppleError> {
        let client = self.client.clone();
        match tool {
            DiscoveryTool::Atem | DiscoveryTool::Middlecontrol => {
                let atem = matches!(tool, DiscoveryTool::Atem);
                let devices = self
                    .run(async move { client.discover_network(atem).await })
                    .await?;
                Ok(devices
                    .into_iter()
                    .map(|device| DiscoveredDeviceRecord {
                        id: format!("{}:{}", device.host, device.port),
                        name: if device.name.is_empty() {
                            (if atem { "ATEM" } else { "Middle Control" }).into()
                        } else {
                            device.name
                        },
                        detail: if device.usb {
                            "USB network link".into()
                        } else {
                            "Network".into()
                        },
                        host: device.host,
                        port: device.port,
                    })
                    .collect())
            }
            DiscoveryTool::Camera => {
                let devices = self
                    .run(async move { client.discover_cameras().await })
                    .await?;
                Ok(devices
                    .into_iter()
                    .map(|device| DiscoveredDeviceRecord {
                        id: format!("{}:{}", device.unique_id, device.host),
                        name: device.device_name,
                        detail: device.product_name,
                        host: device.host,
                        port: 0,
                    })
                    .collect())
            }
            DiscoveryTool::Broadlink => {
                self.run(async move { client.discover_broadlink().await })
                    .await?;
                // Discovery is asynchronous; callers display saved devices separately.
                Ok(vec![])
            }
            DiscoveryTool::Obs => {
                self.send(ScanCommand::Obs).await?;
                Ok(vec![])
            }
            DiscoveryTool::Rodecaster => {
                self.send(ScanCommand::Rodecaster).await?;
                Ok(vec![])
            }
        }
    }

    pub async fn saved_broadlink_devices(&self) -> Result<Vec<DiscoveredDeviceRecord>, AppleError> {
        let client = self.client.clone();
        let devices = self
            .run(async move { client.broadlink_devices().await })
            .await?;
        Ok(devices
            .into_iter()
            .map(|device| DiscoveredDeviceRecord {
                id: device.id,
                name: device.name,
                detail: format!("{} · {}", device.model.unwrap_or_default(), device.mac),
                host: device.host,
                port: 0,
            })
            .collect())
    }

    pub async fn network_device_config(
        &self,
        atem: bool,
    ) -> Result<NetworkDeviceConfig, AppleError> {
        let client = self.client.clone();
        self.run(async move { client.network_device_config(atem).await })
            .await
    }

    pub async fn save_network_device(
        &self,
        atem: bool,
        config: NetworkDeviceConfig,
    ) -> Result<(), AppleError> {
        if config.host.trim().is_empty() || config.port == 0 {
            return Err(AppleError::InvalidInput);
        }
        let client = self.client.clone();
        self.run(async move { client.save_network_device(atem, config).await })
            .await
    }
}

pub fn obs_devices(devices: ObsDevices) -> Vec<DiscoveredDeviceRecord> {
    [
        ("Display", devices.displays),
        ("Audio input", devices.audio_inputs),
        ("Audio output", devices.audio_outputs),
        ("Video input", devices.video_inputs),
        ("Capture card", devices.capture_cards),
    ]
    .into_iter()
    .flat_map(|(category, devices)| {
        devices
            .into_iter()
            .map(move |device| DiscoveredDeviceRecord {
                id: format!("{category}:{}", device.item_value),
                name: device.item_name,
                detail: category.into(),
                host: String::new(),
                port: 0,
            })
    })
    .collect()
}

pub fn audio_devices(discovery: AudioDiscovery) -> (Vec<DiscoveredDeviceRecord>, Option<String>) {
    let Some(endpoint) = discovery.endpoint else {
        return (vec![], discovery.unavailable_reason);
    };
    let mut devices = vec![DiscoveredDeviceRecord {
        id: endpoint.id.clone(),
        name: endpoint.name,
        detail: format!(
            "{} Hz · {} channels",
            endpoint.sample_rate, endpoint.channels
        ),
        host: String::new(),
        port: 0,
    }];
    devices.extend(
        endpoint
            .sources
            .into_iter()
            .enumerate()
            .map(|(index, source)| DiscoveredDeviceRecord {
                id: format!("{}:{index}", endpoint.id),
                name: source.label,
                detail: format!(
                    "Input channels {}",
                    source
                        .host_channels
                        .iter()
                        .map(|channel| (channel + 1).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                host: String::new(),
                port: 0,
            }),
    );
    (devices, discovery.unavailable_reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppleEvent;
    use metocast_core::protocol::ServerEvent;

    #[test]
    fn existing_discovery_messages_reach_native_device_rows() {
        let obs: ServerEvent = serde_json::from_str(
            r#"{
            "type":"obs.devices.available", "listenerStatuses":[],
            "devices":{"displays":[],"audioInputs":[{"itemName":"Mic","itemValue":"mic-1"}],
            "audioOutputs":[],"videoInputs":[],"captureCards":[],"scannedAt":"2026-09-19T12:00:00Z"}
        }"#,
        )
        .unwrap();
        let AppleEvent::Discovery {
            tool: DiscoveryTool::Obs,
            devices,
            ..
        } = AppleEvent::from(obs)
        else {
            panic!("missing OBS discovery");
        };
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Mic");
        assert_eq!(devices[0].detail, "Audio input");
        let audio: ServerEvent = serde_json::from_str(r#"{
            "type":"rodecaster.audio.discovery", "discovery":{"endpoint":{
                "id":"usb-1","name":"RØDECaster","sampleRate":48000,"channels":16,
                "sources":[{"identity":{"kind":"mainMix"},"label":"Main mix","layout":"stereo","hostChannels":[0,1]}]
            },"unavailableReason":null}}
        "#).unwrap();
        let AppleEvent::Discovery {
            devices, message, ..
        } = AppleEvent::from(audio)
        else {
            panic!("missing audio discovery");
        };
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].detail, "Input channels 1, 2");
        assert!(message.is_none());
        let unavailable: ServerEvent = serde_json::from_str(r#"{"type":"rodecaster.audio.discovery","discovery":{"endpoint":null,"unavailableReason":"USB not connected"}}"#).unwrap();
        let AppleEvent::Discovery {
            devices, message, ..
        } = AppleEvent::from(unavailable)
        else {
            panic!("missing failure");
        };
        assert!(devices.is_empty());
        assert_eq!(message.as_deref(), Some("USB not connected"));
        assert!(
            serde_json::from_str::<ServerEvent>(
                r#"{"type":"obs.devices.available","devices":{"audioInputs":"invalid"}}"#
            )
            .is_err()
        );
    }
}
