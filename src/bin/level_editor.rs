//! Level editor: browse and edit the level files in `levels/`.
//!
//! It renders each level the way the game does (tiles, moving platforms in
//! their starting positions, and the player at the spawn point) and lets you
//! paint tiles onto the grid. Edits are kept in memory per level and are only
//! written to disk when you save, so switching levels never loses work.
//!
//! The editor has three mutually-exclusive modes, switched with the grouped
//! toolbar buttons (or the keys noted below): Normal (paint tiles), Path (edit
//! path blocks) and Deco (place decorations). The bottom tool palette is only
//! shown in Normal mode.
//!
//! Controls:
//! - Click a mode button in the toolbar (or press B / X) to switch modes
//! - Left mouse   : in Normal mode, apply the selected palette tool to the tile under the cursor
//! - Right mouse  : in Normal mode, erase the tile under the cursor
//! - (click+drag paints continuously)
//! - Click the bottom palette bar to choose a tool (Normal mode)
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
//! - B : enter path mode (press again to return to Normal), or click the path button

//! - Left-click an empty cell : append a control point to the active block, snapped to stay horizontal/vertical from the previous one
//! - Left-click a control point : select its block, and drag it if it is an open path's endpoint
//! - Left-click an edge (between two points) : drag the whole edge perpendicular to itself
//! - Right-click a control point : delete it
//! - N : start a new block (the next click places its first point)
//! - L : toggle the active block between an open path and a closed loop (or click
//!   the Toggle-loop button in the path menu that appears in the bottom bar)
//! - Tab : cycle which block is active
//! - Delete / Backspace : remove the active block
//!
//! The active block is drawn in yellow, others in cyan; the green dot marks each
//! block's start (its resting position) and arrows show the travel direction.
//!
//! Decoration editing (render-only sprites that do not affect gameplay):
//! - X : enter decoration mode (press again to return to Normal), or click the picture button
//! - The tilemap sheet appears as a picker overlay; click a cell to choose a sprite
//! - Left-click (or drag) a cell : place the chosen decoration, snapped to the grid
//! - Right-click (or drag) a cell : erase the decoration there

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

use rustgamex::level::{self, Decoration, Edge, LevelData, PathBlock};
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

// Path-mode action menu, drawn in the (otherwise empty) HUD bar while in path
// mode. For now it holds a single "toggle loop" button.
const PATH_MENU_BTN_H: i32 = 36;
const PATH_LOOP_BTN: (i32, i32, u32, u32) = (
    HUD_MARGIN_X,
    HUD_TOP + (HUD_HEIGHT - PATH_MENU_BTN_H) / 2,
    52,
    PATH_MENU_BTN_H as u32,
);

// Tile area is between top bar and palette HUD
const TILE_AREA_TOP: i32 = TOP_BAR_HEIGHT;

// Decoration sprite picker: the whole 240x240 tilemap sheet (6x6 sprites at full
// scale) drawn as an overlay in the top-right of the tile area while in
// decoration mode. Clicking a cell selects that sprite.
const PICKER_ROWS: i32 = 6;
const PICKER_CELL: i32 = TILE_SIZE as i32;
const PICKER_W: i32 = tiles::SHEET_COLUMNS as i32 * PICKER_CELL;
const PICKER_H: i32 = PICKER_ROWS * PICKER_CELL;
const PICKER_X: i32 = VIEW_WIDTH as i32 - PICKER_W;
const PICKER_Y: i32 = TILE_AREA_TOP;

