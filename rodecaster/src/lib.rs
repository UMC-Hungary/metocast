mod labels;

pub use labels::{Shape, source_label};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use rcp2_core::DeviceViewModel;
use rcp2_core::ops::fader;
use rcp2_protocol::device::{DeviceConnection, DeviceEvent, DeviceModel};
use rcp2_protocol::transport::Transport;
use rcp2_protocol::transport::hid::{
    HID_INTERFACE, PRODUCT_IDS_DUO, PRODUCT_IDS_PRO_II, VENDOR_ID,
};
use rcp2_protocol::types::{Structured, Value};
use serde::Serialize;

const REPORT_ID_SEND: u8 = 0x03;
const REPORT_SIZE: usize = 256;
const READ_TIMEOUT_MS: i32 = 100;
const MUTE_PROPERTY: &str = "channelOutputMute";
const WIRELESS_MUTE_PROPERTY: &str = "channelWirelessMute";
const CUE_PROPERTY: &str = "channelCueEnable";
const SOURCE_PROPERTY: &str = "channelInputSource";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no RØDECaster found on USB")]
    NotFound,
    #[error("the RØDECaster is open in another application")]
    Busy,
    #[error("the device did not send its state within {0:?}")]
    Handshake(Duration),
    #[error("no channel {0}")]
    NoChannel(usize),
    #[error("{0}")]
    Device(String),
}

impl From<rcp2_protocol::Error> for Error {
    fn from(e: rcp2_protocol::Error) -> Self {
        Error::Device(e.to_string())
    }
}

/// The reader re-locks the moment it releases, and a plain `Mutex` is not fair, so a
/// writer waiting behind it was starved for seconds at a time. `writing` makes the
/// reader stand aside, which bounds a write's wait by one `READ_TIMEOUT_MS`.
struct SharedHid {
    hid: Arc<Mutex<HidDevice>>,
    writing: Arc<AtomicBool>,
}

impl SharedHid {
    fn pair(hid: HidDevice) -> (Self, Self) {
        let hid = Arc::new(Mutex::new(hid));
        let writing = Arc::new(AtomicBool::new(false));
        (
            Self {
                hid: Arc::clone(&hid),
                writing: Arc::clone(&writing),
            },
            Self { hid, writing },
        )
    }

    fn lock(&self) -> rcp2_protocol::Result<std::sync::MutexGuard<'_, HidDevice>> {
        self.hid
            .lock()
            .map_err(|e| rcp2_protocol::Error::Transport(format!("HID lock poisoned: {e}")))
    }
}

impl Transport for SharedHid {
    fn send(&mut self, data: &[u8]) -> rcp2_protocol::Result<()> {
        let mut report = Vec::with_capacity(REPORT_SIZE);
        report.push(REPORT_ID_SEND);
        report.extend_from_slice(data);
        report.resize(REPORT_SIZE.max(report.len()), 0x00);
        self.writing.store(true, Ordering::Release);
        let sent = self.lock().and_then(|hid| {
            hid.send_output_report(&report)
                .map_err(|e| rcp2_protocol::Error::Transport(format!("HID send failed: {e}")))
        });
        self.writing.store(false, Ordering::Release);
        sent
    }

