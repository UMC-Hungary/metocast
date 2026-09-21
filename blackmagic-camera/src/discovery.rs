//! mDNS/Bonjour discovery.
//!
//! Manual add by host/IP is the equally-supported path — on VLANs, VPNs and
//! Wi-Fi-isolated networks mDNS simply does not arrive. Discovery returning
//! nothing is a normal outcome, never an error.

use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

/// Verified against a Cinema Camera 6K (firmware 10.2.2): Blackmagic cameras
/// advertise on the generic `_http._tcp`, not a vendor-specific type. They are told
/// apart from every other web server on the LAN by their TXT records — see [`identify`].
pub const DEFAULT_SERVICE: &str = "_http._tcp.local.";

/// TXT keys a Blackmagic camera always carries. A plain web server (a gate opener, a
/// NAS) advertises `_http._tcp` with no TXT at all, so this is a cheap, reliable filter.
const REQUIRED_TXT: [&str; 2] = ["product name", "unique id"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    /// e.g. `Cinema-Camera-6K.local.` — the camera's configured name with spaces
    /// turned into dashes. Renaming the camera changes this.
    pub hostname: String,
    pub addresses: Vec<String>,
    pub port: u16,
    /// From TXT, so a multi-camera list can be labelled without any REST call.
    pub device_name: String,
    pub product_name: String,
    /// Stable across renames and DHCP — the right key for persisting a camera.
    pub unique_id: String,
    pub software_version: String,
}

impl Discovered {
    /// What to hand [`crate::Camera::connect`], carrying the advertised port and scheme.
    /// Prefers the `.local` name over the IP: it survives DHCP, and it is how
    /// Blackmagic's own documentation addresses cameras.
    pub fn host(&self) -> String {
        let host = self.hostname.trim_end_matches('.');
        match self.port {
            443 => host.to_string(),
            80 => format!("http://{host}"),
            port => format!("http://{host}:{port}"),
        }
    }
}

/// `Some(camera)` if this service advert is a Blackmagic camera, `None` for any other
/// `_http._tcp` device on the network.
fn identify(info: &ServiceInfo) -> Option<Discovered> {
    // mdns-sd lowercases TXT keys; compare case-insensitively regardless.
    let txt = |key: &str| -> Option<String> {
        info.get_properties()
            .iter()
            .find(|p| p.key().eq_ignore_ascii_case(key))
            .map(|p| p.val_str().to_string())
    };

    if !REQUIRED_TXT.iter().all(|k| txt(k).is_some()) {
        return None;
    }

    Some(Discovered {
        hostname: info.get_hostname().to_string(),
        addresses: info.get_addresses().iter().map(|a| a.to_string()).collect(),
        port: info.get_port(),
        device_name: txt("device name").unwrap_or_default(),
        product_name: txt("product name").unwrap_or_default(),
        unique_id: txt("unique id").unwrap_or_default(),
        // Cameras carry both; the user-facing one is `release version` (e.g. 10.2.2).
        software_version: txt("release version")
            .or_else(|| txt("sw version"))
            .unwrap_or_default(),
    })
}

/// Every service instance resolved, unfiltered, as readable lines. For diagnosing a
/// site where `browse` comes back empty but the camera is demonstrably on the network.
pub async fn browse_raw(service: &str, timeout: Duration) -> Result<Vec<String>, String> {
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let receiver = daemon.browse(service).map_err(|e| e.to_string())?;

    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;

    while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, receiver.recv_async()).await {
        let line = match event {
            ServiceEvent::ServiceResolved(info) => format!(
                "resolved {} at {}:{} txt{:?}",
                info.get_fullname(),
                info.get_hostname(),
                info.get_port(),
                info.get_properties()
                    .iter()
                    .map(|p| format!("{}={}", p.key(), p.val_str()))
                    .collect::<Vec<_>>()
            ),
            ServiceEvent::ServiceFound(ty, name) => format!("found    {name} ({ty})"),
            other => format!("event    {other:?}"),
        };
        seen.push(line);
    }

    let _ = daemon.shutdown();
    Ok(seen)
}

pub async fn browse(service: &str, timeout: Duration) -> Result<Vec<Discovered>, String> {
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let receiver = daemon.browse(service).map_err(|e| e.to_string())?;

    let mut found: Vec<Discovered> = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;

    while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, receiver.recv_async()).await {
        if let ServiceEvent::ServiceResolved(info) = event
            && let Some(camera) = identify(&info)
            && !found.contains(&camera)
        {
            found.push(camera);
        }
    }

    let _ = daemon.shutdown();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(port: u16) -> Discovered {
        Discovered {
            hostname: "Cinema-Camera-6K.local.".into(),
            addresses: vec!["192.168.0.27".into()],
            port,
            device_name: "Cinema Camera 6K".into(),
            product_name: "Cinema Camera 6K".into(),
            unique_id: "DD04938CACD141FF9D01D08748C8EB9E".into(),
            software_version: "10.2.2".into(),
        }
    }

    #[test]
    fn host_carries_the_advertised_scheme_and_port() {
        // The real camera advertises :80, so discovery must not hand back an https URL.
        assert_eq!(camera(80).host(), "http://Cinema-Camera-6K.local");
        assert_eq!(camera(443).host(), "Cinema-Camera-6K.local");
        assert_eq!(camera(8080).host(), "http://Cinema-Camera-6K.local:8080");
    }
}