// Top-bar button rects (all 44×28, y=4)
const PREV_BTN: (i32, i32, u32, u32) = (8, BTN_Y, 44, BTN_H);
const NEXT_BTN: (i32, i32, u32, u32) = (56, BTN_Y, 44, BTN_H);
// Mode-switch buttons, grouped left-to-right: place normal tiles, edit path
// blocks, place decorations. Exactly one mode is active at a time.
const MODE_BTN_W: u32 = 52;
const MODE_BTN_X0: i32 = 116;
const MODE_BTN_STEP: i32 = MODE_BTN_W as i32 + 6;
const NORMAL_BTN: (i32, i32, u32, u32) = (MODE_BTN_X0, BTN_Y, MODE_BTN_W, BTN_H);
const BLOCK_BTN: (i32, i32, u32, u32) = (MODE_BTN_X0 + MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
const DECO_BTN: (i32, i32, u32, u32) = (MODE_BTN_X0 + 2 * MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
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

/// Which editing mode the editor is in. Exactly one is active at a time; the
/// bottom tool palette is shown only in [`Mode::Normal`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Paint normal tiles from the bottom palette.
    Normal,
    /// Edit path-block control points.
    Path,
    /// Place render-only decorations.
    Deco,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Normal => "Normal (tiles)",
            Mode::Path => "Path blocks",
            Mode::Deco => "Decorations",
        }
    }
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

    // The active editing mode (normal tiles / path blocks / decorations).
    let mut mode = Mode::Normal;

    // Path-block editing state (used while in `Mode::Path`).
    let mut active_block: Option<usize> = None;
    // The point or edge currently being dragged within the active block.
    let mut dragging: Option<Drag> = None;
    // When set, the next left-click starts a fresh block instead of appending.
    let mut start_new = false;

    // Decoration-placement state: which sprite-sheet index the next placement
    // uses (while in `Mode::Deco`).
    let mut deco_sprite: u32 = 1;

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
                            // B and X toggle their mode on/off (returning to
                            // Normal); the three are mutually exclusive.
                            Keycode::B => {
                                mode = if mode == Mode::Path { Mode::Normal } else { Mode::Path };
                                dragging = None;
                                start_new = false;
                                println!("Mode: {}", mode.name());
                            }
                            Keycode::X => {
                                mode = if mode == Mode::Deco { Mode::Normal } else { Mode::Deco };
                                dragging = None;
                                start_new = false;
                                println!("Mode: {}", mode.name());
                            }
                            Keycode::N if mode == Mode::Path => {
                                // Next click starts a new block.
                                start_new = true;
                                active_block = None;
                            }
                            Keycode::Tab if mode == Mode::Path => {
                                let n = docs[current].level.path_blocks.len();
                                active_block = (n > 0).then(|| {
                                    active_block.map_or(0, |b| (b + 1) % n)
                                });
                                dragging = None;
                            }
                            Keycode::L if mode == Mode::Path => {
                                if toggle_loop(&mut docs[current].level, active_block) {
                                    docs[current].modified = true;
                                    dirty = true;
                                }
                            }
                            Keycode::Delete | Keycode::Backspace if mode == Mode::Path => {
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
                    if mode == Mode::Path {
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
                    } else if mode == Mode::Deco {
                        // Drag to paint or erase a run of decorations, but never
                        // while the cursor is over the picker overlay.
                        let changed = if in_deco_picker(x, y) {
                            false
                        } else if mousestate.left() {
                            place_deco(&mut docs[current].level, deco_sprite, x, y, camera_x, camera_y)
                        } else if mousestate.right() {
                            erase_deco(&mut docs[current].level, x, y, camera_x, camera_y)
                        } else {
                            false
                        };
                        if changed {
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
                            } else if btn_hit(NORMAL_BTN, x, y) {
                                mode = Mode::Normal;
                                dragging = None;
                                start_new = false;
                            } else if btn_hit(BLOCK_BTN, x, y) {
                                // Toggle: a second click on the active mode's
                                // button returns to Normal.
                                mode = if mode == Mode::Path { Mode::Normal } else { Mode::Path };
                                dragging = None;
                                start_new = false;
                            } else if btn_hit(DECO_BTN, x, y) {
                                mode = if mode == Mode::Deco { Mode::Normal } else { Mode::Deco };
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
                        // The HUD bar holds the tool palette in Normal mode and the
                        // path action menu in Path mode; Deco mode leaves it empty.
                        match mode {
                            Mode::Normal => {
                                if mouse_btn == MouseButton::Left
                                    && let Some(slot) = palette_slot_at(x, palette.len())
                                {
                                    selected = slot;
                                    set_title(&mut canvas, &docs, current, palette[selected]);
                                }
                            }
                            Mode::Path => {
                                if mouse_btn == MouseButton::Left
                                    && btn_hit(PATH_LOOP_BTN, x, y)
                                    && toggle_loop(&mut docs[current].level, active_block)
                                {
                                    docs[current].modified = true;
                                    dirty = true;
                                }
                            }
                            Mode::Deco => {}
                        }
                    } else if mode == Mode::Deco {
                        // Clicking the sprite picker selects a sprite; clicking
                        // the level places (left) or erases (right) a decoration.
                        if in_deco_picker(x, y) {
                            if mouse_btn == MouseButton::Left
                                && let Some(s) = picker_sprite_at(x, y)
                            {
                                deco_sprite = s;
                            }
                        } else {
                            let changed = match mouse_btn {
                                MouseButton::Left => place_deco(
                                    &mut docs[current].level,
                                    deco_sprite,
                                    x,
                                    y,
                                    camera_x,
                                    camera_y,
                                ),
                                MouseButton::Right => erase_deco(
                                    &mut docs[current].level,
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
                        }
                    } else if mode == Mode::Path {
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
            mode == Mode::Path,
            camera_xi,
            render_cam_y,
        );
        draw_decorations(
            &mut canvas,
            &docs[current].level,
            mode == Mode::Deco,
            camera_xi,
            render_cam_y,
        );
        draw_hover(&mut canvas, &tilemap, mouse, camera_xi, render_cam_y);
        if mode == Mode::Deco {
            draw_deco_picker(&mut canvas, &tilemap_texture, deco_sprite);
        }
        draw_hud(
            &mut canvas,
            &tilemap_texture,
            &character_texture,
            &palette,
            selected,
            mode,
        );
        if mode == Mode::Path {
            draw_path_menu(&mut canvas, &docs[current].level, active_block);
        }
        draw_top_bar(&mut canvas, &docs, current, mode);

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

/// Whether a screen position is over the decoration sprite-picker overlay.
fn in_deco_picker(x: i32, y: i32) -> bool {
    x >= PICKER_X && x < PICKER_X + PICKER_W && y >= PICKER_Y && y < PICKER_Y + PICKER_H
}

/// The 1-based sprite-sheet index for the picker cell under a screen position,
/// or `None` if the position is outside the picker.
fn picker_sprite_at(x: i32, y: i32) -> Option<u32> {
    if !in_deco_picker(x, y) {
        return None;
    }
    let col = (x - PICKER_X) / PICKER_CELL;
    let row = (y - PICKER_Y) / PICKER_CELL;
    Some((row * tiles::SHEET_COLUMNS as i32 + col + 1) as u32)
}

/// Index of the decoration snapped to the given tile cell, if any. Decorations
/// placed through the editor are grid-aligned, so a cell holds at most one.
fn deco_index_at(level: &LevelData, tile: (usize, usize)) -> Option<usize> {
    level.decorations.iter().position(|d| {
        (d.x / TILE_SIZE).round() as i64 == tile.0 as i64
            && (d.y / TILE_SIZE).round() as i64 == tile.1 as i64
    })
}

/// Place a decoration of `sprite` at the grid cell under a screen position,
/// snapped to the grid. Replaces any decoration already in that cell. Returns
/// whether the level changed.
fn place_deco(
    level: &mut LevelData,
    sprite: u32,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    if let Some(i) = deco_index_at(level, tile) {
        if level.decorations[i].sprite == sprite {
            return false;
        }
        level.decorations[i].sprite = sprite;
        return true;
    }
    level.decorations.push(Decoration {
        x: tile.0 as f32 * TILE_SIZE,
        y: tile.1 as f32 * TILE_SIZE,
        sprite,
    });
    true
}

/// Remove the decoration in the grid cell under a screen position, if any.
/// Returns whether the level changed.
fn erase_deco(
    level: &mut LevelData,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    match deco_index_at(level, tile) {
        Some(i) => {
            level.decorations.remove(i);
            true
        }
        None => false,
    }
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

/// Mark every decoration with a corner-bracket outline so its render-only sprite
/// is distinguishable from the real gameplay tiles in the editor. Brighter while
/// decoration mode is active, dimmed otherwise (mirroring the path overlay).
fn draw_decorations(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    deco_mode: bool,
    camera_x: i32,
    camera_y: i32,
) {
    if level.decorations.is_empty() {
        return;
    }
    // Keep the overlay inside the play area so brackets never bleed into the bars.
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));

    let size = TILE_SIZE as i32;
    let alpha = if deco_mode { 235 } else { 110 };
    // Magenta reads distinctly from the path overlay (yellow/cyan) and the hover
    // highlight (yellow).
    canvas.set_draw_color(Color::RGBA(235, 110, 230, alpha));

    for deco in &level.decorations {
        let x = deco.x as i32 - camera_x;
        let y = deco.y as i32 - camera_y;
        draw_corner_brackets(canvas, x, y, size);
    }

    canvas.set_clip_rect(prev_clip);
}

/// Draw an L-shaped bracket at each corner of the `size`x`size` cell at (x, y),
/// using the canvas's current draw colour. Reads as a "marked object" outline
/// without looking like a full tile border.
fn draw_corner_brackets(canvas: &mut WindowCanvas, x: i32, y: i32, size: i32) {
    let b = (size / 4).max(4); // bracket arm length
    let (l, t, r, bot) = (x, y, x + size - 1, y + size - 1);
    // Top-left
    let _ = canvas.draw_line((l, t), (l + b, t));
    let _ = canvas.draw_line((l, t), (l, t + b));
    // Top-right
    let _ = canvas.draw_line((r, t), (r - b, t));
    let _ = canvas.draw_line((r, t), (r, t + b));
    // Bottom-left
    let _ = canvas.draw_line((l, bot), (l + b, bot));
    let _ = canvas.draw_line((l, bot), (l, bot - b));
    // Bottom-right
    let _ = canvas.draw_line((r, bot), (r - b, bot));
    let _ = canvas.draw_line((r, bot), (r, bot - b));
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

/// Draw the decoration sprite picker: the whole tilemap sheet at full scale with
/// a grid and the selected cell highlighted. Shown only in decoration mode.
fn draw_deco_picker(canvas: &mut WindowCanvas, tilemap_texture: &Texture, selected: u32) {
    // Backdrop framing the sheet so it reads as a panel over the level.
    let frame = Rect::new(
        PICKER_X - 4,
        PICKER_Y - 4,
        PICKER_W as u32 + 8,
        PICKER_H as u32 + 8,
    );
    canvas.set_draw_color(Color::RGB(20, 20, 28));
    let _ = canvas.fill_rect(frame);
    canvas.set_draw_color(Color::RGB(90, 90, 110));
    let _ = canvas.draw_rect(frame);

    // The sheet itself, drawn 1:1 (it is exactly PICKER_W x PICKER_H).
    let dst = Rect::new(PICKER_X, PICKER_Y, PICKER_W as u32, PICKER_H as u32);
    let _ = canvas.copy(tilemap_texture, None, Some(dst));

    // Cell grid.
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 40));
    for col in 0..=tiles::SHEET_COLUMNS as i32 {
        let x = PICKER_X + col * PICKER_CELL;
        let _ = canvas.draw_line((x, PICKER_Y), (x, PICKER_Y + PICKER_H));
    }
    for row in 0..=PICKER_ROWS {
        let y = PICKER_Y + row * PICKER_CELL;
        let _ = canvas.draw_line((PICKER_X, y), (PICKER_X + PICKER_W, y));
    }

    // Highlight the selected cell.
    let idx = selected.saturating_sub(1) as i32;
    let col = idx % tiles::SHEET_COLUMNS as i32;
    let row = idx / tiles::SHEET_COLUMNS as i32;
    if row < PICKER_ROWS {
        canvas.set_draw_color(Color::RGB(255, 235, 90));
        let cell = Rect::new(
            PICKER_X + col * PICKER_CELL,
            PICKER_Y + row * PICKER_CELL,
            PICKER_CELL as u32,
            PICKER_CELL as u32,
        );
        let _ = canvas.draw_rect(cell);
        let _ = canvas.draw_rect(Rect::new(
            cell.x() - 1,
            cell.y() - 1,
            PICKER_CELL as u32 + 2,
            PICKER_CELL as u32 + 2,
        ));
    }
}

/// Draw the path-mode action menu in the HUD bar. For now it is a single button
/// that toggles the active block between an open path and a closed loop. The
/// glyph reflects the active block's current state - a full rectangle with corner
/// nodes for a loop, an open "C" for an open path - and lights up while it is a
/// loop. Dimmed when there is no active block to act on.
fn draw_path_menu(canvas: &mut WindowCanvas, level: &LevelData, active_block: Option<usize>) {
    let closed = active_block
        .and_then(|b| level.path_blocks.get(b))
        .map(|blk| blk.closed);
    let lit = closed == Some(true);
    let has_active = closed.is_some();

    let b = PATH_LOOP_BTN;
    let r = to_rect(b);
    canvas.set_draw_color(if lit {
        Color::RGB(80, 75, 40)
    } else {
        Color::RGB(45, 45, 58)
    });
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if lit {
        Color::RGB(255, 235, 90)
    } else if has_active {
        Color::RGB(100, 200, 235)
    } else {
        Color::RGB(80, 80, 100)
    });
    let _ = canvas.draw_rect(r);

    // Loop glyph: top, left and bottom edges of a small rectangle are always
    // drawn; the right edge is only closed when the active block is a loop.
    let glyph = if lit {
        Color::RGB(255, 235, 90)
    } else if has_active {
        Color::RGB(160, 200, 230)
    } else {
        Color::RGB(110, 110, 130)
    };
    canvas.set_draw_color(glyph);
    let cx = b.0 + b.2 as i32 / 2;
    let cy = b.1 + b.3 as i32 / 2;
    let (gw, gh) = (16, 12);
    let (l, t, ri, bo) = (cx - gw / 2, cy - gh / 2, cx + gw / 2, cy + gh / 2);
    let _ = canvas.draw_line((l, t), (ri, t));
    let _ = canvas.draw_line((l, t), (l, bo));
    let _ = canvas.draw_line((l, bo), (ri, bo));
    if lit {
        // Closed loop: complete the rectangle and mark the corners like the
        // path overlay's control points.
        let _ = canvas.draw_line((ri, t), (ri, bo));
        for (px, py) in [(l, t), (ri, t), (ri, bo), (l, bo)] {
            fill_circle(canvas, px, py, 2);
        }
    } else {
        // Open path: leave the right side open, capping the two free ends.
        fill_circle(canvas, ri, t, 2);
        fill_circle(canvas, ri, bo, 2);
    }
}

fn draw_hud(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    palette: &[Tool],
    selected: usize,
    mode: Mode,
) {
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, HUD_TOP, VIEW_WIDTH, HUD_HEIGHT as u32));
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, HUD_TOP), (VIEW_WIDTH as i32, HUD_TOP));

    // The tool palette is only relevant when painting normal tiles; the other
    // modes have their own selectors (the path overlay / the sprite picker), so
    // the bar is left empty for them.
    if mode != Mode::Normal {
        return;
    }

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