    fn recv(&mut self) -> rcp2_protocol::Result<Vec<u8>> {
        while self.writing.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        let mut buf = [0u8; REPORT_SIZE];
        let read = self
            .lock()?
            .read_timeout(&mut buf, READ_TIMEOUT_MS)
            .map_err(|e| rcp2_protocol::Error::Transport(format!("HID recv failed: {e}")))?;
        if read == 0 {
            return Err(rcp2_protocol::Error::Timeout);
        }
        Ok(buf[1..].to_vec())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub channel: usize,
    pub label: String,
    pub source: Option<u32>,
    pub level: f64,
    pub mute: bool,
    pub wireless_mute: bool,
    pub cue: bool,
    pub processing: Vec<String>,
    pub settings: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub model: String,
    pub firmware: String,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaderAssignment {
    pub fader_slot: usize,
    pub input_source: Option<u32>,
    pub label: String,
}

/// A lossless, typed snapshot for discovering firmware-specific properties.
/// `state` deliberately contains the complete device tree: USB multitrack and
/// processing-bypass property names are not assumed before hardware verification.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDump {
    pub model: String,
    pub firmware: String,
    pub fader_assignments: Vec<FaderAssignment>,
    pub state: Structured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MuteChange {
    pub channel: usize,
    pub muted: bool,
    pub remote: bool,
}

#[derive(Debug)]
pub enum Event {
    Mute(MuteChange),
    Resynced,
    Idle,
    Disconnected(String),
}

#[derive(Debug, Default)]
pub struct Mutes {
    roots: Vec<usize>,
    muted: Vec<bool>,
    wireless_muted: Vec<bool>,
}

impl Mutes {
    #[must_use]
    pub fn new(roots: Vec<usize>, muted: Vec<bool>, wireless_muted: Vec<bool>) -> Self {
        Self {
            roots,
            muted,
            wireless_muted,
        }
    }

    pub fn observe(&mut self, indices: &[usize], name: &str, value: &Value) -> Option<MuteChange> {
        let (&Value::Bool(muted), [root]) = (value, indices) else {
            return None;
        };
        let channel = self.roots.iter().position(|r| r == root)?;
        let (values, remote) = match name {
            MUTE_PROPERTY => (&mut self.muted, false),
            WIRELESS_MUTE_PROPERTY => (&mut self.wireless_muted, true),
            _ => return None,
        };
        if values.get(channel) == Some(&muted) {
            return None;
        }
        values.resize(self.roots.len(), false);
        values[channel] = muted;
        Some(MuteChange {
            channel,
            muted,
            remote,
        })
    }

    pub fn set(&mut self, channel: usize, muted: bool) {
        self.muted.resize(self.roots.len(), false);
        if let Some(slot) = self.muted.get_mut(channel) {
            *slot = muted;
        }
    }
}

pub struct Device {
    conn: DeviceConnection,
    model: DeviceModel,
    roots: Vec<usize>,
    mutes: Mutes,
}

impl Device {
    /// Enumerates hidapi's process-global `IOHIDManager`, which is not safe to touch
    /// from two threads at once: keep every call to this on one thread.
    ///
    /// # Errors
    /// [`Error::NotFound`] when no device is attached, [`Error::Busy`] when another
    /// application holds it, [`Error::Handshake`] when it never sends its state.
    pub fn open() -> Result<Self, Error> {
        let api = HidApi::new().map_err(|e| Error::Device(e.to_string()))?;
        let info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == VENDOR_ID
                    && (PRODUCT_IDS_PRO_II.contains(&d.product_id())
                        || PRODUCT_IDS_DUO.contains(&d.product_id()))
                    && d.interface_number() == HID_INTERFACE
            })
            .ok_or(Error::NotFound)?;
        let model = DeviceModel::from_product_id(info.product_id()).ok_or(Error::NotFound)?;
        let (rx, tx) = SharedHid::pair(info.open_device(&api).map_err(|_| Error::Busy)?);

        let conn = DeviceConnection::open(Box::new(rx), Box::new(tx), model, false)?;
        wait_for_state(&conn, HANDSHAKE_TIMEOUT)?;

        let state = conn.state().snapshot()?;
        let roots = fader::channel_indices(&state);
        let mutes = Mutes::new(
            roots.clone(),
            muted_flags(&state, &roots, MUTE_PROPERTY),
            muted_flags(&state, &roots, WIRELESS_MUTE_PROPERTY),
        );
        Ok(Self {
            conn,
            model,
            roots,
            mutes,
        })
    }

    #[must_use]
    pub fn model(&self) -> DeviceModel {
        self.model
    }

    /// # Errors
    /// Fails when the device's state lock is poisoned.
    pub fn profile(&self) -> Result<Profile, Error> {
        Ok(build_profile(&self.conn.state().snapshot()?, self.model))
    }

    /// The desk applies the write but never echoes it back as a property update, so
    /// the local mute state is recorded here — otherwise the next physical press of
    /// the same button would look like a repeat and be swallowed by [`Mutes::observe`].
    ///
    /// # Errors
    /// [`Error::NoChannel`] for an out-of-range channel, otherwise a transport failure.
    pub fn set_mute(&mut self, channel: usize, mute: bool) -> Result<(), Error> {
        let root = *self.roots.get(channel).ok_or(Error::NoChannel(channel))?;
        fader::set_mute(&self.conn, root, mute)?;
        self.conn.flush()?;
        self.mutes.set(channel, mute);
        Ok(())
    }

    /// The complete typed device state plus the identity fields needed to compare
    /// USB recording settings and fader assignments across hardware changes.
    ///
    /// # Errors
    /// Fails when the device's state lock is poisoned.
    pub fn dump(&self) -> Result<DeviceDump, Error> {
        let state = self.conn.state().snapshot()?;
        Ok(build_dump(state, self.model))
    }

