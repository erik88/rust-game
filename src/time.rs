use std::time::{Duration, Instant};

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
    target_frame_time: Duration,
}

impl RealTime {
    pub fn new() -> Self {
        Self {
            last_frame: Instant::now(),
            frame_start: Instant::now(),
            target_frame_time: Duration::from_micros(16667), // ~60 Hz
        }
    }
}

impl TimeProvider for RealTime {
    fn delta_time(&mut self) -> f32 {
        let now = Instant::now();
        self.frame_start = now;
        let delta = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        delta
    }

    fn wait_for_next_frame(&mut self) {
        let frame_time = self.frame_start.elapsed();
        if frame_time < self.target_frame_time {
            std::thread::sleep(self.target_frame_time - frame_time);
        }
    }
}
