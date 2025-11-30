//! Game Tests with Optional Visualization
//!
//! This test module provides comprehensive tests for the game engine.
//!
//! # Visualization
//!
//! Tests can be visualized by setting the `VISUALIZE_TEST` environment variable:
//!
//! ```bash
//! # Visualize a specific test
//! VISUALIZE_TEST=1 cargo test test_downward_platform_stays_inactive_while_player_on_solid_ground -- --nocapture
//!
//! # Visualize all tests (run one at a time)
//! VISUALIZE_TEST=1 cargo test --test game_tests -- --test-threads=1 --nocapture
//! ```
//!
//! When visualization is enabled:
//! - A window will open showing the game scene
//! - The test will run frame-by-frame with rendering
//! - The window will pause at the end, showing the final state
//! - Close the window or press ESC to continue to the next test

use rustgame::engine::GameEngine;
use rustgame::input::{InputSource, InputState, QueuedInput};
use rustgame::player::Player;
use rustgame::time::{FixedTime, TimeProvider};
use rustgame::tilemap::TileMap;
use std::env;

/// Test runner for game simulation
pub struct TestRunner {
    engine: GameEngine,
    input_source: QueuedInput,
    time_provider: FixedTime,
    visualize: bool,
    #[allow(dead_code)]
    sdl_context: Option<sdl2::Sdl>,
    canvas: Option<sdl2::render::WindowCanvas>,
    character_texture: Option<sdl2::render::Texture<'static>>,
    tilemap_texture: Option<sdl2::render::Texture<'static>>,
}

impl TestRunner {
    /// Create a new test runner with default game state
    /// Checks VISUALIZE_TEST environment variable to enable visualization
    pub fn new() -> Self {
        let visualize = env::var("VISUALIZE_TEST").is_ok();

        if visualize {
            Self::new_visualized()
        } else {
            Self {
                engine: GameEngine::new(),
                input_source: QueuedInput::new(),
                time_provider: FixedTime::new(),
                visualize: false,
                sdl_context: None,
                canvas: None,
                character_texture: None,
                tilemap_texture: None,
            }
        }
    }

    /// Create a test runner with custom player position and tilemap
    pub fn new_with(player: Player, tilemap: TileMap) -> Self {
        let visualize = env::var("VISUALIZE_TEST").is_ok();

        if visualize {
            let mut runner = Self::new_visualized();
            runner.engine = GameEngine::new_with(player, tilemap);
            runner
        } else {
            Self {
                engine: GameEngine::new_with(player, tilemap),
                input_source: QueuedInput::new(),
                time_provider: FixedTime::new(),
                visualize: false,
                sdl_context: None,
                canvas: None,
                character_texture: None,
                tilemap_texture: None,
            }
        }
    }

    /// Create a new test runner with visualization enabled
    fn new_visualized() -> Self {
        use sdl2::image::{InitFlag, LoadTexture};

        // Initialize SDL2
        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");
        let video_subsystem = sdl_context.video().expect("Failed to get video subsystem");

        // Initialize SDL2_image with PNG support
        let _image_context = sdl2::image::init(InitFlag::PNG).expect("Failed to initialize SDL2_image");

        // Create a window
        let window = video_subsystem
            .window("Test Visualization", 800, 600)
            .position_centered()
            .build()
            .expect("Failed to create window");

        // Create a canvas for rendering
        let canvas = window
            .into_canvas()
            .accelerated()
            .build()
            .expect("Failed to create canvas");

        // Get the texture creator
        let texture_creator = canvas.texture_creator();

        // Load textures - we need to leak them to get 'static lifetime
        // This is okay for tests since they're short-lived
        let character_texture = texture_creator.load_texture("character.png")
            .expect("Failed to load character.png");
        let tilemap_texture = texture_creator.load_texture("tilemap.png")
            .expect("Failed to load tilemap.png");

        // Transmute to 'static - this is safe because we're holding the SDL context
        let character_texture: sdl2::render::Texture<'static> = unsafe { std::mem::transmute(character_texture) };
        let tilemap_texture: sdl2::render::Texture<'static> = unsafe { std::mem::transmute(tilemap_texture) };

        Self {
            engine: GameEngine::new(),
            input_source: QueuedInput::new(),
            time_provider: FixedTime::new(),
            visualize: true,
            sdl_context: Some(sdl_context),
            canvas: Some(canvas),
            character_texture: Some(character_texture),
            tilemap_texture: Some(tilemap_texture),
        }
    }

