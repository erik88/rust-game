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
//! - Click [◀] / [▶] buttons in the toolbar to switch levels
//! - Click resize arrow buttons (top bar right side) to grow/shrink level edges
//!   Left-click = grow, Right-click = shrink
//! - Arrow keys (or WASD)      : pan the camera
//! - Ctrl+Arrow   : grow the canvas toward that edge
//! - Ctrl+Shift+Arrow : shrink the canvas from that edge
//! - `,` / `.` (or PageUp/PageDown) : previous / next level
//! - Home         : scroll back to the start of the level
//! - G            : toggle the grid overlay
//! - Ctrl+S       : save the current level back to its file
//! - Esc / Q      : quit
//!
//! Path-block editing (the moving blocks defined by `block:` headers):
//! - B : toggle path-edit mode (or click the path button in the toolbar)
//! - Left-click an empty cell : append a control point to the active block, snapped to stay horizontal/vertical from the previous one
//! - Left-click a control point : select its block, and drag it if it is an open path's endpoint
//! - Left-click an edge (between two points) : drag the whole edge perpendicular to itself
//! - Right-click a control point : delete it
//! - N : start a new block (the next click places its first point)
//! - L : toggle the active block between an open path and a closed loop
//! - Tab : cycle which block is active
//! - Delete / Backspace : remove the active block
//!
//! The active block is drawn in yellow, others in cyan; the green dot marks each
//! block's start (its resting position) and arrows show the travel direction.

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

use rustgamex::level::{self, Edge, LevelData, PathBlock};
use rustgamex::player::{PLAYER_HEIGHT, PLAYER_WIDTH, Player};
use rustgamex::tilemap::TileMap;
use rustgamex::texture::load_png_texture;
use rustgamex::tiles::{self, TILE_SIZE};
use rustgamex::time::{RealTime, TimeProvider};
use std::path::PathBuf;

const VIEW_WIDTH: u32 = 800;
// The visible play area should match the game's so levels look the same in the
// editor. The window is taller to fit the UI controls above and below it.
const PLAY_HEIGHT: u32 = 600;
const VIEW_HEIGHT: u32 = PLAY_HEIGHT + TOP_BAR_HEIGHT as u32 + HUD_HEIGHT as u32;
const PAN_SPEED: f32 = 600.0;

// Top toolbar
const TOP_BAR_HEIGHT: i32 = 36;
const BTN_Y: i32 = 4;
const BTN_H: u32 = 28;

// Palette HUD layout
const HUD_HEIGHT: i32 = 56;
const HUD_TOP: i32 = VIEW_HEIGHT as i32 - HUD_HEIGHT;
const SLOT: i32 = 40;
const SLOT_PAD: i32 = 6;
const HUD_MARGIN_X: i32 = 8;

// Tile area is between top bar and palette HUD
const TILE_AREA_TOP: i32 = TOP_BAR_HEIGHT;

// Top-bar button rects (all 44×28, y=4)
const PREV_BTN: (i32, i32, u32, u32) = (8, BTN_Y, 44, BTN_H);
const NEXT_BTN: (i32, i32, u32, u32) = (56, BTN_Y, 44, BTN_H);
// Toggles path-block editing mode.
const BLOCK_BTN: (i32, i32, u32, u32) = (170, BTN_Y, 52, BTN_H);
// Resize buttons: left-click = grow, right-click = shrink
const RESIZE_TOP_BTN: (i32, i32, u32, u32) = (580, BTN_Y, 36, BTN_H);
const RESIZE_BOT_BTN: (i32, i32, u32, u32) = (620, BTN_Y, 36, BTN_H);
const RESIZE_LEFT_BTN: (i32, i32, u32, u32) = (660, BTN_Y, 36, BTN_H);
const RESIZE_RIGHT_BTN: (i32, i32, u32, u32) = (700, BTN_Y, 36, BTN_H);
// Play button
const PLAY_BTN: (i32, i32, u32, u32) = (748, BTN_Y, 44, BTN_H);

fn to_rect(b: (i32, i32, u32, u32)) -> Rect {
    Rect::new(b.0, b.1, b.2, b.3)
}

fn btn_hit(b: (i32, i32, u32, u32), x: i32, y: i32) -> bool {
    x >= b.0 && x < b.0 + b.2 as i32 && y >= b.1 && y < b.1 + b.3 as i32
}

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

