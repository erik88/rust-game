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

mod fixed_time;

use fixed_time::FixedTime;
use rustgamex::engine::{GameEngine, OnDeath};
use rustgamex::input::{InputSource, InputState, QueuedInput};
use rustgamex::player::Player;
use rustgamex::tilemap::TileMap;
use rustgamex::time::TimeProvider;
use std::env;

/// Test runner for game simulation
pub struct TestRunner {
    engine: GameEngine,
    input_source: QueuedInput,
    time_provider: FixedTime,
    visualize: bool,
    sdl_context: Option<sdl2::Sdl>,
    canvas: Option<sdl2::render::WindowCanvas>,
    character_texture: Option<sdl2::render::Texture<'static>>,
    tilemap_texture: Option<sdl2::render::Texture<'static>>,
}

impl TestRunner {
    pub fn new_with(player: Player, tilemap: TileMap) -> Self {
        let visualize = env::var("VISUALIZE_TEST").is_ok();
        let engine = GameEngine::new_with(player, tilemap, OnDeath::Stop);

        if visualize {
            let mut runner = Self::new_visualized(engine);
            runner
        } else {
            Self {
                engine: engine,
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
    fn new_visualized(engine: GameEngine) -> Self {
        use sdl2::image::{InitFlag, LoadTexture};

        // Initialize SDL2
        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");
        let video_subsystem = sdl_context.video().expect("Failed to get video subsystem");

        // Initialize SDL2_image with PNG support
        let _image_context =
            sdl2::image::init(InitFlag::PNG).expect("Failed to initialize SDL2_image");

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
        let character_texture = texture_creator
            .load_texture("character.png")
            .expect("Failed to load character.png");
        let tilemap_texture = texture_creator
            .load_texture("tilemap.png")
            .expect("Failed to load tilemap.png");

        // Transmute to 'static - this is safe because we're holding the SDL context
        let character_texture: sdl2::render::Texture<'static> =
            unsafe { std::mem::transmute(character_texture) };
        let tilemap_texture: sdl2::render::Texture<'static> =
            unsafe { std::mem::transmute(tilemap_texture) };

        Self {
            engine,
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

        if let (Some(canvas), Some(character_tex), Some(tilemap_tex)) = (
            &mut self.canvas,
            &self.character_texture,
            &self.tilemap_texture,
        ) {
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

            self.time_provider.wait_for_next_frame();
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
                        Event::Quit { .. } => break 'wait,
                        Event::KeyDown {
                            keycode: Some(sdl2::keyboard::Keycode::Escape),
                            ..
                        } => break 'wait,
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

/// Helper function to create a tilemap from a 2D array of tile types
/// Moving platforms (tiles 9-12) are automatically extracted
pub fn create_tilemap(level_data: Vec<Vec<u32>>) -> TileMap {
    use rustgamex::tilemap::MovingPlatform;

    let height = level_data.len();
    let width = if height > 0 { level_data[0].len() } else { 0 };

    // Extract moving platforms (tiles 9, 10, 11, 12)
    let mut platforms = Vec::new();
    let mut tiles = level_data.clone();

    for (y, row) in level_data.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            if tile >= 9 && tile <= 12 {
                platforms.push(MovingPlatform {
                    x: (x as i32 * 40) as f32,
                    y: (y as i32 * 40) as f32,
                    tile_type: tile,
                    active: false,
                    vel_x: 0.0,
                    vel_y: 0.0,
                    original_tile_x: x as i32,
                    original_tile_y: y as i32,
                });
                // Remove platform from static grid
                tiles[y][x] = 0;
            }
        }
    }

    // Create a base tilemap and replace its data
    let mut tilemap = TileMap::new();

    // Clear everything first
    for y in 0..tilemap.height {
        for x in 0..tilemap.width {
            tilemap.set_tile(x as i32, y as i32, 0);
        }
    }
    tilemap.clear_platforms();

    // Set our custom tiles
    for y in 0..height.min(tilemap.height) {
        for x in 0..width.min(tilemap.width) {
            tilemap.set_tile(x as i32, y as i32, tiles[y][x]);
        }
    }

    // Add our platforms
    for platform in platforms {
        tilemap.add_platform(
            platform.original_tile_x,
            platform.original_tile_y,
            platform.tile_type,
        );
    }

    tilemap
}

mod tests {
    use super::*;

    #[test]
    fn test_player_falls_and_lands_on_tile() {
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(0.0, 40.0),
            create_tilemap(vec![
                vec![0],
                vec![0],
                vec![1]
            ]),
        );

        // Record initial position
        let initial_y = runner.engine().player().y;

        // Run enough frames for player to fall and land
        runner.run_frames(10);

        let engine = runner.engine();
        let player = engine.player();

        // Player should have fallen (y increased)
        assert!(
            player.y > initial_y,
            "Player should have fallen from y={} to y={}",
            initial_y,
            player.y
        );

        // Player should be on ground
        assert!(player.on_ground, "Player should be on ground after falling");

        // Player's vertical velocity should be zero (stopped falling)
        assert_eq!(
            player.vel_y, 0.0,
            "Player should have zero vertical velocity when on ground"
        );

        // Player should have landed on top of a tile (y position should be stable)
        // Run a few more frames to ensure player stays in place
        let landed_y = player.y;
        runner.run_frames(10);
        let final_y = runner.engine().player().y;

        assert_eq!(
            landed_y, final_y,
            "Player should remain at y={} after landing, but is at y={}",
            landed_y, final_y
        );
    }

    #[test]
    fn test_player_rides_horizontally_moving_platform() {
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(40.0, 2.0),
            create_tilemap(vec![
                vec![0, 0, 0, 0, 0],
                vec![0, 10, 0, 0, 0]
            ]),
        );

        // Run a few frames to let player fall and land on the platform
        runner.run_frames(2);

        // Verify player has landed on the platform
        assert!(
            runner.engine().player().on_ground,
            "Player should have landed on the platform"
        );

        // Record player's position after landing
        let player_landing_pos = runner.engine().player().position();

        // Run more frames - platform should be moving and carrying the player
        // Platform speed is 100 px/s, so in 0.5 seconds (30 frames) it should move ~50 pixels
        runner.run_frames(30);

        let final_pos = runner.engine().player().position();

        // Player should have moved significantly to the right with the platform
        let distance = final_pos - player_landing_pos;
        assert!(
            distance.x > 40.0,
            "Player should have moved right with platform (moved {} pixels, expected > 40)",
            distance.x
        );

        // Player should not have changed y position while riding the platform
        assert!(
            distance.y < 2.0,
            "Player should not have moved vertically with platform (moved {} pixels, expected < 2.0)",
            distance.y
        );

        // Player should still be on ground (riding the p latform)
        assert!(
            runner.engine().player().on_ground,
            "Player should still be on the platform"
        );

        // Player should have the same y-position as when landing
    }

    #[test]
    fn test_player_on_decaying_block_falls_through() {
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(0.0, 2.0),
            create_tilemap(vec![
                vec![0],
                vec![4],
            ]),
        );

        // Run 1 frame to ensure player is settled on the block
        runner.run_frames(1);

        // Verify player is on ground and block is type 4
        assert!(
            runner.engine().player().on_ground,
            "Player should be standing on the decaying block"
        );

        // Decaying blocks take 1.0 seconds to fully disappear (60 frames at 60 FPS)
        // Let's run 65 frames to ensure it has fully decayed
        runner.run_frames(65);

        let engine = runner.engine();
        let player = engine.player();
        let tilemap = engine.tilemap();

        // Verify the block has disappeared (type 0 = empty)
        assert_eq!(
            tilemap.get_tile(0, 1),
            0,
            "Block should have disappeared (type 0) after 1 second"
        );

        // Verify player is no longer on ground (falling)
        assert!(
            !player.on_ground,
            "Player should be falling after block disappeared"
        );

        // Verify player has downward velocity (is falling)
        assert!(
            player.vel_y > 0.0,
            "Player should have positive (downward) velocity, but has vel_y={}",
            player.vel_y
        );

        // Verify player has moved down from where they were
        assert!(
            player.y > 2.0,
            "Player should have fallen from y={} to y={}",
            2.0,
            player.y
        );
    }

    #[test]
    fn test_moving_platform_pushes_player() {
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(80.0, 42.0),
            create_tilemap(vec![
                vec![0, 0, 0, 0, 0],
                vec![10, 0, 0, 0, 0],
                vec![1, 1, 1, 1, 1],
            ])
        );

        // Run 1 frame to settle player on ground
        runner.run_frames(1);
        assert!(
            runner.engine().player().on_ground,
            "Player should be standing on ground"
        );

        // Manually activate the platform (simulate it already moving)
        runner.engine_mut().tilemap_mut().activate_platform_at(0, 1);

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
        assert!(
            distance_pushed > 10.0,
            "Player should have been pushed right by platform (moved {} pixels, expected > 10)",
            distance_pushed
        );

        // Player should still be on ground (not knocked into the air)
        assert!(
            runner.engine().player().on_ground,
            "Player should still be on ground after being pushed"
        );
    }

    #[test]
    fn test_player_standing_on_solid_and_death_block_survives() {
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(30.0, 2.0),
            create_tilemap(vec![
                vec![0, 0], // Empty space above for player
                vec![1, 3], // Solid block (1) and death block (3) side by side
            ])
        );

        // Run a few frames with no input - player should just stand there
        runner.run_frames(10);

        let player = runner.engine().player();
    }