    /// Queue an input state for a specific frame number
    pub fn queue_input(&mut self, frame: usize, input: InputState) {
        self.input_source.queue_input(frame, input);
    }

    /// Render the current frame (if visualization is enabled)
    fn render(&mut self) {
        if !self.visualize {
            return;
        }

        if let (Some(canvas), Some(character_tex), Some(tilemap_tex)) =
            (&mut self.canvas, &self.character_texture, &self.tilemap_texture) {

            // Camera follows player
            let player = self.engine.player();
            let tilemap = self.engine.tilemap();
            let level_width = (tilemap.width as i32) * (tilemap.tile_size as i32);
            let max_camera_x = (level_width - 800).max(0);
            let camera_x = (player.x as i32 - 400).max(0).min(max_camera_x);

            // Clear the canvas with sky blue background
            canvas.set_draw_color(sdl2::pixels::Color::RGB(135, 206, 235));
            canvas.clear();

            // Render tilemap
            tilemap.render(canvas, tilemap_tex, camera_x);

            // Render player
            player.render(canvas, character_tex, camera_x);

            // Present the rendered frame
            canvas.present();

            // Small delay to make it visible
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }

    /// Run N frames of the game simulation
    /// Returns a reference to the engine for inspection
    pub fn run_frames(&mut self, count: usize) -> &GameEngine {
        for _ in 0..count {
            let delta_time = self.time_provider.delta_time();
            let input = self.input_source.poll();
            self.engine.step(&input, delta_time);

            // Render if visualization is enabled
            self.render();
        }
        &self.engine
    }

    /// Pause and wait for user to close the window (if visualization is enabled)
    pub fn pause_for_inspection(&mut self) {
        if !self.visualize {
            return;
        }

        println!("Test visualization paused. Close the window to continue...");

        if let Some(ref sdl_context) = self.sdl_context {
            let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

            'wait: loop {
                // Render one more time
                self.render();

                // Check for quit events
                for event in event_pump.poll_iter() {
                    use sdl2::event::Event;
                    match event {
                        Event::Quit {..} => break 'wait,
                        Event::KeyDown { keycode: Some(sdl2::keyboard::Keycode::Escape), .. } => break 'wait,
                        _ => {}
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        }
    }

    /// Get reference to the game engine
    pub fn engine(&self) -> &GameEngine {
        &self.engine
    }

    /// Get mutable reference to the game engine
    pub fn engine_mut(&mut self) -> &mut GameEngine {
        &mut self.engine
    }
}

impl Drop for TestRunner {
    fn drop(&mut self) {
        // If visualization was enabled, pause at the end of the test
        if self.visualize {
            self.pause_for_inspection();
        }
    }
}

/// Helper function to create a simple tilemap with a platform at a specific position
pub fn create_platform_at(y_tile: usize) -> TileMap {
    let height = 12;
    let width = 30;

    // Create empty level
    let mut level_data = vec![vec![0; width]; height];

    // Add a solid platform row at the specified y position
    if y_tile < height {
        for x in 0..width {
            level_data[y_tile][x] = 1; // Solid tile
        }
    }

    // We need to construct TileMap properly, but TileMap::new() creates a hardcoded level
    // For now, let's just use the default TileMap
    TileMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_falls_and_lands_on_tile() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Position player in the air above a known solid tile
        // Looking at the default level, there's a floor at y=11 (tile row 11)
        // Tile row 11 starts at y = 11 * 40 = 440 pixels
        // Player should land on top of this at y = 440 - 38 (player height) = 402
        runner.engine_mut().player_mut().x = 100.0;
        runner.engine_mut().player_mut().y = 100.0; // Start in the air
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Record initial position
        let initial_y = runner.engine().player().y;

        // Run enough frames for player to fall and land
        // With gravity of 1200 px/s² and starting from rest:
        // Distance to fall: ~300 pixels
        // Time to fall: sqrt(2 * 300 / 1200) ≈ 0.7 seconds ≈ 42 frames
        // Let's run 60 frames to be safe
        runner.run_frames(60);

        let engine = runner.engine();
        let player = engine.player();

        // Player should have fallen (y increased)
        assert!(player.y > initial_y,
            "Player should have fallen from y={} to y={}", initial_y, player.y);

        // Player should be on ground
        assert!(player.on_ground,
            "Player should be on ground after falling");

        // Player's vertical velocity should be zero (stopped falling)
        assert_eq!(player.vel_y, 0.0,
            "Player should have zero vertical velocity when on ground");

        // Player should have landed on top of a tile (y position should be stable)
        // Run a few more frames to ensure player stays in place
        let landed_y = player.y;
        runner.run_frames(10);
        let final_y = runner.engine().player().y;

        assert_eq!(landed_y, final_y,
            "Player should remain at y={} after landing, but is at y={}",
            landed_y, final_y);
    }

    #[test]
    fn test_player_rides_horizontally_moving_platform() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Set up a clean test area with a right-moving platform (type 10)
        let platform_tile_x = 10;
        let platform_tile_y = 8;

        // Clear the area
        for dy in -1..=5 {
            for dx in -2..=2 {
                runner.engine_mut().tilemap_mut().set_tile(platform_tile_x + dx, platform_tile_y + dy, 0);
            }
        }

        // Clear all existing platforms and add our test platform
        runner.engine_mut().tilemap_mut().clear_platforms();
        runner.engine_mut().tilemap_mut().add_platform(platform_tile_x, platform_tile_y, 10); // 10 = right-moving

        // Position player in the air above the platform
        // Platform is at (400, 320), player starts at (400, 250) - 70 pixels above
        let platform_x = (platform_tile_x as f32) * 40.0;
        let platform_y = (platform_tile_y as f32) * 40.0;
        let player_start_x = platform_x;
        let player_start_y = platform_y - 70.0;

        runner.engine_mut().player_mut().x = player_start_x;
        runner.engine_mut().player_mut().y = player_start_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Run a few frames to let player fall and land on the platform
        runner.run_frames(30);

        // Verify player has landed on the platform
        assert!(runner.engine().player().on_ground,
            "Player should have landed on the platform");

        // Record player's X position after landing
        let x_after_landing = runner.engine().player().x;

        // Run more frames - platform should be moving and carrying the player
        // Platform speed is 100 px/s, so in 0.5 seconds (30 frames) it should move ~50 pixels
        runner.run_frames(30);

        let final_x = runner.engine().player().x;

        // Player should have moved significantly to the right with the platform
        let distance_moved = final_x - x_after_landing;
        assert!(distance_moved > 40.0,
            "Player should have moved right with platform (moved {} pixels, expected > 40)",
            distance_moved);

        // Player should still be on ground (riding the platform)
        assert!(runner.engine().player().on_ground,
            "Player should still be on the platform");
    }

    #[test]
    fn test_player_on_decaying_block_falls_through() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Clear the area and set up a single decaying block (type 4)
        // We'll use tile position (10, 8) - above the floor
        let tile_x = 10;
        let tile_y = 8;

        // Clear surrounding tiles and below to ensure clean test
        // Clear a column of tiles below so player can actually fall
        for dy in -1..=5 {
            for dx in -1..=1 {
                runner.engine_mut().tilemap_mut().set_tile(tile_x + dx, tile_y + dy, 0);
            }
        }

        // Place a decaying block at (10, 8)
        runner.engine_mut().tilemap_mut().set_tile(tile_x, tile_y, 4);

        // Position player standing on top of this block
        // Block is at pixel position: (10 * 40, 8 * 40) = (400, 320)
        // Player should be at y = 320 - 38 (player height) = 282
        let player_x = (tile_x as f32) * 40.0;
        let player_y = (tile_y as f32) * 40.0 - 38.0;

        runner.engine_mut().player_mut().x = player_x;
        runner.engine_mut().player_mut().y = player_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Run 1 frame to ensure player is settled on the block
        runner.run_frames(1);

        // Verify player is on ground and block is type 4
        assert!(runner.engine().player().on_ground,
            "Player should be standing on the decaying block");
        assert_eq!(runner.engine().tilemap().get_tile(tile_x, tile_y), 4,
            "Block should be type 4 (undecayed)");

        // Decaying blocks take 1.0 seconds to fully disappear (60 frames at 60 FPS)
        // Let's run 65 frames to ensure it has fully decayed
        runner.run_frames(65);

        let engine = runner.engine();
        let player = engine.player();
        let tilemap = engine.tilemap();

        // Verify the block has disappeared (type 0 = empty)
        assert_eq!(tilemap.get_tile(tile_x, tile_y), 0,
            "Block should have disappeared (type 0) after 1 second");

        // Verify player is no longer on ground (falling)
        assert!(!player.on_ground,
            "Player should be falling after block disappeared");

        // Verify player has downward velocity (is falling)
        assert!(player.vel_y > 0.0,
            "Player should have positive (downward) velocity, but has vel_y={}",
            player.vel_y);

        // Verify player has moved down from where they were
        assert!(player.y > player_y,
            "Player should have fallen from y={} to y={}",
            player_y, player.y);
    }

