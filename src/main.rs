// On Windows release builds, use the GUI subsystem so launching the game does
// not also open a console window. Debug builds keep the console for panic/log
// output, and this has no effect on other platforms.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use rustgamex::engine::{GameEngine, OnDeath};
use rustgamex::input::{InputSource, SdlInput};
use rustgamex::level;
use rustgamex::texture::load_png_texture;
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

fn set_window_title(canvas: &mut sdl2::render::WindowCanvas, engine: &GameEngine) {
    let title = match engine.current_level_name() {
        Some(name) if !name.is_empty() => format!("Platform Game - {}", name),
        _ => "Platform Game".to_string(),
    };
    let _ = canvas.window_mut().set_title(&title);
}

/// When launched from a macOS `.app` bundle (e.g. double-clicked in Finder),
/// the working directory is `/`, so the relative asset paths (character.png,
/// tilemap.png, levels/) don't resolve. In that case switch into the bundle's
/// Resources directory, where the build script places the assets. During normal
/// `cargo run` development there is no bundle, so this is a no-op.
#[cfg(target_os = "macos")]
fn chdir_to_bundle_resources() {
    if let Ok(exe) = std::env::current_exe() {
        // exe lives at <App>.app/Contents/MacOS/<binary>
        if let Some(macos_dir) = exe.parent() {
            let resources = macos_dir.join("../Resources");
            if resources.join("tilemap.png").exists() {
                let _ = std::env::set_current_dir(&resources);
            }
        }
    }
}

fn main() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    chdir_to_bundle_resources();

    // Initialize SDL2
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let controller_subsystem = sdl_context.game_controller()?;

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
    let character_texture = load_png_texture(&texture_creator, "character.png")?;
    let tilemap_texture = load_png_texture(&texture_creator, "tilemap.png")?;

    // Create game engine with all levels from the levels/ directory
    let (start, levels_dir) = parse_args();
    let mut engine =
        GameEngine::from_levels_at(level::load_dir(&levels_dir)?, start, OnDeath::Respawn)?;

    let mut input = SdlInput::new(sdl_context.event_pump()?, controller_subsystem);
    let mut time_provider = RealTime::new();

    // Keep the window title in sync with the current level's name
    set_window_title(&mut canvas, &engine);
    let mut titled_level = engine.current_level();

    'running: loop {
        let delta_time = time_provider.delta_time();

        let input_state = input.poll();
        if input.should_quit() {
            break 'running;
        }

        // Update game engine
        engine.step(&input_state, delta_time);

        // Update the window title when the level changes
        if engine.current_level() != titled_level {
            set_window_title(&mut canvas, &engine);
            titled_level = engine.current_level();
        }

        // Camera follows player, centered, clamped to level bounds
        let player = engine.player();
        let tilemap = engine.tilemap();
        let level_width = (tilemap.width as i32) * (tilemap.tile_size as i32);
        let level_height = (tilemap.height as i32) * (tilemap.tile_size as i32);
        let max_camera_x = (level_width - 800).max(0);
        let max_camera_y = (level_height - 600).max(0);
        let camera_x = (player.x as i32 - 400).max(0).min(max_camera_x);
        let camera_y = (player.y as i32 - 300).max(0).min(max_camera_y);

        // Clear the canvas with sky blue background
        canvas.set_draw_color(sdl2::pixels::Color::RGB(135, 206, 235));
        canvas.clear();

        // Render tilemap
        tilemap.render(&mut canvas, &tilemap_texture, camera_x, camera_y);

        // Render player
        player.render(&mut canvas, &character_texture, camera_x, camera_y);

        // Foreground decorations draw over the player, coins and platforms
        tilemap.render_foreground(&mut canvas, &tilemap_texture, camera_x, camera_y);

        // Present the rendered frame
        canvas.present();

        // Wait for next frame to maintain 60 Hz
        time_provider.wait_for_next_frame();
    }

    Ok(())
}
