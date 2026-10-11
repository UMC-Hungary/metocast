//! The decisions automations make, as pure functions so they can be tested without a device:
//! whether a trigger matches, whether a recorder needs a request, which actions run together.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use metocast_core::automations::{AutomationAction, AutomationTrigger, ChannelSource};
use uuid::Uuid;

/// A chain of rules, each fired by the move before it, stops after this many.
pub const MAX_CHAIN: usize = 3;

/// What a recorder is doing, as far as the core can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    Recording,
    Idle,
    Stopping,
    /// Not known, or neither: a request does something either way.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Act,
    Skip(&'static str),
}

/// Whether a recorder needs the request. Starting one that is recording, or stopping one that
/// is idle or already stopping, is skipped: that is what keeps OBS's "output already active"
/// error and a double toggle out of a run. A state nobody knows gets the request.
#[must_use]
pub fn decide(observed: Observed, start: bool) -> Decision {
    match (observed, start) {
        (Observed::Recording, true) => Decision::Skip("already recording"),
        (Observed::Idle, false) => Decision::Skip("not recording"),
        (Observed::Stopping, false) => Decision::Skip("already stopping"),
        _ => Decision::Act,
    }
}

/// What fired a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    /// A preset's move finished.
    Preset { preset: u8, camera_id: u8 },
    /// A channel went live or silent.
    Channel { label: String, live: bool },
    /// "Run now".
    Manual,
}

impl Cause {
    /// In words, as a run reports it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Preset { preset, camera_id } => {
                format!("Preset {preset} finished on camera {camera_id}")
            }
            Self::Channel { label, live } => {
                format!("{label} went {}", if *live { "live" } else { "silent" })
            }
            Self::Manual => "Run now".to_string(),
        }
    }
}

/// A finished preset move fires a trigger for that preset on that camera, or on any camera
/// when the trigger names none.
#[must_use]
pub fn preset_trigger_matches(trigger: &AutomationTrigger, preset: u8, camera_id: u8) -> bool {
    matches!(
        trigger,
        AutomationTrigger::MiddlecontrolPreset { preset: wanted, camera_id: camera }
            if *wanted == preset && camera.is_none_or(|id| id == camera_id)
    )
}

/// A rule already in the chain that caused an event doesn't fire again, and a chain stops
/// after [`MAX_CHAIN`] rules: one preset recall can set off another rule's trigger, but not
/// round and round.
#[must_use]
pub fn chain_allows(rule: Uuid, chain: &[Uuid]) -> bool {
    chain.len() < MAX_CHAIN && !chain.contains(&rule)
}

/// Whether a rule that fired `last` ago is still cooling down.
#[must_use]
pub fn cooling(last: Option<Instant>, now: Instant, cooldown_seconds: f32) -> bool {
    last.is_some_and(|last| {
        now.saturating_duration_since(last) < Duration::from_secs_f32(cooldown_seconds.max(0.0))
    })
}

/// A channel's two mutes: the desk's, and the Wireless PRO transmitter's. `true` is muted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mutes {
    pub desk: bool,
    pub transmitter: bool,
}

impl Mutes {
    /// Whether the channel is live by the mute a trigger goes by.
    #[must_use]
    pub fn live(self, source: ChannelSource) -> bool {
        match source {
            ChannelSource::Either => !self.desk && !self.transmitter,
            ChannelSource::Desk => !self.desk,
            ChannelSource::Transmitter => !self.transmitter,
        }
    }
}

/// A channel trigger fires when its channel's mutes change so that it goes live (or silent) by
/// the mute the trigger goes by. A transmitter unmute while the desk is muted is no change to
/// `either`: the channel is still silent.
#[must_use]
pub fn channel_trigger_fires(
    trigger: &AutomationTrigger,
    channel: usize,
    before: Mutes,
    after: Mutes,
) -> bool {
    matches!(
        trigger,
        AutomationTrigger::RodecasterChannel { channel: wanted, live, source, .. }
            if usize::try_from(*wanted).is_ok_and(|wanted| wanted == channel)
                && before.live(*source) != after.live(*source)
                && after.live(*source) == *live
    )
}

/// After a hold: the channel is still as the trigger said.
#[must_use]
pub fn still_holds(trigger: &AutomationTrigger, mutes: Mutes) -> bool {
    matches!(
        trigger,
        AutomationTrigger::RodecasterChannel { live, source, .. } if mutes.live(*source) == *live
    )
}

/// The mutes the engine believes each channel has, seeded from the desk's profile, which says
/// nothing happened, and moved by mute events, which say it did.
#[derive(Debug, Default)]
pub struct ChannelMemory {
    channels: HashMap<usize, Mutes>,
}

impl ChannelMemory {
    /// Replaces everything: the desk (re)connected or resent its state.
    pub fn seed(&mut self, channels: impl IntoIterator<Item = (usize, Mutes)>) {
        self.channels = channels.into_iter().collect();
    }