    #[test]
    fn test_moving_platform_pushes_player() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Set up a test area with solid ground and a moving platform
        // The platform needs to be at player height to push the player horizontally
        let ground_tile_y = 9;
        let platform_tile_y = 8; // One tile above ground, at player's mid-height
        let player_tile_x = 12;
        let platform_tile_x = 10; // Platform starts to the left of player

        // Clear the area
        for dy in -2..=5 {
            for dx in -3..=3 {
                runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y + dy, 0);
            }
        }

        // Place solid ground for player to stand on
        for dx in -3..=3 {
            runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y, 1);
        }

        // Clear all existing platforms and add a right-moving platform at player height
        runner.engine_mut().tilemap_mut().clear_platforms();
        runner.engine_mut().tilemap_mut().add_platform(platform_tile_x, platform_tile_y, 10); // 10 = right-moving

        // Position player standing on the ground
        let player_x = (player_tile_x as f32) * 40.0;
        let player_y = (ground_tile_y as f32) * 40.0 - 38.0; // On top of ground

        runner.engine_mut().player_mut().x = player_x;
        runner.engine_mut().player_mut().y = player_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Run 1 frame to settle player on ground
        runner.run_frames(1);
        assert!(runner.engine().player().on_ground,
            "Player should be standing on ground");

        // Manually activate the platform (simulate it already moving)
        runner.engine_mut().tilemap_mut().activate_platform_at(platform_tile_x, platform_tile_y);

        // Record player's starting X position
        let start_x = runner.engine().player().x;

        // Run frames to let the platform move and push the player
        // Platform moves at 100 px/s to the right
        // Platform starts at x=400, player at x=480 (80 pixels apart)
        // Need at least 0.8 seconds (48 frames) for platform to reach player, plus extra to push
        runner.run_frames(60);

        let final_x = runner.engine().player().x;

        // Player should have been pushed to the right
        let distance_pushed = final_x - start_x;
        assert!(distance_pushed > 10.0,
            "Player should have been pushed right by platform (moved {} pixels, expected > 10)",
            distance_pushed);

        // Player should still be on ground (not knocked into the air)
        assert!(runner.engine().player().on_ground,
            "Player should still be on ground after being pushed");

        // Verify player position is to the right of where they started
        assert!(final_x > start_x,
            "Player should be further right than starting position (start={}, final={})",
            start_x, final_x);
    }

    #[test]
    fn test_player_standing_on_solid_and_death_block_survives() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Set up a clean area with a solid block and death block side by side
        let solid_tile_x = 10;
        let death_tile_x = 11; // Right next to solid block
        let tile_y = 8;

        // Clear the area
        for dy in -1..=5 {
            for dx in -2..=3 {
                runner.engine_mut().tilemap_mut().set_tile(solid_tile_x + dx, tile_y + dy, 0);
            }
        }

        // Place a solid block and a death block side by side
        runner.engine_mut().tilemap_mut().set_tile(solid_tile_x, tile_y, 1); // Solid
        runner.engine_mut().tilemap_mut().set_tile(death_tile_x, tile_y, 3); // Death block

        // Position player already standing on both blocks
        // Position player so they're straddling the boundary
        let player_x = (solid_tile_x as f32) * 40.0 + 30.0; // Near the right edge of solid block
        let player_y = (tile_y as f32) * 40.0 - 38.0; // On top of blocks

        runner.engine_mut().player_mut().x = player_x;
        runner.engine_mut().player_mut().y = player_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Run a few frames with no input - player should just stand there
        runner.run_frames(10);

        let player = runner.engine().player();

        // Player should still be at approximately the same position (not reset)
        assert!((player.x - player_x).abs() < 5.0,
            "Player should not have moved much (x should be ~{} but is {})",
            player_x, player.x);

        assert!((player.y - player_y).abs() < 5.0,
            "Player should not have reset (y should be ~{} but is {})",
            player_y, player.y);

        // Player should still be on ground
        assert!(player.on_ground,
            "Player should still be on ground");
    }

    #[test]
    fn test_player_intersecting_death_block_dies() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Set up a clean area with a death block
        let death_tile_x = 10;
        let death_tile_y = 8;

        // Clear the area
        for dy in -1..=5 {
            for dx in -2..=2 {
                runner.engine_mut().tilemap_mut().set_tile(death_tile_x + dx, death_tile_y + dy, 0);
            }
        }

        // Place a death block
        runner.engine_mut().tilemap_mut().set_tile(death_tile_x, death_tile_y, 3); // Death block

        // Get the player's spawn position (where they reset to)
        let spawn_x = runner.engine().player().x;
        let spawn_y = runner.engine().player().y;

        // Position player intersecting the death block (not just standing on top)
        // Death block is at (400, 320), put player in the middle of it
        let player_x = (death_tile_x as f32) * 40.0 + 12.0; // Inside the block
        let player_y = (death_tile_y as f32) * 40.0 + 10.0; // Inside the block vertically

        runner.engine_mut().player_mut().x = player_x;
        runner.engine_mut().player_mut().y = player_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Run a single frame - player should die and reset
        runner.run_frames(1);

        let player = runner.engine().player();

        // Player should have reset to spawn position
        assert_eq!(player.x, spawn_x,
            "Player should have reset to spawn x={} but is at x={}",
            spawn_x, player.x);

        assert_eq!(player.y, spawn_y,
            "Player should have reset to spawn y={} but is at y={}",
            spawn_y, player.y);

        // Velocity should be reset to zero
        assert_eq!(player.vel_x, 0.0,
            "Player velocity should be reset to 0");
        assert_eq!(player.vel_y, 0.0,
            "Player velocity should be reset to 0");
    }

    #[test]
    fn test_holding_jump_produces_high_jump() {
        // Create a test runner with default level
        let mut runner = TestRunner::new();

        // Set up clean ground
        let ground_tile_y = 9;
        let player_tile_x = 12;

        // Clear the area
        for dy in -2..=5 {
            for dx in -3..=3 {
                runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y + dy, 0);
            }
        }

        // Place solid ground
        for dx in -3..=3 {
            runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y, 1);
        }

        runner.engine_mut().tilemap_mut().clear_platforms();

        // Position player on the ground
        let player_x = (player_tile_x as f32) * 40.0;
        let player_y = (ground_tile_y as f32) * 40.0 - 38.0; // On top of ground
        let initial_y = player_y;

        runner.engine_mut().player_mut().x = player_x;
        runner.engine_mut().player_mut().y = player_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Queue settle frame (no input)
        runner.queue_input(0, InputState::new());

        // Run 1 frame to settle player on ground
        runner.run_frames(1);
        assert!(runner.engine().player().on_ground,
            "Player should be standing on ground");

        // Queue input: jump and HOLD jump button
        // This should produce a high jump (reduced gravity while holding)
        for i in 0..60 {
            let mut input = InputState::new();
            if i == 0 {
                input.jump_pressed = true; // Press jump on first frame
            }
            input.jump = true; // Hold jump for all frames
            runner.queue_input(i + 1, input); // +1 because frame 0 was the settle frame
        }

        // Run frames and track the highest point
        let mut min_y = player_y; // Min Y because Y decreases when going up
        let mut frames_run = 0;

        for i in 0..60 {
            runner.run_frames(1);
            frames_run += 1;
            let current_y = runner.engine().player().y;
            let on_ground = runner.engine().player().on_ground;

            if current_y < min_y {
                min_y = current_y;
            }

            // If player has landed back on ground after jumping, we've completed the jump arc
            if i > 5 && on_ground {
                break;
            }
        }

        // Calculate jump height (initial_y - min_y because Y decreases when going up)
        let jump_height = initial_y - min_y;

        assert!(jump_height > 80.0,
            "Holding jump should produce a high jump > 80 pixels (actual: {} pixels, ran {} frames)",
            jump_height, frames_run);
    }

    #[test]
    fn test_player_jumps_against_platform_direction_while_being_pushed() {
        let test_frames = 5; // Use fewer frames so player stays at platform height
        // First, establish baseline: how high does a normal jump go in test_frames frames?
        let mut baseline_runner = TestRunner::new();

        // Set up ground
        let ground_tile_y = 9;
        let player_tile_x = 12;

        for dy in -2..=5 {
            for dx in -3..=3 {
                baseline_runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y + dy, 0);
            }
        }
        for dx in -3..=3 {
            baseline_runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y, 1);
        }
        baseline_runner.engine_mut().tilemap_mut().clear_platforms();

        let player_y = (ground_tile_y as f32) * 40.0 - 38.0;
        baseline_runner.engine_mut().player_mut().x = (player_tile_x as f32) * 40.0;
        baseline_runner.engine_mut().player_mut().y = player_y;
        baseline_runner.engine_mut().player_mut().vel_x = 0.0;
        baseline_runner.engine_mut().player_mut().vel_y = 0.0;

        // Queue settle frame (no input)
        baseline_runner.queue_input(0, InputState::new());

        // Settle on ground
        baseline_runner.run_frames(1);

        // Queue just a jump (no directional input)
        for i in 0..test_frames {
            let mut input = InputState::new();
            if i == 0 {
                input.jump_pressed = true;
            }
            input.jump = true;
            baseline_runner.queue_input(i + 1, input); // +1 because frame 0 was settle
        }

        baseline_runner.run_frames(test_frames);
        let baseline_y = baseline_runner.engine().player().y;

        // Now run the actual test with platform pushing
        let mut runner = TestRunner::new();

        // Set up a test area with solid ground and a moving platform beside the player
        let platform_tile_y = 8; // One tile above ground, at player's mid-height
        let platform_tile_x = 10; // Platform starts to the left of player

        // Clear the area
        for dy in -2..=5 {
            for dx in -3..=3 {
                runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y + dy, 0);
            }
        }

        // Place solid ground for player to stand on
        for dx in -3..=3 {
            runner.engine_mut().tilemap_mut().set_tile(player_tile_x + dx, ground_tile_y, 1);
        }

        // Clear all existing platforms and add a right-moving platform at player height
        runner.engine_mut().tilemap_mut().clear_platforms();
        runner.engine_mut().tilemap_mut().add_platform(platform_tile_x, platform_tile_y, 10); // 10 = right-moving

        // Position player standing on the ground
        let player_x = (player_tile_x as f32) * 40.0;

        runner.engine_mut().player_mut().x = player_x;
        runner.engine_mut().player_mut().y = player_y;
        runner.engine_mut().player_mut().vel_x = 0.0;
        runner.engine_mut().player_mut().vel_y = 0.0;

        // Queue settle frame (no input)
        runner.queue_input(0, InputState::new());

        // Run 1 frame to settle player on ground
        runner.run_frames(1);
        assert!(runner.engine().player().on_ground,
            "Player should be standing on ground");

        // Manually activate the platform (simulate it already moving)
        runner.engine_mut().tilemap_mut().activate_platform_at(platform_tile_x, platform_tile_y);

        // Queue inputs for platform approach (50 frames of no input)
        for i in 1..=50 {
            runner.queue_input(i, InputState::new());
        }

        // Wait for platform to get close and start pushing
        runner.run_frames(50);

        // Record position after platform starts pushing
        let x_before_jump = runner.engine().player().x;
        let platform_y_top = (platform_tile_y as f32) * 40.0;

        // Queue input: player jumps while pressing LEFT (opposite to platform's RIGHT push)
        // Expected: while at platform height, player should behave as if they only jumped
        // (platform push overrides directional input)
        for i in 0..test_frames {
            let mut input = InputState::new();
            input.left = true;  // Press left (opposite to platform direction)
            if i == 0 {
                input.jump_pressed = true;  // Jump on first frame
            }
            input.jump = true;  // Hold jump
            runner.queue_input(51 + i, input); // +51 because frames 0-50 are already queued
        }

        // Run just a few frames - focusing on when player is still at platform height
        // Once player jumps above the platform, they'll be free to move in their input direction
        runner.run_frames(test_frames);

        let x_after_early_jump = runner.engine().player().x;
        let y_after_early_jump = runner.engine().player().y;

        // Check if player is still at a height where platform can push them
        let player_bottom = y_after_early_jump + 38.0;

        // During early jump frames while still at platform height:
        // Expected behavior: player should behave as if they only jumped (no left movement)
        // The platform push should override the directional input
        let distance_moved = x_after_early_jump - x_before_jump;

        // Verify player is still at platform height during these frames
        assert!(player_bottom > platform_y_top,
            "Player should still be at platform height (player bottom: {}, platform top: {})",
            player_bottom, platform_y_top);

        // Expected: player should NOT move left while being pushed
        // They should either stay roughly in place or move right with the platform
        assert!(distance_moved >= -2.0,
            "Player should not move left while being pushed by platform during jump (moved {} pixels)",
            distance_moved);

        // Ideally, they should move right (being pushed)
        assert!(distance_moved > 0.0,
            "Player should move right (pushed by platform) despite pressing left (moved {} pixels)",
            distance_moved);

        // Verify that vertical jump height is the same as a normal jump
        assert!((y_after_early_jump - baseline_y).abs() < 0.5,
            "Player should jump to same height as normal jump (actual: {}, baseline: {}, diff: {})",
            y_after_early_jump, baseline_y, y_after_early_jump - baseline_y);
    }

    #[test]
    fn test_player_can_move_from_solid_ground_to_platform() {
        // TODO implement
    }

    #[test]
    fn test_downward_platform_stays_inactive_while_player_on_solid_ground() {
        // TODO implement
    }
}
