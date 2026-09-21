//! Broadlink RF/IR device integration module
//!
//! This module provides integration with Broadlink devices (RM4, RM Pro, etc.)
//! for sending and learning IR/RF codes using raw UDP protocol implementation.

use aes::Aes128;
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Default Broadlink encryption key (before auth)
const DEFAULT_KEY: [u8; 16] = [
    0x09, 0x76, 0x28, 0x34, 0x3f, 0xe9, 0x9e, 0x23, 0x76, 0x5c, 0x15, 0x13, 0xac, 0xcf, 0x8b, 0x02,
];

/// Default Broadlink IV
const DEFAULT_IV: [u8; 16] = [
    0x56, 0x2e, 0x17, 0x99, 0x6d, 0x09, 0x3d, 0x28, 0xdd, 0xb3, 0xba, 0x69, 0x5a, 0x2e, 0x6f, 0x58,
];

/// Raw Broadlink device handler for direct protocol communication
struct BroadlinkDevice {
    socket: UdpSocket,
    device_mac: [u8; 6],
    device_type: u16,
    key: [u8; 16],
    iv: [u8; 16],
    id: [u8; 4],
    count: u16,
}

impl BroadlinkDevice {
    /// Connect to a Broadlink device
    fn connect(host: &str, mac: &str, devtype: &str, local_ip: Ipv4Addr) -> Result<Self, String> {
        let device_ip: Ipv4Addr = host.parse().map_err(|e| format!("Invalid IP: {}", e))?;

        // Parse MAC address - use as-is from discovery response (no reversal needed)
        // The MAC bytes from discovery are already in the correct format for packets
        let mac_bytes: Vec<u8> = mac
            .split(':')
            .map(|s| u8::from_str_radix(s, 16).unwrap_or(0))
            .collect();
        let mut device_mac = [0u8; 6];
        if mac_bytes.len() == 6 {
            device_mac.copy_from_slice(&mac_bytes);
        }

        // Parse device type (e.g., "0x520b")
        let device_type = if let Some(hex) = devtype.strip_prefix("0x") {
            u16::from_str_radix(hex, 16).unwrap_or(0)
        } else {
            devtype.parse().unwrap_or(0)
        };

        // Bind socket to specific local IP (required for proper routing on Windows with multiple interfaces)
        let bind_addr = SocketAddr::new(local_ip.into(), 0);
        let socket =
            UdpSocket::bind(bind_addr).map_err(|e| format!("Failed to bind socket: {}", e))?;

        // Set socket options (matching python-broadlink)
        socket
            .set_broadcast(true)
            .map_err(|e| format!("Failed to set broadcast: {}", e))?;

        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| format!("Failed to set timeout: {}", e))?;

        // Connect to device (helps Windows route correctly)
        let device_addr = SocketAddr::new(device_ip.into(), 80);
        socket
            .connect(device_addr)
            .map_err(|e| format!("Failed to connect socket: {}", e))?;

        tracing::info!(
            "Socket bound to {:?}, connected to {:?}",
            socket.local_addr().ok(),
            device_addr
        );

        let mut dev = BroadlinkDevice {
            socket,
            device_mac,
            device_type,
            key: DEFAULT_KEY,
            iv: DEFAULT_IV,
            id: [0, 0, 0, 0],
            count: 0,
        };

        // Authenticate
        dev.auth()?;