    pub fn poll(&mut self, timeout: Duration) -> Event {
        match self.conn.events().recv_timeout(timeout) {
            Ok(DeviceEvent::PropertyUpdated {
                indices,
                name,
                value,
            }) => self
                .mutes
                .observe(&indices, &name, &value)
                .map_or(Event::Idle, Event::Mute),
            Ok(DeviceEvent::StateInitialized) => {
                if let Ok(state) = self.conn.state().snapshot() {
                    self.roots = fader::channel_indices(&state);
                    self.mutes = Mutes::new(
                        self.roots.clone(),
                        muted_flags(&state, &self.roots, MUTE_PROPERTY),
                        muted_flags(&state, &self.roots, WIRELESS_MUTE_PROPERTY),
                    );
                }
                Event::Resynced
            }
            Ok(DeviceEvent::Error(e)) => Event::Disconnected(e),
            Ok(DeviceEvent::Disconnected) => Event::Disconnected("device disconnected".into()),
            Ok(DeviceEvent::UnknownPacket(_)) => Event::Idle,
            Err(RecvTimeoutError::Timeout) => Event::Idle,
            Err(RecvTimeoutError::Disconnected) => {
                Event::Disconnected("event channel closed".into())
            }
        }
    }
}

#[must_use]
pub fn build_dump(state: Structured, model: DeviceModel) -> DeviceDump {
    let view = DeviceViewModel::from_state(&state, model.profile());
    let profile = build_profile(&state, model);
    DeviceDump {
        model: model.profile().display_name.to_string(),
        firmware: view.system.firmware,
        fader_assignments: profile
            .channels
            .into_iter()
            .map(|channel| FaderAssignment {
                fader_slot: channel.channel + 1,
                input_source: channel.source,
                label: channel.label,
            })
            .collect(),
        state,
    }
}

fn wait_for_state(conn: &DeviceConnection, timeout: Duration) -> Result<(), Error> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::Handshake(timeout));
        }
        match conn.events().recv_timeout(left) {
            Ok(DeviceEvent::StateInitialized) => return Ok(()),
            Ok(DeviceEvent::Error(e)) => return Err(Error::Device(e)),
            Ok(DeviceEvent::Disconnected) => return Err(Error::Device("disconnected".into())),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return Err(Error::Handshake(timeout)),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(Error::Device("event channel closed".into()));
            }
        }
    }
}

fn muted_flags(state: &Structured, roots: &[usize], property: &str) -> Vec<bool> {
    roots
        .iter()
        .map(|&root| {
            matches!(
                state.get_property(&[root], property),
                Some(Value::Bool(true))
            )
        })
        .collect()
}

#[must_use]
pub fn build_profile(state: &Structured, model: DeviceModel) -> Profile {
    let view = DeviceViewModel::from_state(state, model.profile());
    let roots = fader::channel_indices(state);
    let sources: Vec<&Structured> = state
        .children
        .iter()
        .filter(|c| c.name == "INPUTSOURCE")
        .collect();
    let shape = Shape {
        input_sources: sources.len(),
        channels: view.channels.len(),
    };

    let channels = view
        .faders
        .iter()
        .enumerate()
        .map(|(channel, fader)| {
            let source = view.channels.get(channel).map(|c| c.input_source);
            let source = fader.configured.then_some(source).flatten();
            let node = roots.get(channel).and_then(|&r| state.children.get(r));
            let input = source.and_then(|s| sources.get(s as usize)).copied();
            Channel {
                channel,
                label: source.map_or_else(
                    || format!("Channel {}", channel + 1),
                    |s| source_label(shape, s as usize),
                ),
                source,
                level: fader.level,
                mute: fader.mute,
                wireless_mute: matches!(
                    node.and_then(|n| n.properties.get(WIRELESS_MUTE_PROPERTY)),
                    Some(Value::Bool(true))
                ),
                cue: fader.cue,
                processing: [node, input]
                    .into_iter()
                    .flatten()
                    .flat_map(enabled_blocks)
                    .collect(),
                settings: [node, input]
                    .into_iter()
                    .flatten()
                    .flat_map(fields)
                    .collect(),
            }
        })
        .collect();

    Profile {
        model: model.profile().display_name.to_string(),
        firmware: view.system.firmware,
        channels,
    }
}

const SKIP: [&str; 4] = [
    MUTE_PROPERTY,
    WIRELESS_MUTE_PROPERTY,
    CUE_PROPERTY,
    SOURCE_PROPERTY,
];

fn enabled_blocks(node: &Structured) -> Vec<String> {
    let mut blocks: Vec<String> = node
        .properties
        .iter()
        .filter(|(name, value)| !SKIP.contains(&name.as_str()) && **value == Value::Bool(true))
        .map(|(name, _)| humanise(name))
        .collect();
    blocks.sort();
    blocks
}

fn fields(node: &Structured) -> Vec<Field> {
    let mut fields: Vec<Field> = node
        .properties
        .iter()
        .filter(|(name, _)| !SKIP.contains(&name.as_str()))
        .filter_map(|(name, value)| {
            render(value).map(|value| Field {
                name: humanise(name),
                value,
            })
        })
        .collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    fields
}

