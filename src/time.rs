use std::time::{Duration, Instant};

/// Upper bound on a single frame's delta, in seconds.
///
/// A hitch (level load, dragging the window, the OS descheduling us) would
/// otherwise hand the physics one enormous step and teleport the player, or
/// tunnel him through a platform. Clamping trades that for a brief slow-motion
/// stretch instead.
///
/// The specific value is tied to [`crate::player`]'s `PLATFORM_RIDE_TOLERANCE`
/// (6 px): platforms move at 100 px/s, so a frame longer than 60 ms carries a
/// platform further than the tolerance and the player loses his footing on
/// something he was riding. 50 ms — a 20 fps floor — stays inside that.
const MAX_DELTA: f32 = 0.05;

/// Frames faster than this are paced by sleeping. See
/// [`RealTime::wait_for_next_frame`].
const MIN_FRAME_TIME: Duration = Duration::from_millis(4);

/// The size of one physics step, in seconds.
///
/// Physics runs at this rate no matter how fast the display refreshes, because
/// two things in [`crate::player`] are step-size dependent and were both tuned
/// at 60 Hz: gravity is integrated with semi-implicit Euler, whose jump apex
/// varies with `dt`, and the jump-cut damping is applied once per step, so its
/// effective strength scales with the step rate.
pub const FIXED_DT: f32 = 1.0 / 60.0;

/// Decouples the physics rate from the display rate.
///
/// Frame times are banked and paid out in whole [`FIXED_DT`] steps, so a 120 Hz
/// display runs one step every other rendered frame and a 60 Hz display runs
/// one per frame, with the remainder carried forward rather than dropped.
///
/// There is no render interpolation, so the on-screen position is whatever the
/// last completed step produced. That is exact when the refresh rate is a whole
/// multiple of 60 (each state is simply shown once or twice); on an oddly
/// divided rate such as 144 Hz the repeat pattern is uneven and shows up as
/// mild judder. Interpolating would mean carrying a previous position for the
/// player and every platform, block and effect through rendering.
#[derive(Default)]
pub struct FixedTimestep {
    /// Held in f64 deliberately. An f32 accumulator loses enough precision
    /// across a few hundred additions to shed roughly a step per second, which
    /// would make the game run slightly slow the longer it stayed open.
    accumulator: f64,
}

impl FixedTimestep {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bank a rendered frame's elapsed time and report how many physics steps
    /// are now due. Callers pass a delta already capped by [`MAX_DELTA`], which
    /// holds this to at most three steps and so rules out a death spiral where
    /// each frame owes more simulation than it can run.
    pub fn steps_for(&mut self, frame_delta: f32) -> usize {
        self.accumulator += frame_delta as f64;
        let steps = (self.accumulator / FIXED_DT as f64) as usize;
        self.accumulator -= steps as f64 * FIXED_DT as f64;
        steps
    }
}

/// Trait for abstracting time measurement
pub trait TimeProvider {
    /// Get the time delta since the last frame
    fn delta_time(&mut self) -> f32;

    /// Wait for the next frame to maintain target frame rate
    /// This method should sleep if needed to pace the frame rate
    fn wait_for_next_frame(&mut self);
}

/// Real-time provider using actual system time
pub struct RealTime {
    last_frame: Instant,
    frame_start: Instant,
}

impl Default for RealTime {
    fn default() -> Self {
        Self::new()
    }
}

impl RealTime {
    pub fn new() -> Self {
        Self {
            last_frame: Instant::now(),
            frame_start: Instant::now(),
        }
    }
}

impl TimeProvider for RealTime {
    fn delta_time(&mut self) -> f32 {
        let now = Instant::now();
        self.frame_start = now;
        let delta = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        delta.min(MAX_DELTA)
    }

    /// Safety net only — frame pacing comes from vsync, which blocks inside
    /// `canvas.present()` until the display's next refresh.
    ///
    /// Sleeping to a fixed ~60 Hz budget here instead would be wrong twice
    /// over: a free-running software timer drifts against the display's refresh
    /// (the source of the periodic stutter this replaces), and on a 120 Hz
    /// display the sleep would push the loop past one refresh interval so it
    /// missed every other one.
    ///
    /// What is left is a floor rather than a cap: if a frame comes back
    /// implausibly fast we did not really get vsync (software renderer, some VM
    /// and screen-sharing setups), and without this the loop would spin at
    /// thousands of frames per second, pegging the CPU and draining the
    /// battery. A legitimate high-refresh display never trips it.
    fn wait_for_next_frame(&mut self) {
        let frame_time = self.frame_start.elapsed();
        if frame_time < MIN_FRAME_TIME {
            std::thread::sleep(MIN_FRAME_TIME - frame_time);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_stall_is_clamped_to_max_delta() {
        let mut time = RealTime::new();
        time.delta_time();
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            time.delta_time() <= MAX_DELTA,
            "a stalled frame must not hand the physics an unbounded step"
        );
    }

    #[test]
    fn max_delta_keeps_platforms_within_ride_tolerance() {
        // Platforms move at PLATFORM_SPEED (100 px/s) and are updated before the
        // player, so one frame's travel has to stay under the 6 px
        // PLATFORM_RIDE_TOLERANCE or a rider loses his footing. Kept here so
        // raising MAX_DELTA can't silently break that.
        assert!(100.0 * MAX_DELTA < 6.0);
    }

    #[test]
    fn a_120hz_display_steps_physics_every_other_frame() {
        let mut timestep = FixedTimestep::new();
        let frame = 1.0 / 120.0;
        let pattern: Vec<usize> = (0..6).map(|_| timestep.steps_for(frame)).collect();
        assert_eq!(pattern, vec![0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn a_60hz_display_steps_physics_once_per_frame() {
        let mut timestep = FixedTimestep::new();
        let pattern: Vec<usize> = (0..4).map(|_| timestep.steps_for(FIXED_DT)).collect();
        assert_eq!(pattern, vec![1, 1, 1, 1]);
    }

    #[test]
    fn leftover_time_is_carried_rather_than_dropped() {
        // 144 Hz divides into no whole number of 60 Hz steps, so every frame
        // leaves a remainder. Ten seconds of them must still add up to ten
        // seconds of physics: dropping the remainder would yield zero steps
        // (a frame is shorter than a step), and an f32 accumulator drifts to
        // roughly a step short per second.
        let mut timestep = FixedTimestep::new();
        let frame = 1.0 / 144.0;
        let steps: i64 = (0..1440).map(|_| timestep.steps_for(frame) as i64).sum();
        assert!(
            (steps - 600).abs() <= 1,
            "ten seconds of 144 Hz frames gave {steps} steps, expected ~600"
        );
    }

    #[test]
    fn a_clamped_stall_cannot_spiral() {
        // Sustained worst-case frames must keep owing a bounded amount of
        // simulation; if the debt grew, each frame would run more steps than
        // the last and the game would seize up.
        let mut timestep = FixedTimestep::new();
        for _ in 0..100 {
            assert!(timestep.steps_for(MAX_DELTA) <= 3);
        }
    }

    #[test]
    fn short_frame_is_floored_so_a_missing_vsync_cannot_spin() {
        let mut time = RealTime::new();
        time.delta_time();
        let before = Instant::now();
        time.wait_for_next_frame();
        assert!(before.elapsed() >= MIN_FRAME_TIME / 2);
    }
}