        Ok(dev)
    }

    /// Encrypt data using AES-128-CBC with zero padding
    fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        // Pad to 16-byte boundary with zeros
        let padded_len = data.len().div_ceil(16) * 16;
        let mut padded = vec![0u8; padded_len];
        padded[..data.len()].copy_from_slice(data);

        let cipher = Aes128::new(GenericArray::from_slice(&self.key));
        let mut iv = self.iv;

        // CBC mode encryption
        let mut result = Vec::with_capacity(padded_len);
        for chunk in padded.chunks(16) {
            let mut block = [0u8; 16];
            for i in 0..16 {
                block[i] = chunk[i] ^ iv[i];
            }
            let block_arr = GenericArray::from_mut_slice(&mut block);
            cipher.encrypt_block(block_arr);
            result.extend_from_slice(&block);
            iv = block;
        }
        result
    }

    /// Decrypt data using AES-128-CBC
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        if !data.len().is_multiple_of(16) {
            return Err("Invalid encrypted data length".to_string());
        }

        let cipher = Aes128::new(GenericArray::from_slice(&self.key));
        let mut iv = self.iv;

        // CBC mode decryption
        let mut result = Vec::with_capacity(data.len());
        for chunk in data.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            let block_arr = GenericArray::from_mut_slice(&mut block);
            cipher.decrypt_block(block_arr);
            for i in 0..16 {
                block[i] ^= iv[i];
            }
            result.extend_from_slice(&block);
            iv.copy_from_slice(chunk);
        }
        Ok(result)
    }

    /// Send a command packet to the device
    fn send_packet(&mut self, command: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
        self.count = self.count.wrapping_add(1);

        // Encrypt payload
        let encrypted = self.encrypt(payload);

        // Build packet
        let mut packet = vec![0u8; 0x38];

        // Header
        packet[0x00] = 0x5a;
        packet[0x01] = 0xa5;
        packet[0x02] = 0xaa;
        packet[0x03] = 0x55;
        packet[0x04] = 0x5a;
        packet[0x05] = 0xa5;
        packet[0x06] = 0xaa;
        packet[0x07] = 0x55;

        // Device type
        packet[0x24] = (self.device_type & 0xff) as u8;
        packet[0x25] = ((self.device_type >> 8) & 0xff) as u8;

        // Command
        packet[0x26] = command;

        // Count
        packet[0x28] = (self.count & 0xff) as u8;
        packet[0x29] = ((self.count >> 8) & 0xff) as u8;

        // MAC address
        packet[0x2a..0x30].copy_from_slice(&self.device_mac);

        // Device ID
        packet[0x30..0x34].copy_from_slice(&self.id);

        // Payload checksum (over unencrypted payload)
        let mut payload_checksum: u16 = 0xbeaf;
        for byte in payload {
            payload_checksum = payload_checksum.wrapping_add(*byte as u16);
        }
        packet[0x34] = (payload_checksum & 0xff) as u8;
        packet[0x35] = ((payload_checksum >> 8) & 0xff) as u8;

        // Append encrypted payload
        packet.extend_from_slice(&encrypted);

        // Header checksum (bytes 0x20-0x21 are still 0, so they don't affect the sum)
        let mut checksum: u16 = 0xbeaf;
        for byte in &packet {
            checksum = checksum.wrapping_add(*byte as u16);
        }
        packet[0x20] = (checksum & 0xff) as u8;
        packet[0x21] = ((checksum >> 8) & 0xff) as u8;

        tracing::debug!(
            "Sending packet: cmd=0x{:02x}, payload_len={}, encrypted_len={}, total_len={}",
            command,
            payload.len(),
            encrypted.len(),
            packet.len()
        );
        tracing::debug!("Using key: {:02x?}", &self.key);
        tracing::debug!("Device ID: {:02x?}, count: {}", &self.id, self.count);
        tracing::debug!(
            "Packet header (first 56 bytes): {:02x?}",
            &packet[..0x38.min(packet.len())]
        );

        // Send (using send() since we used connect())
        let sent = self
            .socket
            .send(&packet)
            .map_err(|e| format!("Send failed: {}", e))?;
        tracing::debug!("Sent {} bytes", sent);

        // Receive response
        let mut buf = [0u8; 2048];
        let len = self.socket.recv(&mut buf).map_err(|e| {
            tracing::error!("Receive failed (timeout or error): {}", e);
            format!("Receive failed: {}", e)
        })?;
        tracing::debug!("Received {} bytes", len);

        tracing::debug!("Received response: {} bytes", len);

        if len < 0x38 {
            return Err(format!("Response too short: {} bytes", len));
        }

        // Check error code
        let err = (buf[0x22] as u16) | ((buf[0x23] as u16) << 8);
        if err != 0 {
            tracing::error!("Device returned error: 0x{:04x}", err);
            return Err(format!("Device error: 0x{:04x}", err));
        }

        // Decrypt payload
        if len > 0x38 {
            let encrypted_payload = &buf[0x38..len];
            self.decrypt(encrypted_payload)
        } else {
            Ok(Vec::new())
        }
    }

    /// Authenticate with the device
    fn auth(&mut self) -> Result<(), String> {
        let mut payload = vec![0u8; 0x50];

        // Fill with device ID - 16 bytes (0x04 to 0x13 inclusive, matching python-broadlink)
        for byte in payload.iter_mut().take(0x14).skip(0x04) {
            *byte = 0x31;
        }
        payload[0x1e] = 0x01;
        payload[0x2d] = 0x01;
        // Device name "Test 1" (6 bytes at 0x30-0x35)
        payload[0x30..0x36].copy_from_slice(b"Test 1");

        tracing::info!("Sending auth packet...");
        let response = self.send_packet(0x65, &payload)?;

        tracing::info!(
            "Auth response length: {}, data: {:02x?}",
            response.len(),
            &response[..response.len().min(32)]
        );

        if response.len() < 0x14 {
            return Err(format!("Auth response too short: {} bytes", response.len()));
        }

        // Extract session ID and key
        self.id.copy_from_slice(&response[0x00..0x04]);
        self.key.copy_from_slice(&response[0x04..0x14]);

        tracing::info!(
            "Authenticated with device, session ID: {:02x?}, new key: {:02x?}",
            self.id,
            &self.key[..8]
        );

        Ok(())
    }

    /// Encode command for RM4 devices (length-prefixed format)
    /// Format: 2 bytes length (little-endian) + 4 bytes command (little-endian) + data
    fn encode_rm4_command(&self, command: u32, data: &[u8]) -> Vec<u8> {
        let length = (data.len() + 4) as u16;
        let mut packet = Vec::with_capacity(6 + data.len());
        // Length (2 bytes, little-endian)
        packet.push((length & 0xff) as u8);
        packet.push(((length >> 8) & 0xff) as u8);
        // Command (4 bytes, little-endian)
        packet.push((command & 0xff) as u8);
        packet.push(((command >> 8) & 0xff) as u8);
        packet.push(((command >> 16) & 0xff) as u8);
        packet.push(((command >> 24) & 0xff) as u8);
        // Data
        packet.extend_from_slice(data);
        packet
    }

    /// Decode RM4 response (has length prefix)
    /// Response format: 2 bytes length + 4 bytes header + data
    fn decode_rm4_response(&self, payload: &[u8]) -> Vec<u8> {
        if payload.len() < 6 {
            return Vec::new();
        }
        let p_len = (payload[0] as usize) | ((payload[1] as usize) << 8);
        let end = (p_len + 2).min(payload.len());
        if end > 6 {
            payload[6..end].to_vec()
        } else {
            Vec::new()
        }
    }

    /// Enter IR learning mode and wait for code
    fn learn_ir(&mut self) -> Result<Vec<u8>, String> {
        // Enter learning mode - RM4 format: command 0x03
        tracing::info!("Sending enter learning mode command (RM4 format)...");
        let payload = self.encode_rm4_command(0x03, &[]);
        tracing::debug!("RM4 command payload: {:02x?}", payload);
        let response = self.send_packet(0x6a, &payload)?;
        tracing::info!(
            "Enter learning response: {} bytes, data: {:02x?}",
            response.len(),
            &response[..response.len().min(20)]
        );

        tracing::info!("Entered IR learning mode, waiting for signal...");

        // Poll for data (up to 30 seconds)
        let start = Instant::now();
        let timeout = Duration::from_secs(30);

        while start.elapsed() < timeout {
            if LEARN_CANCEL.load(Ordering::SeqCst) {
                return Err("Learning cancelled".to_string());
            }

            std::thread::sleep(Duration::from_millis(500));

            // Check for data - RM4 format: command 0x04
            // Note: RM4 devices may return error 0xfffb when no data is available yet
            let check_payload = self.encode_rm4_command(0x04, &[]);
            match self.send_packet(0x6a, &check_payload) {
                Ok(data) => {
                    tracing::debug!(
                        "Check data response: {} bytes, data: {:02x?}",
                        data.len(),
                        &data[..data.len().min(20)]
                    );
                    let code = self.decode_rm4_response(&data);
                    if !code.is_empty() && code.iter().any(|&b| b != 0) {
                        tracing::info!("Received IR code: {} bytes", code.len());
                        return Ok(code);
                    }
                }
                Err(e) => {
                    // Error 0xfffb means "no data available yet" on some RM4 devices
                    // This is NOT a fatal error during learning, just keep polling
                    if e.contains("0xfffb") {
                        tracing::debug!("No IR data yet (0xfffb), continuing to poll...");
                    } else {
                        tracing::warn!("Check data error: {}", e);
                    }
                }
            }
        }

        Err("Learning timeout - no signal received".to_string())
    }

    /// Enter RF learning mode and wait for code
    fn learn_rf(&mut self) -> Result<Vec<u8>, String> {
        // RF learning - sweep frequency (RM4 format: command 0x19)
        let payload = self.encode_rm4_command(0x19, &[]);
        self.send_packet(0x6a, &payload)?;

        tracing::info!("RF learning: Press and hold the remote button...");

        // Wait for frequency lock
        let start = Instant::now();
        let timeout = Duration::from_secs(30);
        let mut freq_locked = false;

        while start.elapsed() < timeout && !freq_locked {
            if LEARN_CANCEL.load(Ordering::SeqCst) {
                return Err("Learning cancelled".to_string());
            }

            std::thread::sleep(Duration::from_millis(500));

            // Check frequency lock (RM4 format: command 0x1a)
            let check_payload = self.encode_rm4_command(0x1a, &[]);
            if let Ok(data) = self.send_packet(0x6a, &check_payload) {
                let decoded = self.decode_rm4_response(&data);
                if !decoded.is_empty() && decoded[0] == 1 {
                    freq_locked = true;
                    tracing::info!("RF frequency locked");
                }
            }
        }

        if !freq_locked {
            return Err("RF frequency lock timeout".to_string());
        }

        // Now capture the code (RM4 format: command 0x1b)
        let payload = self.encode_rm4_command(0x1b, &[]);
        self.send_packet(0x6a, &payload)?;

        tracing::info!("RF learning: Now tap the button briefly...");

        // Poll for data
        let start = Instant::now();
        while start.elapsed() < timeout {
            if LEARN_CANCEL.load(Ordering::SeqCst) {
                return Err("Learning cancelled".to_string());
            }

            std::thread::sleep(Duration::from_millis(500));

            // Check data (RM4 format: command 0x04)
            // Note: RM4 devices may return error 0xfffb when no data is available yet
            let check_payload = self.encode_rm4_command(0x04, &[]);
            match self.send_packet(0x6a, &check_payload) {
                Ok(data) => {
                    let code = self.decode_rm4_response(&data);
                    if !code.is_empty() && code.iter().any(|&b| b != 0) {
                        // Cancel sweep (RM4 format: command 0x1e)
                        let cancel = self.encode_rm4_command(0x1e, &[]);
                        let _ = self.send_packet(0x6a, &cancel);
                        tracing::info!("Received RF code: {} bytes", code.len());
                        return Ok(code);
                    }
                }
                Err(e) => {
                    // Error 0xfffb means "no data available yet" on some RM4 devices
                    if !e.contains("0xfffb") {
                        tracing::debug!("RF check data error: {}", e);
                    }
                }
            }
        }

        // Cancel sweep on timeout
        let cancel = self.encode_rm4_command(0x1e, &[]);
        let _ = self.send_packet(0x6a, &cancel);

        Err("RF learning timeout - no signal received".to_string())
    }

    /// Send an IR/RF code (RM4 format: command 0x02 with code as data)
    fn send_code(&mut self, code: &[u8]) -> Result<(), String> {
        let payload = self.encode_rm4_command(0x02, code);
        self.send_packet(0x6a, &payload)?;
        Ok(())
    }
}