/// What the cursor is currently dragging within the active path block.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drag {
    /// A single control point (only open-path endpoints are draggable this way).
    Point(usize),
    /// A whole edge `points[i] -> points[(i+1)%n]`, moved perpendicular to itself
    /// so it carries both endpoints and keeps the path orthogonal.
    Segment(usize),
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

    let palette: Vec<Tool> = std::iter::once(Tool::Erase)
        .chain(std::iter::once(Tool::Spawn))
        // Tile id 2 is unused, so skip it when listing the paintable tiles.
        .chain(std::iter::once(Tool::Tile(tiles::SOLID)))
        .chain((tiles::DEATH..=tiles::EXIT).map(Tool::Tile))
        .chain(std::iter::once(Tool::Tile(tiles::COIN)))
        .collect();

    print_controls();

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

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

    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);

    let texture_creator = canvas.texture_creator();
    let character_texture = load_png_texture(&texture_creator, "character.png")?;
    let tilemap_texture = load_png_texture(&texture_creator, "tilemap.png")?;

    let mut event_pump = sdl_context.event_pump()?;
    let mut time_provider = RealTime::new();

    let mut current = 0;
    let mut camera_x = 0.0f32;
    let mut camera_y = 0.0f32;
    let mut selected = 2;
    let mut show_grid = true;
    let mut pan_left = false;
    let mut pan_right = false;
    let mut pan_up = false;
    let mut pan_down = false;
    let mut mouse = (0i32, 0i32);

    // Path-block editing state.
    let mut path_mode = false;
    let mut active_block: Option<usize> = None;
    // The point or edge currently being dragged within the active block.
    let mut dragging: Option<Drag> = None;
    // When set, the next left-click starts a fresh block instead of appending.
    let mut start_new = false;

    let mut tilemap = TileMap::from_level(&docs[current].level);
    let mut player = spawn_player(&docs[current].level);
    let mut dirty = false;
    set_title(&mut canvas, &docs, current, palette[selected]);

    'running: loop {
        let delta_time = time_provider.delta_time();
        let mut switch_to = None;
        let was_modified = docs[current].modified;

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
                            Keycode::Comma | Keycode::PageUp => {
                                switch_to = Some((current + docs.len() - 1) % docs.len());
                            }
                            Keycode::Period | Keycode::PageDown => {
                                switch_to = Some((current + 1) % docs.len());
                            }
                            Keycode::Home => {
                                camera_x = 0.0;
                                camera_y = 0.0;
                            }
                            Keycode::G => show_grid = !show_grid,
                            Keycode::B => {
                                path_mode = !path_mode;
                                dragging = None;
                                start_new = false;
                                println!(
                                    "Path-edit mode {}",
                                    if path_mode { "ON" } else { "OFF" }
                                );
                            }
                            Keycode::N if path_mode => {
                                // Next click starts a new block.
                                start_new = true;
                                active_block = None;
                            }
                            Keycode::Tab if path_mode => {
                                let n = docs[current].level.path_blocks.len();
                                active_block = (n > 0).then(|| {
                                    active_block.map_or(0, |b| (b + 1) % n)
                                });
                                dragging = None;
                            }
                            Keycode::L if path_mode => {
                                if toggle_loop(&mut docs[current].level, active_block) {
                                    docs[current].modified = true;
                                    dirty = true;
                                }
                            }
                            Keycode::Delete | Keycode::Backspace if path_mode => {
                                if let Some(b) = active_block {
                                    docs[current].level.path_blocks.remove(b);
                                    active_block = None;
                                    dragging = None;
                                    docs[current].modified = true;
                                    dirty = true;
                                }
                            }
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
                    if path_mode {
                        if mousestate.left()
                            && drag_path(
                                &mut docs[current].level,
                                active_block,
                                dragging,
                                x,
                                y,
                                camera_x,
                                camera_y,
                            )
                        {
                            docs[current].modified = true;
                            dirty = true;
                        }
                    } else if y >= TILE_AREA_TOP && y < HUD_TOP {
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

                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    dragging = None;
                }

                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => {
                    if y < TOP_BAR_HEIGHT {
                        // Top toolbar clicks
                        if mouse_btn == MouseButton::Left {
                            if btn_hit(PREV_BTN, x, y) {
                                switch_to =
                                    Some((current + docs.len() - 1) % docs.len());
                            } else if btn_hit(NEXT_BTN, x, y) {
                                switch_to = Some((current + 1) % docs.len());
                            } else if btn_hit(BLOCK_BTN, x, y) {
                                path_mode = !path_mode;
                                dragging = None;
                                start_new = false;
                            } else if btn_hit(PLAY_BTN, x, y) {
                                launch_game(&docs[current]);
                                // Reset pan keys so held keys don't carry over
                                pan_left = false;
                                pan_right = false;
                                pan_up = false;
                                pan_down = false;
                            }
                        }
                        // Resize buttons: left = grow, right = shrink
                        let resize_edge = if btn_hit(RESIZE_TOP_BTN, x, y) {
                            Some(Edge::Top)
                        } else if btn_hit(RESIZE_BOT_BTN, x, y) {
                            Some(Edge::Bottom)
                        } else if btn_hit(RESIZE_LEFT_BTN, x, y) {
                            Some(Edge::Left)
                        } else if btn_hit(RESIZE_RIGHT_BTN, x, y) {
                            Some(Edge::Right)
                        } else {
                            None
                        };
                        if let Some(edge) = resize_edge {
                            let changed = if mouse_btn == MouseButton::Right {
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
                        }
                    } else if y >= HUD_TOP {
                        if mouse_btn == MouseButton::Left
                            && let Some(slot) = palette_slot_at(x, palette.len())
                        {
                            selected = slot;
                            set_title(&mut canvas, &docs, current, palette[selected]);
                        }
                    } else if path_mode {
                        let changed = match mouse_btn {
                            MouseButton::Left => path_left_click(
                                &mut docs[current].level,
                                &mut active_block,
                                &mut dragging,
                                &mut start_new,
                                x,
                                y,
                                camera_x,
                                camera_y,
                            ),
                            MouseButton::Right => path_right_click(
                                &mut docs[current].level,
                                &mut active_block,
                                x,
                                y,
                                camera_x,
                                camera_y,
                            ),
                            _ => false,
                        };
                        if changed {
                            docs[current].modified = true;
                            dirty = true;
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

        if docs[current].modified != was_modified {
            set_title(&mut canvas, &docs, current, palette[selected]);
        }

        if let Some(index) = switch_to {
            current = index;
            camera_x = 0.0;
            camera_y = 0.0;
            pan_left = false;
            pan_right = false;
            pan_up = false;
            pan_down = false;
            active_block = None;
            dragging = None;
            start_new = false;
            dirty = true;
            set_title(&mut canvas, &docs, current, palette[selected]);
        }

        if dirty {
            tilemap = TileMap::from_level(&docs[current].level);
            player = spawn_player(&docs[current].level);
            dirty = false;
        }

        let pan = PAN_SPEED * delta_time;
        if pan_left { camera_x -= pan; }
        if pan_right { camera_x += pan; }
        if pan_up { camera_y -= pan; }
        if pan_down { camera_y += pan; }

        let tile_area_h = (HUD_TOP - TILE_AREA_TOP) as f32;
        let level_width = tilemap.width as f32 * tilemap.tile_size as f32;
        let level_height = tilemap.height as f32 * tilemap.tile_size as f32;
        let max_camera_x = (level_width - VIEW_WIDTH as f32).max(0.0);
        let max_camera_y = (level_height - tile_area_h).max(0.0);
        camera_x = camera_x.clamp(0.0, max_camera_x);
        camera_y = camera_y.clamp(0.0, max_camera_y);
        let camera_xi = camera_x as i32;
        let camera_yi = camera_y as i32;

        // Effective camera_y passed to render functions shifts tiles down by
        // TILE_AREA_TOP so they appear below the top toolbar.
        let render_cam_y = camera_yi - TILE_AREA_TOP;

        canvas.set_draw_color(Color::RGB(135, 206, 235));
        canvas.clear();
        tilemap.render(&mut canvas, &tilemap_texture, camera_xi, render_cam_y);
        player.render(&mut canvas, &character_texture, camera_xi, render_cam_y);

        if show_grid {
            draw_grid(&mut canvas, &tilemap, camera_xi, render_cam_y);
        }
        draw_paths(
            &mut canvas,
            &docs[current].level,
            active_block,
            path_mode,
            camera_xi,
            render_cam_y,
        );
        draw_hover(&mut canvas, &tilemap, mouse, camera_xi, render_cam_y);
        draw_hud(
            &mut canvas,
            &tilemap_texture,
            &character_texture,
            &palette,
            selected,
        );
        draw_top_bar(&mut canvas, &docs, current, path_mode);

        canvas.present();
        time_provider.wait_for_next_frame();
    }

    Ok(())
}

fn spawn_player(level: &LevelData) -> Player {
    Player::new(level.spawn.0, level.spawn.1)
}

/// Apply a tool to the tile under a screen position. Returns true if the level changed.
fn apply_tool(
    level: &mut LevelData,
    tool: Tool,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    if screen_y < TILE_AREA_TOP || screen_y >= HUD_TOP {
        return false;
    }
    let world_x = screen_x as f32 + camera_x;
    let world_y = (screen_y - TILE_AREA_TOP) as f32 + camera_y;
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
            level.tiles[ty][tx] = tiles::EMPTY;
            level.spawn = (
                tx as f32 * TILE_SIZE + (TILE_SIZE - PLAYER_WIDTH as f32) / 2.0,
                ty as f32 * TILE_SIZE + TILE_SIZE - PLAYER_HEIGHT as f32,
            );
        }
    }
    true
}

