use sdl2::image::{InitFlag, LoadTexture};

use rustgamex::engine::{GameEngine, OnDeath};
use rustgamex::input::InputHandler;
use rustgamex::player::Player;
use rustgamex::tilemap::TileMap;
use rustgamex::time::{RealTime, TimeProvider};

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

    // Create game engine
    let mut engine =
        GameEngine::new_with(Player::new(100.0, 300.0), TileMap::new(), OnDeath::Respawn);

    let mut event_pump = sdl_context.event_pump()?;
    let mut input_handler = InputHandler::new();
    let mut time_provider = RealTime::new();

    'running: loop {
        let delta_time = time_provider.delta_time();

        // Update input state
        input_handler.update(&mut event_pump);

        if input_handler.should_quit() {
            break 'running;
        }

        // Update game engine
        engine.step(input_handler.state(), delta_time);

        // Camera follows player
        let player = engine.player();
        let tilemap = engine.tilemap();
        let level_width = (tilemap.width as i32) * (tilemap.tile_size as i32);
        let max_camera_x = (level_width - 800).max(0);
        let camera_x = (player.x as i32 - 400).max(0).min(max_camera_x);

        // Clear the canvas with sky blue background
        canvas.set_draw_color(sdl2::pixels::Color::RGB(135, 206, 235));
        canvas.clear();

        // Render tilemap
        tilemap.render(&mut canvas, &tilemap_texture, camera_x);

        // Render player
        player.render(&mut canvas, &character_texture, camera_x);

        // Present the rendered frame
        canvas.present();

        // Wait for next frame to maintain 60 Hz
        time_provider.wait_for_next_frame();
    }

    Ok(())
}
