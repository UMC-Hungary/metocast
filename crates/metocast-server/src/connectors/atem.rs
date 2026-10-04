use std::net::Ipv4Addr;
use std::sync::Arc;

use if_addrs::IfAddr;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent};
use serde::Serialize;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, interval, timeout_at};

use super::{
    AtemConfig, AtemInput, AtemState, ConnectorConfig, ConnectorStatus, RecordStatus, StreamStatus,
};

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const HELLO_TIMEOUT: Duration = Duration::from_secs(1);
const HELLO_ATTEMPTS: u32 = 3;
const SESSION_TIMEOUT: Duration = Duration::from_secs(5);
const RETRANSMIT_AFTER: Duration = Duration::from_millis(200);
const MAX_SENDS: u8 = 10;
const TICK: Duration = Duration::from_millis(100);

const FLAG_ACK_REQUEST: u8 = 0x01;
const FLAG_HELLO: u8 = 0x02;
const FLAG_RETRANSMIT: u8 = 0x04;
const FLAG_ACK_REPLY: u8 = 0x10;
const PACKET_ID_MASK: u16 = 0x7fff;
const HEADER_LEN: usize = 12;

const MDNS_SERVICE: &str = "_switcher_ctrl._udp.local.";

const HELLO_PACKET: [u8; 20] = [
    0x10, 0x14, 0x53, 0xab, 0, 0, 0, 0, 0, 0x3a, 0, 0, 0x01, 0, 0, 0, 0, 0, 0, 0,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredAtem {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub usb: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtemCommand {
    Program(u16),
    Preview(u16),
    Cut,
    Auto,
    Record(bool),
    Stream(bool),
    StreamDestination {
        service: String,
        url: String,
        key: String,
    },
}

impl AtemCommand {
    fn check(&self, state: &AtemState) -> Result<(), String> {
        match self {
            Self::Program(id) | Self::Preview(id) if !state.inputs.iter().any(|i| i.id == *id) => {
                Err("atem_unknown_input".into())
            }
            Self::Record(_) if state.recording.is_none() => {
                Err("atem_recording_unsupported".into())
            }
            Self::Stream(_) | Self::StreamDestination { .. } if state.streaming.is_none() => {
                Err("atem_streaming_unsupported".into())
            }
            _ => Ok(()),
        }
    }

    fn block(&self) -> Vec<u8> {
        let [hi, lo] = match self {
            Self::Program(id) | Self::Preview(id) => id.to_be_bytes(),
            _ => [0, 0],
        };
        match self {
            Self::Program(_) => command_block(b"CPgI", &[0, 0, hi, lo]),
            Self::Preview(_) => command_block(b"CPvI", &[0, 0, hi, lo]),
            Self::Cut => command_block(b"DCut", &[0; 4]),
            Self::Auto => command_block(b"DAut", &[0; 4]),
            Self::Record(on) => command_block(b"RcTM", &[u8::from(*on), 0, 0, 0]),
            Self::Stream(on) => command_block(b"StrR", &[u8::from(*on), 0, 0, 0]),
            Self::StreamDestination { service, url, key } => {
                let mut body = vec![0; 1100];
                body[0] = 0b111;
                put_text(&mut body[1..65], service);
                put_text(&mut body[65..577], url);
                put_text(&mut body[577..1089], key);
                command_block(b"CRSS", &body)
            }
        }
    }
}

type Outbox = mpsc::Sender<(Vec<u8>, oneshot::Sender<Result<(), String>>)>;

pub struct AtemConnector {
    status: RwLock<ConnectorStatus>,
    state: RwLock<Option<AtemState>>,
    outbox: RwLock<Option<Outbox>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    pub state_tx: broadcast::Sender<AtemState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl AtemConnector {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(ConnectorStatus::Disconnected),
            state: RwLock::new(None),
            outbox: RwLock::new(None),
            status_tx: broadcast::channel(16).0,
            state_tx: broadcast::channel(16).0,
            worker: Mutex::new(None),
        }
    }

    pub async fn start(self: &Arc<Self>, config: AtemConfig) {
        self.stop().await;
        if config.is_configured() {
            let this = Arc::clone(self);
            *self.worker.lock().await = Some(tokio::spawn(async move { this.run(config).await }));
        }
    }

    pub async fn stop(&self) {
        if let Some(worker) = self.worker.lock().await.take() {
            worker.abort();
            let _ = worker.await;
        }
        self.clear().await;
        self.set_status(ConnectorStatus::Disconnected).await;
    }

    pub async fn get_status(&self) -> ConnectorStatus {
        self.status.read().await.clone()
    }

    pub async fn get_state(&self) -> Option<AtemState> {
        self.state.read().await.clone()
    }

    pub async fn send(&self, command: AtemCommand) -> Result<(), String> {
        let state = self.get_state().await.ok_or("atem_not_connected")?;
        command.check(&state)?;
        let outbox = self
            .outbox
            .read()
            .await
            .clone()
            .ok_or("atem_not_connected")?;
        let (done_tx, done_rx) = oneshot::channel();
        outbox
            .send((command.block(), done_tx))
            .await
            .map_err(|_| "atem_not_connected")?;
        done_rx.await.unwrap_or_else(|_| Err("atem_no_ack".into()))
    }

    async fn run(&self, config: AtemConfig) {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            self.set_status(ConnectorStatus::Connecting).await;
            let (message, was_connected) = self.session(&config).await;
            self.clear().await;
            tracing::warn!(connector = "atem", "session ended: {message}");
            self.set_status(ConnectorStatus::Error { message }).await;
            if was_connected {
                backoff = INITIAL_BACKOFF;
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    async fn session(&self, config: &AtemConfig) -> (String, bool) {
        let socket = match open_socket(config).await {
            Ok(socket) => socket,
            Err(e) => return (e.to_string(), false),
        };
        let mut buf = [0u8; 2048];
        let mut session_id = match handshake(&socket, &mut buf).await {
            Ok(id) => id,
            Err(e) => return (e, false),
        };

        let (outbox_tx, mut outbox_rx) = mpsc::channel(16);
        let mut state = AtemState::default();
        let mut connected = false;
        let mut last_remote: u16 = 0;
        let mut next_local: u16 = 1;
        let mut pending: Vec<Pending> = Vec::new();
        let mut last_heard = Instant::now();
        let mut tick = interval(TICK);

        loop {
            tokio::select! {
                received = socket.recv(&mut buf) => {
                    let n = match received {
                        Ok(n) => n,
                        Err(e) => return (e.to_string(), connected),
                    };
                    let Some(packet) = Packet::parse(&buf[..n]) else { continue };
                    last_heard = Instant::now();
                    session_id = packet.session;
                    if packet.flags & FLAG_ACK_REPLY != 0 {
                        for acked in pending.extract_if(.., |p| covered(packet.ack_id, p.id)) {
                            let _ = acked.done.send(Ok(()));
                        }
                    }
                    if packet.flags & FLAG_ACK_REQUEST == 0 {
                        continue;
                    }
                    if packet.remote_id == (last_remote + 1) & PACKET_ID_MASK {
                        last_remote = packet.remote_id;
                        let mut init_complete = false;
                        for (name, body) in blocks(packet.payload) {
                            init_complete |= name == b"InCm";
                            apply_block(&mut state, name, body);
                        }
                        let _ = socket.send(&ack_packet(session_id, last_remote)).await;
                        if init_complete && !connected {
                            connected = true;
                            *self.outbox.write().await = Some(outbox_tx.clone());
                            self.set_status(ConnectorStatus::Connected).await;
                        }
                        if connected {
                            self.publish(&state).await;
                        }
                    } else if covered(last_remote, packet.remote_id) {
                        let _ = socket.send(&ack_packet(session_id, last_remote)).await;
                    }
                }
                Some((block, done)) = outbox_rx.recv() => {
                    let id = next_local;
                    next_local = (next_local + 1) & PACKET_ID_MASK;
                    let packet = command_packet(session_id, id, &block);
                    if let Err(e) = socket.send(&packet).await {
                        let _ = done.send(Err(e.to_string()));
                        return (e.to_string(), connected);
                    }
                    pending.push(Pending { id, packet, sent_at: Instant::now(), sends: 1, done });
                }
                _ = tick.tick() => {
                    if last_heard.elapsed() > SESSION_TIMEOUT {
                        return ("atem_timeout".into(), connected);
                    }
                    for p in pending.iter_mut().filter(|p| p.sent_at.elapsed() > RETRANSMIT_AFTER) {
                        if p.sends >= MAX_SENDS {
                            return ("atem_no_ack".into(), connected);
                        }
                        p.packet[0] |= FLAG_RETRANSMIT << 3;
                        p.sent_at = Instant::now();
                        p.sends += 1;
                        let _ = socket.send(&p.packet).await;
                    }
                }
            }
        }
    }

    async fn clear(&self) {
        *self.outbox.write().await = None;
        *self.state.write().await = None;
    }

    async fn publish(&self, state: &AtemState) {
        let mut current = self.state.write().await;
        if current.as_ref() != Some(state) {
            *current = Some(state.clone());
            let _ = self.state_tx.send(state.clone());
        }
    }

    async fn set_status(&self, new_status: ConnectorStatus) {
        let mut status = self.status.write().await;
        if *status != new_status {
            tracing::info!(connector = "atem", status = ?new_status, "connector status");
            *status = new_status.clone();
            let _ = self.status_tx.send(new_status);
        }
    }
}

impl Default for AtemConnector {
    fn default() -> Self {
        Self::new()
    }
}

struct Pending {
    id: u16,
    packet: Vec<u8>,
    sent_at: Instant,
    sends: u8,
    done: oneshot::Sender<Result<(), String>>,
}

pub async fn discover(timeout: Duration) -> Result<Vec<DiscoveredAtem>, String> {
    let usb_links: Vec<(Ipv4Addr, Ipv4Addr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.addr {
            IfAddr::V4(v4) if v4.prefixlen == 30 => Some((v4.ip, v4.netmask)),
            _ => None,
        })
        .collect();
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    daemon
        .disable_interface(IfKind::IPv6)
        .map_err(|e| e.to_string())?;
    let events = daemon.browse(MDNS_SERVICE).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    let mut found = Vec::new();
    while let Ok(Ok(event)) = timeout_at(deadline, events.recv_async()).await {
        let ServiceEvent::ServiceResolved(info) = event else {
            continue;
        };
        let name = info.get_fullname().trim_end_matches(MDNS_SERVICE);
        for ip in info.get_addresses_v4() {
            let atem = DiscoveredAtem {
                name: name.trim_end_matches('.').to_string(),
                host: ip.to_string(),
                port: info.get_port(),
                usb: on_link(*ip, &usb_links),
            };
            if !found.contains(&atem) {
                found.push(atem);
            }
        }
    }
    if let Ok(shutdown) = daemon.shutdown() {
        let _ = shutdown.recv_async().await;
    }
    Ok(found)
}

fn on_link(ip: Ipv4Addr, links: &[(Ipv4Addr, Ipv4Addr)]) -> bool {
    links.iter().any(|(local, mask)| {
        let mask = u32::from(*mask);
        u32::from(ip) & mask == u32::from(*local) & mask && ip != *local
    })
}

async fn open_socket(config: &AtemConfig) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect((config.host.as_str(), config.port)).await?;
    Ok(socket)
}

async fn handshake(socket: &UdpSocket, buf: &mut [u8]) -> Result<u16, String> {
    for _ in 0..HELLO_ATTEMPTS {
        socket
            .send(&HELLO_PACKET)
            .await
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + HELLO_TIMEOUT;
        while let Ok(received) = timeout_at(deadline, socket.recv(buf)).await {
            let n = received.map_err(|e| e.to_string())?;
            if let Some(packet) = Packet::parse(&buf[..n])
                && packet.flags & FLAG_HELLO != 0
            {
                socket
                    .send(&ack_packet(packet.session, 0))
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(packet.session);
            }
        }
    }
    Err("atem_no_response".into())
}

struct Packet<'a> {
    flags: u8,
    session: u16,
    ack_id: u16,
    remote_id: u16,
    payload: &'a [u8],
}

impl<'a> Packet<'a> {
    fn parse(bytes: &'a [u8]) -> Option<Self> {
        let word = |at: usize| Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
        let length = usize::from(word(0)? & 0x07ff);
        Some(Self {
            flags: bytes[0] >> 3,
            session: word(2)?,
            ack_id: word(4)?,
            remote_id: word(10)?,
            payload: bytes.get(HEADER_LEN..length)?,
        })
    }
}

fn header(flags: u8, length: usize, session: u16, ack_id: u16, local_id: u16) -> [u8; HEADER_LEN] {
    let [a, b] = ((u16::from(flags) << 11) | length as u16).to_be_bytes();
    let [s1, s2] = session.to_be_bytes();
    let [k1, k2] = ack_id.to_be_bytes();
    let [l1, l2] = local_id.to_be_bytes();
    [a, b, s1, s2, k1, k2, 0, 0, 0, 0, l1, l2]
}

fn ack_packet(session: u16, remote_id: u16) -> [u8; HEADER_LEN] {
    header(FLAG_ACK_REPLY, HEADER_LEN, session, remote_id, 0)
}

fn command_packet(session: u16, local_id: u16, block: &[u8]) -> Vec<u8> {
    let length = HEADER_LEN + block.len();
    [
        &header(FLAG_ACK_REQUEST, length, session, 0, local_id)[..],
        block,
    ]
    .concat()
}

fn command_block(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let length = (body.len() + 8) as u16;
    [&length.to_be_bytes()[..], &[0, 0], name, body].concat()
}

fn covered(ack: u16, id: u16) -> bool {
    ack.wrapping_sub(id) & PACKET_ID_MASK < 0x4000
}

fn blocks(mut payload: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    std::iter::from_fn(move || {
        let length = usize::from(u16::from_be_bytes([*payload.first()?, *payload.get(1)?]));
        let block = payload.get(..length).filter(|_| length >= 8)?;
        payload = &payload[length..];
        Some((&block[4..8], &block[8..]))
    })
}

fn apply_block(state: &mut AtemState, name: &[u8], body: &[u8]) -> Option<()> {
    let word = |at: usize| Some(u16::from_be_bytes([*body.get(at)?, *body.get(at + 1)?]));
    let text = |from: usize, len: usize| {
        let raw = body.get(from..from + len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(len);
        Some(String::from_utf8_lossy(&raw[..end]).into_owned())
    };
    let main_bus = body.first() == Some(&0);
    match name {
        b"_pin" => state.product = text(0, 44)?,
        b"PrgI" if main_bus => state.program = Some(word(2)?),
        b"PrvI" if main_bus => state.preview = Some(word(2)?),
        b"InPr" => {
            let input = AtemInput {
                id: word(0)?,
                name: text(2, 20)?,
                short_name: text(22, 4)?,
            };
            state.inputs.retain(|i| i.id != input.id);
            if body.get(35)? & 1 == 1 {
                let at = state.inputs.partition_point(|i| i.id < input.id);
                state.inputs.insert(at, input);
            }
        }
        b"StRS" => {
            let bits = word(0)?;
            state.streaming = Some(match bits {
                _ if bits & 0x20 != 0 => StreamStatus::Stopping,
                _ if bits & 0x04 != 0 => StreamStatus::Streaming,
                _ if bits & 0x02 != 0 => StreamStatus::Connecting,
                _ => StreamStatus::Idle,
            });
        }
        b"RTMS" => {
            let bits = word(0)?;
            state.recording = Some(match bits {
                _ if bits & 0x80 != 0 => RecordStatus::Stopping,
                _ if bits & 0x01 != 0 => RecordStatus::Recording,
                _ => RecordStatus::Idle,
            });
        }
        b"SRSU" => state.stream_service = Some(text(0, 64)?),
        _ => {}
    }
    Some(())
}

fn put_text(field: &mut [u8], value: &str) {
    let len = value.len().min(field.len());
    field[..len].copy_from_slice(&value.as_bytes()[..len]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(flags: u8, remote_id: u16, blocks: &[Vec<u8>]) -> Vec<u8> {
        let payload = blocks.concat();
        let mut bytes = header(flags, HEADER_LEN + payload.len(), 0x8003, 0, 0).to_vec();
        bytes[10..12].copy_from_slice(&remote_id.to_be_bytes());
        [bytes, payload].concat()
    }

    fn input_block(id: u16, name: &str, short: &str, me: u8) -> Vec<u8> {
        let mut body = vec![0; 36];
        body[0..2].copy_from_slice(&id.to_be_bytes());
        put_text(&mut body[2..22], name);
        put_text(&mut body[22..26], short);
        body[35] = me;
        command_block(b"InPr", &body)
    }

    #[test]
    fn a_state_dump_packet_parses_into_state_and_skips_unknown_blocks() {
        let mut pin = [0; 44];
        put_text(&mut pin, "ATEM Mini Pro");
        let bytes = packet(
            FLAG_ACK_REQUEST,
            1,
            &[
                command_block(b"_pin", &pin),
                command_block(b"_FAC", &[6, 0, 0, 0]),
                input_block(2, "Camera 2", "CAM2", 1),
                input_block(1, "Camera 1", "CAM1", 1),
                input_block(10010, "Program", "PGM", 0),
                command_block(b"PrgI", &[0, 0xc3, 0, 1]),
                command_block(b"PrvI", &[0, 0x72, 0, 2, 0, 0, 0, 0]),
                command_block(b"PrgI", &[1, 0, 0, 9]),
                command_block(b"StRS", &[0, 1, 0x64, 0x50]),
                command_block(b"RTMS", &[0, 0, 5, 0x80, 0xff, 0xff, 0xff, 0xff]),
            ],
        );
        let packet = Packet::parse(&bytes).expect("packet");
        assert_eq!(packet.flags, FLAG_ACK_REQUEST);
        assert_eq!(packet.remote_id, 1);

        let mut state = AtemState::default();
        for (name, body) in blocks(packet.payload) {
            apply_block(&mut state, name, body);
        }
        assert_eq!(state.product, "ATEM Mini Pro");
        assert_eq!(state.program, Some(1));
        assert_eq!(state.preview, Some(2));
        assert_eq!(
            state
                .inputs
                .iter()
                .map(|i| (i.id, i.short_name.as_str()))
                .collect::<Vec<_>>(),
            [(1, "CAM1"), (2, "CAM2")],
        );
        assert_eq!(state.streaming, Some(StreamStatus::Idle));
        assert_eq!(state.recording, Some(RecordStatus::Idle));
        assert_eq!(state.stream_service, None);
    }

    #[test]
    fn stream_status_keeps_the_transitions() {
        let mut state = AtemState::default();
        for (bits, expected) in [
            (0x02, StreamStatus::Connecting),
            (0x04, StreamStatus::Streaming),
            (0x24, StreamStatus::Stopping),
            (0x01, StreamStatus::Idle),
        ] {
            apply_block(&mut state, b"StRS", &[0, bits, 0, 0]);
            assert_eq!(state.streaming, Some(expected));
        }
    }

    #[test]
    fn a_truncated_block_ends_parsing_without_panicking() {
        let mut bytes = command_block(b"PrgI", &[0, 0, 0, 3]);
        bytes.extend_from_slice(&[0, 40, 0, 0, b'I', b'n']);
        let parsed: Vec<_> = blocks(&bytes).collect();
        assert_eq!(parsed.len(), 1);
        assert!(Packet::parse(&[0x08, 0x20, 0, 0]).is_none());
    }

    #[test]
    fn a_switcher_on_its_usb_link_is_told_from_one_on_the_lan() {
        let usb_link = [(
            Ipv4Addr::new(172, 25, 69, 186),
            Ipv4Addr::new(255, 255, 255, 252),
        )];
        assert!(on_link(Ipv4Addr::new(172, 25, 69, 185), &usb_link));
        assert!(!on_link(Ipv4Addr::new(172, 25, 69, 186), &usb_link));
        assert!(!on_link(Ipv4Addr::new(192, 168, 0, 40), &usb_link));
    }

    #[test]
    fn packet_ids_wrap_in_fifteen_bits() {
        assert!(covered(5, 5));
        assert!(covered(5, 1));
        assert!(!covered(5, 6));
        assert!(covered(2, 0x7ffe));
        assert!(!covered(0x7ffe, 2));
    }

    #[test]
    fn commands_serialize_to_the_switcher_layout() {
        assert_eq!(
            AtemCommand::Program(3).block(),
            [0, 12, 0, 0, b'C', b'P', b'g', b'I', 0, 0, 0, 3]
        );
        assert_eq!(
            AtemCommand::Stream(true).block(),
            [0, 12, 0, 0, b'S', b't', b'r', b'R', 1, 0, 0, 0]
        );
        let destination = AtemCommand::StreamDestination {
            service: "YouTube RTMP".into(),
            url: "rtmp://a.rtmp.youtube.com/live2".into(),
            key: "abcd".into(),
        }
        .block();
        assert_eq!(destination.len(), 1108);
        assert_eq!(&destination[4..9], b"CRSS\x07");
        assert_eq!(&destination[9..21], b"YouTube RTMP");
        assert_eq!(&destination[73..80], b"rtmp://");
        assert_eq!(&destination[585..590], b"abcd\0");

        let packet = command_packet(0x8003, 7, &AtemCommand::Cut.block());
        assert_eq!(
            &packet[..12],
            [0x08, 24, 0x80, 0x03, 0, 0, 0, 0, 0, 0, 0, 7]
        );
    }

    #[test]
    fn commands_check_what_the_switcher_reported() {
        let state = AtemState {
            inputs: vec![AtemInput {
                id: 1,
                name: "Camera 1".into(),
                short_name: "CAM1".into(),
            }],
            recording: Some(RecordStatus::Idle),
            ..AtemState::default()
        };
        assert!(AtemCommand::Program(1).check(&state).is_ok());
        assert_eq!(
            AtemCommand::Preview(7).check(&state).unwrap_err(),
            "atem_unknown_input"
        );
        assert!(AtemCommand::Record(true).check(&state).is_ok());
        assert_eq!(
            AtemCommand::Stream(true).check(&state).unwrap_err(),
            "atem_streaming_unsupported"
        );
    }
}