/// Discovered Broadlink device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    #[serde(rename = "type")]
    pub device_type: String,
    pub model: String,
    pub host: String,
    pub mac: String,
    pub name: String,
}

/// Format MAC address bytes as colon-separated hex string
fn format_mac(mac: &[u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

/// Get device model name from device type code
fn get_device_model(devtype: u16) -> (&'static str, &'static str) {
    match devtype {
        // RM Mini 3
        0x2737 => ("RM Mini 3", "Remote"),
        0x27c2 => ("RM Mini 3", "Remote"),
        // RM Pro
        0x2787 => ("RM Pro", "Remote"),
        0x279d => ("RM Pro", "Remote"),
        0x27a9 => ("RM Pro", "Remote"),
        // RM4 Mini
        0x5f36 => ("RM4 Mini", "Remote"),
        0x520b => ("RM4 Mini", "Remote"),
        0x520c => ("RM4 Mini", "Remote"),
        0x520d => ("RM4 Mini", "Remote"),
        // RM4 Pro
        0x6026 => ("RM4 Pro", "Remote"),
        0x61a2 => ("RM4 Pro", "Remote"),
        0x6184 => ("RM4 Pro", "Remote"),
        0x649b => ("RM4 Pro", "Remote"),
        0x653a => ("RM4 Pro", "Remote"),
        // RM4C Mini
        0x6508 => ("RM4C Mini", "Remote"),
        0x6070 => ("RM4C Mini", "Remote"),
        // RM4 TV Mate
        0x6539 => ("RM4 TV Mate", "Remote"),
        // SP Mini
        0x7547 => ("SP Mini", "Plug"),
        0x7918 => ("SP Mini", "Plug"),
        // SP2
        0x2711 => ("SP2", "Plug"),
        0x2719 => ("SP2", "Plug"),
        0x7919 => ("SP2", "Plug"),
        0x791a => ("SP2", "Plug"),
        // Default
        _ => ("Unknown", "Unknown"),
    }
}

/// Raw UDP discovery - bypasses rbroadlink library parsing issues
fn raw_discover_on_interface(local_ip: Ipv4Addr, timeout_secs: u64) -> Vec<DiscoveredDevice> {
    let mut devices = Vec::new();

    // Build discovery packet
    let mut packet = vec![0u8; 0x30];

    // Timezone offset
    let tz_offset: i32 = 0;
    packet[0x08] = (tz_offset & 0xff) as u8;
    packet[0x09] = ((tz_offset >> 8) & 0xff) as u8;
    packet[0x0a] = ((tz_offset >> 16) & 0xff) as u8;
    packet[0x0b] = ((tz_offset >> 24) & 0xff) as u8;

    // Year
    let year: u16 = 2024;
    packet[0x0c] = (year & 0xff) as u8;
    packet[0x0d] = ((year >> 8) & 0xff) as u8;

    // Time fields
    packet[0x0e] = 0; // seconds
    packet[0x0f] = 0; // minutes
    packet[0x10] = 12; // hours
    packet[0x11] = 1; // weekday
    packet[0x12] = 1; // day
    packet[0x13] = 1; // month

    // Local IP
    let octets = local_ip.octets();
    packet[0x18] = octets[0];
    packet[0x19] = octets[1];
    packet[0x1a] = octets[2];
    packet[0x1b] = octets[3];

    // Source port
    let port: u16 = 0; // Will be assigned by OS
    packet[0x1c] = (port & 0xff) as u8;
    packet[0x1d] = ((port >> 8) & 0xff) as u8;

    // Command: discover (0x0006)
    packet[0x26] = 0x06;

    // Calculate checksum
    let mut checksum: u16 = 0xbeaf;
    for byte in &packet {
        checksum = checksum.wrapping_add(*byte as u16);
    }
    packet[0x20] = (checksum & 0xff) as u8;
    packet[0x21] = ((checksum >> 8) & 0xff) as u8;

    // Bind socket
    let bind_addr = SocketAddr::new(local_ip.into(), 0);
    let socket = match UdpSocket::bind(bind_addr) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to bind UDP socket on {}: {}", local_ip, e);
            return devices;
        }
    };

    if let Err(e) = socket.set_broadcast(true) {
        tracing::warn!("Failed to set broadcast: {}", e);
        return devices;
    }

    if let Err(e) = socket.set_read_timeout(Some(Duration::from_secs(timeout_secs))) {
        tracing::warn!("Failed to set timeout: {}", e);
        return devices;
    }

    // Send discovery broadcast
    let broadcast_addr: SocketAddr = "255.255.255.255:80".parse().unwrap();
    if let Err(e) = socket.send_to(&packet, broadcast_addr) {
        tracing::warn!("Failed to send discovery on {}: {}", local_ip, e);
        return devices;
    }

    tracing::info!("Sent discovery broadcast from {}", local_ip);

    // Receive responses
    let mut buf = [0u8; 1024];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => {
                if len < 0x40 {
                    tracing::debug!("Response too short ({} bytes) from {}", len, src);
                    continue;
                }

                // Parse device info from response
                // Device type at 0x34-0x35 (little-endian)
                let devtype = (buf[0x34] as u16) | ((buf[0x35] as u16) << 8);

                // MAC at 0x3a-0x3f
                let mac: [u8; 6] = [
                    buf[0x3a], buf[0x3b], buf[0x3c], buf[0x3d], buf[0x3e], buf[0x3f],
                ];

                // Device IP from source address
                let host = match src.ip() {
                    IpAddr::V4(ip) => ip.to_string(),
                    IpAddr::V6(_) => continue,
                };

                // Get friendly model name
                let (model, _) = get_device_model(devtype);

                // Try to get device name from response (if available)
                // Name starts at 0x40 and is null-terminated
                let name = if len > 0x40 {
                    let name_bytes: Vec<u8> = buf[0x40..len]
                        .iter()
                        .take_while(|&&b| b != 0)
                        .copied()
                        .collect();
                    String::from_utf8_lossy(&name_bytes).to_string()
                } else {
                    String::new()
                };

                let device = DiscoveredDevice {
                    device_type: format!("0x{:04x}", devtype),
                    model: model.to_string(),
                    host,
                    mac: format_mac(&mac),
                    name: if name.is_empty() {
                        model.to_string()
                    } else {
                        name
                    },
                };

                tracing::info!(
                    "Found device: {} ({}) at {} [{}]",
                    device.model,
                    device.device_type,
                    device.host,
                    device.mac
                );

                devices.push(device);
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                {
                    break; // Timeout, done receiving
                }
                tracing::debug!("Receive error: {}", e);
                break;
            }
        }
    }

    devices
}