/// Convert a screen position to the tile it sits over, or `None` if it is
/// outside the tile-editing area. Mirrors the coordinate math in `apply_tool`.
fn screen_to_tile(
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> Option<(i32, i32)> {
    if !(TILE_AREA_TOP..HUD_TOP).contains(&screen_y) {
        return None;
    }
    let world_x = screen_x as f32 + camera_x;
    let world_y = (screen_y - TILE_AREA_TOP) as f32 + camera_y;
    if world_x < 0.0 || world_y < 0.0 {
        return None;
    }
    Some(((world_x / TILE_SIZE) as i32, (world_y / TILE_SIZE) as i32))
}

/// Snap `to` so it lies strictly horizontal or vertical from `from`, by keeping
/// whichever axis moves the most and locking the other to `from`.
fn snap_axis(from: (usize, usize), to: (usize, usize)) -> (usize, usize) {
    let dx = (to.0 as i32 - from.0 as i32).abs();
    let dy = (to.1 as i32 - from.1 as i32).abs();
    if dx >= dy {
        (to.0, from.1) // horizontal move
    } else {
        (from.0, to.1) // vertical move
    }
}

/// True if `a` and `b` are strictly horizontal or vertical neighbours (share
/// exactly one coordinate), matching the rule the level parser enforces.
fn axis_aligned(a: (usize, usize), b: (usize, usize)) -> bool {
    (a.0 == b.0) != (a.1 == b.1)
}

/// Find the (block index, point index) of a control point sitting on the given
/// tile, if any.
fn point_at_tile(level: &LevelData, tile: (usize, usize)) -> Option<(usize, usize)> {
    for (b, block) in level.path_blocks.iter().enumerate() {
        if let Some(p) = block.points.iter().position(|&pt| pt == tile) {
            return Some((b, p));
        }
    }
    None
}

/// True if `tile` lies strictly between the endpoints of the axis-aligned edge
/// `a`-`b` (endpoints excluded - those are handled as control points).
fn on_segment(a: (usize, usize), b: (usize, usize), tile: (usize, usize)) -> bool {
    if a.1 == b.1 && tile.1 == a.1 {
        return tile.0 > a.0.min(b.0) && tile.0 < a.0.max(b.0);
    }
    if a.0 == b.0 && tile.0 == a.0 {
        return tile.1 > a.1.min(b.1) && tile.1 < a.1.max(b.1);
    }
    false
}

/// Find the (block index, segment index) of an edge passing through `tile`. The
/// segment index `i` refers to the edge `points[i] -> points[(i+1) % n]`.
fn segment_at_tile(level: &LevelData, tile: (usize, usize)) -> Option<(usize, usize)> {
    for (b, block) in level.path_blocks.iter().enumerate() {
        let n = block.points.len();
        if n < 2 {
            continue;
        }
        let segments = if block.closed { n } else { n - 1 };
        for i in 0..segments {
            if on_segment(block.points[i], block.points[(i + 1) % n], tile) {
                return Some((b, i));
            }
        }
    }
    None
}

/// Whether every edge of the block (including the closing wrap when looped) is a
/// valid strictly horizontal/vertical segment.
fn block_is_valid(block: &PathBlock) -> bool {
    let n = block.points.len();
    if n < 2 {
        return false;
    }
    let segments = if block.closed { n } else { n - 1 };
    (0..segments).all(|i| axis_aligned(block.points[i], block.points[(i + 1) % n]))
}

/// Tile coordinate under the cursor, clamped to the level bounds. `None` if the
/// cursor is outside the tile area entirely.
fn cursor_tile(
    level: &LevelData,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> Option<(usize, usize)> {
    let (tx, ty) = screen_to_tile(screen_x, screen_y, camera_x, camera_y)?;
    if tx < 0 || ty < 0 || tx >= level.width() as i32 || ty >= level.height() as i32 {
        return None;
    }
    Some((tx as usize, ty as usize))
}

/// Handle a left-click while in path mode: select/drag an existing point, or
/// append a new one to (or start) the active block. Returns whether the level
/// changed.
#[allow(clippy::too_many_arguments)]
fn path_left_click(
    level: &mut LevelData,
    active_block: &mut Option<usize>,
    dragging: &mut Option<Drag>,
    start_new: &mut bool,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };

    // Clicking an existing point selects its block; an open path's endpoint can
    // then be dragged.
    if let Some((b, p)) = point_at_tile(level, tile) {
        *active_block = Some(b);
        let block = &level.path_blocks[b];
        let is_endpoint = !block.closed && (p == 0 || p == block.points.len() - 1);
        *dragging = is_endpoint.then_some(Drag::Point(p));
        return false;
    }

    // Clicking on an edge (between two points) selects its block and starts
    // dragging that whole edge.
    if let Some((b, s)) = segment_at_tile(level, tile) {
        *active_block = Some(b);
        *dragging = Some(Drag::Segment(s));
        return false;
    }

    // Start a new block when asked, or when there is none active yet.
    if *start_new || active_block.is_none() {
        level.path_blocks.push(PathBlock {
            points: vec![tile],
            closed: false,
        });
        *active_block = Some(level.path_blocks.len() - 1);
        *start_new = false;
        return true;
    }

    // Otherwise append a snapped point to the active (open) block.
    let block = &mut level.path_blocks[active_block.unwrap()];
    if block.closed {
        return false; // press L to open the loop before extending it
    }
    let last = *block.points.last().unwrap();
    let next = snap_axis(last, tile);
    if next == last {
        return false;
    }
    block.points.push(next);
    true
}

/// Handle a right-click while in path mode: delete the control point under the
/// cursor when doing so keeps the path valid. Returns whether the level changed.
fn path_right_click(
    level: &mut LevelData,
    active_block: &mut Option<usize>,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    let Some((b, p)) = point_at_tile(level, tile) else {
        return false;
    };

    let block = &mut level.path_blocks[b];
    let n = block.points.len();
    // Removing a point is allowed when it is an open endpoint, or when its two
    // neighbours stay axis-aligned once it is gone (a redundant point on a
    // straight run). This keeps every path valid by construction.
    let removable = if block.closed {
        n > 2 && axis_aligned(block.points[(p + n - 1) % n], block.points[(p + 1) % n])
    } else if p == 0 || p == n - 1 {
        true
    } else {
        axis_aligned(block.points[p - 1], block.points[p + 1])
    };
    if !removable {
        return false;
    }

    block.points.remove(p);
    if block.points.len() < 2 {
        level.path_blocks.remove(b);
        *active_block = None;
    } else {
        *active_block = Some(b);
    }
    true
}

/// Move whatever is being dragged (a point or a whole edge) to follow the
/// cursor, keeping the path orthogonal. Returns whether the level changed.
fn drag_path(
    level: &mut LevelData,
    active_block: Option<usize>,
    dragging: Option<Drag>,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let (Some(b), Some(drag)) = (active_block, dragging) else {
        return false;
    };
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    let block = &mut level.path_blocks[b];
    match drag {
        Drag::Point(p) => {
            // Endpoints have a single neighbour; snap to keep that segment valid.
            let neighbor = if p == 0 {
                block.points.get(1).copied()
            } else {
                block.points.get(p - 1).copied()
            };
            let new_pt = neighbor.map_or(tile, |n| snap_axis(n, tile));
            if block.points[p] == new_pt {
                return false;
            }
            block.points[p] = new_pt;
            true
        }
        Drag::Segment(s) => drag_segment(block, s, tile),
    }
}

/// Slide edge `s` perpendicular to itself onto `tile`, carrying both its
/// endpoints. The move is reverted if it would collapse an adjacent edge to zero
/// length (which would break the path). Returns whether the block changed.
fn drag_segment(block: &mut PathBlock, s: usize, tile: (usize, usize)) -> bool {
    let n = block.points.len();
    let j = (s + 1) % n;
    let (a, c) = (block.points[s], block.points[j]);

    let (new_a, new_c) = if a.1 == c.1 {
        // Horizontal edge: move both endpoints' row to the cursor's row.
        if tile.1 == a.1 {
            return false;
        }
        ((a.0, tile.1), (c.0, tile.1))
    } else if a.0 == c.0 {
        // Vertical edge: move both endpoints' column to the cursor's column.
        if tile.0 == a.0 {
            return false;
        }
        ((tile.0, a.1), (tile.0, c.1))
    } else {
        return false;
    };

    block.points[s] = new_a;
    block.points[j] = new_c;
    if !block_is_valid(block) {
        block.points[s] = a;
        block.points[j] = c;
        return false;
    }
    true
}

/// Toggle the active block between an open path and a closed loop. Closing
/// requires at least three points and an axis-aligned wrap from the last point
/// back to the first. Returns whether anything changed.
fn toggle_loop(level: &mut LevelData, active_block: Option<usize>) -> bool {
    let Some(b) = active_block else {
        return false;
    };
    let block = &mut level.path_blocks[b];
    if block.closed {
        block.closed = false;
        return true;
    }
    let first = block.points[0];
    let last = *block.points.last().unwrap();
    if block.points.len() >= 3 && axis_aligned(last, first) {
        block.closed = true;
        return true;
    }
    println!("Can't close this path: need 3+ points and an aligned closing segment");
    false
}

/// Write the current level to a temporary directory and launch the game binary
/// pointing at it. The temp dir contains only one level so completing it loops
/// back to the start rather than advancing. Blocks until the game window closes.
fn launch_game(doc: &Document) {
    let tmp = std::env::temp_dir().join("rustgame_preview");
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        eprintln!("Failed to create temp dir: {e}");
        return;
    }
    if let Err(e) = std::fs::write(tmp.join("level.txt"), doc.level.to_text()) {
        eprintln!("Failed to write preview level: {e}");
        return;
    }

    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("rustgame")));

    let status = exe
        .filter(|p| p.exists())
        .map(|p| {
            std::process::Command::new(p)
                .args(["--levels-dir", tmp.to_str().unwrap_or("levels")])
                .status()
        })
        .unwrap_or_else(|| {
            std::process::Command::new("cargo")
                .args(["run", "--bin", "rustgame", "--", "--levels-dir"])
                .arg(tmp.to_str().unwrap_or("levels"))
                .status()
        });

    if let Err(e) = status {
        eprintln!("Failed to launch game: {e}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn save(doc: &mut Document) {
    match std::fs::write(&doc.path, doc.level.to_text()) {
        Ok(()) => {
            doc.modified = false;
            println!("Saved {}", doc.path.display());
        }
        Err(e) => eprintln!("Failed to save {}: {}", doc.path.display(), e),
    }
}

fn palette_slot_at(x: i32, count: usize) -> Option<usize> {
    for i in 0..count {
        let slot_x = HUD_MARGIN_X + i as i32 * (SLOT + SLOT_PAD);
        if x >= slot_x && x < slot_x + SLOT {
            return Some(i);
        }
    }
    None
}

fn draw_grid(canvas: &mut WindowCanvas, tilemap: &TileMap, camera_x: i32, camera_y: i32) {
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 60));
    let size = tilemap.tile_size as i32;

    let level_w = tilemap.width as i32 * size;
    let level_h = tilemap.height as i32 * size;
    // camera_y is already offset so that row 0 draws at TILE_AREA_TOP when camera scroll=0.
    // y0 must be clamped to TILE_AREA_TOP so grid never bleeds into the top bar.
    let x0 = (-camera_x).max(0);
    let x1 = (level_w - camera_x).min(VIEW_WIDTH as i32);
    let y0 = (-camera_y).max(TILE_AREA_TOP);
    let y1 = (level_h - camera_y).min(HUD_TOP);

    for col in 0..=tilemap.width as i32 {
        let x = col * size - camera_x;
        if (0..=VIEW_WIDTH as i32).contains(&x) {
            let _ = canvas.draw_line((x, y0), (x, y1));
        }
    }
    for row in 0..=tilemap.height as i32 {
        let y = row * size - camera_y;
        if (TILE_AREA_TOP..=HUD_TOP).contains(&y) {
            let _ = canvas.draw_line((x0, y), (x1, y));
        }
    }
}

