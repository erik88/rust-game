//! Level editor: browse and edit the level files in `levels/`.
//!
//! It renders each level the way the game does (tiles, moving platforms in
//! their starting positions, and the player at the spawn point) and lets you
//! paint tiles onto the grid. Edits are kept in memory per level and are only
//! written to disk when you save, so switching levels never loses work.
//!
//! Controls:
//! - Left mouse   : apply the selected palette tool to the tile under the cursor
//! - Right mouse  : erase the tile under the cursor
//! - (click+drag paints continuously)
//! - Click the bottom palette bar to choose a tool
//! - Arrow keys (or WASD)      : pan the camera
//! - Ctrl+Arrow   : grow the canvas toward that edge
//! - Ctrl+Shift+Arrow : shrink the canvas from that edge
//! - `[` / `]`    : previous / next level
//! - Home         : scroll back to the start of the level
//! - G            : toggle the grid overlay
//! - Ctrl+S       : save the current level back to its file
//! - Esc / Q      : quit

use sdl2::event::Event;
use sdl2::image::{InitFlag, LoadTexture};
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

use rustgamex::level::{self, Edge, LevelData};
use rustgamex::player::{PLAYER_HEIGHT, PLAYER_WIDTH, Player};
use rustgamex::tilemap::TileMap;
use rustgamex::tiles::{self, TILE_SIZE};
use rustgamex::time::{RealTime, TimeProvider};
use std::path::PathBuf;

const VIEW_WIDTH: u32 = 800;
const VIEW_HEIGHT: u32 = 600;
const PAN_SPEED: f32 = 600.0; // camera pan speed, pixels per second

// Palette HUD layout
const HUD_HEIGHT: i32 = 56;
const HUD_TOP: i32 = VIEW_HEIGHT as i32 - HUD_HEIGHT;
const SLOT: i32 = 40; // a palette slot is one tile wide
const SLOT_PAD: i32 = 6;
const HUD_MARGIN_X: i32 = 8;
const TILES_PER_ROW: u32 = 6; // layout of tilemap.png

/// A tool the user can paint with.
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Erase,
    Spawn,
    Tile(u32),
}

impl Tool {
    fn name(self) -> String {
        match self {
            Tool::Erase => "Erase".to_string(),
            Tool::Spawn => "Spawn".to_string(),
            Tool::Tile(n) => format!("Tile {}", n),
        }
    }
}

/// A level being edited, paired with the file it came from.
struct Document {
    path: PathBuf,
    level: LevelData,
    modified: bool,
}