/// Draw the top toolbar: Prev/Next level buttons, mode-switch buttons, and
/// resize buttons.
fn draw_top_bar(canvas: &mut WindowCanvas, docs: &[Document], current: usize, mode: Mode) {
    // Background
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, 0, VIEW_WIDTH, TOP_BAR_HEIGHT as u32));
    // Bottom border
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, TOP_BAR_HEIGHT - 1), (VIEW_WIDTH as i32, TOP_BAR_HEIGHT - 1));

    // --- Prev / Next buttons ---
    draw_nav_button(canvas, PREV_BTN, ArrowDir::Left);
    draw_nav_button(canvas, NEXT_BTN, ArrowDir::Right);

    // --- Mode-switch buttons (exactly one active) ---
    draw_normal_button(canvas, NORMAL_BTN, mode == Mode::Normal);
    draw_block_button(canvas, BLOCK_BTN, mode == Mode::Path);
    draw_deco_button(canvas, DECO_BTN, mode == Mode::Deco);

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

/// Draw the normal-mode button: a small brick/tile glyph, lit up yellow while
/// normal (tile-painting) mode is active.
fn draw_normal_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), active: bool) {
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

    // A little brick wall: an outer block with a couple of mortar lines.
    let glyph = if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(160, 200, 230)
    };
    canvas.set_draw_color(glyph);
    let cx = b.0 + b.2 as i32 / 2;
    let cy = b.1 + b.3 as i32 / 2;
    let block = Rect::new(cx - 11, cy - 7, 22, 14);
    let _ = canvas.draw_rect(block);
    // Horizontal mortar line splitting the two courses.
    let _ = canvas.draw_line((cx - 11, cy), (cx + 11, cy));
    // Staggered vertical mortar lines (offset between courses, like real bricks).
    let _ = canvas.draw_line((cx, cy - 7), (cx, cy));
    let _ = canvas.draw_line((cx - 6, cy), (cx - 6, cy + 7));
    let _ = canvas.draw_line((cx + 6, cy), (cx + 6, cy + 7));
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

