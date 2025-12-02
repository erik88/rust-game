use rustgame::time::TimeProvider;

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
}

impl TimeProvider for FixedTime {
    fn delta_time(&mut self) -> f32 {
        // Returns immediately without sleeping
        self.frame_time
    }

    fn wait_for_next_frame(&mut self) {
        // No-op: tests run at full speed without sleeping
    }
}