fn render(value: &Value) -> Option<String> {
    match value {
        Value::Bool(true) => None,
        Value::Bool(false) => Some("off".to_string()),
        Value::U32(v) => Some(v.to_string()),
        Value::F64(v) | Value::Double(v) => Some(format!("{v:.3}")),
        Value::String(v) if !v.is_empty() => Some(v.clone()),
        _ => None,
    }
}

fn humanise(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let spaced: String = chars
        .iter()
        .enumerate()
        .flat_map(|(i, &c)| {
            let boundary = c.is_uppercase()
                && i > 0
                && (chars[i - 1].is_lowercase()
                    || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
            boundary.then_some(' ').into_iter().chain([c])
        })
        .collect();
    let mut rest = spaced.chars();
    rest.next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(rest).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mutes() -> Mutes {
        Mutes::new(
            vec![2, 3, 5],
            vec![false, false, false],
            vec![false, false, false],
        )
    }

    #[test]
    fn a_mute_burst_yields_one_event() {
        let mut m = mutes();
        assert_eq!(
            m.observe(&[3], MUTE_PROPERTY, &Value::Bool(true)),
            Some(MuteChange {
                channel: 1,
                muted: true,
                remote: false,
            })
        );
        for _ in 0..11 {
            assert_eq!(m.observe(&[3], "mixMute", &Value::Bool(true)), None);
        }
        assert_eq!(m.observe(&[3], MUTE_PROPERTY, &Value::Bool(true)), None);
    }

    #[test]
    fn unmuting_after_a_mute_is_a_change() {
        let mut m = mutes();
        m.observe(&[5], MUTE_PROPERTY, &Value::Bool(true));
        assert_eq!(
            m.observe(&[5], MUTE_PROPERTY, &Value::Bool(false)),
            Some(MuteChange {
                channel: 2,
                muted: false,
                remote: false,
            })
        );
    }

    #[test]
    fn wireless_mute_is_reported_separately() {
        let mut m = mutes();
        assert_eq!(
            m.observe(&[3], WIRELESS_MUTE_PROPERTY, &Value::Bool(true)),
            Some(MuteChange {
                channel: 1,
                muted: true,
                remote: true,
            })
        );
        assert_eq!(m.observe(&[3], MUTE_PROPERTY, &Value::Bool(false)), None);
    }

    #[test]
    fn updates_outside_the_channel_list_are_ignored() {
        let mut m = mutes();
        assert_eq!(m.observe(&[9], MUTE_PROPERTY, &Value::Bool(true)), None);
        assert_eq!(m.observe(&[3, 0], MUTE_PROPERTY, &Value::Bool(true)), None);
        assert_eq!(m.observe(&[3], MUTE_PROPERTY, &Value::U32(1)), None);
    }

    #[test]
    fn every_property_is_rendered_exactly_once() {
        assert_eq!(render(&Value::Bool(true)), None);
        assert_eq!(render(&Value::Bool(false)).as_deref(), Some("off"));
        assert_eq!(render(&Value::U32(7)).as_deref(), Some("7"));
        assert_eq!(render(&Value::String(String::new())), None);
        assert_eq!(render(&Value::Unknown(vec![1])), None);
    }

    #[test]
    fn property_names_read_as_words() {
        assert_eq!(humanise("channelOutputMute"), "Channel Output Mute");
        assert_eq!(humanise("channelHPFEnable"), "Channel HPF Enable");
        assert_eq!(humanise("input48VEnable"), "Input48V Enable");
    }

    #[test]
    fn dump_keeps_property_value_types_and_promotes_identity_fields() {
        let mut system = Structured {
            name: "SYSTEM".into(),
            properties: std::collections::HashMap::new(),
            children: vec![],
        };
        system.properties.insert(
            "systemFirmwareVersion".into(),
            Value::String("1.7.3".into()),
        );
        system
            .properties
            .insert("usbMultitrackMode".into(), Value::U32(2));
        let mut channel = Structured {
            name: "CHANNEL".into(),
            properties: std::collections::HashMap::new(),
            children: vec![],
        };
        channel
            .properties
            .insert(SOURCE_PROPERTY.into(), Value::U32(4));
        let dump = build_dump(
            Structured {
                name: "Rodecaster".into(),
                properties: std::collections::HashMap::new(),
                children: vec![system, channel],
            },
            DeviceModel::ProII,
        );
        let json = serde_json::to_value(dump).expect("dump is valid JSON");

        assert_eq!(json["model"], "RØDECaster Pro II");
        assert_eq!(json["firmware"], "1.7.3");
        assert_eq!(json["faderAssignments"][0]["faderSlot"], 1);
        assert_eq!(json["faderAssignments"][0]["inputSource"], 4);
        assert_eq!(
            json["state"]["children"][0]["properties"]["usbMultitrackMode"]["U32"],
            2
        );
    }
}
