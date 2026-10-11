//! Follows Middle Control's preset moves from its feedback frames.
//!
//! Middle Control 4.0.4 reports the number of the running preset in `PRES_ACTIVE` (the API
//! page says 1/0, which is wrong for 4.0.4) and returns it to `0` when the move ends. There is
//! no completion token, so a move *finishes* when the number drops back to `0` after at least
//! most of the transition time `PRES_D`. The tracker is a pure state machine: the connector
//! feeds it one [`Observation`] per frame and publishes what it returns.
//!
//! The frames describe the selected camera only, so the tracker follows that camera.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A move that ends before this share of its transition time was cut short (`STOPPRESET`, or
/// the operator taking the joystick), not finished.
const FINISHED_SHARE: f32 = 0.9;
/// With no `PRES_D` to judge by, a move shorter than this was not a move.
const SHORTEST_UNJUDGED_MOVE: Duration = Duration::from_secs(1);
/// A recall Metocast sent marks the move it causes when the move starts within this long.
pub const RECALL_WINDOW: Duration = Duration::from_secs(1);

/// What one feedback frame says about presets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    /// `CAM`: the selected camera.
    pub camera: Option<u8>,
    /// `PRES_ACTIVE`: `Some(0)` at rest, `Some(n)` while preset `n` runs, `None` when Middle
    /// Control has no answer.
    pub active: Option<u8>,
    /// `PRES_D`: how long a preset move takes, in seconds.
    pub preset_seconds: Option<f32>,
    /// `PRESETSEQ_RUN`: Middle Control's own preset sequence is running.
    pub sequence_running: Option<bool>,
}