/// Result of a learning operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnResult {
    pub code: Option<String>,
    pub error: Option<String>,
}

/// Result of a send operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Global state for managing learn cancellation
static LEARN_CANCEL: AtomicBool = AtomicBool::new(false);

/// Get all IPv4 addresses from network interfaces (excluding loopback and virtual)
pub(crate) fn get_local_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut addresses = Vec::new();

    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in interfaces {
            if let IpAddr::V4(ipv4) = ip {
                // Skip loopback
                if ipv4.is_loopback() {
                    continue;
                }

                // Skip common virtual interface prefixes
                let name_lower = name.to_lowercase();

                // On Windows, "vEthernet (External Switch)" is the actual physical NIC
                // bridged to Hyper-V - we need to keep it for LAN access
                let is_external_switch = name_lower.contains("external switch");

                // Skip link-local addresses (169.254.x.x) — unusable for LAN broadcast
                if ipv4.is_link_local() {
                    tracing::debug!("Skipping link-local address: {} ({})", name, ipv4);
                    continue;
                }

                if !is_external_switch
                    && (name_lower.starts_with("veth")
                    || name_lower.starts_with("docker")
                    || name_lower.starts_with("br-")
                    || name_lower.starts_with("virbr")
                    || name_lower.contains("wsl")
                    || name_lower.contains("hyper-v")
                    || name_lower.contains("virtualbox")
                    || name_lower.contains("default switch")
                    // macOS VPN / tunnel interfaces — point-to-point, no broadcast subnet
                    || name_lower.starts_with("utun")
                    || name_lower.starts_with("awdl")
                    || name_lower.starts_with("llw")
                    || name_lower.starts_with("anpi")
                    || name_lower.starts_with("bridge"))
                {
                    tracing::debug!("Skipping virtual/tunnel interface: {} ({})", name, ipv4);
                    continue;
                }

                tracing::info!("Found network interface: {} ({})", name, ipv4);
                addresses.push(ipv4);
            }
        }
    }

    addresses
}

