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
            Self::new_visualized(engine)
        } else {
            Self {
                engine,
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
/// Moving platforms (tiles 9-12) are automatically extracted.
/// The grid is padded with empty space to the default level size (30x12) so
/// small test maps keep a roomy playable area.
pub fn create_tilemap(level_data: Vec<Vec<u32>>) -> TileMap {
    const DEFAULT_WIDTH: usize = 30;
    const DEFAULT_HEIGHT: usize = 12;

    let width = level_data
        .iter()
        .map(|row| row.len())
        .max()
        .unwrap_or(0)
        .max(DEFAULT_WIDTH);

    let mut tiles = level_data;
    for row in &mut tiles {
        row.resize(width, 0);
    }
    while tiles.len() < DEFAULT_HEIGHT {
        tiles.push(vec![0; width]);
    }

    TileMap::from_data(tiles)
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
    fn test_player_rides_upward_moving_platform() {
        #[rustfmt::skip]
        let mut engine = GameEngine::new_with(
            Player::new(52.0, 40.0),
            create_tilemap(vec![
                vec![0, 0, 0],
                vec![0, 0, 0],
                vec![0, 0, 0],
                vec![0, 0, 0],
                vec![0, 9, 0],
            ]),
            OnDeath::Stop,
        );

        // The game runs with real (variable) frame times, so riding must not
        // depend on exact 60fps timing. Use a 50fps frame time here.
        let dt = 0.02;
        let input = InputState::new();

        // Let the player fall from a height and land hard on the platform,
        // which activates it
        for _ in 0..20 {
            engine.step(&input, dt);
        }

        assert!(
            engine.player().on_ground,
            "Player should have landed on the platform"
        );

        let landing_y = engine.player().y;

        // Run 1 second - platform moves up at 100 px/s, carrying the player
        for _ in 0..50 {
            engine.step(&input, dt);
        }

        let player = engine.player();

        // Player should have been carried upwards with the platform
        let distance_up = landing_y - player.y;
        assert!(
            distance_up > 60.0,
            "Player should have been carried up by platform (moved {} pixels, expected > 60)",
            distance_up
        );

        // Player should still be riding the platform
        assert!(
            player.on_ground,
            "Player should still be standing on the platform"
        );

        // Player must not be stuck inside the platform - his feet should be
        // at (or above) the platform's top edge
        let platform = &engine.tilemap().moving_platforms[0];
        let player = engine.player();
        let player_bottom = player.y + player.height as f32;
        assert!(
            player_bottom <= platform.y + 1.0,
            "Player is stuck inside the platform (feet at {}, platform top at {})",
            player_bottom,
            platform.y
        );
    }

    #[test]
    fn test_completing_level_advances_to_next() {
        use rustgamex::level::LevelData;

        // Level 1: spawn two tiles left of the exit door
        let level1 = LevelData::parse("P.E\n111").unwrap();
        // Level 2: distinct layout, spawn on the second row
        let level2 = LevelData::parse("....\n.P..\n1111").unwrap();
        let mut engine = GameEngine::from_levels(vec![level1, level2], OnDeath::Stop).unwrap();

        let dt = 1.0 / 60.0;
        let mut input = InputState::new();
        input.right = true;

        // Walk right into the exit door (2 tiles at 150 px/s < 1 second)
        let mut reached_exit = false;
        for _ in 0..60 {
            engine.step(&input, dt);
            if engine.is_transitioning() {
                reached_exit = true;
                break;
            }
        }
        assert!(reached_exit, "Player should have reached the exit tile");
        assert_eq!(
            engine.current_level(),
            0,
            "Still on level 1 during transition"
        );

        // The world is frozen during the transition
        let pos_during_transition = engine.player().position();
        engine.step(&input, dt);
        assert_eq!(
            engine.player().position(),
            pos_during_transition,
            "Player should not move during the level transition"
        );

        // After the transition the next level is loaded
        let input = InputState::new();
        for _ in 0..60 {
            engine.step(&input, dt);
        }
        assert!(!engine.is_transitioning(), "Transition should be over");
        assert_eq!(engine.current_level(), 1, "Level 2 should be loaded");
        assert!(
            !engine.stopped && !engine.player().is_dead,
            "Player should be alive at the start of level 2"
        );
    }

    #[test]
    fn test_all_shipped_level_files_parse() {
        use rustgamex::level::LevelData;

        let mut count = 0;
        for entry in std::fs::read_dir("levels").expect("levels directory should exist") {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "txt") {
                let text = std::fs::read_to_string(&path).unwrap();
                LevelData::parse(&text)
                    .unwrap_or_else(|e| panic!("{} failed to parse: {}", path.display(), e));
                count += 1;
            }
        }
        assert!(
            count >= 2,
            "Expected at least 2 level files, found {}",
            count
        );
    }

    #[test]
    fn test_shipped_levels_have_safe_spawn() {
        use rustgamex::level::LevelData;

        for entry in std::fs::read_dir("levels").expect("levels directory should exist") {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "txt") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let level = LevelData::parse(&text).unwrap();
            let mut engine = GameEngine::from_levels(vec![level], OnDeath::Stop).unwrap();

            // Let the player drop from the spawn point and settle
            let input = InputState::new();
            for _ in 0..120 {
                engine.step(&input, 1.0 / 60.0);
            }

            assert!(
                !engine.player().is_dead,
                "{}: player dies right after spawning",
                path.display()
            );
            assert!(
                engine.is_player_on_ground(),
                "{}: player does not land on ground after spawning",
                path.display()
            );
        }
    }

    #[test]
    fn test_periodic_tiles_swap_every_second() {
        let mut tilemap = create_tilemap(vec![vec![7, 8]]);
        assert_eq!(tilemap.get_tile(0, 0), 7);
        assert_eq!(tilemap.get_tile(1, 0), 8);

        // After 0.9 seconds nothing has changed
        for _ in 0..54 {
            tilemap.update(1.0 / 60.0);
        }
        assert_eq!(
            tilemap.get_tile(0, 0),
            7,
            "Tile should not swap before 1 second"
        );
        assert_eq!(
            tilemap.get_tile(1, 0),
            8,
            "Tile should not swap before 1 second"
        );

        // After 1.05 seconds both tiles have swapped
        for _ in 0..9 {
            tilemap.update(1.0 / 60.0);
        }
        assert_eq!(
            tilemap.get_tile(0, 0),
            8,
            "Tile 7 should swap to 8 after 1 second"
        );
        assert_eq!(
            tilemap.get_tile(1, 0),
            7,
            "Tile 8 should swap to 7 after 1 second"
        );

        // After another second they swap back
        for _ in 0..60 {
            tilemap.update(1.0 / 60.0);
        }
        assert_eq!(
            tilemap.get_tile(0, 0),
            7,
            "Tile should swap back after 2 seconds"
        );
        assert_eq!(
            tilemap.get_tile(1, 0),
            8,
            "Tile should swap back after 2 seconds"
        );
    }

    #[test]
    fn test_player_falls_through_phased_out_periodic_tile() {
        // Player stands on a periodic tile with nothing below it
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(12.0, 0.0),
            create_tilemap(vec![
                vec![0],
                vec![7],
            ]),
        );

        // Land on the tile while it is solid
        runner.run_frames(5);
        assert!(
            runner.engine().player().on_ground,
            "Player should stand on the periodic tile in its solid phase"
        );

        // After the tile phases out (1s) the player falls through and off
        // the screen (12 rows = 480px + 100px margin, well under 2.5s of falling)
        runner.run_frames(150);
        assert!(
            runner.engine().player().is_dead,
            "Player should have fallen through the phased-out periodic tile"
        );
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
        runner.engine_mut().tilemap_mut().activate_platform(0, 1);

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
        assert!(!player.is_dead, "Expected player to be alive.");
    }

    #[test]
    fn test_player_intersecting_death_block_dies() {
        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(12.0, 40.0),
            create_tilemap(vec![
                vec![0], // Empty space above
                vec![0], // Empty space above
                vec![3], // Death block at (0, 2)
            ])
        );

        // Run a few frames to let player fall and die
        runner.run_frames(5);

        // Player should have reset to spawn position
        assert!(
            runner.engine().player().is_dead,
            "Expected player to be dead"
        );
    }

    #[test]
    fn test_holding_jump_produces_high_jump() {
        let player_start_y = 122.0;

        #[rustfmt::skip]
        let mut runner = TestRunner::new_with(
            Player::new(5.0, player_start_y),
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

        runner.run_frames(30);

        assert!(
            player_start_y - runner.engine.player.y > 80.0,
            "Holding jump should produce a high jump {} {}",
            player_start_y,
            runner.engine.player.y
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