    /// Records a mute and returns the channel's mutes before and after. A channel the engine
    /// has no state for yields `None`: there is no telling what changed.
    pub fn apply(&mut self, channel: usize, muted: bool, remote: bool) -> Option<(Mutes, Mutes)> {
        let known = self.channels.contains_key(&channel);
        let mutes = self.channels.entry(channel).or_default();
        let before = *mutes;
        if remote {
            mutes.transmitter = muted;
        } else {
            mutes.desk = muted;
        }
        known.then_some((before, *mutes))
    }

    #[must_use]
    pub fn get(&self, channel: usize) -> Option<Mutes> {
        self.channels.get(&channel).copied()
    }
}

/// A rule's actions as the worker runs them.
#[derive(Debug, Clone, PartialEq)]
pub enum Step<'a> {
    /// A camera move, run alone: what comes after may have to wait for it.
    Recall(&'a AutomationAction),
    /// Recorder actions next to each other, started together so their files begin within a
    /// fraction of a second of each other and one dead device holds up none.
    Recorders(Vec<&'a AutomationAction>),
}

#[must_use]
pub fn steps(actions: &[AutomationAction]) -> Vec<Step<'_>> {
    let mut steps: Vec<Step<'_>> = Vec::new();
    for action in actions {
        match action {
            AutomationAction::PresetRecall { .. } => steps.push(Step::Recall(action)),
            AutomationAction::RecordStart { .. } | AutomationAction::RecordStop { .. } => {
                if let Some(Step::Recorders(group)) = steps.last_mut() {
                    group.push(action);
                } else {
                    steps.push(Step::Recorders(vec![action]));
                }
            }
        }
    }
    steps
}