fn draw_hover(
    canvas: &mut WindowCanvas,
    tilemap: &TileMap,
    mouse: (i32, i32),
    camera_x: i32,
    camera_y: i32,
) {
    let (mx, my) = mouse;
    if my < TILE_AREA_TOP || my >= HUD_TOP {
        return;
    }
    let size = tilemap.tile_size as i32;
    let world_x = mx + camera_x;
    // camera_y here is camera_yi - TILE_AREA_TOP, so world tile coords use it directly.
    let world_y = my + camera_y; // = my - TILE_AREA_TOP + camera_yi → tile row
    if world_x < 0 || world_y < TILE_AREA_TOP {
        return;
    }
    // Convert back to tile indices: tile row = (world_y - TILE_AREA_TOP + camera_yi) / size
    // But world_y = my + camera_y = my + camera_yi - TILE_AREA_TOP
    // tile_y = (my - TILE_AREA_TOP + camera_yi) / size = world_y / size  (since camera_y = camera_yi - TILE_AREA_TOP)
    // Actually: tile screen top = tile_row * size - camera_y = tile_row * size - camera_yi + TILE_AREA_TOP
    // so tile_row * size = screen_y + camera_yi - TILE_AREA_TOP = (my - TILE_AREA_TOP) + camera_yi
    // tile_row = ((my - TILE_AREA_TOP) + camera_yi) / size
    // world_y = my + camera_yi - TILE_AREA_TOP, so tile_row = world_y / size ✓
    let tx = world_x / size;
    let ty = world_y / size;
    if tx >= tilemap.width as i32 || ty >= tilemap.height as i32 || ty < 0 {
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

/// Draw the path-block overlay: each block's control points joined by lines,
/// with a green start marker and direction arrows. The active block is drawn in
/// yellow, others in cyan; everything is dimmed when not in path-edit mode.
fn draw_paths(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    active_block: Option<usize>,
    path_mode: bool,
    camera_x: i32,
    camera_y: i32,
) {
    if level.path_blocks.is_empty() {
        return;
    }
    let size = TILE_SIZE as i32;
    // Keep the overlay inside the play area so lines never bleed into the bars.
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));

    let alpha = if path_mode { 235 } else { 110 };
    let center = |pt: (usize, usize)| -> (i32, i32) {
        (
            pt.0 as i32 * size + size / 2 - camera_x,
            pt.1 as i32 * size + size / 2 - camera_y,
        )
    };

    for (b, block) in level.path_blocks.iter().enumerate() {
        let is_active = path_mode && active_block == Some(b);
        let line_color = if is_active {
            Color::RGBA(255, 235, 90, alpha)
        } else {
            Color::RGBA(100, 200, 235, alpha)
        };

        // Segments between consecutive points (plus the closing wrap if looped).
        let n = block.points.len();
        let last_seg = if block.closed { n } else { n.saturating_sub(1) };
        for i in 0..last_seg {
            let a = center(block.points[i]);
            let c = center(block.points[(i + 1) % n]);
            canvas.set_draw_color(line_color);
            draw_thick_line(canvas, a, c);
            if path_mode {
                draw_seg_arrow(canvas, a, c, line_color);
            }
        }

        // Control points: the start (index 0, the resting position) is a larger
        // green dot; the rest are smaller dots in the block's colour.
        for (i, &pt) in block.points.iter().enumerate() {
            let (px, py) = center(pt);
            if i == 0 {
                canvas.set_draw_color(Color::RGBA(90, 220, 120, alpha));
                fill_circle(canvas, px, py, 6);
            } else {
                canvas.set_draw_color(line_color);
                fill_circle(canvas, px, py, 4);
            }
        }
    }

    canvas.set_clip_rect(prev_clip);
}

