//! Waiting for a camera to reach a preset, so the actions after a camera move start on the
//! right shot.
//!
//! Middle Control reports the preset's number in `PRES_ACTIVE` while it moves and `0` when it is
//! done (see `connectors::preset_tracker`). It runs the whole transition time (`PRES_D`) even
//! when the camera is already at the preset, so the position is watched too: a camera that has
//! hardly moved after a moment is there.

use std::time::{Duration, Instant};

use tokio::sync::broadcast::{self, error::TryRecvError};
use uuid::Uuid;

use crate::connectors::preset_tracker::{Motion, PresetEvent};

/// "Hardly": half a degree, in the tenths of a degree the frames report. A real move covered
/// 3.7° in its first second on the test rig; a camera already there stayed within 0.2°.
const ALREADY_THERE_WITHIN: i32 = 5;
/// The transition time when Middle Control has not said (it is 8 s out of the box).
pub const DEFAULT_PRESET_SECONDS: f32 = 8.0;

/// How long each part of a wait takes. The tests run it scaled down.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Middle Control that has not started a move by now is not going to (the camera isn't
    /// connected, say).
    pub no_move_after: Duration,
    /// A camera that has hardly moved this long after its move started is already at the preset.
    pub already_there_after: Duration,
    /// A move this long past its transition time is not coming to an end.
    pub overrun: Duration,
    /// How often the frames are looked at.
    pub tick: Duration,
}

impl Timing {
    pub const LIVE: Self = Self {
        no_move_after: Duration::from_secs(1),
        already_there_after: Duration::from_millis(1500),
        overrun: Duration::from_secs(5),
        tick: Duration::from_millis(100),
    };
}

/// How a wait for a camera ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrival {
    /// The move ran its course.
    Arrived,
    /// The camera had hardly moved: it was at the preset already.
    AlreadyThere,
    /// Middle Control did not start a move.
    NotStarted,
    /// The move ran well past its transition time without ending.
    Overran,
    /// The camera isn't the one the frames describe, so its transition time was waited out.
    Waited,
}

