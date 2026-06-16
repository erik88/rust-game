use sdl2::image::{InitFlag, LoadTexture};

use rustgamex::engine::{GameEngine, OnDeath};
use rustgamex::input::{InputSource, SdlInput};
use rustgamex::level;
use rustgamex::time::{RealTime, TimeProvider};

fn parse_args() -> (usize, String) {
    let args: Vec<String> = std::env::args().collect();
    let start = args.iter()
        .position(|a| a == "--start-level")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(0);
    let levels_dir = args.iter()
        .position(|a| a == "--levels-dir")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| "levels".to_string());
    (start, levels_dir)
}

fn main() -> Result<(), String> {
    // Initialize SDL2
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    // Initialize SDL2_image with PNG support
    let _image_context = sdl2::image::init(InitFlag::PNG)?;

    // Create a window
    let window = video_subsystem
        .window("Platform Game", 800, 600)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    // Create a canvas for rendering
    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;

    // Get the texture creator
    let texture_creator = canvas.texture_creator();

    // Load textures
    let character_texture = texture_creator.load_texture("character.png")?;
    let tilemap_texture = texture_creator.load_texture("tilemap.png")?;

    // Create game engine with all levels from the levels/ directory
    let (start, levels_dir) = parse_args();
    let mut engine =
        GameEngine::from_levels_at(level::load_dir(&levels_dir)?, start, OnDeath::Respawn)?;

    let mut input = SdlInput::new(sdl_context.event_pump()?);
    let mut time_provider = RealTime::new();

    'running: loop {
        let delta_time = time_provider.delta_time();

        let input_state = input.poll();
        if input.should_quit() {
            break 'running;
        }

        // Update game engine
        engine.step(&input_state, delta_time);

        // Camera follows player
        let player = engine.player();
        let tilemap = engine.tilemap();
        let level_width = (tilemap.width as i32) * (tilemap.tile_size as i32);
        let max_camera_x = (level_width - 800).max(0);
        let camera_x = (player.x as i32 - 400).max(0).min(max_camera_x);

        // Clear the canvas with sky blue background
        canvas.set_draw_color(sdl2::pixels::Color::RGB(135, 206, 235));
        canvas.clear();

        // Render tilemap (the game only scrolls horizontally)
        tilemap.render(&mut canvas, &tilemap_texture, camera_x, 0);

        // Render player
        player.render(&mut canvas, &character_texture, camera_x, 0);

        // Present the rendered frame
        canvas.present();

        // Wait for next frame to maintain 60 Hz
        time_provider.wait_for_next_frame();
    }

    Ok(())
}