/// Draw a 3px-wide line by stacking three 1px lines.
fn draw_thick_line(canvas: &mut WindowCanvas, a: (i32, i32), b: (i32, i32)) {
    let _ = canvas.draw_line(a, b);
    let _ = canvas.draw_line((a.0 + 1, a.1), (b.0 + 1, b.1));
    let _ = canvas.draw_line((a.0, a.1 + 1), (b.0, b.1 + 1));
}

/// Draw a small chevron at the midpoint of an axis-aligned segment, pointing
/// from `a` toward `b` to show the block's travel direction.
fn draw_seg_arrow(canvas: &mut WindowCanvas, a: (i32, i32), b: (i32, i32), color: Color) {
    canvas.set_draw_color(color);
    let mx = (a.0 + b.0) / 2;
    let my = (a.1 + b.1) / 2;
    let dx = (b.0 - a.0).signum();
    let dy = (b.1 - a.1).signum();
    let s = 5;
    if dx != 0 {
        let _ = canvas.draw_line((mx, my), (mx - dx * s, my - s));
        let _ = canvas.draw_line((mx, my), (mx - dx * s, my + s));
    } else if dy != 0 {
        let _ = canvas.draw_line((mx, my), (mx - s, my - dy * s));
        let _ = canvas.draw_line((mx, my), (mx + s, my - dy * s));
    }
}