impl Arrival {
    /// What a run notes about the wait, when it didn't see the camera arrive.
    #[must_use]
    pub fn note(self) -> Option<&'static str> {
        match self {
            Self::Arrived => None,
            Self::AlreadyThere => Some("the camera was already there"),
            Self::NotStarted => Some("Middle Control did not start the move"),
            Self::Overran => Some("the camera did not report arriving"),
            Self::Waited => Some("waited out the transition time"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The camera is the one the frames describe: watch it.
    Watch { camera: u8 },
    /// It isn't, or nothing is selected: wait the transition time.
    Timed,
}

/// The wait for one recall. A pure state machine over a clock and what Middle Control says.
#[derive(Debug)]
pub struct ArrivalWatch {
    preset: u8,
    mode: Mode,
    timing: Timing,
    sent: Instant,
    /// Pan and tilt when the recall went out.
    baseline: Option<(i32, i32)>,
    started: Option<Instant>,
    seconds: f32,
}

impl ArrivalWatch {
    /// `camera` is the camera the recall names; `None` is the selected one.
    #[must_use]
    pub fn new(
        preset: u8,
        camera: Option<u8>,
        motion: &Motion,
        now: Instant,
        timing: Timing,
    ) -> Self {
        let mode = match (camera, motion.camera) {
            (None, Some(selected)) => Mode::Watch { camera: selected },
            (Some(named), Some(selected)) if named == selected => Mode::Watch { camera: named },
            // Another camera: the frames describe the selected one only.
            _ => Mode::Timed,
        };
        Self {
            preset,
            mode,
            timing,
            sent: now,
            baseline: motion.pan.zip(motion.tilt),
            started: None,
            seconds: motion.preset_seconds.unwrap_or(DEFAULT_PRESET_SECONDS),
        }
    }

    fn barely_moved(&self, motion: &Motion) -> bool {
        match (self.baseline, motion.pan.zip(motion.tilt)) {
            (Some((pan, tilt)), Some((now_pan, now_tilt))) => {
                (now_pan - pan).abs() < ALREADY_THERE_WITHIN
                    && (now_tilt - tilt).abs() < ALREADY_THERE_WITHIN
            }
            // Without an APC-R there is no position to go by.
            _ => false,
        }
    }

    /// One look: the time, what the frames last said, and the preset events since the last
    /// look. `Some` ends the wait.
    pub fn poll(
        &mut self,
        now: Instant,
        motion: &Motion,
        events: &[PresetEvent],
    ) -> Option<Arrival> {
        let waited = now.saturating_duration_since(self.sent);
        if let Mode::Watch { camera } = self.mode {
            if motion.camera != Some(camera) {
                // Somebody selected another camera: its move can no longer be watched.
                self.mode = Mode::Timed;
            } else {
                for event in events {
                    match event {
                        PresetEvent::Finished(done)
                            if done.preset == self.preset && done.camera_id == camera =>
                        {
                            return Some(Arrival::Arrived);
                        }
                        PresetEvent::Started(start)
                            if start.preset == self.preset && start.camera_id == camera =>
                        {
                            self.started.get_or_insert(now);
                        }
                        _ => {}
                    }
                }
                return match self.started {
                    None => (waited >= self.timing.no_move_after).then_some(Arrival::NotStarted),
                    Some(started) => {
                        let moving = now.saturating_duration_since(started);
                        if moving >= self.timing.already_there_after && self.barely_moved(motion) {
                            Some(Arrival::AlreadyThere)
                        } else if moving
                            >= Duration::from_secs_f32(self.seconds) + self.timing.overrun
                        {
                            Some(Arrival::Overran)
                        } else {
                            None
                        }
                    }
                };
            }
        }
        (waited >= Duration::from_secs_f32(self.seconds)).then_some(Arrival::Waited)
    }
}

/// What a wait needs of Middle Control.
pub trait Camera: Send + Sync {
    fn motion(&self) -> Motion;
    fn preset_events(&self) -> broadcast::Receiver<PresetEvent>;
    fn recall(
        &self,
        preset: u8,
        camera_id: Option<u8>,
        chain: Vec<Uuid>,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

async fn watch_arrival<C: Camera>(
    camera: &C,
    events: &mut broadcast::Receiver<PresetEvent>,
    mut watch: ArrivalWatch,
) -> Arrival {
    let mut ticker = tokio::time::interval(watch.timing.tick);
    loop {
        ticker.tick().await;
        let mut seen = Vec::new();
        loop {
            match events.try_recv() {
                Ok(event) => seen.push(event),
                // Events missed are rare and the position and the timeouts still end the wait.
                Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
        if let Some(arrival) = watch.poll(Instant::now(), &camera.motion(), &seen) {
            return arrival;
        }
    }
}

/// Recalls a preset and, when `wait`, waits for the camera to arrive.
///
/// # Errors
/// The recall's own error: nothing was sent, so there is nothing to wait for.
pub async fn recall_and_wait<C: Camera>(
    camera: &C,
    preset: u8,
    camera_id: Option<u8>,
    wait: bool,
    chain: Vec<Uuid>,
) -> Result<Option<Arrival>, String> {
    recall_and_wait_timed(camera, preset, camera_id, wait, chain, Timing::LIVE).await
}

pub(crate) async fn recall_and_wait_timed<C: Camera>(
    camera: &C,
    preset: u8,
    camera_id: Option<u8>,
    wait: bool,
    chain: Vec<Uuid>,
    timing: Timing,
) -> Result<Option<Arrival>, String> {
    // Subscribed before the recall goes out, so its first frame can't be missed.
    let mut events = camera.preset_events();
    let watch = ArrivalWatch::new(preset, camera_id, &camera.motion(), Instant::now(), timing);
    camera.recall(preset, camera_id, chain).await?;
    if !wait {
        return Ok(None);
    }
    Ok(Some(watch_arrival(camera, &mut events, watch).await))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::Utc;
    use tokio::sync::watch;

    use super::*;
    use crate::connectors::preset_tracker::{PresetFinished, PresetStarted};

    fn motion(camera: Option<u8>) -> Motion {
        Motion {
            camera,
            active: Some(0),
            preset_seconds: Some(8.0),
            pan: Some(653),
            tilt: Some(-139),
        }
    }

    fn started(preset: u8, camera_id: u8) -> PresetEvent {
        PresetEvent::Started(PresetStarted {
            preset,
            camera_id,
            at: Utc::now(),
            chain: Vec::new(),
        })
    }

    fn finished(preset: u8, camera_id: u8) -> PresetEvent {
        PresetEvent::Finished(PresetFinished {
            preset,
            camera_id,
            at: Utc::now(),
            chain: Vec::new(),
        })
    }

    struct Rig {
        base: Instant,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                base: Instant::now(),
            }
        }

        fn at(&self, millis: u64) -> Instant {
            self.base + Duration::from_millis(millis)
        }
    }

    #[test]
    fn the_preset_finishing_on_the_watched_camera_is_arrival() {
        let rig = Rig::new();
        let moving = Motion {
            pan: Some(300),
            ..motion(Some(1))
        };
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(
            watch.poll(rig.at(300), &motion(Some(1)), &[started(3, 1)]),
            None
        );
        assert_eq!(watch.poll(rig.at(5_000), &moving, &[]), None);
        assert_eq!(
            watch.poll(rig.at(8_300), &moving, &[finished(3, 1)]),
            Some(Arrival::Arrived)
        );
    }

    #[test]
    fn another_preset_or_camera_finishing_is_not_arrival() {
        let rig = Rig::new();
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(
            watch.poll(
                rig.at(300),
                &motion(Some(1)),
                &[started(3, 1), finished(2, 1), finished(3, 2)]
            ),
            None
        );
    }

    #[test]
    fn a_camera_that_has_hardly_moved_after_a_second_and_a_half_is_already_there() {
        let rig = Rig::new();
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(
            watch.poll(rig.at(200), &motion(Some(1)), &[started(3, 1)]),
            None
        );
        // 1.4 s into the move: too early to say.
        assert_eq!(watch.poll(rig.at(1_600), &motion(Some(1)), &[]), None);
        // Wobble of 0.2° (2 tenths) either way is no move.
        let wobble = Motion {
            pan: Some(651),
            tilt: Some(-141),
            ..motion(Some(1))
        };
        assert_eq!(
            watch.poll(rig.at(1_700), &wobble, &[]),
            Some(Arrival::AlreadyThere)
        );
    }

    #[test]
    fn a_camera_that_is_really_moving_is_not_already_there() {
        let rig = Rig::new();
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(
            watch.poll(rig.at(200), &motion(Some(1)), &[started(3, 1)]),
            None
        );
        // 3.7° in the first second, as measured.
        let moving = Motion {
            pan: Some(616),
            tilt: Some(-133),
            ..motion(Some(1))
        };
        assert_eq!(watch.poll(rig.at(1_700), &moving, &[]), None);
        assert_eq!(watch.poll(rig.at(2_700), &moving, &[]), None);
    }

    #[test]
    fn half_a_degree_is_a_move_and_just_under_is_not() {
        let rig = Rig::new();
        let at = |pan: i32| Motion {
            pan: Some(pan),
            ..motion(Some(1))
        };
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        watch.poll(rig.at(200), &motion(Some(1)), &[started(3, 1)]);
        assert_eq!(watch.poll(rig.at(1_800), &at(658), &[]), None);
        assert_eq!(
            watch.poll(rig.at(1_900), &at(657), &[]),
            Some(Arrival::AlreadyThere)
        );
    }

    #[test]
    fn middle_control_starting_no_move_within_a_second_ends_the_wait() {
        let rig = Rig::new();
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(watch.poll(rig.at(900), &motion(Some(1)), &[]), None);
        assert_eq!(
            watch.poll(rig.at(1_000), &motion(Some(1)), &[]),
            Some(Arrival::NotStarted)
        );
    }

    #[test]
    fn a_camera_that_is_not_selected_waits_out_the_transition_time() {
        let rig = Rig::new();
        // Preset 3 on camera 2 while camera 1 is selected.
        let mut watch = ArrivalWatch::new(3, Some(2), &motion(Some(1)), rig.at(0), Timing::LIVE);
        // Frames about camera 1 are no news of camera 2, however they look.
        assert_eq!(
            watch.poll(
                rig.at(500),
                &motion(Some(1)),
                &[started(3, 1), finished(3, 1)]
            ),
            None
        );
        assert_eq!(watch.poll(rig.at(7_900), &motion(Some(1)), &[]), None);
        assert_eq!(
            watch.poll(rig.at(8_000), &motion(Some(1)), &[]),
            Some(Arrival::Waited)
        );
    }

    #[test]
    fn the_transition_time_is_eight_seconds_when_middle_control_has_not_said() {
        let rig = Rig::new();
        let unknown = Motion {
            preset_seconds: None,
            ..motion(Some(1))
        };
        let mut watch = ArrivalWatch::new(3, Some(2), &unknown, rig.at(0), Timing::LIVE);
        assert_eq!(watch.poll(rig.at(7_900), &unknown, &[]), None);
        assert_eq!(
            watch.poll(rig.at(8_000), &unknown, &[]),
            Some(Arrival::Waited)
        );
    }

    #[test]
    fn with_no_camera_selected_the_transition_time_is_waited_out() {
        let rig = Rig::new();
        let mut watch = ArrivalWatch::new(3, None, &motion(None), rig.at(0), Timing::LIVE);
        assert_eq!(watch.poll(rig.at(7_900), &motion(None), &[]), None);
        assert_eq!(
            watch.poll(rig.at(8_000), &motion(None), &[]),
            Some(Arrival::Waited)
        );
    }

    #[test]
    fn selecting_another_camera_mid_wait_falls_back_to_the_transition_time() {
        let rig = Rig::new();
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(
            watch.poll(rig.at(300), &motion(Some(1)), &[started(3, 1)]),
            None
        );
        // Camera 2 is selected now: camera 1's end will never be seen.
        assert_eq!(watch.poll(rig.at(2_000), &motion(Some(2)), &[]), None);
        assert_eq!(
            watch.poll(rig.at(8_000), &motion(Some(2)), &[]),
            Some(Arrival::Waited)
        );
    }

    #[test]
    fn a_move_five_seconds_past_its_transition_time_is_given_up_on() {
        let rig = Rig::new();
        let moving = Motion {
            pan: Some(300),
            ..motion(Some(1))
        };
        let mut watch = ArrivalWatch::new(3, None, &motion(Some(1)), rig.at(0), Timing::LIVE);
        assert_eq!(watch.poll(rig.at(300), &moving, &[started(3, 1)]), None);
        assert_eq!(watch.poll(rig.at(13_200), &moving, &[]), None);
        assert_eq!(
            watch.poll(rig.at(13_300), &moving, &[]),
            Some(Arrival::Overran)
        );
    }

    #[test]
    fn a_camera_without_an_apcr_is_waited_for_until_the_end_or_the_overrun() {
        let rig = Rig::new();
        let blind = Motion {
            pan: None,
            tilt: None,
            ..motion(Some(1))
        };
        let mut watch = ArrivalWatch::new(3, None, &blind, rig.at(0), Timing::LIVE);
        assert_eq!(watch.poll(rig.at(300), &blind, &[started(3, 1)]), None);
        // Never taken for "already there": there is no position.
        assert_eq!(watch.poll(rig.at(5_000), &blind, &[]), None);
        assert_eq!(
            watch.poll(rig.at(8_300), &blind, &[finished(3, 1)]),
            Some(Arrival::Arrived)
        );
    }

    #[test]
    fn the_notes_say_what_a_run_did_not_see() {
        assert_eq!(Arrival::Arrived.note(), None);
        assert!(
            Arrival::NotStarted
                .note()
                .unwrap()
                .contains("did not start")
        );
        assert!(Arrival::Overran.note().unwrap().contains("did not report"));
        assert!(Arrival::AlreadyThere.note().unwrap().contains("already"));
        assert!(Arrival::Waited.note().unwrap().contains("transition"));
    }

    /// A recall as the camera was asked: preset, camera, chain.
    type Recall = (u8, Option<u8>, Vec<Uuid>);

    /// A camera whose frames the test writes.
    struct FakeCamera {
        motion: watch::Sender<Motion>,
        events: broadcast::Sender<PresetEvent>,
        recalls: Mutex<Vec<Recall>>,
        refuse: Option<String>,
    }

    impl FakeCamera {
        /// A camera whose move takes 160 ms, the eight seconds of the real thing at 1/50.
        fn new(selected: Option<u8>) -> Self {
            Self {
                motion: watch::channel(Motion {
                    preset_seconds: Some(0.16),
                    ..motion(selected)
                })
                .0,
                events: broadcast::channel(16).0,
                recalls: Mutex::new(Vec::new()),
                refuse: None,
            }
        }
    }

    impl Camera for FakeCamera {
        fn motion(&self) -> Motion {
            *self.motion.borrow()
        }

        fn preset_events(&self) -> broadcast::Receiver<PresetEvent> {
            self.events.subscribe()
        }

        async fn recall(
            &self,
            preset: u8,
            camera_id: Option<u8>,
            chain: Vec<Uuid>,
        ) -> Result<(), String> {
            if let Some(error) = &self.refuse {
                return Err(error.clone());
            }
            self.recalls
                .lock()
                .unwrap()
                .push((preset, camera_id, chain));
            Ok(())
        }
    }

    /// The live timings at 1/50: a second becomes 20 ms.
    const FAST: Timing = Timing {
        no_move_after: Duration::from_millis(20),
        already_there_after: Duration::from_millis(30),
        overrun: Duration::from_millis(100),
        tick: Duration::from_millis(2),
    };

    /// Writes the camera's events at the given times, from now.
    async fn script(camera: &FakeCamera, steps: Vec<(u64, PresetEvent)>) {
        let mut at = 0;
        for (millis, event) in steps {
            tokio::time::sleep(Duration::from_millis(millis - at)).await;
            at = millis;
            let _ = camera.events.send(event);
        }
    }

    /// `moves`: the gimbal really moves once the recall is out, as it does for a camera that is
    /// not at the preset.
    async fn run(
        camera: &FakeCamera,
        camera_id: Option<u8>,
        wait: bool,
        moves: bool,
        steps: Vec<(u64, PresetEvent)>,
    ) -> (Result<Option<Arrival>, String>, Duration) {
        let began = Instant::now();
        let gimbal = async {
            if moves {
                tokio::time::sleep(Duration::from_millis(2)).await;
                camera.motion.send_modify(|m| m.pan = Some(300));
            }
        };
        let (arrival, (), ()) = tokio::join!(
            recall_and_wait_timed(camera, 3, camera_id, wait, Vec::new(), FAST),
            script(camera, steps),
            gimbal
        );
        (arrival, began.elapsed())
    }

    // The windows below have a hard floor (a wait never ends early) and a loose ceiling, so a
    // busy test machine cannot fail them.
    const LATE: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn the_rest_of_a_rule_waits_until_the_camera_arrives() {
        let camera = FakeCamera::new(Some(1));
        let (arrival, waited) = run(
            &camera,
            None,
            true,
            true,
            vec![(6, started(3, 1)), (166, finished(3, 1))],
        )
        .await;
        assert_eq!(arrival, Ok(Some(Arrival::Arrived)));
        assert!(waited >= Duration::from_millis(166), "{waited:?}");
        assert!(waited < LATE, "{waited:?}");
        assert_eq!(
            camera.recalls.lock().unwrap().as_slice(),
            &[(3, None, vec![])]
        );
    }

    #[tokio::test]
    async fn the_chain_goes_to_the_camera_with_the_recall() {
        let camera = FakeCamera::new(Some(1));
        let chain = vec![Uuid::new_v4()];
        let arrival = recall_and_wait_timed(&camera, 3, None, false, chain.clone(), FAST).await;
        assert_eq!(arrival, Ok(None));
        assert_eq!(
            camera.recalls.lock().unwrap().as_slice(),
            &[(3, None, chain)]
        );
    }

    #[tokio::test]
    async fn a_camera_already_at_the_preset_lets_the_rest_go_after_a_moment() {
        let camera = FakeCamera::new(Some(1));
        let (arrival, waited) = run(&camera, None, true, false, vec![(6, started(3, 1))]).await;
        assert_eq!(arrival, Ok(Some(Arrival::AlreadyThere)));
        // 30 ms after the start, long before the 160 ms the move would have run.
        assert!(waited >= Duration::from_millis(36), "{waited:?}");
        assert!(waited < LATE, "{waited:?}");
    }

    #[tokio::test]
    async fn a_move_that_never_starts_lets_the_rest_go_after_a_second() {
        let camera = FakeCamera::new(Some(1));
        let (arrival, waited) = run(&camera, None, true, false, Vec::new()).await;
        assert_eq!(arrival, Ok(Some(Arrival::NotStarted)));
        assert!(waited >= Duration::from_millis(20), "{waited:?}");
        assert!(waited < LATE, "{waited:?}");
    }

    #[tokio::test]
    async fn a_camera_that_is_not_selected_costs_the_transition_time() {
        let camera = FakeCamera::new(Some(1));
        let (arrival, waited) = run(&camera, Some(2), true, false, Vec::new()).await;
        assert_eq!(arrival, Ok(Some(Arrival::Waited)));
        assert!(waited >= Duration::from_millis(160), "{waited:?}");
        assert!(waited < LATE, "{waited:?}");
        assert_eq!(camera.recalls.lock().unwrap()[0].1, Some(2));
    }

    #[tokio::test]
    async fn a_move_that_overruns_lets_the_rest_go_five_seconds_late() {
        let camera = FakeCamera::new(Some(1));
        // The camera keeps moving, so it is never taken for already there.
        let (arrival, waited) = run(&camera, None, true, true, vec![(6, started(3, 1))]).await;
        assert_eq!(arrival, Ok(Some(Arrival::Overran)));
        // 160 ms of transition and 100 ms of grace, from the start.
        assert!(waited >= Duration::from_millis(266), "{waited:?}");
        assert!(waited < LATE, "{waited:?}");
    }

    #[tokio::test]
    async fn a_recall_that_fails_lets_the_rest_go_at_once() {
        let mut camera = FakeCamera::new(Some(1));
        camera.refuse = Some("middlecontrol_not_connected".to_string());
        let (arrival, waited) = run(&camera, None, true, false, Vec::new()).await;
        assert_eq!(arrival, Err("middlecontrol_not_connected".to_string()));
        assert!(waited < Duration::from_millis(20), "{waited:?}");
    }

    #[tokio::test]
    async fn a_recall_with_no_wait_returns_when_it_is_sent() {
        let camera = FakeCamera::new(Some(1));
        let (arrival, waited) = run(&camera, None, false, false, Vec::new()).await;
        assert_eq!(arrival, Ok(None));
        assert!(waited < Duration::from_millis(20), "{waited:?}");
        assert_eq!(camera.recalls.lock().unwrap().len(), 1);
    }
}