/// What the frames last said about the selected camera's position, for callers that watch a
/// move rather than wait for its end.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Motion {
    pub camera: Option<u8>,
    pub active: Option<u8>,
    pub preset_seconds: Option<f32>,
    /// `aPAN`, in tenths of a degree. Absent without an APC-R.
    pub pan: Option<i32>,
    /// `aTILT`, in tenths of a degree.
    pub tilt: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetStarted {
    pub preset: u8,
    pub camera_id: u8,
    pub at: DateTime<Utc>,
    /// The rules that caused this move, when Metocast itself sent it.
    pub chain: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetFinished {
    pub preset: u8,
    pub camera_id: u8,
    pub at: DateTime<Utc>,
    /// The rules that caused this move, when Metocast itself sent it.
    pub chain: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetEvent {
    Started(PresetStarted),
    Finished(PresetFinished),
}

/// A preset Metocast has just asked Middle Control to recall.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recall {
    pub preset: u8,
    /// `None` is the selected camera.
    pub camera_id: Option<u8>,
    pub chain: Vec<Uuid>,
}

#[derive(Debug)]
struct Move {
    preset: u8,
    camera: u8,
    since: Instant,
    seconds: Option<f32>,
    /// The move's start was seen and nothing makes its end unreliable.
    counted: bool,
    chain: Vec<Uuid>,
}

#[derive(Debug, Default)]
pub struct PresetTracker {
    /// The last frame's camera and `PRES_ACTIVE`; `None` before the first frame.
    previous: Option<(Option<u8>, Option<u8>)>,
    moving: Option<Move>,
    preset_seconds: Option<f32>,
    recall: Option<(Recall, Instant)>,
}

fn long_enough(elapsed: Duration, seconds: Option<f32>) -> bool {
    match seconds {
        Some(seconds) => elapsed.as_secs_f32() >= seconds * FINISHED_SHARE,
        None => elapsed >= SHORTEST_UNJUDGED_MOVE,
    }
}

impl PresetTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets everything seen: after a reconnect, a move already running is not one whose
    /// start was seen.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Marks the move a recall is about to cause, so its events carry the rules behind it.
    pub fn note_recall(&mut self, recall: Recall, now: Instant) {
        self.recall = Some((recall, now));
    }

    /// The recall was never sent.
    pub fn forget_recall(&mut self) {
        self.recall = None;
    }

    fn take_chain(&mut self, preset: u8, camera: Option<u8>, now: Instant) -> Vec<Uuid> {
        let matches = self.recall.as_ref().is_some_and(|(recall, at)| {
            recall.preset == preset
                && recall.camera_id.is_none_or(|id| Some(id) == camera)
                && now.saturating_duration_since(*at) <= RECALL_WINDOW
        });
        if !matches {
            return Vec::new();
        }
        self.recall
            .take()
            .map(|(recall, _)| recall.chain)
            .unwrap_or_default()
    }

    /// One frame. Returns what it started or finished, in order.
    pub fn observe(
        &mut self,
        frame: &Observation,
        now: Instant,
        wall: DateTime<Utc>,
    ) -> Vec<PresetEvent> {
        let mut events = Vec::new();
        if let Some(seconds) = frame.preset_seconds {
            self.preset_seconds = Some(seconds);
        }
        if self
            .recall
            .as_ref()
            .is_some_and(|(_, at)| now.saturating_duration_since(*at) > RECALL_WINDOW)
        {
            self.recall = None;
        }
        // The frames describe the selected camera: a move followed on another one is gone.
        if self
            .moving
            .as_ref()
            .is_some_and(|moving| frame.camera != Some(moving.camera))
        {
            self.moving = None;
        }
        // Middle Control's own preset sequence makes its own moves.
        if frame.sequence_running == Some(true)
            && let Some(moving) = &mut self.moving
        {
            moving.counted = false;
        }

        // Rest on this camera, the frame before this one: only then did a move's start show.
        let seen_rest = self
            .previous
            .is_some_and(|(camera, active)| camera == frame.camera && active == Some(0));
        self.previous = Some((frame.camera, frame.active));

        match frame.active {
            None => self.moving = None,
            Some(0) => {
                if let Some(finished) = self.moving.take()
                    && finished.counted
                    && long_enough(
                        now.saturating_duration_since(finished.since),
                        finished.seconds.or(self.preset_seconds),
                    )
                {
                    events.push(PresetEvent::Finished(PresetFinished {
                        preset: finished.preset,
                        camera_id: finished.camera,
                        at: wall,
                        chain: finished.chain,
                    }));
                }
            }
            Some(preset) => {
                let same_move = self
                    .moving
                    .as_ref()
                    .is_some_and(|moving| moving.preset == preset);
                if same_move {
                    return events;
                }
                // From rest, or straight from another preset (which that one's end never
                // came for): a start. Otherwise the move was already running.
                let started = seen_rest || self.moving.is_some();
                let camera = frame.camera;
                let counted = started
                    && camera.is_some()
                    && (1..=12).contains(&preset)
                    && frame.sequence_running != Some(true);
                let chain = if counted {
                    self.take_chain(preset, camera, now)
                } else {
                    Vec::new()
                };
                self.moving = camera.map(|camera| Move {
                    preset,
                    camera,
                    since: now,
                    seconds: self.preset_seconds,
                    counted,
                    chain: chain.clone(),
                });
                if counted && let Some(camera) = camera {
                    events.push(PresetEvent::Started(PresetStarted {
                        preset,
                        camera_id: camera,
                        at: wall,
                        chain,
                    }));
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Clock {
        base: Instant,
        wall: DateTime<Utc>,
    }

    impl Clock {
        fn new() -> Self {
            Self {
                base: Instant::now(),
                wall: "2026-10-07T08:00:00Z".parse().unwrap(),
            }
        }

        fn at(&self, millis: u64) -> (Instant, DateTime<Utc>) {
            (
                self.base + Duration::from_millis(millis),
                self.wall + chrono::Duration::milliseconds(i64::try_from(millis).unwrap()),
            )
        }
    }

    fn frame(camera: Option<u8>, active: Option<u8>) -> Observation {
        Observation {
            camera,
            active,
            preset_seconds: Some(8.0),
            sequence_running: Some(false),
        }
    }

    fn observe(
        tracker: &mut PresetTracker,
        clock: &Clock,
        millis: u64,
        frame: Observation,
    ) -> Vec<PresetEvent> {
        let (now, wall) = clock.at(millis);
        tracker.observe(&frame, now, wall)
    }

    fn finished(events: &[PresetEvent]) -> Vec<(u8, u8)> {
        events
            .iter()
            .filter_map(|event| match event {
                PresetEvent::Finished(done) => Some((done.preset, done.camera_id)),
                PresetEvent::Started(_) => None,
            })
            .collect()
    }

    fn started(events: &[PresetEvent]) -> Vec<(u8, u8)> {
        events
            .iter()
            .filter_map(|event| match event {
                PresetEvent::Started(start) => Some((start.preset, start.camera_id)),
                PresetEvent::Finished(_) => None,
            })
            .collect()
    }

    /// Frames as 4.0.4 sends them: preset 3 from rest, held for the transition time.
    #[test]
    fn a_completed_move_starts_then_finishes_when_the_number_drops_to_zero() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        assert!(observe(&mut tracker, &clock, 0, frame(Some(1), Some(0))).is_empty());

        let begin = observe(&mut tracker, &clock, 1_500, frame(Some(1), Some(3)));
        assert_eq!(started(&begin), vec![(3, 1)]);
        assert!(finished(&begin).is_empty());
        for millis in [1_750, 4_000, 8_000, 9_000] {
            assert!(observe(&mut tracker, &clock, millis, frame(Some(1), Some(3))).is_empty());
        }

        let end = observe(&mut tracker, &clock, 9_500, frame(Some(1), Some(0)));
        assert_eq!(finished(&end), vec![(3, 1)]);
        // Rest does not finish it twice.
        assert!(observe(&mut tracker, &clock, 9_750, frame(Some(1), Some(0))).is_empty());
    }

    #[test]
    fn a_move_stopped_early_is_not_a_finish() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        observe(&mut tracker, &clock, 500, frame(Some(1), Some(1)));
        // STOPPRESET dropped it after 2.2 s of the 8.
        let end = observe(&mut tracker, &clock, 2_700, frame(Some(1), Some(0)));
        assert!(end.is_empty());
    }

    #[test]
    fn a_move_just_under_nine_tenths_of_the_transition_is_not_finished_and_just_over_is() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        observe(&mut tracker, &clock, 100, frame(Some(1), Some(2)));
        // 7.1 s elapsed of 8: under 7.2.
        assert!(observe(&mut tracker, &clock, 7_200, frame(Some(1), Some(0))).is_empty());

        observe(&mut tracker, &clock, 8_000, frame(Some(1), Some(2)));
        let end = observe(&mut tracker, &clock, 15_300, frame(Some(1), Some(0)));
        assert_eq!(finished(&end), vec![(2, 1)]);
    }

    #[test]
    fn a_change_from_one_preset_to_another_starts_the_second() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        observe(&mut tracker, &clock, 500, frame(Some(1), Some(1)));

        // Clicked preset 3 two seconds in: 1 → 3 with no 0 between.
        let switch = observe(&mut tracker, &clock, 2_500, frame(Some(1), Some(3)));
        assert_eq!(started(&switch), vec![(3, 1)]);
        assert!(finished(&switch).is_empty());

        // Preset 1 never finishes; preset 3 does, a full transition after it started.
        let end = observe(&mut tracker, &clock, 10_700, frame(Some(1), Some(0)));
        assert_eq!(finished(&end), vec![(3, 1)]);
    }

    #[test]
    fn a_move_already_running_at_connect_does_not_count() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        // The first frame Metocast ever sees is mid-move.
        assert!(observe(&mut tracker, &clock, 0, frame(Some(1), Some(3))).is_empty());
        assert!(observe(&mut tracker, &clock, 4_000, frame(Some(1), Some(3))).is_empty());
        assert!(observe(&mut tracker, &clock, 9_000, frame(Some(1), Some(0))).is_empty());

        // A reconnect forgets what was followed.
        observe(&mut tracker, &clock, 10_000, frame(Some(1), Some(0)));
        observe(&mut tracker, &clock, 10_500, frame(Some(1), Some(2)));
        tracker.reset();
        assert!(observe(&mut tracker, &clock, 11_000, frame(Some(1), Some(2))).is_empty());
        assert!(observe(&mut tracker, &clock, 19_000, frame(Some(1), Some(0))).is_empty());
    }

    #[test]
    fn a_camera_switch_mid_move_ends_the_followed_move_without_a_finish() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        observe(&mut tracker, &clock, 500, frame(Some(1), Some(3)));

        // Camera 2 is selected: the frames now describe it, and it is at rest.
        assert!(observe(&mut tracker, &clock, 3_000, frame(Some(2), Some(0))).is_empty());
        assert!(observe(&mut tracker, &clock, 9_000, frame(Some(2), Some(0))).is_empty());
    }

    #[test]
    fn a_move_already_running_on_the_camera_just_selected_does_not_count() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        // Camera 2 was already moving when it was selected: the rest on camera 1 before it is
        // not rest on camera 2.
        assert!(observe(&mut tracker, &clock, 500, frame(Some(2), Some(5))).is_empty());
        assert!(observe(&mut tracker, &clock, 9_000, frame(Some(2), Some(0))).is_empty());
    }

    #[test]
    fn nothing_during_a_preset_sequence_counts() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        let in_sequence = |active| Observation {
            sequence_running: Some(true),
            ..frame(Some(1), active)
        };
        observe(&mut tracker, &clock, 0, in_sequence(Some(0)));
        let begin = observe(&mut tracker, &clock, 500, in_sequence(Some(2)));
        assert!(begin.is_empty());
        assert!(observe(&mut tracker, &clock, 9_000, in_sequence(Some(0))).is_empty());

        // A sequence that begins mid-move takes it back.
        observe(&mut tracker, &clock, 10_000, frame(Some(1), Some(0)));
        let begin = observe(&mut tracker, &clock, 10_500, frame(Some(1), Some(3)));
        assert_eq!(started(&begin), vec![(3, 1)]);
        observe(&mut tracker, &clock, 12_000, in_sequence(Some(3)));
        assert!(observe(&mut tracker, &clock, 19_000, frame(Some(1), Some(0))).is_empty());
    }

    #[test]
    fn a_frame_with_no_answer_forgets_the_move() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        observe(&mut tracker, &clock, 500, frame(Some(1), Some(3)));
        assert!(observe(&mut tracker, &clock, 4_000, frame(Some(1), None)).is_empty());
        // And a move after that is not mistaken for one that started from rest.
        assert!(observe(&mut tracker, &clock, 4_500, frame(Some(1), Some(3))).is_empty());
        assert!(observe(&mut tracker, &clock, 12_500, frame(Some(1), Some(0))).is_empty());
    }

    #[test]
    fn no_selected_camera_means_no_move_can_be_placed() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(None, Some(0)));
        assert!(observe(&mut tracker, &clock, 500, frame(None, Some(3))).is_empty());
        assert!(observe(&mut tracker, &clock, 9_000, frame(None, Some(0))).is_empty());
    }

    #[test]
    fn a_preset_number_outside_one_to_twelve_is_not_a_preset() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        assert!(observe(&mut tracker, &clock, 500, frame(Some(1), Some(13))).is_empty());
        assert!(observe(&mut tracker, &clock, 9_000, frame(Some(1), Some(0))).is_empty());
    }

    #[test]
    fn without_a_transition_time_only_a_move_of_a_second_or_more_finishes() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        let unjudged = |active| Observation {
            preset_seconds: None,
            ..frame(Some(1), active)
        };
        observe(&mut tracker, &clock, 0, unjudged(Some(0)));
        observe(&mut tracker, &clock, 100, unjudged(Some(2)));
        assert!(observe(&mut tracker, &clock, 600, unjudged(Some(0))).is_empty());

        observe(&mut tracker, &clock, 1_000, unjudged(Some(2)));
        let end = observe(&mut tracker, &clock, 2_200, unjudged(Some(0)));
        assert_eq!(finished(&end), vec![(2, 1)]);
    }

    #[test]
    fn a_transition_time_seen_in_an_earlier_frame_is_remembered() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        let without = |active| Observation {
            preset_seconds: None,
            ..frame(Some(1), active)
        };
        observe(&mut tracker, &clock, 500, without(Some(2)));
        // 3 s of an 8 s transition, known from the first frame: stopped early.
        assert!(observe(&mut tracker, &clock, 3_500, without(Some(0))).is_empty());
    }

    #[test]
    fn a_recall_metocast_sent_marks_the_move_with_its_chain() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        let rule = Uuid::new_v4();
        let (sent, _) = clock.at(1_000);
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        tracker.note_recall(
            Recall {
                preset: 3,
                camera_id: None,
                chain: vec![rule],
            },
            sent,
        );

        let begin = observe(&mut tracker, &clock, 1_300, frame(Some(1), Some(3)));
        let PresetEvent::Started(start) = &begin[0] else {
            panic!("expected a start");
        };
        assert_eq!(start.chain, vec![rule]);

        let end = observe(&mut tracker, &clock, 9_500, frame(Some(1), Some(0)));
        let PresetEvent::Finished(done) = &end[0] else {
            panic!("expected a finish");
        };
        assert_eq!(done.chain, vec![rule]);
    }

    #[test]
    fn a_move_that_starts_more_than_a_second_after_the_recall_is_not_its_move() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        let (sent, _) = clock.at(1_000);
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        tracker.note_recall(
            Recall {
                preset: 3,
                camera_id: None,
                chain: vec![Uuid::new_v4()],
            },
            sent,
        );
        let begin = observe(&mut tracker, &clock, 2_500, frame(Some(1), Some(3)));
        let PresetEvent::Started(start) = &begin[0] else {
            panic!("expected a start");
        };
        assert!(start.chain.is_empty());
    }

    #[test]
    fn a_recall_of_another_preset_or_camera_does_not_mark_the_move() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        let (sent, _) = clock.at(500);
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        tracker.note_recall(
            Recall {
                preset: 2,
                camera_id: None,
                chain: vec![Uuid::new_v4()],
            },
            sent,
        );
        let begin = observe(&mut tracker, &clock, 700, frame(Some(1), Some(3)));
        let PresetEvent::Started(start) = &begin[0] else {
            panic!("expected a start");
        };
        assert!(start.chain.is_empty());

        // Preset 4 sent to camera 2 while camera 1 is selected.
        observe(&mut tracker, &clock, 9_000, frame(Some(1), Some(0)));
        let (sent, _) = clock.at(9_500);
        tracker.note_recall(
            Recall {
                preset: 4,
                camera_id: Some(2),
                chain: vec![Uuid::new_v4()],
            },
            sent,
        );
        let begin = observe(&mut tracker, &clock, 9_700, frame(Some(1), Some(4)));
        let PresetEvent::Started(start) = &begin[0] else {
            panic!("expected a start");
        };
        assert!(start.chain.is_empty());
    }

    #[test]
    fn a_recall_that_was_never_sent_marks_nothing() {
        let clock = Clock::new();
        let mut tracker = PresetTracker::new();
        let (sent, _) = clock.at(500);
        observe(&mut tracker, &clock, 0, frame(Some(1), Some(0)));
        tracker.note_recall(
            Recall {
                preset: 3,
                camera_id: None,
                chain: vec![Uuid::new_v4()],
            },
            sent,
        );
        tracker.forget_recall();
        let begin = observe(&mut tracker, &clock, 700, frame(Some(1), Some(3)));
        let PresetEvent::Started(start) = &begin[0] else {
            panic!("expected a start");
        };
        assert!(start.chain.is_empty());
    }
}