/// Draw the decoration-mode toggle button: a small "picture" glyph (a framed
/// tile with a little mountain/sun), lit green while decoration mode is active.
fn draw_deco_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), active: bool) {
    let r = to_rect(b);
    if active {
        canvas.set_draw_color(Color::RGB(40, 70, 55));
    } else {
        canvas.set_draw_color(Color::RGB(50, 50, 65));
    }
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(120, 220, 150)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);

    // A little framed picture: a border, a sun, and a mountain.
    let glyph = if active {
        Color::RGB(150, 230, 175)
    } else {
        Color::RGB(160, 200, 230)
    };
    canvas.set_draw_color(glyph);
    let cx = b.0 + b.2 as i32 / 2;
    let cy = b.1 + b.3 as i32 / 2;
    let frame = Rect::new(cx - 11, cy - 8, 22, 16);
    let _ = canvas.draw_rect(frame);
    // Sun in the upper-left.
    fill_circle(canvas, cx - 5, cy - 3, 2);
    // Mountain along the bottom.
    let base = cy + 7;
    let _ = canvas.draw_line((cx - 9, base), (cx - 1, base - 9));
    let _ = canvas.draw_line((cx - 1, base - 9), (cx + 9, base));
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
    println!("  Modes (toolbar buttons): Normal tiles | B path blocks | X decorations");
    println!("  Normal mode : click palette bar to pick a tool, left-click paints, right-click erases");
    println!("  Path mode   : left-click adds points / drags points & edges, right-click deletes");
    println!("                N new block, L open/close loop, Tab cycle, Del remove block");
    println!("                (bottom bar shows a Toggle-loop button)");
    println!("  Deco mode   : click picker to choose a sprite, left-click places, right-click erases");
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
