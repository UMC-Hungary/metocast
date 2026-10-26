//! Automations: rules that run inside the core. When one device does something, Metocast does
//! something on others. These are the records that cross the wire; the engine that runs them
//! is in `metocast-server`.
//!
//! A rule fires when **any** of its triggers happens and runs **all** of its actions, in order.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The most rules the core keeps.
pub const MAX_AUTOMATIONS: usize = 50;
pub const NAME_MAX_CHARS: usize = 80;
/// A rule with more triggers or actions than this is a mistake, not a setup.
pub const MAX_TRIGGERS: usize = 20;
pub const MAX_ACTIONS: usize = 40;
pub const PRESETS: std::ops::RangeInclusive<u8> = 1..=12;
pub const CAMERAS: std::ops::RangeInclusive<u8> = 1..=99;
pub const HOLD_SECONDS: std::ops::RangeInclusive<f32> = 0.0..=60.0;
pub const COOLDOWN_SECONDS: std::ops::RangeInclusive<f32> = 1.0..=600.0;
pub const DEFAULT_COOLDOWN_SECONDS: f32 = 5.0;
/// How long a reversed rule waits for a channel to stay silent before it stops recording.
pub const REVERSE_HOLD_SECONDS: f32 = 5.0;
/// The highest fader slot a trigger can name.
pub const MAX_CHANNEL: u32 = 63;

pub(crate) const fn default_true() -> bool {
    true
}

const fn default_cooldown() -> f32 {
    DEFAULT_COOLDOWN_SECONDS
}

/// Which mute counts when a channel trigger decides the channel is live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelSource {
    /// Live only while both the desk and the Wireless PRO transmitter are unmuted.
    #[default]
    Either,
    /// Only the desk button, or Metocast's own mute toggle.
    Desk,
    /// Only the Wireless PRO transmitter's button.
    Transmitter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AutomationTrigger {
    /// A Middle Control preset's move finished, wherever it was clicked. With no camera, any
    /// camera matches.
    #[serde(rename = "middlecontrol.preset", rename_all = "camelCase")]
    MiddlecontrolPreset { preset: u8, camera_id: Option<u8> },
    /// A RØDECaster channel went live or silent, and stayed so for `hold_seconds`.
    #[serde(rename = "rodecaster.channel", rename_all = "camelCase")]
    RodecasterChannel {
        /// The zero-based fader slot, as in `rodecaster.mute.set`.
        channel: u32,
        /// A snapshot of the channel's name, for display only.
        #[serde(default)]
        label: String,
        live: bool,
        #[serde(default)]
        source: ChannelSource,
        #[serde(default)]
        hold_seconds: f32,
    },
}

/// What a `record.start` or `record.stop` action drives. The names match the connectors'.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "device")]
pub enum RecordTarget {
    #[serde(rename = "obs")]
    Obs,
    #[serde(rename = "atem")]
    Atem,
    #[serde(rename = "blackmagic-camera")]
    BlackmagicCamera,
    /// Cameras recorded through Middle Control. With no camera, every camera.
    #[serde(rename = "middlecontrol", rename_all = "camelCase")]
    Middlecontrol { camera_id: Option<u8> },
    /// The desk's own SD or USB recorder.
    #[serde(rename = "rodecaster")]
    Rodecaster,
    /// Metocast's FLAC recording of the desk's USB audio.
    #[serde(rename = "rodecaster-audio")]
    RodecasterAudio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum AutomationAction {
    #[serde(rename = "record.start")]
    RecordStart { target: RecordTarget },
    #[serde(rename = "record.stop")]
    RecordStop { target: RecordTarget },
    /// Moves a camera to a Middle Control preset. With no camera, the selected one. It is
    /// skipped when that same preset is what fired the rule.
    #[serde(rename = "middlecontrol.preset.recall", rename_all = "camelCase")]
    PresetRecall {
        preset: u8,
        camera_id: Option<u8>,
        /// The actions after this one wait until the camera arrives.
        #[serde(default = "default_true")]
        wait_for_arrival: bool,
    },
}

/// A rule as a client writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationDraft {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub triggers: Vec<AutomationTrigger>,
    pub actions: Vec<AutomationAction>,
    /// Re-fires of the same rule inside this window are ignored.
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: f32,
    /// The rule fires only when the server has a current event today.
    #[serde(default)]
    pub require_event: bool,
}