fn draw_hud(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    palette: &[Tool],
    selected: usize,
) {
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, HUD_TOP, VIEW_WIDTH, HUD_HEIGHT as u32));
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, HUD_TOP), (VIEW_WIDTH as i32, HUD_TOP));

    let slot_y = HUD_TOP + (HUD_HEIGHT - SLOT) / 2;
    for (i, tool) in palette.iter().enumerate() {
        let slot_x = HUD_MARGIN_X + i as i32 * (SLOT + SLOT_PAD);
        let dst = Rect::new(slot_x, slot_y, SLOT as u32, SLOT as u32);

        canvas.set_draw_color(Color::RGB(55, 55, 70));
        let _ = canvas.fill_rect(dst);

        match *tool {
            Tool::Tile(n) => {
                let (sx, sy) = tiles::tile_src_xy(n);
                let src = Rect::new(sx, sy, TILE_SIZE as u32, TILE_SIZE as u32);
                let _ = canvas.copy(tilemap_texture, Some(src), Some(dst));
            }
            Tool::Spawn => {
                let src = Rect::new(0, 0, PLAYER_WIDTH, PLAYER_HEIGHT);
                let player_dst = Rect::new(
                    slot_x + (SLOT - PLAYER_WIDTH as i32) / 2,
                    slot_y + (SLOT - PLAYER_HEIGHT as i32) / 2,
                    PLAYER_WIDTH,
                    PLAYER_HEIGHT,
                );
                let _ = canvas.copy(character_texture, Some(src), Some(player_dst));
            }
            Tool::Erase => {
                canvas.set_draw_color(Color::RGB(220, 80, 80));
                let _ = canvas
                    .draw_line((slot_x + 8, slot_y + 8), (slot_x + SLOT - 8, slot_y + SLOT - 8));
                let _ = canvas
                    .draw_line((slot_x + SLOT - 8, slot_y + 8), (slot_x + 8, slot_y + SLOT - 8));
            }
        }

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

/// Draw the top toolbar: Prev/Next level buttons and resize buttons.
fn draw_top_bar(canvas: &mut WindowCanvas, docs: &[Document], current: usize, path_mode: bool) {
    // Background
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, 0, VIEW_WIDTH, TOP_BAR_HEIGHT as u32));
    // Bottom border
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, TOP_BAR_HEIGHT - 1), (VIEW_WIDTH as i32, TOP_BAR_HEIGHT - 1));

    // --- Prev / Next buttons ---
    draw_nav_button(canvas, PREV_BTN, ArrowDir::Left);
    draw_nav_button(canvas, NEXT_BTN, ArrowDir::Right);

    // --- Path-mode toggle button ---
    draw_block_button(canvas, BLOCK_BTN, path_mode);

    // Level indicator dots between the nav buttons
    let n = docs.len();
    let dot_r = 3i32;
    let dot_spacing = 10i32;
    let dots_w = (n as i32 - 1) * dot_spacing;
    let dots_cx = (PREV_BTN.0 + PREV_BTN.2 as i32 + NEXT_BTN.0) / 2;
    let dots_y = BTN_Y + BTN_H as i32 / 2;
    for i in 0..n {
        let cx = dots_cx - dots_w / 2 + i as i32 * dot_spacing;
        if i == current {
            canvas.set_draw_color(Color::RGB(255, 235, 90));
        } else {
            canvas.set_draw_color(Color::RGB(90, 90, 110));
        }
        fill_circle(canvas, cx, dots_y, dot_r);
    }

    // --- Separator before resize ---
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line(
        (RESIZE_TOP_BTN.0 - 8, BTN_Y + 4),
        (RESIZE_TOP_BTN.0 - 8, BTN_Y + BTN_H as i32 - 4),
    );

    // --- Resize buttons ---
    draw_resize_button(canvas, RESIZE_TOP_BTN, ArrowDir::Up);
    draw_resize_button(canvas, RESIZE_BOT_BTN, ArrowDir::Down);
    draw_resize_button(canvas, RESIZE_LEFT_BTN, ArrowDir::Left);
    draw_resize_button(canvas, RESIZE_RIGHT_BTN, ArrowDir::Right);

    // --- Separator before play ---
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line(
        (PLAY_BTN.0 - 8, BTN_Y + 4),
        (PLAY_BTN.0 - 8, BTN_Y + BTN_H as i32 - 4),
    );

    // --- Play button ---
    draw_play_button(canvas, PLAY_BTN);
}

