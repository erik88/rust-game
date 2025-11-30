use std::time::Instant;

/// Trait for abstracting time measurement
pub trait TimeProvider {
    /// Get the time delta since the last frame
    fn delta_time(&mut self) -> f32;
}

/// Real-time provider using actual system time
pub struct RealTime {
    last_frame: Instant,
}

impl RealTime {
    pub fn new() -> Self {
        Self {
            last_frame: Instant::now(),
        }
    }
}

impl TimeProvider for RealTime {
    fn delta_time(&mut self) -> f32 {
        let now = Instant::now();
        let delta = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        delta
    }
}

/// Fixed-time provider for testing - always returns 1/60th of a second
/// Does NOT sleep, allowing tests to run at full speed
pub struct FixedTime {
    frame_time: f32,
}

impl FixedTime {
    pub fn new() -> Self {
        Self {
            frame_time: 1.0 / 60.0, // ~0.01667 seconds
        }
    }

    pub fn new_with_fps(fps: u32) -> Self {
        Self {
            frame_time: 1.0 / fps as f32,
        }
    }
}

impl TimeProvider for FixedTime {
    fn delta_time(&mut self) -> f32 {
        // Returns immediately without sleeping
        self.frame_time
    }
}
