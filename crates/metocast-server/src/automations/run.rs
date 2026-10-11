//! Running a rule's actions, in order, and saying what each came to.

use std::time::Duration;

use futures_util::future::join_all;
use metocast_core::automations::{ActionOutcome, ActionResult, AutomationAction, RecordTarget};
use uuid::Uuid;

use super::arrival::{Arrival, Camera, Timing, recall_and_wait_timed};
use super::matching::{Cause, Step, recall_is_redundant, steps};
use crate::recording_control::Outcome;

/// A recorder that has not answered by now is not going to, and the others go on without it.
pub const RECORDER_TIMEOUT: Duration = Duration::from_secs(10);

/// The devices a rule drives.
pub trait Devices: Camera {
    fn ensure(
        &self,
        target: &RecordTarget,
        recording: bool,
    ) -> impl Future<Output = Result<Outcome, String>> + Send;
}

/// How long the waits in a run take. The tests run it scaled down.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub recorder_timeout: Duration,
    pub camera: Timing,
}

impl Limits {
    pub const LIVE: Self = Self {
        recorder_timeout: RECORDER_TIMEOUT,
        camera: Timing::LIVE,
    };
}

fn result(
    action: &AutomationAction,
    outcome: ActionOutcome,
    message: Option<String>,
) -> ActionResult {
    ActionResult {
        action: action.label(),
        outcome,
        message,
    }
}

async fn recall<D: Devices>(
    devices: &D,
    action: &AutomationAction,
    cause: &Cause,
    chain: &[Uuid],
    limits: Limits,
) -> ActionResult {
    let AutomationAction::PresetRecall {
        preset,
        camera_id,
        wait_for_arrival,
    } = action
    else {
        return result(
            action,
            ActionOutcome::Failed,
            Some("not a camera move".to_string()),
        );
    };
    if recall_is_redundant(action, cause) {
        return result(
            action,
            ActionOutcome::Skipped,
            Some("that preset is what fired the rule".to_string()),
        );
    }
    match recall_and_wait_timed(
        devices,
        *preset,
        *camera_id,
        *wait_for_arrival,
        chain.to_vec(),
        limits.camera,
    )
    .await
    {
        Ok(arrival) => result(
            action,
            ActionOutcome::Done,
            arrival.and_then(Arrival::note).map(str::to_string),
        ),
        Err(error) => result(action, ActionOutcome::Failed, Some(error)),
    }
}

async fn recorder<D: Devices>(
    devices: &D,
    action: &AutomationAction,
    limits: Limits,
) -> ActionResult {
    let (target, recording) = match action {
        AutomationAction::RecordStart { target } => (target, true),
        AutomationAction::RecordStop { target } => (target, false),
        AutomationAction::PresetRecall { .. } => {
            return result(
                action,
                ActionOutcome::Failed,
                Some("not a recorder".to_string()),
            );
        }
    };
    match tokio::time::timeout(limits.recorder_timeout, devices.ensure(target, recording)).await {
        Ok(Ok(Outcome::Done)) => result(action, ActionOutcome::Done, None),
        Ok(Ok(Outcome::Skipped(reason))) => result(action, ActionOutcome::Skipped, Some(reason)),
        Ok(Err(error)) => result(action, ActionOutcome::Failed, Some(error)),
        Err(_) => result(
            action,
            ActionOutcome::Failed,
            Some(format!(
                "no answer in {} s",
                limits.recorder_timeout.as_secs_f32()
            )),
        ),
    }
}