/// A camera move is skipped when that preset's move is what fired the rule: the camera is
/// already there.
#[must_use]
pub fn recall_is_redundant(action: &AutomationAction, cause: &Cause) -> bool {
    matches!(
        (action, cause),
        (
            AutomationAction::PresetRecall { preset, camera_id, .. },
            Cause::Preset { preset: fired, camera_id: fired_on },
        ) if preset == fired && camera_id.is_none_or(|id| id == *fired_on)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use metocast_core::automations::RecordTarget;

    fn channel(live: bool, source: ChannelSource) -> AutomationTrigger {
        AutomationTrigger::RodecasterChannel {
            channel: 2,
            label: "Wireless 1".to_string(),
            live,
            source,
            hold_seconds: 0.0,
        }
    }

    fn mutes(desk: bool, transmitter: bool) -> Mutes {
        Mutes { desk, transmitter }
    }

    fn recall(preset: u8, camera_id: Option<u8>) -> AutomationAction {
        AutomationAction::PresetRecall {
            preset,
            camera_id,
            wait_for_arrival: true,
        }
    }

    fn start(target: RecordTarget) -> AutomationAction {
        AutomationAction::RecordStart { target }
    }

    fn stop(target: RecordTarget) -> AutomationAction {
        AutomationAction::RecordStop { target }
    }

    #[test]
    fn decide_skips_what_is_already_so_and_acts_on_the_rest() {
        use Decision::{Act, Skip};
        use Observed::{Idle, Recording, Stopping, Unknown};

        // Start: only a recording recorder is left alone.
        assert_eq!(decide(Recording, true), Skip("already recording"));
        assert_eq!(decide(Idle, true), Act);
        assert_eq!(decide(Stopping, true), Act);
        assert_eq!(decide(Unknown, true), Act);
        // Stop: an idle or stopping one is left alone.
        assert_eq!(decide(Idle, false), Skip("not recording"));
        assert_eq!(decide(Stopping, false), Skip("already stopping"));
        assert_eq!(decide(Recording, false), Act);
        assert_eq!(decide(Unknown, false), Act);
    }

    #[test]
    fn a_preset_trigger_matches_its_preset_on_any_camera_or_the_one_it_names() {
        let any_camera = AutomationTrigger::MiddlecontrolPreset {
            preset: 3,
            camera_id: None,
        };
        let camera_two = AutomationTrigger::MiddlecontrolPreset {
            preset: 3,
            camera_id: Some(2),
        };
        assert!(preset_trigger_matches(&any_camera, 3, 1));
        assert!(preset_trigger_matches(&any_camera, 3, 7));
        assert!(!preset_trigger_matches(&any_camera, 4, 1));
        assert!(preset_trigger_matches(&camera_two, 3, 2));
        assert!(!preset_trigger_matches(&camera_two, 3, 1));
        assert!(!preset_trigger_matches(
            &channel(true, ChannelSource::Either),
            3,
            1
        ));
    }

    #[test]
    fn a_chain_from_a_to_b_fires_b_but_stops_at_a_again() {
        let (a, b, c, d) = (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        // Rule A set off the move, so its finish may fire B.
        assert!(chain_allows(b, &[a]));
        // A's move then B's: A is in the chain and does not fire again.
        assert!(!chain_allows(a, &[a, b]));
        assert!(chain_allows(c, &[a, b]));
        // Nothing starts a chain of three more.
        assert!(!chain_allows(d, &[a, b, c]));
        // A move nobody caused fires anything.
        assert!(chain_allows(a, &[]));
    }

    #[test]
    fn the_cooldown_ignores_refires_inside_the_window_only() {
        let base = Instant::now();
        assert!(!cooling(None, base, 5.0));
        assert!(cooling(Some(base), base + Duration::from_secs(4), 5.0));
        assert!(!cooling(Some(base), base + Duration::from_secs(5), 5.0));
        assert!(!cooling(Some(base), base + Duration::from_secs(60), 5.0));
        // A clock that went backwards is not a reason to fire.
        assert!(cooling(Some(base + Duration::from_secs(2)), base, 5.0));
    }

    #[test]
    fn either_is_live_only_while_neither_the_desk_nor_the_transmitter_is_muted() {
        assert!(mutes(false, false).live(ChannelSource::Either));
        assert!(!mutes(true, false).live(ChannelSource::Either));
        assert!(!mutes(false, true).live(ChannelSource::Either));
        assert!(!mutes(true, true).live(ChannelSource::Either));
    }

    #[test]
    fn desk_and_transmitter_each_look_at_one_mute() {
        assert!(mutes(false, true).live(ChannelSource::Desk));
        assert!(!mutes(true, false).live(ChannelSource::Desk));
        assert!(mutes(true, false).live(ChannelSource::Transmitter));
        assert!(!mutes(false, true).live(ChannelSource::Transmitter));
    }

    #[test]
    fn a_pastor_who_mutes_on_the_transmitter_goes_silent_for_either() {
        let live = channel(true, ChannelSource::Either);
        let silent = channel(false, ChannelSource::Either);
        // Unmuted on the transmitter, desk open: live.
        assert!(channel_trigger_fires(
            &live,
            2,
            mutes(false, true),
            mutes(false, false)
        ));
        assert!(!channel_trigger_fires(
            &silent,
            2,
            mutes(false, true),
            mutes(false, false)
        ));
        // Muted on the transmitter: silent.
        assert!(channel_trigger_fires(
            &silent,
            2,
            mutes(false, false),
            mutes(false, true)
        ));
        assert!(!channel_trigger_fires(
            &live,
            2,
            mutes(false, false),
            mutes(false, true)
        ));
    }

    #[test]
    fn a_transmitter_unmute_while_the_desk_is_muted_is_not_live() {
        let live = channel(true, ChannelSource::Either);
        assert!(!channel_trigger_fires(
            &live,
            2,
            mutes(true, true),
            mutes(true, false)
        ));
        // And the same unmute does count when only the transmitter decides.
        let by_transmitter = channel(true, ChannelSource::Transmitter);
        assert!(channel_trigger_fires(
            &by_transmitter,
            2,
            mutes(true, true),
            mutes(true, false)
        ));
        // A second mute on an already silent channel is no change either.
        let silent = channel(false, ChannelSource::Either);
        assert!(!channel_trigger_fires(
            &silent,
            2,
            mutes(true, false),
            mutes(true, true)
        ));
    }

    #[test]
    fn a_desk_trigger_ignores_the_transmitter_and_the_other_way_round() {
        let desk_live = channel(true, ChannelSource::Desk);
        assert!(channel_trigger_fires(
            &desk_live,
            2,
            mutes(true, true),
            mutes(false, true)
        ));
        assert!(!channel_trigger_fires(
            &desk_live,
            2,
            mutes(false, true),
            mutes(false, false)
        ));
        let transmitter_silent = channel(false, ChannelSource::Transmitter);
        assert!(channel_trigger_fires(
            &transmitter_silent,
            2,
            mutes(false, false),
            mutes(false, true)
        ));
        assert!(!channel_trigger_fires(
            &transmitter_silent,
            2,
            mutes(false, false),
            mutes(true, false)
        ));
    }

    #[test]
    fn a_channel_trigger_is_for_its_own_channel_only() {
        let live = channel(true, ChannelSource::Either);
        assert!(!channel_trigger_fires(
            &live,
            3,
            mutes(true, false),
            mutes(false, false)
        ));
    }

    #[test]
    fn after_a_hold_the_channel_must_still_be_as_the_trigger_said() {
        let silent = channel(false, ChannelSource::Either);
        // Muted for the whole hold.
        assert!(still_holds(&silent, mutes(true, false)));
        // A cough-mute: back to live before the hold ran out.
        assert!(!still_holds(&silent, mutes(false, false)));
        assert!(!still_holds(&silent, mutes(false, false)));
        let live = channel(true, ChannelSource::Desk);
        assert!(still_holds(&live, mutes(false, true)));
        assert!(!still_holds(
            &AutomationTrigger::MiddlecontrolPreset {
                preset: 1,
                camera_id: None
            },
            mutes(false, false)
        ));
    }

    #[test]
    fn a_resync_seeds_the_memory_without_reporting_any_change() {
        let mut memory = ChannelMemory::default();
        memory.seed([(2, mutes(true, false)), (3, mutes(false, false))]);
        assert_eq!(memory.get(2), Some(mutes(true, false)));
        assert_eq!(memory.get(9), None);

        // Seeding again replaces what was there.
        memory.seed([(2, mutes(false, false))]);
        assert_eq!(memory.get(2), Some(mutes(false, false)));
        assert_eq!(memory.get(3), None);
    }

    #[test]
    fn a_mute_event_moves_one_of_the_channels_mutes_and_reports_both_sides() {
        let mut memory = ChannelMemory::default();
        memory.seed([(2, mutes(true, false))]);
        assert_eq!(
            memory.apply(2, false, false),
            Some((mutes(true, false), mutes(false, false)))
        );
        assert_eq!(
            memory.apply(2, true, true),
            Some((mutes(false, false), mutes(false, true)))
        );
        assert_eq!(memory.get(2), Some(mutes(false, true)));
    }

    #[test]
    fn a_mute_on_a_channel_nothing_is_known_about_fires_nothing_but_is_remembered() {
        let mut memory = ChannelMemory::default();
        assert_eq!(memory.apply(5, true, false), None);
        // Now it is known, so the next change is an edge.
        assert_eq!(
            memory.apply(5, false, false),
            Some((mutes(true, false), mutes(false, false)))
        );
    }

    #[test]
    fn adjacent_recorders_run_together_and_a_camera_move_stays_in_its_place() {
        let actions = [
            recall(3, None),
            start(RecordTarget::Obs),
            start(RecordTarget::Atem),
            start(RecordTarget::RodecasterAudio),
        ];
        assert_eq!(
            steps(&actions),
            vec![
                Step::Recall(&actions[0]),
                Step::Recorders(vec![&actions[1], &actions[2], &actions[3]]),
            ]
        );
    }

    #[test]
    fn a_stop_rule_stops_recording_before_it_moves_the_camera() {
        let actions = [
            stop(RecordTarget::Obs),
            stop(RecordTarget::Atem),
            recall(1, None),
        ];
        assert_eq!(
            steps(&actions),
            vec![
                Step::Recorders(vec![&actions[0], &actions[1]]),
                Step::Recall(&actions[2]),
            ]
        );
    }

    #[test]
    fn a_camera_move_between_recorders_splits_them_into_two_groups() {
        let actions = [
            start(RecordTarget::Obs),
            recall(2, Some(2)),
            start(RecordTarget::Atem),
            start(RecordTarget::Rodecaster),
            recall(3, None),
            recall(4, None),
        ];
        assert_eq!(
            steps(&actions),
            vec![
                Step::Recorders(vec![&actions[0]]),
                Step::Recall(&actions[1]),
                Step::Recorders(vec![&actions[2], &actions[3]]),
                Step::Recall(&actions[4]),
                Step::Recall(&actions[5]),
            ]
        );
        assert!(steps(&[]).is_empty());
    }

    #[test]
    fn a_recall_is_skipped_only_when_that_same_preset_fired_the_rule() {
        let fired = Cause::Preset {
            preset: 3,
            camera_id: 1,
        };
        assert!(recall_is_redundant(&recall(3, None), &fired));
        assert!(recall_is_redundant(&recall(3, Some(1)), &fired));
        assert!(!recall_is_redundant(&recall(3, Some(2)), &fired));
        assert!(!recall_is_redundant(&recall(1, None), &fired));
        assert!(!recall_is_redundant(&recall(3, None), &Cause::Manual));
        assert!(!recall_is_redundant(
            &recall(3, None),
            &Cause::Channel {
                label: "Wireless 1".to_string(),
                live: true
            }
        ));
        assert!(!recall_is_redundant(&start(RecordTarget::Obs), &fired));
    }

    #[test]
    fn causes_read_as_words() {
        assert_eq!(
            Cause::Preset {
                preset: 3,
                camera_id: 1
            }
            .describe(),
            "Preset 3 finished on camera 1"
        );
        assert_eq!(
            Cause::Channel {
                label: "Wireless 1".to_string(),
                live: true
            }
            .describe(),
            "Wireless 1 went live"
        );
        assert_eq!(
            Cause::Channel {
                label: "Host".to_string(),
                live: false
            }
            .describe(),
            "Host went silent"
        );
        assert_eq!(Cause::Manual.describe(), "Run now");
    }
}