#[derive(Clone, Copy)]
enum ArrowDir {
    Left,
    Right,
    Up,
    Down,
}

/// Draw a Prev/Next navigation button.
fn draw_nav_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), dir: ArrowDir) {
    let r = to_rect(b);
    canvas.set_draw_color(Color::RGB(50, 50, 65));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(90, 90, 110));
    let _ = canvas.draw_rect(r);
    draw_arrow_shape(canvas, b, dir, Color::RGB(200, 200, 220));
}

/// Draw a resize button (grow on left-click, shrink on right-click).
fn draw_resize_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), dir: ArrowDir) {
    let r = to_rect(b);
    canvas.set_draw_color(Color::RGB(45, 55, 65));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(80, 100, 110));
    let _ = canvas.draw_rect(r);
    draw_arrow_shape(canvas, b, dir, Color::RGB(120, 200, 160));
}

/// Draw a chevron arrow inside a button rect.
fn draw_arrow_shape(
    canvas: &mut WindowCanvas,
    b: (i32, i32, u32, u32),
    dir: ArrowDir,
    color: Color,
) {
    canvas.set_draw_color(color);
    let cx = b.0 + b.2 as i32 / 2;
    let cy = b.1 + b.3 as i32 / 2;
    let half = 7i32;
    let tip = 7i32;
    match dir {
        ArrowDir::Left => {
            let _ = canvas.draw_line((cx + tip, cy - half), (cx - tip, cy));
            let _ = canvas.draw_line((cx - tip, cy), (cx + tip, cy + half));
        }
        ArrowDir::Right => {
            let _ = canvas.draw_line((cx - tip, cy - half), (cx + tip, cy));
            let _ = canvas.draw_line((cx + tip, cy), (cx - tip, cy + half));
        }
        ArrowDir::Up => {
            let _ = canvas.draw_line((cx - half, cy + tip), (cx, cy - tip));
            let _ = canvas.draw_line((cx, cy - tip), (cx + half, cy + tip));
        }
        ArrowDir::Down => {
            let _ = canvas.draw_line((cx - half, cy - tip), (cx, cy + tip));
            let _ = canvas.draw_line((cx, cy + tip), (cx + half, cy - tip));
        }
    }
}

/// Draw a green play (triangle) button.
fn draw_play_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32)) {
    let r = to_rect(b);
    canvas.set_draw_color(Color::RGB(40, 80, 50));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(80, 160, 100));
    let _ = canvas.draw_rect(r);

    // Filled triangle pointing right
    let cx = b.0 + b.2 as i32 / 2;
    let cy = b.1 + b.3 as i32 / 2;
    let half_h = 8i32;
    let depth = 9i32;
    canvas.set_draw_color(Color::RGB(100, 220, 130));
    for dy in -half_h..=half_h {
        let width = depth * (half_h - dy.abs()) / half_h;
        let _ = canvas.draw_line((cx - width, cy + dy), (cx + depth - width, cy + dy));
    }
}

/// Draw the path-edit toggle button: a little zig-zag path with dotted nodes,
/// lit up yellow while path mode is active.
fn draw_block_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), active: bool) {
    let r = to_rect(b);
    if active {
        canvas.set_draw_color(Color::RGB(80, 75, 40));
    } else {
        canvas.set_draw_color(Color::RGB(50, 50, 65));
    }
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);

    // A small zig-zag path with node dots, matching the overlay's look.
    let glyph = if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(160, 200, 230)
    };
    canvas.set_draw_color(glyph);
    let cy = b.1 + b.3 as i32 / 2;
    let nodes = [
        (b.0 + 12, cy + 6),
        (b.0 + 26, cy + 6),
        (b.0 + 26, cy - 6),
        (b.0 + 40, cy - 6),
    ];
    for w in nodes.windows(2) {
        let _ = canvas.draw_line(w[0], w[1]);
    }
    for n in nodes {
        fill_circle(canvas, n.0, n.1, 2);
    }
}