fn main() -> Result<(), String> {
    let mut docs: Vec<Document> = level::load_dir_entries("levels")?
        .into_iter()
        .map(|(path, level)| Document {
            path,
            level,
            modified: false,
        })
        .collect();
    if docs.is_empty() {
        return Err("no level files found in levels/".to_string());
    }

    // Palette: eraser, spawn placer, then every paintable tile in order
    let palette: Vec<Tool> = std::iter::once(Tool::Erase)
        .chain(std::iter::once(Tool::Spawn))
        .chain((1..=tiles::EXIT).map(Tool::Tile))
        .collect();

    print_controls();

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let _image_context = sdl2::image::init(InitFlag::PNG)?;

    let window = video_subsystem
        .window("Level Editor", VIEW_WIDTH, VIEW_HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;

    // Needed for the translucent grid overlay to blend over the level
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);

    let texture_creator = canvas.texture_creator();
    let character_texture = texture_creator.load_texture("character.png")?;
    let tilemap_texture = texture_creator.load_texture("tilemap.png")?;

    let mut event_pump = sdl_context.event_pump()?;
    let mut time_provider = RealTime::new();

    let mut current = 0;
    let mut camera_x = 0.0f32;
    let mut camera_y = 0.0f32;
    let mut selected = 2; // first real tile (after Erase, Spawn)
    let mut show_grid = true;
    let mut pan_left = false;
    let mut pan_right = false;
    let mut pan_up = false;
    let mut pan_down = false;
    let mut mouse = (0i32, 0i32);

    // Rendered representation of the current level, rebuilt whenever it changes
    let mut tilemap = TileMap::from_data(docs[current].level.tiles.clone());
    let mut player = spawn_player(&docs[current].level);
    let mut dirty = false;
    set_title(&mut canvas, &docs, current, palette[selected]);

    'running: loop {
        let delta_time = time_provider.delta_time();
        let mut switch_to = None;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'running,

                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } => {
                    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                    // Ctrl+Arrow grows the canvas toward that edge; add Shift to
                    // shrink from it instead.
                    let resize_edge = match key {
                        Keycode::Up if ctrl => Some(Edge::Top),
                        Keycode::Down if ctrl => Some(Edge::Bottom),
                        Keycode::Left if ctrl => Some(Edge::Left),
                        Keycode::Right if ctrl => Some(Edge::Right),
                        _ => None,
                    };

                    if let Some(edge) = resize_edge {
                        let changed = if shift {
                            docs[current].level.shrink(edge)
                        } else {
                            docs[current].level.grow(edge);
                            true
                        };
                        if changed {
                            docs[current].modified = true;
                            dirty = true;
                            set_title(&mut canvas, &docs, current, palette[selected]);
                        }
                    } else {
                        match key {
                            Keycode::S if ctrl => {
                                save(&mut docs[current]);
                                set_title(&mut canvas, &docs, current, palette[selected]);
                            }
                            Keycode::Escape | Keycode::Q => break 'running,
                            Keycode::Left | Keycode::A => pan_left = true,
                            Keycode::Right | Keycode::D => pan_right = true,
                            Keycode::Up | Keycode::W => pan_up = true,
                            Keycode::Down | Keycode::S => pan_down = true,
                            Keycode::LeftBracket => {
                                switch_to = Some((current + docs.len() - 1) % docs.len());
                            }
                            Keycode::RightBracket => {
                                switch_to = Some((current + 1) % docs.len());
                            }
                            Keycode::Home => {
                                camera_x = 0.0;
                                camera_y = 0.0;
                            }
                            Keycode::G => show_grid = !show_grid,
                            _ => {}
                        }
                    }
                }

                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Left | Keycode::A => pan_left = false,
                    Keycode::Right | Keycode::D => pan_right = false,
                    Keycode::Up | Keycode::W => pan_up = false,
                    Keycode::Down | Keycode::S => pan_down = false,
                    _ => {}
                },

                Event::MouseMotion {
                    x, y, mousestate, ..
                } => {
                    mouse = (x, y);
                    // Continue painting while a button is held and the cursor is
                    // over the level (not the palette bar)
                    if y < HUD_TOP {
                        if mousestate.left() {
                            if apply_tool(
                                &mut docs[current].level,
                                palette[selected],
                                x,
                                y,
                                camera_x,
                                camera_y,
                            ) {
                                docs[current].modified = true;
                                dirty = true;
                            }
                        } else if mousestate.right()
                            && apply_tool(
                                &mut docs[current].level,
                                Tool::Erase,
                                x,
                                y,
                                camera_x,
                                camera_y,
                            )
                        {
                            docs[current].modified = true;
                            dirty = true;
                        }
                    }
                }

                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => {
                    if y >= HUD_TOP {
                        // Click in the palette bar selects a tool
                        if mouse_btn == MouseButton::Left
                            && let Some(slot) = palette_slot_at(x, palette.len())
                        {
                            selected = slot;
                            set_title(&mut canvas, &docs, current, palette[selected]);
                        }
                    } else {
                        let tool = match mouse_btn {
                            MouseButton::Left => palette[selected],
                            MouseButton::Right => Tool::Erase,
                            _ => continue,
                        };
                        if apply_tool(&mut docs[current].level, tool, x, y, camera_x, camera_y) {
                            docs[current].modified = true;
                            dirty = true;
                        }
                    }
                }

                _ => {}
            }
        }

        if let Some(index) = switch_to {
            current = index;
            camera_x = 0.0;
            camera_y = 0.0;
            pan_left = false;
            pan_right = false;
            pan_up = false;
            pan_down = false;
            dirty = true;
            set_title(&mut canvas, &docs, current, palette[selected]);
        }

        if dirty {
            tilemap = TileMap::from_data(docs[current].level.tiles.clone());
            player = spawn_player(&docs[current].level);
            dirty = false;
        }

        // Pan and clamp the camera to the level bounds. The level is shown in
        // the window above the HUD bar, so the vertical viewport is HUD_TOP tall.
        let pan = PAN_SPEED * delta_time;
        if pan_left {
            camera_x -= pan;
        }
        if pan_right {
            camera_x += pan;
        }
        if pan_up {
            camera_y -= pan;
        }
        if pan_down {
            camera_y += pan;
        }
        let level_width = tilemap.width as f32 * tilemap.tile_size as f32;
        let level_height = tilemap.height as f32 * tilemap.tile_size as f32;
        let max_camera_x = (level_width - VIEW_WIDTH as f32).max(0.0);
        let max_camera_y = (level_height - HUD_TOP as f32).max(0.0);
        camera_x = camera_x.clamp(0.0, max_camera_x);
        camera_y = camera_y.clamp(0.0, max_camera_y);
        let camera_xi = camera_x as i32;
        let camera_yi = camera_y as i32;

        canvas.set_draw_color(Color::RGB(135, 206, 235));
        canvas.clear();
        tilemap.render(&mut canvas, &tilemap_texture, camera_xi, camera_yi);
        player.render(&mut canvas, &character_texture, camera_xi, camera_yi);

        if show_grid {
            draw_grid(&mut canvas, &tilemap, camera_xi, camera_yi);
        }
        draw_hover(&mut canvas, &tilemap, mouse, camera_xi, camera_yi);
        draw_hud(
            &mut canvas,
            &tilemap_texture,
            &character_texture,
            &palette,
            selected,
        );

        canvas.present();
        time_provider.wait_for_next_frame();
    }

    Ok(())
}