/// A rule as the core stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub triggers: Vec<AutomationTrigger>,
    pub actions: Vec<AutomationAction>,
    pub cooldown_seconds: f32,
    pub require_event: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `GET /api/automations` and `automations.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationList {
    /// The master switch: nothing fires while it is off.
    pub armed: bool,
    pub automations: Vec<Automation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionOutcome {
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    /// What the action was, in words: "Start OBS", "Move camera to preset 3".
    pub action: String,
    pub outcome: ActionOutcome,
    /// Why it was skipped, or a connector's error code or the device's own words.
    pub message: Option<String>,
}

/// One run of a rule: what fired it and what each action came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRun {
    pub id: Uuid,
    pub automation_id: Uuid,
    pub name: String,
    /// What fired it, in words: "Preset 3 finished", "Wireless 1 went live".
    pub cause: String,
    /// "Run now" from a client, which ignores the cooldown, the event check and the master
    /// switch.
    pub manual: bool,
    pub started_at: DateTime<Utc>,
    pub results: Vec<ActionResult>,
}

impl AutomationRun {
    #[must_use]
    pub fn failed(&self) -> bool {
        self.results
            .iter()
            .any(|result| result.outcome == ActionOutcome::Failed)
    }
}

fn seconds(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

fn camera_suffix(camera_id: Option<u8>) -> String {
    camera_id.map_or_else(String::new, |id| format!(" on camera {id}"))
}

impl RecordTarget {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Obs => "OBS".to_string(),
            Self::Atem => "ATEM".to_string(),
            Self::BlackmagicCamera => "Blackmagic camera".to_string(),
            Self::Middlecontrol { camera_id: None } => "all Middle Control cameras".to_string(),
            Self::Middlecontrol {
                camera_id: Some(id),
            } => format!("Middle Control camera {id}"),
            Self::Rodecaster => "RØDECaster SD".to_string(),
            Self::RodecasterAudio => "Metocast audio".to_string(),
        }
    }
}

impl AutomationAction {
    /// The action in words, as a run reports it.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::RecordStart { target } => format!("Start {}", target.label()),
            Self::RecordStop { target } => format!("Stop {}", target.label()),
            Self::PresetRecall {
                preset, camera_id, ..
            } => format!(
                "Move camera to preset {preset}{}",
                camera_suffix(*camera_id)
            ),
        }
    }
}

impl AutomationTrigger {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::MiddlecontrolPreset { preset, camera_id } => {
                format!("Preset {preset} finished{}", camera_suffix(*camera_id))
            }
            Self::RodecasterChannel {
                channel,
                label,
                live,
                hold_seconds,
                ..
            } => {
                let name = if label.trim().is_empty() {
                    format!("Channel {}", channel + 1)
                } else {
                    label.clone()
                };
                let what = if *live { "goes live" } else { "goes silent" };
                if *hold_seconds > 0.0 {
                    format!("{name} {what} for {} s", seconds(*hold_seconds))
                } else {
                    format!("{name} {what}")
                }
            }
        }
    }
}

fn check_preset(preset: u8) -> Result<(), String> {
    PRESETS
        .contains(&preset)
        .then_some(())
        .ok_or_else(|| "preset must be between 1 and 12".to_string())
}

fn check_camera(camera_id: Option<u8>) -> Result<(), String> {
    camera_id.map_or(Ok(()), |id| {
        CAMERAS
            .contains(&id)
            .then_some(())
            .ok_or_else(|| "camera_id must be between 1 and 99".to_string())
    })
}

impl AutomationTrigger {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::MiddlecontrolPreset { preset, camera_id } => {
                check_preset(*preset)?;
                check_camera(*camera_id)
            }
            Self::RodecasterChannel {
                channel,
                label,
                hold_seconds,
                ..
            } => {
                if *channel > MAX_CHANNEL {
                    return Err(format!("channel must be between 0 and {MAX_CHANNEL}"));
                }
                if label.chars().count() > NAME_MAX_CHARS {
                    return Err(format!("label must be at most {NAME_MAX_CHARS} characters"));
                }
                if !HOLD_SECONDS.contains(hold_seconds) {
                    return Err("holdSeconds must be between 0 and 60".to_string());
                }
                Ok(())
            }
        }
    }
}

impl AutomationAction {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::RecordStart { target } | Self::RecordStop { target } => match target {
                RecordTarget::Middlecontrol { camera_id } => check_camera(*camera_id),
                _ => Ok(()),
            },
            Self::PresetRecall {
                preset, camera_id, ..
            } => {
                check_preset(*preset)?;
                check_camera(*camera_id)
            }
        }
    }
}

impl AutomationDraft {
    /// The rule's limits, so the server and every editor refuse the same things.
    ///
    /// # Errors
    /// The first limit the rule breaks, in words.
    pub fn validate(&self) -> Result<(), String> {
        let name_chars = self.name.trim().chars().count();
        if name_chars == 0 || name_chars > NAME_MAX_CHARS {
            return Err(format!("name must be 1 to {NAME_MAX_CHARS} characters"));
        }
        if self.triggers.is_empty() {
            return Err("at least one trigger is required".to_string());
        }
        if self.triggers.len() > MAX_TRIGGERS {
            return Err(format!("at most {MAX_TRIGGERS} triggers"));
        }
        if self.actions.is_empty() {
            return Err("at least one action is required".to_string());
        }
        if self.actions.len() > MAX_ACTIONS {
            return Err(format!("at most {MAX_ACTIONS} actions"));
        }
        for trigger in &self.triggers {
            trigger.validate()?;
        }
        for action in &self.actions {
            action.validate()?;
        }
        if !COOLDOWN_SECONDS.contains(&self.cooldown_seconds) {
            return Err("cooldownSeconds must be between 1 and 600".to_string());
        }
        Ok(())
    }

    /// The opposite rule: the actions in reverse order with start and stop swapped, so a rule
    /// that moves the camera and then starts recording becomes one that stops recording and
    /// then moves the camera.
    ///
    /// - A channel trigger flips: one that went live now goes silent for at least
    ///   [`REVERSE_HOLD_SECONDS`], one that went silent now goes live.
    /// - Preset triggers are dropped, because the opposite of "preset 3 finished" is a preset
    ///   the person picks.
    /// - Every camera move goes to `camera_preset`, or is left out when there is none.
    #[must_use]
    pub fn reversed(&self, camera_preset: Option<u8>) -> Self {
        let triggers = self
            .triggers
            .iter()
            .filter_map(|trigger| match trigger {
                AutomationTrigger::MiddlecontrolPreset { .. } => None,
                AutomationTrigger::RodecasterChannel {
                    channel,
                    label,
                    live,
                    source,
                    hold_seconds,
                } => Some(AutomationTrigger::RodecasterChannel {
                    channel: *channel,
                    label: label.clone(),
                    live: !live,
                    source: *source,
                    hold_seconds: if *live {
                        hold_seconds.max(REVERSE_HOLD_SECONDS)
                    } else {
                        0.0
                    },
                }),
            })
            .collect();
        let actions = self
            .actions
            .iter()
            .rev()
            .filter_map(|action| match action {
                AutomationAction::RecordStart { target } => Some(AutomationAction::RecordStop {
                    target: target.clone(),
                }),
                AutomationAction::RecordStop { target } => Some(AutomationAction::RecordStart {
                    target: target.clone(),
                }),
                AutomationAction::PresetRecall {
                    camera_id,
                    wait_for_arrival,
                    ..
                } => camera_preset.map(|preset| AutomationAction::PresetRecall {
                    preset,
                    camera_id: *camera_id,
                    wait_for_arrival: *wait_for_arrival,
                }),
            })
            .collect();
        Self {
            name: format!("{} (reverse)", self.name.trim()),
            enabled: self.enabled,
            triggers,
            actions,
            cooldown_seconds: self.cooldown_seconds,
            require_event: self.require_event,
        }
    }
}

impl Automation {
    #[must_use]
    pub fn draft(&self) -> AutomationDraft {
        AutomationDraft {
            name: self.name.clone(),
            enabled: self.enabled,
            triggers: self.triggers.clone(),
            actions: self.actions.clone(),
            cooldown_seconds: self.cooldown_seconds,
            require_event: self.require_event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn service_start() -> AutomationDraft {
        AutomationDraft {
            name: "Service start".to_string(),
            enabled: true,
            triggers: vec![
                AutomationTrigger::MiddlecontrolPreset {
                    preset: 3,
                    camera_id: None,
                },
                AutomationTrigger::RodecasterChannel {
                    channel: 2,
                    label: "Wireless 1".to_string(),
                    live: true,
                    source: ChannelSource::Either,
                    hold_seconds: 0.0,
                },
            ],
            actions: vec![
                AutomationAction::PresetRecall {
                    preset: 3,
                    camera_id: None,
                    wait_for_arrival: true,
                },
                AutomationAction::RecordStart {
                    target: RecordTarget::Obs,
                },
                AutomationAction::RecordStart {
                    target: RecordTarget::Atem,
                },
                AutomationAction::RecordStart {
                    target: RecordTarget::RodecasterAudio,
                },
            ],
            cooldown_seconds: 5.0,
            require_event: false,
        }
    }

    #[test]
    fn the_wire_format_is_the_documented_json() {
        let draft = service_start();
        assert_eq!(
            serde_json::to_value(&draft).unwrap(),
            json!({
                "name": "Service start",
                "enabled": true,
                "triggers": [
                    { "kind": "middlecontrol.preset", "preset": 3, "cameraId": null },
                    {
                        "kind": "rodecaster.channel", "channel": 2, "label": "Wireless 1",
                        "live": true, "source": "either", "holdSeconds": 0.0
                    }
                ],
                "actions": [
                    {
                        "kind": "middlecontrol.preset.recall", "preset": 3,
                        "cameraId": null, "waitForArrival": true
                    },
                    { "kind": "record.start", "target": { "device": "obs" } },
                    { "kind": "record.start", "target": { "device": "atem" } },
                    { "kind": "record.start", "target": { "device": "rodecaster-audio" } }
                ],
                "cooldownSeconds": 5.0,
                "requireEvent": false
            })
        );
    }

    #[test]
    fn every_record_target_has_its_connectors_name() {
        let names = [
            (RecordTarget::Obs, json!({"device": "obs"})),
            (RecordTarget::Atem, json!({"device": "atem"})),
            (
                RecordTarget::BlackmagicCamera,
                json!({"device": "blackmagic-camera"}),
            ),
            (
                RecordTarget::Middlecontrol { camera_id: Some(2) },
                json!({"device": "middlecontrol", "cameraId": 2}),
            ),
            (RecordTarget::Rodecaster, json!({"device": "rodecaster"})),
            (
                RecordTarget::RodecasterAudio,
                json!({"device": "rodecaster-audio"}),
            ),
        ];
        for (target, wire) in names {
            assert_eq!(serde_json::to_value(&target).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<RecordTarget>(wire).unwrap(),
                target
            );
        }
    }

    #[test]
    fn a_rule_written_with_only_the_required_fields_gets_its_defaults() {
        let draft: AutomationDraft = serde_json::from_value(json!({
            "name": "Service end",
            "triggers": [{ "kind": "middlecontrol.preset", "preset": 1 }],
            "actions": [
                { "kind": "record.stop", "target": { "device": "obs" } },
                { "kind": "middlecontrol.preset.recall", "preset": 1, "cameraId": null }
            ]
        }))
        .unwrap();
        assert!(draft.enabled);
        assert!(!draft.require_event);
        assert!((draft.cooldown_seconds - 5.0).abs() < f32::EPSILON);
        assert!(matches!(
            draft.actions[1],
            AutomationAction::PresetRecall {
                wait_for_arrival: true,
                ..
            }
        ));
        assert!(draft.validate().is_ok());
    }

    #[test]
    fn a_channel_trigger_needs_no_label() {
        let trigger: AutomationTrigger = serde_json::from_value(json!({
            "kind": "rodecaster.channel", "channel": 4, "live": true
        }))
        .unwrap();
        assert_eq!(trigger.label(), "Channel 5 goes live");
    }

    #[test]
    fn a_channel_trigger_defaults_to_either_source_and_no_hold() {
        let trigger: AutomationTrigger = serde_json::from_value(json!({
            "kind": "rodecaster.channel", "channel": 0, "label": "Host", "live": false
        }))
        .unwrap();
        assert_eq!(
            trigger,
            AutomationTrigger::RodecasterChannel {
                channel: 0,
                label: "Host".to_string(),
                live: false,
                source: ChannelSource::Either,
                hold_seconds: 0.0,
            }
        );
    }

    #[test]
    fn a_run_serialises_with_outcomes_in_lowercase() {
        let run = AutomationRun {
            id: Uuid::nil(),
            automation_id: Uuid::nil(),
            name: "Service start".to_string(),
            cause: "Preset 3 finished".to_string(),
            manual: false,
            started_at: "2026-10-07T08:00:00Z".parse().unwrap(),
            results: vec![
                ActionResult {
                    action: "Start OBS".to_string(),
                    outcome: ActionOutcome::Failed,
                    message: Some("obs_not_connected".to_string()),
                },
                ActionResult {
                    action: "Start ATEM".to_string(),
                    outcome: ActionOutcome::Skipped,
                    message: None,
                },
            ],
        };
        assert!(run.failed());
        assert_eq!(
            serde_json::to_value(&run).unwrap(),
            json!({
                "id": "00000000-0000-0000-0000-000000000000",
                "automationId": "00000000-0000-0000-0000-000000000000",
                "name": "Service start",
                "cause": "Preset 3 finished",
                "manual": false,
                "startedAt": "2026-10-07T08:00:00Z",
                "results": [
                    { "action": "Start OBS", "outcome": "failed", "message": "obs_not_connected" },
                    { "action": "Start ATEM", "outcome": "skipped", "message": null }
                ]
            })
        );
    }

    #[test]
    fn a_good_rule_validates() {
        assert_eq!(service_start().validate(), Ok(()));
    }

    #[test]
    fn each_limit_is_refused_in_words() {
        let refused = |change: &dyn Fn(&mut AutomationDraft)| {
            let mut draft = service_start();
            change(&mut draft);
            draft.validate().unwrap_err()
        };

        assert!(refused(&|d| d.name = "   ".to_string()).contains("name"));
        assert!(refused(&|d| d.name = "x".repeat(81)).contains("name"));
        assert!(refused(&|d| d.triggers.clear()).contains("trigger"));
        assert!(refused(&|d| d.actions.clear()).contains("action"));
        assert_eq!(
            refused(&|d| {
                d.triggers = vec![AutomationTrigger::MiddlecontrolPreset {
                    preset: 13,
                    camera_id: None,
                }];
            }),
            "preset must be between 1 and 12"
        );
        assert_eq!(
            refused(&|d| {
                d.triggers = vec![AutomationTrigger::MiddlecontrolPreset {
                    preset: 0,
                    camera_id: None,
                }];
            }),
            "preset must be between 1 and 12"
        );
        assert_eq!(
            refused(&|d| {
                d.triggers = vec![AutomationTrigger::MiddlecontrolPreset {
                    preset: 3,
                    camera_id: Some(100),
                }];
            }),
            "camera_id must be between 1 and 99"
        );
        assert_eq!(
            refused(&|d| {
                d.actions = vec![AutomationAction::PresetRecall {
                    preset: 13,
                    camera_id: None,
                    wait_for_arrival: true,
                }];
            }),
            "preset must be between 1 and 12"
        );
        assert_eq!(
            refused(&|d| {
                d.actions = vec![AutomationAction::RecordStart {
                    target: RecordTarget::Middlecontrol { camera_id: Some(0) },
                }];
            }),
            "camera_id must be between 1 and 99"
        );
        assert!(
            refused(&|d| {
                d.triggers = vec![AutomationTrigger::RodecasterChannel {
                    channel: 2,
                    label: String::new(),
                    live: true,
                    source: ChannelSource::Either,
                    hold_seconds: 61.0,
                }];
            })
            .contains("holdSeconds")
        );
        assert!(refused(&|d| d.cooldown_seconds = 0.5).contains("cooldownSeconds"));
        assert!(refused(&|d| d.cooldown_seconds = 601.0).contains("cooldownSeconds"));
        assert!(refused(&|d| d.cooldown_seconds = f32::NAN).contains("cooldownSeconds"));
    }

    #[test]
    fn the_limits_are_inclusive() {
        let mut draft = service_start();
        draft.name = "x".repeat(80);
        draft.cooldown_seconds = 600.0;
        draft.triggers = vec![
            AutomationTrigger::MiddlecontrolPreset {
                preset: 12,
                camera_id: Some(99),
            },
            AutomationTrigger::RodecasterChannel {
                channel: 0,
                label: String::new(),
                live: false,
                source: ChannelSource::Desk,
                hold_seconds: 60.0,
            },
        ];
        assert_eq!(draft.validate(), Ok(()));
        draft.cooldown_seconds = 1.0;
        assert_eq!(draft.validate(), Ok(()));
    }

    #[test]
    fn labels_read_as_words() {
        let draft = service_start();
        assert_eq!(draft.triggers[0].label(), "Preset 3 finished");
        assert_eq!(draft.triggers[1].label(), "Wireless 1 goes live");
        assert_eq!(draft.actions[0].label(), "Move camera to preset 3");
        assert_eq!(draft.actions[3].label(), "Start Metocast audio");
        assert_eq!(
            AutomationTrigger::RodecasterChannel {
                channel: 4,
                label: " ".to_string(),
                live: false,
                source: ChannelSource::Either,
                hold_seconds: 5.0,
            }
            .label(),
            "Channel 5 goes silent for 5 s"
        );
        assert_eq!(
            AutomationAction::RecordStop {
                target: RecordTarget::Middlecontrol { camera_id: Some(2) },
            }
            .label(),
            "Stop Middle Control camera 2"
        );
        assert_eq!(
            AutomationAction::PresetRecall {
                preset: 1,
                camera_id: Some(2),
                wait_for_arrival: false,
            }
            .label(),
            "Move camera to preset 1 on camera 2"
        );
    }

    #[test]
    fn the_reverse_of_a_start_rule_stops_recording_before_it_moves_the_camera() {
        let reversed = service_start().reversed(Some(1));

        assert_eq!(reversed.name, "Service start (reverse)");
        assert_eq!(
            reversed.triggers,
            vec![AutomationTrigger::RodecasterChannel {
                channel: 2,
                label: "Wireless 1".to_string(),
                live: false,
                source: ChannelSource::Either,
                hold_seconds: 5.0,
            }]
        );
        assert_eq!(
            reversed.actions,
            vec![
                AutomationAction::RecordStop {
                    target: RecordTarget::RodecasterAudio
                },
                AutomationAction::RecordStop {
                    target: RecordTarget::Atem
                },
                AutomationAction::RecordStop {
                    target: RecordTarget::Obs
                },
                AutomationAction::PresetRecall {
                    preset: 1,
                    camera_id: None,
                    wait_for_arrival: true
                },
            ]
        );
        assert_eq!(reversed.validate(), Ok(()));
    }

    #[test]
    fn the_reverse_without_a_preset_leaves_the_camera_alone() {
        let reversed = service_start().reversed(None);
        assert_eq!(reversed.actions.len(), 3);
        assert!(
            reversed
                .actions
                .iter()
                .all(|action| matches!(action, AutomationAction::RecordStop { .. }))
        );
    }

    #[test]
    fn reversing_a_stop_rule_makes_a_start_rule_with_no_hold() {
        let mut stop = service_start();
        stop.triggers = vec![AutomationTrigger::RodecasterChannel {
            channel: 2,
            label: "Wireless 1".to_string(),
            live: false,
            source: ChannelSource::Desk,
            hold_seconds: 8.0,
        }];
        stop.actions = vec![AutomationAction::RecordStop {
            target: RecordTarget::Obs,
        }];

        let reversed = stop.reversed(None);
        assert_eq!(
            reversed.triggers,
            vec![AutomationTrigger::RodecasterChannel {
                channel: 2,
                label: "Wireless 1".to_string(),
                live: true,
                source: ChannelSource::Desk,
                hold_seconds: 0.0,
            }]
        );
        assert_eq!(
            reversed.actions,
            vec![AutomationAction::RecordStart {
                target: RecordTarget::Obs
            }]
        );
    }

    #[test]
    fn a_long_hold_survives_the_reverse() {
        let mut start = service_start();
        start.triggers = vec![AutomationTrigger::RodecasterChannel {
            channel: 1,
            label: String::new(),
            live: true,
            source: ChannelSource::Either,
            hold_seconds: 12.0,
        }];
        let reversed = start.reversed(None);
        assert!(matches!(
            reversed.triggers[0],
            AutomationTrigger::RodecasterChannel { hold_seconds, .. } if (hold_seconds - 12.0).abs() < f32::EPSILON
        ));
    }

    #[test]
    fn a_reversed_rule_with_only_preset_triggers_has_none_until_one_is_picked() {
        let mut start = service_start();
        start.triggers = vec![AutomationTrigger::MiddlecontrolPreset {
            preset: 3,
            camera_id: None,
        }];
        let reversed = start.reversed(Some(1));
        assert!(reversed.triggers.is_empty());
        assert!(reversed.validate().unwrap_err().contains("trigger"));
    }

    #[test]
    fn a_stored_rule_hands_back_its_draft() {
        let draft = service_start();
        let stored = Automation {
            id: Uuid::nil(),
            name: draft.name.clone(),
            enabled: draft.enabled,
            triggers: draft.triggers.clone(),
            actions: draft.actions.clone(),
            cooldown_seconds: draft.cooldown_seconds,
            require_event: draft.require_event,
            created_at: "2026-10-07T08:00:00Z".parse().unwrap(),
            updated_at: "2026-10-07T08:00:00Z".parse().unwrap(),
        };
        assert_eq!(stored.draft(), draft);
    }
}