    #[test]
    fn test_player_intersecting_death_block_dies() {
        // Create a compact tilemap with a death block
        let level_data = vec![
            vec![0], // Empty space above
            vec![0], // Empty space above
            vec![3], // Death block at (0, 2)
        ];
        let tilemap = create_tilemap(level_data);

        // Position spawn in the playable area (in the empty space)
        let spawn_x = 10.0;
        let spawn_y = 10.0;

        // Now create actual test with player intersecting the death block
        let player_x = 12.0; // Inside the block horizontally
        let player_y = 80.0 + 10.0; // Inside the block vertically (death block at y=80)

        let player = Player::new(player_x, player_y);
        let mut runner = TestRunner::new_with(player, tilemap);

        // Run a single frame - player should die and reset
        runner.run_frames(1);

        let player = runner.engine().player();

        // Player should have reset to spawn position
        assert_eq!(
            player.x, spawn_x,
            "Player should have reset to spawn x={} but is at x={}",
            spawn_x, player.x
        );

        assert_eq!(
            player.y, spawn_y,
            "Player should have reset to spawn y={} but is at y={}",
            spawn_y, player.y
        );

        // Velocity should be reset to zero
        assert_eq!(player.vel_x, 0.0, "Player velocity should be reset to 0");
        assert_eq!(player.vel_y, 0.0, "Player velocity should be reset to 0");
    }