/// Build the display player from a level's spawn position.
fn spawn_player(level: &LevelData) -> Player {
    Player::new(level.spawn.0, level.spawn.1)
}

/// Apply a tool to the tile under a screen position. Returns true if the level
/// changed. No-op when the cursor is over the HUD or outside the grid.
fn apply_tool(
    level: &mut LevelData,
    tool: Tool,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    if screen_y >= HUD_TOP {
        return false;
    }
    let world_x = screen_x as f32 + camera_x;
    let world_y = screen_y as f32 + camera_y;
    if world_x < 0.0 || world_y < 0.0 {
        return false;
    }
    let tx = (world_x / TILE_SIZE) as usize;
    let ty = (world_y / TILE_SIZE) as usize;
    if ty >= level.tiles.len() || tx >= level.tiles[ty].len() {
        return false;
    }

    match tool {
        Tool::Erase => {
            if level.tiles[ty][tx] == tiles::EMPTY {
                return false;
            }
            level.tiles[ty][tx] = tiles::EMPTY;
        }
        Tool::Tile(n) => {
            if level.tiles[ty][tx] == n {
                return false;
            }
            level.tiles[ty][tx] = n;
        }
        Tool::Spawn => {
            // The spawn tile must be empty, mirroring how `P` parses
            level.tiles[ty][tx] = tiles::EMPTY;
            level.spawn = (
                tx as f32 * TILE_SIZE + (TILE_SIZE - PLAYER_WIDTH as f32) / 2.0,
                ty as f32 * TILE_SIZE + TILE_SIZE - PLAYER_HEIGHT as f32,
            );
        }
    }
    true
}

/// Write a document back to its file, reporting success or failure on stdout.
fn save(doc: &mut Document) {
    match std::fs::write(&doc.path, doc.level.to_text()) {
        Ok(()) => {
            doc.modified = false;
            println!("Saved {}", doc.path.display());
        }
        Err(e) => eprintln!("Failed to save {}: {}", doc.path.display(), e),
    }
}

/// Which palette slot, if any, sits under an x coordinate in the HUD.
fn palette_slot_at(x: i32, count: usize) -> Option<usize> {
    for i in 0..count {
        let slot_x = HUD_MARGIN_X + i as i32 * (SLOT + SLOT_PAD);
        if x >= slot_x && x < slot_x + SLOT {
            return Some(i);
        }
    }
    None
}

/// Source rectangle of a tile's graphic inside tilemap.png.
fn tile_src_rect(tile_id: u32) -> Rect {
    let size = TILE_SIZE as u32;
    let sx = ((tile_id - 1) % TILES_PER_ROW) * size;
    let sy = ((tile_id - 1) / TILES_PER_ROW) * size;
    Rect::new(sx as i32, sy as i32, size, size)
}

fn draw_grid(canvas: &mut WindowCanvas, tilemap: &TileMap, camera_x: i32, camera_y: i32) {
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 60));
    let size = tilemap.tile_size as i32;

    // The grid spans the level rectangle, clamped to the level viewport (the
    // window above the HUD bar)
    let level_w = tilemap.width as i32 * size;
    let level_h = tilemap.height as i32 * size;
    let x0 = (-camera_x).max(0);
    let x1 = (level_w - camera_x).min(VIEW_WIDTH as i32);
    let y0 = (-camera_y).max(0);
    let y1 = (level_h - camera_y).min(HUD_TOP);

    for col in 0..=tilemap.width as i32 {
        let x = col * size - camera_x;
        if (0..=VIEW_WIDTH as i32).contains(&x) {
            let _ = canvas.draw_line((x, y0), (x, y1));
        }
    }
    for row in 0..=tilemap.height as i32 {
        let y = row * size - camera_y;
        if (0..=HUD_TOP).contains(&y) {
            let _ = canvas.draw_line((x0, y), (x1, y));
        }
    }
}