/// Runs the actions in the order they're listed, except that recorder actions next to each
/// other run together. One result per action, in the order listed.
///
/// A failure ends nothing: the actions after it still run, so one dead device does not keep the
/// others from recording.
pub async fn execute<D: Devices>(
    devices: &D,
    actions: &[AutomationAction],
    cause: &Cause,
    chain: &[Uuid],
    limits: Limits,
) -> Vec<ActionResult> {
    let mut results = Vec::with_capacity(actions.len());
    for step in steps(actions) {
        match step {
            Step::Recall(action) => {
                results.push(recall(devices, action, cause, chain, limits).await)
            }
            Step::Recorders(group) => results.extend(
                join_all(
                    group
                        .into_iter()
                        .map(|action| recorder(devices, action, limits)),
                )
                .await,
            ),
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use chrono::Utc;
    use tokio::sync::{broadcast, watch};

    use super::*;
    use crate::connectors::preset_tracker::{Motion, PresetEvent, PresetFinished, PresetStarted};
    use metocast_core::best_effort::LogGone;

    /// What the fake devices were asked, and when, from the start of the test.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Call {
        Recall {
            preset: u8,
            camera: Option<u8>,
        },
        Ensure {
            target: RecordTarget,
            recording: bool,
        },
    }

    /// How long a recorder takes, and what it answers.
    type Scripted = (RecordTarget, Duration, Result<Outcome, String>);

    struct Rig {
        began: Instant,
        calls: Mutex<Vec<(Call, Duration)>>,
        motion: Arc<watch::Sender<Motion>>,
        events: broadcast::Sender<PresetEvent>,
        /// Per target: how long it takes and what it answers.
        recorders: Mutex<Vec<Scripted>>,
        recall_error: Option<String>,
        /// The camera finishes this long after the recall (or never).
        finishes_after: Option<Duration>,
        /// The gimbal really moves; a camera already at the preset stays still.
        camera_moves: bool,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                began: Instant::now(),
                calls: Mutex::new(Vec::new()),
                motion: Arc::new(
                    watch::channel(Motion {
                        camera: Some(1),
                        active: Some(0),
                        preset_seconds: Some(0.16),
                        pan: Some(653),
                        tilt: Some(-139),
                    })
                    .0,
                ),
                events: broadcast::channel(16).0,
                recorders: Mutex::new(Vec::new()),
                recall_error: None,
                finishes_after: Some(Duration::from_millis(160)),
                camera_moves: true,
            }
        }

        fn script(&self, target: RecordTarget, takes: Duration, answer: Result<Outcome, String>) {
            self.recorders.lock().unwrap().push((target, takes, answer));
        }

        fn log(&self, call: Call) {
            self.calls
                .lock()
                .unwrap()
                .push((call, self.began.elapsed()));
        }

        fn calls(&self) -> Vec<(Call, Duration)> {
            self.calls.lock().unwrap().clone()
        }

        fn started_at(&self, wanted: &Call) -> Duration {
            self.calls()
                .into_iter()
                .find(|(call, _)| call == wanted)
                .map(|(_, at)| at)
                .unwrap_or_else(|| panic!("{wanted:?} was never called"))
        }
    }

    impl Camera for Rig {
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
            _chain: Vec<Uuid>,
        ) -> Result<(), String> {
            self.log(Call::Recall {
                preset,
                camera: camera_id,
            });
            if let Some(error) = &self.recall_error {
                return Err(error.clone());
            }
            if let Some(after) = self.finishes_after {
                let events = self.events.clone();
                let motion = Arc::clone(&self.motion);
                let moves = self.camera_moves;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(6)).await;
                    if moves {
                        motion.send_modify(|m| m.pan = Some(300));
                    }
                    events
                        .send(PresetEvent::Started(PresetStarted {
                            preset,
                            camera_id: 1,
                            at: Utc::now(),
                            chain: Vec::new(),
                        }))
                        .log_gone();
                    tokio::time::sleep(after).await;
                    events
                        .send(PresetEvent::Finished(PresetFinished {
                            preset,
                            camera_id: 1,
                            at: Utc::now(),
                            chain: Vec::new(),
                        }))
                        .log_gone();
                });
            }
            Ok(())
        }
    }

    impl Devices for Rig {
        async fn ensure(&self, target: &RecordTarget, recording: bool) -> Result<Outcome, String> {
            self.log(Call::Ensure {
                target: target.clone(),
                recording,
            });
            let scripted = self
                .recorders
                .lock()
                .unwrap()
                .iter()
                .find(|(wanted, _, _)| wanted == target)
                .map(|(_, takes, answer)| (*takes, answer.clone()));
            let (takes, answer) = scripted.unwrap_or((Duration::ZERO, Ok(Outcome::Done)));
            tokio::time::sleep(takes).await;
            answer
        }
    }

    const LIMITS: Limits = Limits {
        recorder_timeout: Duration::from_millis(80),
        camera: Timing {
            no_move_after: Duration::from_millis(20),
            already_there_after: Duration::from_millis(30),
            overrun: Duration::from_millis(100),
            tick: Duration::from_millis(2),
        },
    };

    fn recall_action(preset: u8) -> AutomationAction {
        AutomationAction::PresetRecall {
            preset,
            camera_id: None,
            wait_for_arrival: true,
        }
    }

    fn start(target: RecordTarget) -> AutomationAction {
        AutomationAction::RecordStart { target }
    }

    fn stop(target: RecordTarget) -> AutomationAction {
        AutomationAction::RecordStop { target }
    }

    fn outcomes(results: &[ActionResult]) -> Vec<ActionOutcome> {
        results.iter().map(|result| result.outcome).collect()
    }

    fn typed(rig: &Rig, actions: &[AutomationAction]) -> impl Future<Output = Vec<ActionResult>> {
        let cause = Cause::Channel {
            label: "Wireless 1".to_string(),
            live: true,
        };
        let chain = vec![Uuid::new_v4()];
        let actions = actions.to_vec();
        async move { execute(rig, &actions, &cause, &chain, LIMITS).await }
    }

    #[tokio::test]
    async fn a_start_rule_moves_the_camera_first_and_starts_every_recorder_once_it_arrives() {
        let rig = Rig::new();
        let results = typed(
            &rig,
            &[
                recall_action(3),
                start(RecordTarget::Obs),
                start(RecordTarget::Atem),
                start(RecordTarget::RodecasterAudio),
            ],
        )
        .await;

        assert_eq!(outcomes(&results), vec![ActionOutcome::Done; 4]);
        assert_eq!(
            results
                .iter()
                .map(|r| r.action.as_str())
                .collect::<Vec<_>>(),
            [
                "Move camera to preset 3",
                "Start OBS",
                "Start ATEM",
                "Start Metocast audio"
            ]
        );

        // Nothing starts before the camera's move (6 ms in, 160 ms long) has finished.
        let obs = rig.started_at(&Call::Ensure {
            target: RecordTarget::Obs,
            recording: true,
        });
        assert!(obs >= Duration::from_millis(166), "{obs:?}");
        let calls = rig.calls();
        assert_eq!(
            calls[0].0,
            Call::Recall {
                preset: 3,
                camera: None
            }
        );
    }

    #[tokio::test]
    async fn a_camera_already_at_the_preset_does_not_hold_the_recorders_up_for_the_whole_move() {
        let mut rig = Rig::new();
        rig.camera_moves = false;
        let results = typed(&rig, &[recall_action(3), start(RecordTarget::Obs)]).await;

        assert_eq!(outcomes(&results), vec![ActionOutcome::Done; 2]);
        assert_eq!(
            results[0].message.as_deref(),
            Some("the camera was already there")
        );
        // The move would have run 166 ms; the position says it hasn't moved after ~36 ms.
        let obs = rig.started_at(&Call::Ensure {
            target: RecordTarget::Obs,
            recording: true,
        });
        assert!(obs >= Duration::from_millis(36), "{obs:?}");
        assert!(obs < Duration::from_millis(160), "{obs:?}");
    }

    #[tokio::test]
    async fn recorders_next_to_each_other_start_together() {
        let rig = Rig::new();
        for target in [
            RecordTarget::Obs,
            RecordTarget::Atem,
            RecordTarget::Rodecaster,
        ] {
            rig.script(target, Duration::from_millis(40), Ok(Outcome::Done));
        }
        let began = Instant::now();
        let results = typed(
            &rig,
            &[
                start(RecordTarget::Obs),
                start(RecordTarget::Atem),
                start(RecordTarget::Rodecaster),
            ],
        )
        .await;
        let took = began.elapsed();

        assert_eq!(outcomes(&results), vec![ActionOutcome::Done; 3]);
        // Three 40 ms starts one after the other would take 120 ms.
        assert!(took < Duration::from_millis(115), "{took:?}");
        let starts: Vec<Duration> = rig.calls().into_iter().map(|(_, at)| at).collect();
        let spread = *starts.iter().max().unwrap() - *starts.iter().min().unwrap();
        assert!(spread < Duration::from_millis(30), "{spread:?}");
    }

    #[tokio::test]
    async fn one_dead_device_holds_up_none_of_the_rest() {
        let rig = Rig::new();
        rig.script(
            RecordTarget::Obs,
            Duration::from_secs(30),
            Ok(Outcome::Done),
        );
        rig.script(
            RecordTarget::Atem,
            Duration::ZERO,
            Err("atem_not_connected".to_string()),
        );
        let began = Instant::now();
        let results = typed(
            &rig,
            &[
                start(RecordTarget::Obs),
                start(RecordTarget::Atem),
                start(RecordTarget::RodecasterAudio),
            ],
        )
        .await;

        assert_eq!(
            outcomes(&results),
            vec![
                ActionOutcome::Failed,
                ActionOutcome::Failed,
                ActionOutcome::Done
            ]
        );
        assert_eq!(results[0].message.as_deref(), Some("no answer in 0.08 s"));
        assert_eq!(results[1].message.as_deref(), Some("atem_not_connected"));
        assert_eq!(results[2].message, None);
        assert!(began.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_recorder_that_is_already_so_is_skipped_with_its_reason() {
        let rig = Rig::new();
        rig.script(
            RecordTarget::Obs,
            Duration::ZERO,
            Ok(Outcome::Skipped("already recording".to_string())),
        );
        let results = typed(&rig, &[start(RecordTarget::Obs)]).await;
        assert_eq!(outcomes(&results), vec![ActionOutcome::Skipped]);
        assert_eq!(results[0].message.as_deref(), Some("already recording"));
    }

    #[tokio::test]
    async fn the_move_that_fired_the_rule_is_skipped_and_the_recorders_still_start() {
        let rig = Rig::new();
        let cause = Cause::Preset {
            preset: 3,
            camera_id: 1,
        };
        let results = execute(
            &rig,
            &[recall_action(3), start(RecordTarget::Obs)],
            &cause,
            &[],
            LIMITS,
        )
        .await;

        assert_eq!(
            outcomes(&results),
            vec![ActionOutcome::Skipped, ActionOutcome::Done]
        );
        assert!(
            !rig.calls()
                .iter()
                .any(|(call, _)| matches!(call, Call::Recall { .. })),
            "the camera is already there"
        );
    }

    #[tokio::test]
    async fn a_recall_that_fails_does_not_keep_the_recorders_waiting() {
        let mut rig = Rig::new();
        rig.recall_error = Some("middlecontrol_not_connected".to_string());
        let began = Instant::now();
        let results = typed(&rig, &[recall_action(3), start(RecordTarget::Obs)]).await;

        assert_eq!(
            outcomes(&results),
            vec![ActionOutcome::Failed, ActionOutcome::Done]
        );
        assert_eq!(
            results[0].message.as_deref(),
            Some("middlecontrol_not_connected")
        );
        assert!(began.elapsed() < Duration::from_millis(150));
    }

    #[tokio::test]
    async fn a_move_that_never_starts_is_noted_and_the_recorders_start() {
        let mut rig = Rig::new();
        rig.finishes_after = None;
        // Middle Control starts nothing: nothing sends Started either.
        rig.events = broadcast::channel(16).0;
        let results = execute(
            &rig,
            &[recall_action(3), start(RecordTarget::Obs)],
            &Cause::Manual,
            &[],
            LIMITS,
        )
        .await;

        assert_eq!(outcomes(&results), vec![ActionOutcome::Done; 2]);
        assert_eq!(
            results[0].message.as_deref(),
            Some("Middle Control did not start the move")
        );
    }

    #[tokio::test]
    async fn a_stop_rule_stops_the_recorders_before_it_moves_the_camera() {
        let rig = Rig::new();
        rig.script(
            RecordTarget::Obs,
            Duration::from_millis(30),
            Ok(Outcome::Done),
        );
        let results = typed(
            &rig,
            &[
                stop(RecordTarget::Obs),
                stop(RecordTarget::Atem),
                recall_action(1),
            ],
        )
        .await;

        assert_eq!(outcomes(&results), vec![ActionOutcome::Done; 3]);
        let recall_at = rig.started_at(&Call::Recall {
            preset: 1,
            camera: None,
        });
        // The slowest stop (30 ms) is done before the camera starts to move.
        assert!(recall_at >= Duration::from_millis(30), "{recall_at:?}");
        let calls = rig.calls();
        assert!(matches!(calls.last(), Some((Call::Recall { .. }, _))));
    }

    #[tokio::test]
    async fn results_come_back_in_the_order_the_actions_are_listed() {
        let rig = Rig::new();
        let results = typed(
            &rig,
            &[
                start(RecordTarget::Obs),
                recall_action(2),
                stop(RecordTarget::Atem),
                start(RecordTarget::Middlecontrol { camera_id: Some(2) }),
            ],
        )
        .await;
        assert_eq!(
            results
                .iter()
                .map(|r| r.action.as_str())
                .collect::<Vec<_>>(),
            [
                "Start OBS",
                "Move camera to preset 2",
                "Stop ATEM",
                "Start Middle Control camera 2"
            ]
        );
    }

    #[tokio::test]
    async fn no_actions_is_no_results() {
        let rig = Rig::new();
        assert!(
            execute(&rig, &[], &Cause::Manual, &[], LIMITS)
                .await
                .is_empty()
        );
    }
}
