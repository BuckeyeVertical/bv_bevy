//! Render-only pose smoothing for the desktop view.
//!
//! Gazebo poses arrive at ~28 Hz while Bevy renders at an unrelated rate, so
//! snapping to the latest pose makes the vehicle advance in uneven steps (0, 1
//! or 2 updates per frame), which reads as stutter and vibration behind the
//! follow camera. Entities with a [`PoseHistory`] also get a [`SmoothedPose`]:
//! the recorded poses played back slightly in the past on a clock that tracks
//! simulation time, interpolated between snapshots.
//!
//! The entity's `Transform` stays the exact latest pose. The onboard camera and
//! the frame metadata sent to the vision stack never use the smoothed pose.

use std::collections::VecDeque;

use bevy::prelude::*;

/// How far behind the newest snapshot playback runs (~2 snapshots at 28 Hz).
const PLAYBACK_DELAY_S: f64 = 0.08;
/// Larger drift than this (pause, reconnect, sim reset) snaps the clock.
const MAX_DRIFT_S: f64 = 0.35;
/// Rate at which the playback clock converges on its target.
const CLOCK_CORRECTION_PER_S: f64 = 3.0;
const HISTORY_LEN: usize = 32;

#[derive(Component, Debug, Default)]
pub struct PoseHistory {
    samples: VecDeque<(f64, Transform)>,
}

impl PoseHistory {
    pub(super) fn record(&mut self, sim_time_s: f64, pose: Transform) {
        // A reset or new stream goes back in time: start over.
        if self.samples.back().is_some_and(|(t, _)| sim_time_s <= *t) {
            self.samples.clear();
        }
        self.samples.push_back((sim_time_s, pose));
        while self.samples.len() > HISTORY_LEN {
            self.samples.pop_front();
        }
    }

    fn newest_time(&self) -> Option<f64> {
        self.samples.back().map(|(t, _)| *t)
    }

    /// Pose at `time`, interpolated between the bracketing samples. Never
    /// extrapolates: outside the recorded range it holds the nearest sample.
    pub fn sample(&self, time: f64) -> Option<Transform> {
        let (first_t, first) = *self.samples.front()?;
        if time <= first_t {
            return Some(first);
        }
        for pair in self.samples.iter().collect::<Vec<_>>().windows(2) {
            let (t0, a) = *pair[0];
            let (t1, b) = *pair[1];
            if time <= t1 {
                let f = ((time - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
                return Some(Transform {
                    translation: a.translation.lerp(b.translation, f),
                    rotation: a.rotation.slerp(b.rotation, f),
                    scale: a.scale.lerp(b.scale, f),
                });
            }
        }
        self.samples.back().map(|(_, pose)| *pose)
    }
}

/// Smoothed pose for rendering; see the module docs.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct SmoothedPose(pub Transform);

#[derive(Resource, Debug, Default)]
pub(super) struct PlaybackClock {
    time: Option<f64>,
}

impl PlaybackClock {
    fn advance(&mut self, newest: f64, dt: f64) -> f64 {
        let target = newest - PLAYBACK_DELAY_S;
        let time = match self.time {
            Some(time) if (time + dt - target).abs() < MAX_DRIFT_S => {
                let time = time + dt;
                time + (target - time) * (CLOCK_CORRECTION_PER_S * dt).min(1.0)
            }
            _ => target,
        };
        self.time = Some(time);
        time
    }
}

pub(super) fn update_smoothed_poses(
    time: Res<Time>,
    mut clock: ResMut<PlaybackClock>,
    mut poses: Query<(&PoseHistory, &mut SmoothedPose)>,
) {
    let Some(newest) = poses.iter().filter_map(|(history, _)| history.newest_time()).reduce(f64::max) else {
        return;
    };
    let playback = clock.advance(newest, time.delta_secs_f64());
    for (history, mut smoothed) in &mut poses {
        if let Some(pose) = history.sample(playback) {
            smoothed.0 = pose;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32) -> Transform {
        Transform::from_xyz(x, 0.0, 0.0)
    }

    #[test]
    fn interpolates_between_snapshots_and_holds_at_the_ends() {
        let mut history = PoseHistory::default();
        history.record(1.0, at(0.0));
        history.record(1.036, at(0.36));
        assert!((history.sample(1.018).unwrap().translation.x - 0.18).abs() < 1e-4);
        assert_eq!(history.sample(0.5).unwrap().translation.x, 0.0);
        assert_eq!(history.sample(2.0).unwrap().translation.x, 0.36);
    }

    #[test]
    fn stream_reset_clears_history() {
        let mut history = PoseHistory::default();
        history.record(5.0, at(5.0));
        history.record(0.1, at(1.0));
        assert_eq!(history.samples.len(), 1);
    }

    #[test]
    fn playback_clock_advances_smoothly_with_frame_time() {
        let mut clock = PlaybackClock::default();
        let mut newest = 10.0;
        let mut last = clock.advance(newest, 0.0);
        // Snapshots every 36 ms, frames every 50 ms: playback moves forward
        // monotonically with near-frame-sized steps instead of jumping.
        for frame in 0..40 {
            if frame % 3 != 2 {
                newest += 0.036 * 1.4;
            }
            let now = clock.advance(newest, 0.05);
            let step = now - last;
            assert!(step > 0.0 && step < 0.1, "step {step}");
            last = now;
        }
        assert!((newest - PLAYBACK_DELAY_S - last).abs() < 0.1);
    }
}