/// Outline the tile the cursor is hovering over, if it is inside the grid.
fn draw_hover(
    canvas: &mut WindowCanvas,
    tilemap: &TileMap,
    mouse: (i32, i32),
    camera_x: i32,
    camera_y: i32,
) {
    let (mx, my) = mouse;
    if my >= HUD_TOP {
        return;
    }
    let size = tilemap.tile_size as i32;
    let world_x = mx + camera_x;
    let world_y = my + camera_y;
    if world_x < 0 || world_y < 0 {
        return;
    }
    let tx = world_x / size;
    let ty = world_y / size;
    if tx >= tilemap.width as i32 || ty >= tilemap.height as i32 {
        return;
    }
    canvas.set_draw_color(Color::RGB(255, 235, 90));
    let _ = canvas.draw_rect(Rect::new(
        tx * size - camera_x,
        ty * size - camera_y,
        size as u32,
        size as u32,
    ));
}

fn draw_hud(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    palette: &[Tool],
    selected: usize,
) {
    // Bar background and top divider
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, HUD_TOP, VIEW_WIDTH, HUD_HEIGHT as u32));
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, HUD_TOP), (VIEW_WIDTH as i32, HUD_TOP));

    let slot_y = HUD_TOP + (HUD_HEIGHT - SLOT) / 2;
    for (i, tool) in palette.iter().enumerate() {
        let slot_x = HUD_MARGIN_X + i as i32 * (SLOT + SLOT_PAD);
        let dst = Rect::new(slot_x, slot_y, SLOT as u32, SLOT as u32);

        // Slot background
        canvas.set_draw_color(Color::RGB(55, 55, 70));
        let _ = canvas.fill_rect(dst);

        match *tool {
            Tool::Tile(n) => {
                let _ = canvas.copy(tilemap_texture, Some(tile_src_rect(n)), Some(dst));
            }
            Tool::Spawn => {
                let src = Rect::new(0, 0, PLAYER_WIDTH, PLAYER_HEIGHT);
                // Center the (narrow) player sprite within the slot
                let player_dst = Rect::new(
                    slot_x + (SLOT - PLAYER_WIDTH as i32) / 2,
                    slot_y + (SLOT - PLAYER_HEIGHT as i32) / 2,
                    PLAYER_WIDTH,
                    PLAYER_HEIGHT,
                );
                let _ = canvas.copy(character_texture, Some(src), Some(player_dst));
            }
            Tool::Erase => {
                // A red X marks the eraser
                canvas.set_draw_color(Color::RGB(220, 80, 80));
                let _ = canvas
                    .draw_line((slot_x + 8, slot_y + 8), (slot_x + SLOT - 8, slot_y + SLOT - 8));
                let _ = canvas
                    .draw_line((slot_x + SLOT - 8, slot_y + 8), (slot_x + 8, slot_y + SLOT - 8));
            }
        }

        // Highlight the selected slot with a double outline
        if i == selected {
            canvas.set_draw_color(Color::RGB(255, 235, 90));
            let _ = canvas.draw_rect(dst);
            let _ = canvas.draw_rect(Rect::new(
                slot_x - 1,
                slot_y - 1,
                SLOT as u32 + 2,
                SLOT as u32 + 2,
            ));
        } else {
            canvas.set_draw_color(Color::RGB(90, 90, 110));
            let _ = canvas.draw_rect(dst);
        }
    }
}

fn set_title(canvas: &mut WindowCanvas, docs: &[Document], current: usize, tool: Tool) {
    let doc = &docs[current];
    let name = if doc.level.name.is_empty() {
        "(unnamed)"
    } else {
        doc.level.name.as_str()
    };
    let modified = if doc.modified { "*" } else { "" };
    let title = format!(
        "Level Editor  [{}/{}]  {}{}   |   {}x{}   |   Tool: {}",
        current + 1,
        docs.len(),
        name,
        modified,
        doc.level.width(),
        doc.level.height(),
        tool.name(),
    );
    let _ = canvas.window_mut().set_title(&title);
}

fn print_controls() {
    println!("Level Editor controls:");
    println!("  Left mouse  : paint selected tool      Right mouse : erase");
    println!("  Click palette bar to pick a tool       [ / ]       : prev/next level");
    println!("  Arrows / WASD: pan camera              Home        : scroll to start");
    println!("  Ctrl+Arrow  : grow canvas at that edge Ctrl+Shift+Arrow: shrink that edge");
    println!("  G           : toggle grid              Ctrl+S      : save level");
    println!("  Esc / Q     : quit");
}