/// Discover Broadlink devices on the network by trying all interfaces
pub async fn discover_devices(timeout: u32) -> Result<Vec<DiscoveredDevice>, String> {
    let timeout_secs = timeout.max(1) as u64;

    tokio::task::spawn_blocking(move || {
        let local_ips = get_local_ipv4_addresses();

        if local_ips.is_empty() {
            return Err("No suitable network interfaces found".to_string());
        }

        tracing::info!(
            "Attempting discovery on {} network interface(s)",
            local_ips.len()
        );

        let mut all_discovered = Vec::new();
        let mut seen_macs = HashSet::new();

        // Try raw UDP discovery on each interface
        for local_ip in local_ips {
            tracing::info!("Trying raw UDP discovery on interface: {}", local_ip);

            let devices = raw_discover_on_interface(local_ip, timeout_secs);

            for device in devices {
                // Skip duplicates (device might respond on multiple interfaces)
                if seen_macs.contains(&device.mac) {
                    continue;
                }
                seen_macs.insert(device.mac.clone());
                all_discovered.push(device);
            }
        }

        if all_discovered.is_empty() {
            tracing::warn!("No Broadlink devices found on any interface");
        }

        tracing::info!("Total discovered devices: {}", all_discovered.len());
        Ok(all_discovered)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Get the best local IP for communicating with a specific device IP
fn get_local_ip_for_device(device_ip: Ipv4Addr) -> Option<Ipv4Addr> {
    let local_ips = get_local_ipv4_addresses();

    // Try to find an IP in the same subnet (simple heuristic: same first 3 octets)
    let device_octets = device_ip.octets();
    for local_ip in &local_ips {
        let local_octets = local_ip.octets();
        if local_octets[0] == device_octets[0]
            && local_octets[1] == device_octets[1]
            && local_octets[2] == device_octets[2]
        {
            return Some(*local_ip);
        }
    }

    // Fall back to first available IP
    local_ips.into_iter().next()
}

/// Enter learning mode and wait for IR/RF signal
pub async fn learn_code(
    host: &str,
    mac: &str,
    devtype: &str,
    signal_type: &str,
) -> Result<LearnResult, String> {
    // Reset cancellation flag
    LEARN_CANCEL.store(false, Ordering::SeqCst);

    let host = host.to_string();
    let mac = mac.to_string();
    let devtype = devtype.to_string();
    let signal_type = signal_type.to_string();

    tokio::task::spawn_blocking(move || {
        // Parse the IP address
        let ip: Ipv4Addr = host
            .parse()
            .map_err(|e| format!("Invalid IP address '{}': {}", host, e))?;

        // Get the best local IP for this device
        let local_ip =
            get_local_ip_for_device(ip).ok_or_else(|| "No suitable local IP found".to_string())?;
        tracing::info!("Learning from device {} using local IP {}", ip, local_ip);

        // Connect using our custom BroadlinkDevice with RM4 protocol support
        tracing::info!("Connecting to device...");
        let mut device = BroadlinkDevice::connect(&host, &mac, &devtype, local_ip)?;

        tracing::info!(
            "Connected! Starting {} learning (RM4 protocol)...",
            signal_type
        );

        // Learn based on signal type
        let code_result = if signal_type == "rf" {
            device.learn_rf()
        } else {
            device.learn_ir()
        };

        match code_result {
            Ok(code) => {
                tracing::info!("Learned code: {} bytes", code.len());
                // Convert bytes to hex string
                let hex_code = hex::encode(&code);
                Ok(LearnResult {
                    code: Some(hex_code),
                    error: None,
                })
            }
            Err(e) => {
                tracing::error!("Learning failed: {}", e);
                Ok(LearnResult {
                    code: None,
                    error: Some(e),
                })
            }
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

/// Cancel ongoing learning operation
pub async fn cancel_learn() {
    LEARN_CANCEL.store(true, Ordering::SeqCst);
}

/// Send an IR/RF code to a device
pub async fn send_code(
    host: &str,
    mac: &str,
    devtype: &str,
    code: &str,
) -> Result<SendResult, String> {
    let host = host.to_string();
    let mac = mac.to_string();
    let devtype = devtype.to_string();
    let code = code.to_string();

    tokio::task::spawn_blocking(move || {
        // Parse the IP address
        let ip: Ipv4Addr = host
            .parse()
            .map_err(|e| format!("Invalid IP address '{}': {}", host, e))?;

        // Decode the hex code
        let code_bytes = hex::decode(&code).map_err(|e| format!("Invalid hex code: {}", e))?;

        // Get the best local IP for this device
        let local_ip =
            get_local_ip_for_device(ip).ok_or_else(|| "No suitable local IP found".to_string())?;
        tracing::info!("Sending to device {} using local IP {}", ip, local_ip);

        // Connect using our custom BroadlinkDevice with RM4 protocol support
        let mut device = BroadlinkDevice::connect(&host, &mac, &devtype, local_ip)
            .map_err(|e| format!("Failed to connect to device: {}", e))?;

        // Send the code
        match device.send_code(&code_bytes) {
            Ok(_) => Ok(SendResult {
                success: true,
                error: None,
            }),
            Err(e) => Ok(SendResult {
                success: false,
                error: Some(format!("Send failed: {}", e)),
            }),
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}