/// Approximate a filled circle using horizontal lines.
fn fill_circle(canvas: &mut WindowCanvas, cx: i32, cy: i32, r: i32) {
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt() as i32;
        let _ = canvas.draw_line((cx - dx, cy + dy), (cx + dx, cy + dy));
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
    println!("  Click palette bar to pick a tool       , / . (PgUp/PgDn): prev/next level");
    println!("  Toolbar [◀]/[▶] buttons : prev/next level");
    println!("  Toolbar resize arrows   : left-click=grow edge, right-click=shrink edge");
    println!("  Arrows / WASD: pan camera              Home        : scroll to start");
    println!("  Ctrl+Arrow  : grow canvas at that edge Ctrl+Shift+Arrow: shrink that edge");
    println!("  G           : toggle grid              Ctrl+S      : save level");
    println!("  B           : toggle path-edit mode (or click the path toolbar button)");
    println!("  Path mode   : left-click adds points / drags points & edges, right-click deletes");
    println!("                N new block, L open/close loop, Tab cycle, Del remove block");
    println!("  Esc / Q     : quit");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10x10 empty level with a spawn in the bottom-left.
    fn empty_level() -> LevelData {
        let mut rows = vec![".".repeat(10); 10];
        rows[9].replace_range(0..1, "P");
        LevelData::parse(&rows.join("\n")).unwrap()
    }

    /// Screen position at the centre of tile (tx, ty) with the camera at origin,
    /// the inverse of `screen_to_tile`.
    fn click_at(tx: i32, ty: i32) -> (i32, i32) {
        let s = TILE_SIZE as i32;
        (tx * s + s / 2, TILE_AREA_TOP + ty * s + s / 2)
    }

    #[test]
    fn snap_axis_locks_to_dominant_axis() {
        assert_eq!(snap_axis((2, 2), (6, 3)), (6, 2)); // horizontal wins
        assert_eq!(snap_axis((2, 2), (3, 6)), (2, 6)); // vertical wins
    }

    #[test]
    fn axis_aligned_detects_orthogonal_neighbours() {
        assert!(axis_aligned((1, 1), (1, 5)));
        assert!(axis_aligned((1, 1), (4, 1)));
        assert!(!axis_aligned((1, 1), (2, 2)));
        assert!(!axis_aligned((1, 1), (1, 1)));
    }

    #[test]
    fn left_click_builds_an_axis_aligned_path() {
        let mut level = empty_level();
        let (mut active, mut drag, mut new) = (None, None, false);

        let (x, y) = click_at(1, 1);
        assert!(path_left_click(
            &mut level, &mut active, &mut drag, &mut new, x, y, 0.0, 0.0
        ));
        // A diagonal target snaps to a horizontal segment from the last point.
        let (x, y) = click_at(4, 3);
        assert!(path_left_click(
            &mut level, &mut active, &mut drag, &mut new, x, y, 0.0, 0.0
        ));

        assert_eq!(level.path_blocks[0].points, vec![(1, 1), (4, 1)]);
        assert_eq!(active, Some(0));
    }

    #[test]
    fn toggle_loop_requires_a_valid_closing_segment() {
        let mut level = empty_level();
        // Open L-shape: closing (4,4) -> (1,1) would be diagonal, so it can't close.
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4)],
            closed: false,
        });
        assert!(!toggle_loop(&mut level, Some(0)));
        assert!(!level.path_blocks[0].closed);

        // A rectangle closes cleanly.
        level.path_blocks[0].points = vec![(1, 1), (4, 1), (4, 4), (1, 4)];
        assert!(toggle_loop(&mut level, Some(0)));
        assert!(level.path_blocks[0].closed);
    }

    #[test]
    fn right_click_deletes_endpoint_and_drops_degenerate_block() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (4, 1)],
            closed: false,
        });
        let mut active = Some(0);

        let (x, y) = click_at(4, 1);
        assert!(path_right_click(&mut level, &mut active, x, y, 0.0, 0.0));
        // Down to one point, so the whole block is removed.
        assert!(level.path_blocks.is_empty());
        assert_eq!(active, None);
    }

    #[test]
    fn right_click_keeps_corner_points_of_a_loop() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4), (1, 4)],
            closed: true,
        });
        let mut active = Some(0);
        // Deleting a corner would leave a diagonal join, so it is refused.
        let (x, y) = click_at(4, 1);
        assert!(!path_right_click(&mut level, &mut active, x, y, 0.0, 0.0));
        assert_eq!(level.path_blocks[0].points.len(), 4);
    }

    #[test]
    fn segment_at_tile_finds_edges_not_endpoints() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (5, 1)],
            closed: false,
        });
        // A tile between the endpoints hits the edge...
        assert_eq!(segment_at_tile(&level, (3, 1)), Some((0, 0)));
        // ...but the endpoints themselves and off-line tiles do not.
        assert_eq!(segment_at_tile(&level, (1, 1)), None);
        assert_eq!(segment_at_tile(&level, (3, 2)), None);
    }

    #[test]
    fn drag_segment_slides_a_horizontal_edge_keeping_neighbours_valid() {
        // A square loop; dragging the top edge down carries both its corners.
        let mut block = PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4), (1, 4)],
            closed: true,
        };
        assert!(drag_segment(&mut block, 0, (99, 2)));
        assert_eq!(block.points, vec![(1, 2), (4, 2), (4, 4), (1, 4)]);
        assert!(block_is_valid(&block));
    }

    #[test]
    fn drag_segment_slides_a_vertical_edge() {
        let mut block = PathBlock {
            points: vec![(1, 1), (1, 5)],
            closed: false,
        };
        // Vertical edge: only the column moves; both endpoints follow.
        assert!(drag_segment(&mut block, 0, (3, 99)));
        assert_eq!(block.points, vec![(3, 1), (3, 5)]);
    }

    #[test]
    fn drag_segment_reverts_when_it_would_collapse_a_neighbour() {
        // Open L-shape: dragging the top edge down onto row 4 would make its
        // right corner coincide with the next point, so the move is refused.
        let mut block = PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4)],
            closed: false,
        };
        assert!(!drag_segment(&mut block, 0, (99, 4)));
        assert_eq!(block.points, vec![(1, 1), (4, 1), (4, 4)]);
    }

    #[test]
    fn left_click_on_edge_starts_a_segment_drag() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (5, 1)],
            closed: false,
        });
        let (mut active, mut drag, mut new) = (None, None, false);
        let (x, y) = click_at(3, 1); // a tile on the edge
        assert!(!path_left_click(
            &mut level, &mut active, &mut drag, &mut new, x, y, 0.0, 0.0
        ));
        assert_eq!(active, Some(0));
        assert_eq!(drag, Some(Drag::Segment(0)));
    }
}