    #[test]
    fn test_holding_jump_produces_high_jump() {
        // Create a test runner with default level
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(5.0, 62.0),
            create_tilemap(vec![
                vec![0],
                vec![0],
                vec![0],
                vec![0],
                vec![1],
            ])
        );

        // Run 1 frame to settle player on ground
        runner.run_frames(1);
        assert!(
            runner.engine().player().on_ground,
            "Player should be standing on ground"
        );

        // Queue input: jump and HOLD jump button
        // This should produce a high jump (reduced gravity while holding)
        for frame in 1..=60 {
            let mut input = InputState::new();
            if frame == 1 {
                input.jump_pressed = true; // Press jump on first frame
            }
            input.jump = true; // Hold jump for all frames
            runner.queue_input(frame, input); // +1 because frame 0 was the settle frame
        }

        runner.run_frames(60);

        assert!(
            runner.engine.player.y < 20.0,
            "Holding jump should produce a high jump",
        );
    }

    #[test]
    fn test_player_can_move_from_solid_ground_to_platform() {
        // Create a test runner with default level
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(10.0, 2.0),
            create_tilemap(vec![
                vec![0, 0],
                vec![1, 11]
            ])
        );

        // Run 1 frame to settle player on ground
        runner.run_frames(1);
        assert!(
            runner.engine().player().on_ground,
            "Player should be standing on solid ground"
        );

        // Queue input: move right towards the platform
        for frame in 1..=30 {
            let mut input = InputState::new();
            input.right = true; // Move right
            runner.queue_input(frame, input);
        }

        // Run frames to let player move onto the platform
        runner.run_frames(30);

        let player = runner.engine().player();

        // Player should have moved right onto the platform
        assert!(
            player.x > 40.0,
            "Player should have moved right to x>{}, but is at x={}",
            40.0,
            player.x
        );

        // Player should still be on ground (standing on the platform)
        assert!(
            player.on_ground,
            "Player should still be on ground after moving onto platform"
        );

        // Player should be at approximately the same Y position
        assert!(
            (player.y - 2.0).abs() < 5.0,
            "Player should be at same height (y should be ~{} but is {})",
            2.0,
            player.y
        );
    }

    #[test]
    fn test_downward_platform_stays_inactive_while_player_on_solid_ground() {
        // TODO implement
    }
}
