pub mod atem;
pub mod blackmagic_camera;
pub mod broadlink;
pub mod discord;
pub mod facebook;
#[cfg(target_os = "macos")]
pub mod keynote;
pub mod middlecontrol;
pub mod obs;
pub mod preset_tracker;
pub mod rodecaster;
pub mod rodecaster_audio;
pub mod vmix;
pub mod youtube;

pub use metocast_core::connectors::*;

/// A platform's own broadcast state as the one `schedule_status` vocabulary every client reads:
/// `scheduled`, `starting`, `live` or `ended`. `None` for a value we don't know, which leaves the
/// stored status as it is.
pub fn broadcast_status(platform: &str, raw: &str) -> Option<&'static str> {
    Some(match (platform, raw) {
        ("youtube", "created" | "ready")
        | ("facebook", "SCHEDULED_UNPUBLISHED" | "UNPUBLISHED" | "SCHEDULED_LIVE") => "scheduled",
        ("youtube", "testStarting" | "testing" | "liveStarting") => "starting",
        ("youtube", "live") | ("facebook", "LIVE") => "live",
        ("youtube", "complete" | "revoked")
        | (
            "facebook",
            "LIVE_STOPPED" | "PROCESSING" | "VOD" | "SCHEDULED_EXPIRED" | "SCHEDULED_CANCELED",
        ) => "ended",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::broadcast_status;

    #[test]
    fn platform_states_map_to_one_vocabulary() {
        assert_eq!(broadcast_status("youtube", "ready"), Some("scheduled"));
        assert_eq!(broadcast_status("youtube", "testing"), Some("starting"));
        assert_eq!(broadcast_status("youtube", "live"), Some("live"));
        assert_eq!(broadcast_status("youtube", "complete"), Some("ended"));
        assert_eq!(
            broadcast_status("facebook", "SCHEDULED_UNPUBLISHED"),
            Some("scheduled")
        );
        assert_eq!(broadcast_status("facebook", "LIVE"), Some("live"));
        assert_eq!(broadcast_status("facebook", "VOD"), Some("ended"));
        assert_eq!(broadcast_status("facebook", "live"), None);
        assert_eq!(broadcast_status("youtube", "mystery"), None);
    }
}
