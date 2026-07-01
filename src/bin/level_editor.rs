//! Level editor: browse and edit the level files in `levels/`.
//!
//! It renders each level the way the game does (tiles, moving platforms in
//! their starting positions, and the player at the spawn point) and lets you
//! paint tiles onto the grid. Edits are kept in memory per level and are only
//! written to disk when you save, so switching levels never loses work.
//!
//! The editor has four mutually-exclusive modes, switched with the grouped
//! toolbar buttons (or the keys noted below): Normal (paint tiles), Path (edit
//! path blocks), Deco (place decorations) and Exit (route exit doors). The
//! bottom tool palette holds the world-building tiles in Normal mode and the
//! exit doors / coins in Exit mode.
//!
//! A level browser overlay (the LVLS toolbar button, or Tab) lists every level
//! by name; click a row to jump to it. The same list is reused in Exit mode to
//! pick a door's destination.
//!
//! Controls:
//! - Click a mode button in the toolbar (or press F1/F2/F3) to switch modes
//! - Left mouse   : in Normal / Exit mode, apply the selected palette tool to the tile under the cursor
//! - Right mouse  : in Normal / Exit mode, erase the tile under the cursor
//! - (click+drag paints continuously)
//! - Click the bottom palette bar to choose a tool (Normal / Exit mode)
//! - Click the "Lv n/m" button (top-left) to open the level browser and switch levels
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
//! - F2 : enter path mode, or click the path button

//! - Left-click an empty cell : append a control point to the active block, snapped to stay horizontal/vertical from the previous one
//! - Left-click a control point : select its block, and drag it if it is an open path's endpoint
//! - Left-click an edge (between two points) : drag the whole edge perpendicular to itself
//! - Right-click a control point : delete it
//! - N : start a new block (the next click places its first point), or click the
//!   New-block button in the path menu that appears in the bottom bar
//! - L : toggle the active block between an open path and a closed loop (or click
//!   the Toggle-loop button in the path menu that appears in the bottom bar)
//! - Tab : cycle which block is active
//! - Delete / Backspace : remove the active block
//!
//! The active block is drawn in yellow, others in cyan; the green dot marks each
//! block's start (its resting position) and arrows show the travel direction.
//!
//! Decoration editing (render-only sprites that do not affect gameplay):
//! - F3 : enter decoration mode (repeated presses toggles between background/foreground editing), or click the picture button
//! - The tilemap sheet appears as a picker overlay; click a cell to choose a sprite
//! - Left-click (or drag) a cell : place the chosen decoration, snapped to the grid
//! - Right-click (or drag) a cell : erase the decoration there
//! - Decorations live on two layers: background (behind the player, coins and
//!   platforms) and foreground (in front of them, so they can hide them). These
//!   are two separate deco modes, both on the decoration toolbar button: click
//!   it once for the background layer, again for the foreground layer, a third
//!   time to return to Normal. The button shows a two-square "layers" badge with
//!   the active layer lit. The overlay brackets are magenta for background and
//!   orange for foreground, brightest for the layer being edited.

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

use rustgamex::font;
use rustgamex::level::{self, DecoLayer, Decoration, Edge, LevelData, PathBlock};
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
// mode. It holds a "new block" button and a "toggle loop" button.
const PATH_MENU_BTN_H: i32 = 36;
const PATH_NEW_BTN: (i32, i32, u32, u32) = (
    HUD_MARGIN_X,
    HUD_TOP + (HUD_HEIGHT - PATH_MENU_BTN_H) / 2,
    52,
    PATH_MENU_BTN_H as u32,
);
const PATH_LOOP_BTN: (i32, i32, u32, u32) = (
    PATH_NEW_BTN.0 + PATH_NEW_BTN.2 as i32 + 8,
    PATH_NEW_BTN.1,
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

// "Levels" button (top-left): shows the current level and opens the level
// browser overlay, which is the sole way to switch levels with the mouse.
const LEVELS_BTN: (i32, i32, u32, u32) = (8, BTN_Y, 76, BTN_H);
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
// Fourth mode button: edit exit doors.
const EXIT_BTN: (i32, i32, u32, u32) = (MODE_BTN_X0 + 3 * MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
// Play button
const PLAY_BTN: (i32, i32, u32, u32) = (748, BTN_Y, 44, BTN_H);

// Exit-mode HUD: a small tool palette (exit doors + coins) on the left, then a
// button that routes the selected door's destination. The palette occupies the
// first `EXIT_PALETTE_SLOTS` slots from the left margin, so the Set-dest button
// clears them.
const EXIT_MENU_BTN_H: i32 = 36;
const EXIT_PALETTE_SLOTS: i32 = 5;
const EXIT_DEST_BTN: (i32, i32, u32, u32) = (
    HUD_MARGIN_X + EXIT_PALETTE_SLOTS * (SLOT + SLOT_PAD) + 12,
    HUD_TOP + (HUD_HEIGHT - EXIT_MENU_BTN_H) / 2,
    130,
    EXIT_MENU_BTN_H as u32,
);

// Level-browser overlay layout (a centred panel listing every level).
const OVERLAY_W: i32 = 440;
const OVERLAY_ROW_H: i32 = 24;
const OVERLAY_PAD: i32 = 10;
const OVERLAY_TITLE_H: i32 = 26;

fn to_rect(b: (i32, i32, u32, u32)) -> Rect {
    Rect::new(b.0, b.1, b.2, b.3)
}

/// Bounding rect of the level-browser overlay panel for `count` levels.
fn overlay_rect(count: usize) -> Rect {
    let h = OVERLAY_TITLE_H + count as i32 * OVERLAY_ROW_H + OVERLAY_PAD * 2;
    let x = (VIEW_WIDTH as i32 - OVERLAY_W) / 2;
    let avail = HUD_TOP - TILE_AREA_TOP;
    let y = (TILE_AREA_TOP + (avail - h) / 2).max(TILE_AREA_TOP + 8);
    Rect::new(x, y, OVERLAY_W as u32, h as u32)
}

/// Index of the overlay list row under a screen position, if any.
fn overlay_row_at(count: usize, x: i32, y: i32) -> Option<usize> {
    let r = overlay_rect(count);
    if x < r.x() + OVERLAY_PAD || x >= r.x() + r.width() as i32 - OVERLAY_PAD {
        return None;
    }
    let first = r.y() + OVERLAY_TITLE_H + OVERLAY_PAD;
    for i in 0..count {
        let ry = first + i as i32 * OVERLAY_ROW_H;
        if y >= ry && y < ry + OVERLAY_ROW_H {
            return Some(i);
        }
    }
    None
}

/// Whether a screen position falls inside the overlay panel at all.
fn in_overlay(count: usize, x: i32, y: i32) -> bool {
    let r = overlay_rect(count);
    x >= r.x() && x < r.x() + r.width() as i32 && y >= r.y() && y < r.y() + r.height() as i32
}

/// Whether the cell at `tile` holds an exit door, normal or secret.
fn is_door(level: &LevelData, tile: (usize, usize)) -> bool {
    matches!(
        level.tiles[tile.1][tile.0],
        tiles::EXIT | tiles::SECRET_EXIT
    )
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
    /// Select exit doors and set where each one leads.
    Exit,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Normal => "Normal (tiles)",
            Mode::Path => "Path blocks",
            Mode::Deco => "Decorations",
            Mode::Exit => "Exit doors",
        }
    }
}

/// The full-list level overlay, and what a click on one of its rows does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    /// Browsing levels: clicking a row switches to that level.
    Jump,
    /// Choosing where the selected exit door leads: clicking a row sets its
    /// destination and closes the overlay.
    PickDest,
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

/// Camera-panning keys currently held. Grouped so they can all be released in
/// one place (e.g. after launching the game, which can swallow the key-up
/// events that would otherwise clear them).
#[derive(Default)]
struct Pan {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl Pan {
    fn clear(&mut self) {
        *self = Pan::default();
    }
}

/// A discrete editor action that can be triggered from more than one input: a
/// toolbar/menu button click and/or a keyboard shortcut. Both input paths map
/// their event to a `UICommand` and run it through [`Editor::execute`], so each
/// action's behaviour lives in exactly one place instead of being duplicated
/// between the key and mouse handlers.
#[derive(Clone, Copy)]
enum UICommand {
    /// Move to the previous / next level (wrapping).
    PrevLevel,
    NextLevel,
    /// Switch to a specific mode (the F1/F2 keys and the Normal button).
    SetMode(Mode),
    /// Path button: toggle between Path mode and Normal.
    ToggleBlockMode,
    /// Deco button: cycle Normal -> background -> foreground -> Normal.
    CycleDecoMode,
    /// F3: enter deco mode, flipping the active layer on each press.
    FlipDecoLayer,
    /// Grow (or, when `shrink`, shrink) the canvas at one edge.
    Resize { edge: Edge, shrink: bool },
    /// Arm a fresh path block (the next level click places its first point).
    NewBlock,
    /// Toggle the active block between an open path and a closed loop.
    ToggleLoop,
    /// Remove the active path block.
    DeleteBlock,
    /// Cycle which path block is active.
    CycleBlock,
    /// Toggle the grid overlay.
    ToggleGrid,
    /// Open the full-list level browser overlay (click a row to jump).
    OpenLevels,
    /// Close any open overlay.
    CloseOverlay,
    /// Scroll the camera back to the level's origin.
    Home,
    /// Save the current level to its file.
    Save,
    /// Launch the game on the current level.
    Play,
    /// Quit the editor.
    Quit,
}

/// All loop-persistent editor state. Bundling it here lets [`Editor::execute`]
/// run a [`UICommand`] no matter which input triggered it; the SDL canvas and
/// textures stay outside, owned by `main`.
struct Editor {
    docs: Vec<Document>,
    palette: Vec<Tool>,
    /// Exit-mode tool palette: the exit doors (normal / secret) and the coins
    /// that gate them, painted from the bottom bar while in [`Mode::Exit`].
    exit_palette: Vec<Tool>,
    current: usize,
    camera_x: f32,
    camera_y: f32,
    selected: usize,
    /// Selected slot into [`Editor::exit_palette`] (the Exit-mode tool).
    exit_tool: usize,
    show_grid: bool,
    pan: Pan,
    mouse: (i32, i32),
    /// The active editing mode (normal tiles / path blocks / decorations).
    mode: Mode,
    /// Path-block editing state (used while in `Mode::Path`).
    active_block: Option<usize>,
    /// The point or edge currently being dragged within the active block.
    dragging: Option<Drag>,
    /// When set, the next left-click starts a fresh block instead of appending.
    start_new: bool,
    /// Which sprite-sheet index the next decoration placement uses, and which
    /// layer it lands on (while in `Mode::Deco`).
    deco_sprite: u32,
    deco_layer: DecoLayer,
    /// The exit door currently selected for editing (grid coords), in `Mode::Exit`.
    selected_door: Option<(usize, usize)>,
    /// The open full-list level overlay, if any (level browser / destination picker).
    overlay: Option<Overlay>,
    /// Rendered view of the current level, rebuilt from it whenever `dirty`.
    tilemap: TileMap,
    player: Player,
    dirty: bool,
    /// A pending level switch, applied once per frame after event handling so
    /// the index never changes mid-iteration.
    switch_to: Option<usize>,
    /// Set by [`UICommand::Quit`]; breaks the main loop after the frame.
    quit: bool,
    /// Set whenever the window title needs refreshing (level/tool/modified
    /// state changed); `main` re-applies it once per frame.
    retitle: bool,
}

impl Editor {
    fn new(docs: Vec<Document>, palette: Vec<Tool>, exit_palette: Vec<Tool>) -> Self {
        let tilemap = TileMap::from_level(&docs[0].level);
        let player = spawn_player(&docs[0].level);
        Editor {
            docs,
            palette,
            exit_palette,
            current: 0,
            camera_x: 0.0,
            camera_y: 0.0,
            selected: 2,
            // Default to the normal exit door (slot 1, after Erase).
            exit_tool: 1,
            show_grid: true,
            pan: Pan::default(),
            mouse: (0, 0),
            mode: Mode::Normal,
            active_block: None,
            dragging: None,
            start_new: false,
            deco_sprite: 1,
            deco_layer: DecoLayer::Background,
            selected_door: None,
            overlay: None,
            tilemap,
            player,
            dirty: false,
            switch_to: None,
            quit: false,
            retitle: false,
        }
    }

    /// Run a single [`UICommand`]. This is the one place each action's effect
    /// is defined; the keyboard and mouse handlers only translate input into a
    /// command and call here.
    fn execute(&mut self, cmd: UICommand) {
        match cmd {
            UICommand::PrevLevel => self.switch_level(-1),
            UICommand::NextLevel => self.switch_level(1),
            UICommand::SetMode(mode) => self.set_mode(mode),
            UICommand::ToggleBlockMode => {
                // A second click on the path button returns to Normal.
                let next = if self.mode == Mode::Path {
                    Mode::Normal
                } else {
                    Mode::Path
                };
                self.set_mode(next);
            }
            UICommand::CycleDecoMode => {
                // Background and foreground are two separate deco modes on one
                // button: 1st click = background, 2nd = foreground, 3rd = Normal.
                if self.mode != Mode::Deco {
                    self.deco_layer = DecoLayer::Background;
                    self.set_mode(Mode::Deco);
                } else if self.deco_layer == DecoLayer::Background {
                    self.deco_layer = DecoLayer::Foreground;
                    self.set_mode(Mode::Deco);
                } else {
                    self.set_mode(Mode::Normal);
                }
                if self.mode == Mode::Deco {
                    println!("Deco layer: {}", layer_name(self.deco_layer));
                }
            }
            UICommand::FlipDecoLayer => {
                self.deco_layer = if self.deco_layer == DecoLayer::Foreground {
                    DecoLayer::Background
                } else {
                    DecoLayer::Foreground
                };
                self.set_mode(Mode::Deco);
            }
            UICommand::Resize { edge, shrink } => {
                let changed = if shrink {
                    self.docs[self.current].level.shrink(edge)
                } else {
                    self.docs[self.current].level.grow(edge);
                    true
                };
                if changed {
                    self.mark_changed();
                }
            }
            UICommand::NewBlock => {
                // Arm a fresh block; the next level click places its first point.
                self.start_new = true;
                self.active_block = None;
                self.dragging = None;
            }
            UICommand::ToggleLoop => {
                if toggle_loop(&mut self.docs[self.current].level, self.active_block) {
                    self.mark_changed();
                }
            }
            UICommand::DeleteBlock => {
                if let Some(b) = self.active_block {
                    self.docs[self.current].level.path_blocks.remove(b);
                    self.active_block = None;
                    self.dragging = None;
                    self.mark_changed();
                }
            }
            UICommand::CycleBlock => {
                let n = self.docs[self.current].level.path_blocks.len();
                self.active_block = (n > 0).then(|| self.active_block.map_or(0, |b| (b + 1) % n));
                self.dragging = None;
            }
            UICommand::ToggleGrid => self.show_grid = !self.show_grid,
            UICommand::OpenLevels => {
                self.overlay = Some(Overlay::Jump);
            }
            UICommand::CloseOverlay => self.overlay = None,
            UICommand::Home => {
                self.camera_x = 0.0;
                self.camera_y = 0.0;
            }
            UICommand::Save => {
                save(&mut self.docs[self.current]);
                self.retitle = true;
            }
            UICommand::Play => {
                launch_game(&self.docs[self.current]);
                // Reset pan keys so any held during the game don't carry over.
                self.pan.clear();
            }
            UICommand::Quit => self.quit = true,
        }
    }

    /// Switch the active mode, clearing the per-mode transient state both the
    /// key and button paths reset.
    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.dragging = None;
        self.start_new = false;
        // The door selection only makes sense in Exit mode; drop it otherwise.
        if mode != Mode::Exit {
            self.selected_door = None;
        }
        println!("Mode: {}", mode.name());
    }

    /// The level id a newly painted exit door is routed to by default: the next
    /// level in the list (matching the old linear progression), or this level
    /// when it is the only one.
    fn default_exit_dest(&self) -> String {
        let n = self.docs.len();
        let next = (self.current + 1) % n;
        self.docs[next].level.id.clone()
    }

    /// Handle a click while the level overlay is open: a row either jumps to that
    /// level or assigns it as the selected door's destination; a click off the
    /// rows dismisses the overlay.
    fn overlay_click(&mut self, btn: MouseButton, x: i32, y: i32) {
        if btn != MouseButton::Left {
            return;
        }
        let Some(overlay) = self.overlay else {
            return;
        };
        let count = self.docs.len();
        match overlay_row_at(count, x, y) {
            Some(i) => {
                match overlay {
                    Overlay::Jump => self.switch_to = Some(i),
                    Overlay::PickDest => {
                        let dest = self.docs[i].level.id.clone();
                        if let Some(tile) = self.selected_door
                            && let Some(door) = self.docs[self.current]
                                .level
                                .exits
                                .iter_mut()
                                .find(|e| e.tile == tile)
                        {
                            door.dest = dest;
                            self.mark_changed();
                        }
                    }
                }
                self.overlay = None;
            }
            // A click anywhere outside the list rows closes the overlay.
            None if !in_overlay(count, x, y) => self.overlay = None,
            None => {}
        }
    }


    /// Queue a move by `delta` levels (negative = previous), wrapping around.
    fn switch_level(&mut self, delta: isize) {
        let n = self.docs.len();
        let next = (self.current as isize + delta).rem_euclid(n as isize) as usize;
        self.switch_to = Some(next);
    }

    /// Mark the current level edited: flag it for redraw and a title refresh.
    fn mark_changed(&mut self) {
        self.docs[self.current].modified = true;
        self.dirty = true;
        self.retitle = true;
    }

    /// Apply a pending level switch, resetting the per-level view state.
    fn apply_switch(&mut self) {
        if let Some(index) = self.switch_to.take() {
            self.current = index;
            self.camera_x = 0.0;
            self.camera_y = 0.0;
            self.pan.clear();
            self.active_block = None;
            self.dragging = None;
            self.start_new = false;
            self.selected_door = None;
            self.dirty = true;
            self.retitle = true;
        }
    }

    /// Rebuild the rendered view from the level data after an edit.
    fn rebuild_if_dirty(&mut self) {
        if self.dirty {
            self.tilemap = TileMap::from_level(&self.docs[self.current].level);
            self.player = spawn_player(&self.docs[self.current].level);
            self.dirty = false;
        }
    }

    /// Advance the camera by the held pan keys and clamp it to the level.
    fn update_camera(&mut self, delta_time: f32) {
        let pan = PAN_SPEED * delta_time;
        if self.pan.left {
            self.camera_x -= pan;
        }
        if self.pan.right {
            self.camera_x += pan;
        }
        if self.pan.up {
            self.camera_y -= pan;
        }
        if self.pan.down {
            self.camera_y += pan;
        }

        let tile_area_h = (HUD_TOP - TILE_AREA_TOP) as f32;
        let level_width = self.tilemap.width as f32 * self.tilemap.tile_size as f32;
        let level_height = self.tilemap.height as f32 * self.tilemap.tile_size as f32;
        let max_camera_x = (level_width - VIEW_WIDTH as f32).max(0.0);
        let max_camera_y = (level_height - tile_area_h).max(0.0);
        self.camera_x = self.camera_x.clamp(0.0, max_camera_x);
        self.camera_y = self.camera_y.clamp(0.0, max_camera_y);
    }
}

/// Map a key press (with its modifiers) to the [`UICommand`] it triggers, if
/// any. Returns `None` for keys handled directly in the loop (panning) or that
/// are unbound, so the caller can fall through to those.
fn key_command(key: Keycode, ctrl: bool, shift: bool, mode: Mode) -> Option<UICommand> {
    // Ctrl+Arrow resizes the canvas (Ctrl+Shift+Arrow shrinks instead of grows).
    let resize_edge = match key {
        Keycode::Up if ctrl => Some(Edge::Top),
        Keycode::Down if ctrl => Some(Edge::Bottom),
        Keycode::Left if ctrl => Some(Edge::Left),
        Keycode::Right if ctrl => Some(Edge::Right),
        _ => None,
    };
    if let Some(edge) = resize_edge {
        return Some(UICommand::Resize { edge, shrink: shift });
    }

    Some(match key {
        Keycode::S if ctrl => UICommand::Save,
        Keycode::Escape | Keycode::Q => UICommand::Quit,
        Keycode::Comma | Keycode::PageUp => UICommand::PrevLevel,
        Keycode::Period | Keycode::PageDown => UICommand::NextLevel,
        Keycode::Home => UICommand::Home,
        Keycode::G => UICommand::ToggleGrid,
        Keycode::F1 => UICommand::SetMode(Mode::Normal),
        Keycode::F2 => UICommand::SetMode(Mode::Path),
        Keycode::F3 => UICommand::FlipDecoLayer,
        Keycode::F4 => UICommand::SetMode(Mode::Exit),
        Keycode::N if mode == Mode::Path => UICommand::NewBlock,
        Keycode::Tab if mode == Mode::Path => UICommand::CycleBlock,
        Keycode::L if mode == Mode::Path => UICommand::ToggleLoop,
        Keycode::Delete | Keycode::Backspace if mode == Mode::Path => UICommand::DeleteBlock,
        // Tab opens the level browser everywhere except path mode (where it
        // cycles blocks).
        Keycode::Tab => UICommand::OpenLevels,
        _ => return None,
    })
}

/// Map a mouse-button press in the top toolbar to the [`UICommand`] it
/// triggers, if it hit a button. Left-click on a resize button grows, right
/// shrinks; the other toolbar buttons only respond to the left button.
fn topbar_command(btn: MouseButton, x: i32, y: i32) -> Option<UICommand> {
    if btn == MouseButton::Left {
        if btn_hit(LEVELS_BTN, x, y) {
            return Some(UICommand::OpenLevels);
        } else if btn_hit(NORMAL_BTN, x, y) {
            return Some(UICommand::SetMode(Mode::Normal));
        } else if btn_hit(BLOCK_BTN, x, y) {
            return Some(UICommand::ToggleBlockMode);
        } else if btn_hit(DECO_BTN, x, y) {
            return Some(UICommand::CycleDecoMode);
        } else if btn_hit(EXIT_BTN, x, y) {
            return Some(UICommand::SetMode(Mode::Exit));
        } else if btn_hit(PLAY_BTN, x, y) {
            return Some(UICommand::Play);
        }
    }
    // Resize buttons respond to either button: left grows, right shrinks.
    let edge = if btn_hit(RESIZE_TOP_BTN, x, y) {
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
    edge.map(|edge| UICommand::Resize {
        edge,
        shrink: btn == MouseButton::Right,
    })
}

/// Map a left-click in the path-mode HUD menu to the [`UICommand`] it triggers.
fn path_menu_command(btn: MouseButton, x: i32, y: i32) -> Option<UICommand> {
    if btn != MouseButton::Left {
        return None;
    }
    if btn_hit(PATH_NEW_BTN, x, y) {
        Some(UICommand::NewBlock)
    } else if btn_hit(PATH_LOOP_BTN, x, y) {
        Some(UICommand::ToggleLoop)
    } else {
        None
    }
}

fn main() -> Result<(), String> {
    let docs: Vec<Document> = level::load_dir_entries("levels")?
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

    // Normal-mode palette: the world-building tiles. Exit doors and coins live in
    // the Exit-mode menu (`exit_palette`) instead, since they are edited there
    // alongside the door routing.
    let palette: Vec<Tool> = std::iter::once(Tool::Erase)
        .chain(std::iter::once(Tool::Spawn))
        // Tile id 2 is unused, so skip it when listing the paintable tiles.
        .chain(std::iter::once(Tool::Tile(tiles::SOLID)))
        .chain((tiles::DEATH..=tiles::MOVE_LEFT).map(Tool::Tile))
        .collect();

    // Exit-mode palette: the exit doors (normal / secret) and the coins that gate
    // them. Painting a door here auto-syncs its `exit:` routing.
    let exit_palette: Vec<Tool> = vec![
        Tool::Erase,
        Tool::Tile(tiles::EXIT),
        Tool::Tile(tiles::SECRET_EXIT),
        Tool::Tile(tiles::COIN),
        Tool::Tile(tiles::RED_COIN),
    ];

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

    let mut editor = Editor::new(docs, palette, exit_palette);
    set_title(
        &mut canvas,
        &editor.docs,
        editor.current,
        editor.palette[editor.selected],
    );

    'running: loop {
        let delta_time = time_provider.delta_time();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => editor.quit = true,

                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } => {
                    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                    if key == Keycode::Escape && editor.overlay.is_some() {
                        // Esc dismisses an open overlay before it means "quit".
                        editor.execute(UICommand::CloseOverlay);
                    } else if let Some(cmd) = key_command(key, ctrl, shift, editor.mode) {
                        editor.execute(cmd);
                    } else {
                        // Keys that aren't commands drive camera panning.
                        match key {
                            Keycode::Left | Keycode::A => editor.pan.left = true,
                            Keycode::Right | Keycode::D => editor.pan.right = true,
                            Keycode::Up | Keycode::W => editor.pan.up = true,
                            Keycode::Down | Keycode::S => editor.pan.down = true,
                            _ => {}
                        }
                    }
                }

                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Left | Keycode::A => editor.pan.left = false,
                    Keycode::Right | Keycode::D => editor.pan.right = false,
                    Keycode::Up | Keycode::W => editor.pan.up = false,
                    Keycode::Down | Keycode::S => editor.pan.down = false,
                    _ => {}
                },

                Event::MouseMotion {
                    x, y, mousestate, ..
                } => {
                    editor.mouse = (x, y);
                    if editor.overlay.is_some() {
                        // An open overlay captures the view; no painting beneath it.
                    } else if editor.mode == Mode::Path {
                        if mousestate.left()
                            && drag_path(
                                &mut editor.docs[editor.current].level,
                                editor.active_block,
                                editor.dragging,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Deco {
                        // Drag to paint or erase a run of decorations, but never
                        // while the cursor is over the picker overlay.
                        let changed = if in_deco_picker(x, y) {
                            false
                        } else if mousestate.left() {
                            place_deco(
                                &mut editor.docs[editor.current].level,
                                editor.deco_sprite,
                                editor.deco_layer,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        } else if mousestate.right() {
                            erase_deco(
                                &mut editor.docs[editor.current].level,
                                editor.deco_layer,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        } else {
                            false
                        };
                        if changed {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Normal
                        && editor.overlay.is_none()
                        && (TILE_AREA_TOP..HUD_TOP).contains(&y)
                    {
                        let dest = editor.default_exit_dest();
                        if mousestate.left() {
                            if apply_tool(
                                &mut editor.docs[editor.current].level,
                                editor.palette[editor.selected],
                                &dest,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            ) {
                                editor.mark_changed();
                            }
                        } else if mousestate.right()
                            && apply_tool(
                                &mut editor.docs[editor.current].level,
                                Tool::Erase,
                                &dest,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Exit
                        && editor.overlay.is_none()
                        && (TILE_AREA_TOP..HUD_TOP).contains(&y)
                    {
                        // Drag to paint a run of coins/doors, mirroring Normal mode.
                        let dest = editor.default_exit_dest();
                        let tool = if mousestate.left() {
                            Some(editor.exit_palette[editor.exit_tool])
                        } else if mousestate.right() {
                            Some(Tool::Erase)
                        } else {
                            None
                        };
                        if let Some(tool) = tool
                            && apply_tool(
                                &mut editor.docs[editor.current].level,
                                tool,
                                &dest,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.mark_changed();
                        }
                    }
                }

                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    editor.dragging = None;
                }

                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => {
                    if editor.overlay.is_some() {
                        // While the level overlay is up it captures every click.
                        editor.overlay_click(mouse_btn, x, y);
                    } else if y < TOP_BAR_HEIGHT {
                        if let Some(cmd) = topbar_command(mouse_btn, x, y) {
                            editor.execute(cmd);
                        }
                    } else if y >= HUD_TOP {
                        // The HUD bar holds the tool palette in Normal mode and a
                        // small action menu in Path and Exit modes.
                        match editor.mode {
                            Mode::Normal => {
                                if mouse_btn == MouseButton::Left
                                    && let Some(slot) = palette_slot_at(x, editor.palette.len())
                                {
                                    editor.selected = slot;
                                    editor.retitle = true;
                                }
                            }
                            Mode::Path => {
                                if let Some(cmd) = path_menu_command(mouse_btn, x, y) {
                                    editor.execute(cmd);
                                }
                            }
                            Mode::Exit => {
                                // The exit bar holds a tool palette (doors + coins)
                                // on the left; the Set-dest button routes whichever
                                // door is currently selected.
                                if mouse_btn == MouseButton::Left {
                                    if let Some(slot) =
                                        palette_slot_at(x, editor.exit_palette.len())
                                    {
                                        editor.exit_tool = slot;
                                    } else if editor.selected_door.is_some()
                                        && btn_hit(EXIT_DEST_BTN, x, y)
                                    {
                                        editor.overlay = Some(Overlay::PickDest);
                                    }
                                }
                            }
                            // Deco mode's layer is chosen from the toolbar button,
                            // so its bottom bar stays empty.
                            Mode::Deco => {}
                        }
                    } else if editor.mode == Mode::Exit {
                        // Paint the selected exit tool (left) or erase (right); the
                        // reconcile in apply_tool keeps `exit:` routing in step.
                        // Whatever door ends up under the cursor is then selected so
                        // its destination can be set from the bottom bar.
                        let tool = match mouse_btn {
                            MouseButton::Left => editor.exit_palette[editor.exit_tool],
                            MouseButton::Right => Tool::Erase,
                            _ => continue,
                        };
                        let dest = editor.default_exit_dest();
                        if apply_tool(
                            &mut editor.docs[editor.current].level,
                            tool,
                            &dest,
                            x,
                            y,
                            editor.camera_x,
                            editor.camera_y,
                        ) {
                            editor.mark_changed();
                        }
                        let tile = cursor_tile(
                            &editor.docs[editor.current].level,
                            x,
                            y,
                            editor.camera_x,
                            editor.camera_y,
                        );
                        editor.selected_door =
                            tile.filter(|&t| is_door(&editor.docs[editor.current].level, t));
                    } else if editor.mode == Mode::Deco {
                        // Clicking the sprite picker selects a sprite; clicking
                        // the level places (left) or erases (right) a decoration.
                        if in_deco_picker(x, y) {
                            if mouse_btn == MouseButton::Left
                                && let Some(s) = picker_sprite_at(x, y)
                            {
                                editor.deco_sprite = s;
                            }
                        } else {
                            let changed = match mouse_btn {
                                MouseButton::Left => place_deco(
                                    &mut editor.docs[editor.current].level,
                                    editor.deco_sprite,
                                    editor.deco_layer,
                                    x,
                                    y,
                                    editor.camera_x,
                                    editor.camera_y,
                                ),
                                MouseButton::Right => erase_deco(
                                    &mut editor.docs[editor.current].level,
                                    editor.deco_layer,
                                    x,
                                    y,
                                    editor.camera_x,
                                    editor.camera_y,
                                ),
                                _ => false,
                            };
                            if changed {
                                editor.mark_changed();
                            }
                        }
                    } else if editor.mode == Mode::Path {
                        let changed = match mouse_btn {
                            MouseButton::Left => path_left_click(
                                &mut editor.docs[editor.current].level,
                                &mut editor.active_block,
                                &mut editor.dragging,
                                &mut editor.start_new,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            ),
                            MouseButton::Right => path_right_click(
                                &mut editor.docs[editor.current].level,
                                &mut editor.active_block,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            ),
                            _ => false,
                        };
                        if changed {
                            editor.mark_changed();
                        }
                    } else {
                        // Normal mode: paint the selected tool (left) or erase (right).
                        let tool = match mouse_btn {
                            MouseButton::Left => editor.palette[editor.selected],
                            MouseButton::Right => Tool::Erase,
                            _ => continue,
                        };
                        let dest = editor.default_exit_dest();
                        if apply_tool(
                            &mut editor.docs[editor.current].level,
                            tool,
                            &dest,
                            x,
                            y,
                            editor.camera_x,
                            editor.camera_y,
                        ) {
                            editor.mark_changed();
                        }
                    }
                }

                _ => {}
            }
        }

        if editor.quit {
            break 'running;
        }

        editor.apply_switch();
        editor.rebuild_if_dirty();
        editor.update_camera(delta_time);

        if editor.retitle {
            set_title(
                &mut canvas,
                &editor.docs,
                editor.current,
                editor.palette[editor.selected],
            );
            editor.retitle = false;
        }

        let camera_xi = editor.camera_x as i32;
        let camera_yi = editor.camera_y as i32;

        // Effective camera_y passed to render functions shifts tiles down by
        // TILE_AREA_TOP so they appear below the top toolbar.
        let render_cam_y = camera_yi - TILE_AREA_TOP;

        canvas.set_draw_color(Color::RGB(135, 206, 235));
        canvas.clear();
        editor
            .tilemap
            .render(&mut canvas, &tilemap_texture, camera_xi, render_cam_y);
        editor
            .player
            .render(&mut canvas, &character_texture, camera_xi, render_cam_y);
        // Foreground decorations draw over the player, just as in the game.
        editor
            .tilemap
            .render_foreground(&mut canvas, &tilemap_texture, camera_xi, render_cam_y);

        if editor.show_grid {
            draw_grid(&mut canvas, &editor.tilemap, camera_xi, render_cam_y);
        }
        draw_paths(
            &mut canvas,
            &editor.docs[editor.current].level,
            editor.active_block,
            editor.mode == Mode::Path,
            camera_xi,
            render_cam_y,
        );
        draw_decorations(
            &mut canvas,
            &editor.docs[editor.current].level,
            editor.mode == Mode::Deco,
            editor.deco_layer,
            camera_xi,
            render_cam_y,
        );
        draw_hover(&mut canvas, &editor.tilemap, editor.mouse, camera_xi, render_cam_y);
        if editor.mode == Mode::Exit {
            draw_exits(
                &mut canvas,
                &editor.docs[editor.current].level,
                editor.selected_door,
                camera_xi,
                render_cam_y,
            );
        }
        if editor.mode == Mode::Deco {
            draw_deco_picker(&mut canvas, &tilemap_texture, editor.deco_sprite);
        }
        draw_hud(
            &mut canvas,
            &tilemap_texture,
            &character_texture,
            &editor.palette,
            editor.selected,
            editor.mode,
        );
        if editor.mode == Mode::Path {
            draw_path_menu(
                &mut canvas,
                &editor.docs[editor.current].level,
                editor.active_block,
                editor.start_new,
            );
        }
        if editor.mode == Mode::Exit {
            draw_exit_menu(
                &mut canvas,
                &tilemap_texture,
                &character_texture,
                &editor.docs[editor.current].level,
                &editor.exit_palette,
                editor.exit_tool,
                editor.selected_door,
            );
        }
        draw_top_bar(
            &mut canvas,
            &editor.docs,
            editor.current,
            editor.mode,
            editor.deco_layer,
        );
        if let Some(overlay) = editor.overlay {
            draw_level_overlay(&mut canvas, &editor.docs, editor.current, overlay);
        }

        canvas.present();
        time_provider.wait_for_next_frame();
    }

    Ok(())
}

fn spawn_player(level: &LevelData) -> Player {
    Player::new(level.spawn.0, level.spawn.1)
}

/// Apply a tool to the tile under a screen position. Returns true if the level
/// changed. `default_dest` is the level id a freshly painted exit door is routed
/// to until the user reassigns it in Exit mode.
fn apply_tool(
    level: &mut LevelData,
    tool: Tool,
    default_dest: &str,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    if !(TILE_AREA_TOP..HUD_TOP).contains(&screen_y) {
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
    // Every door tile (normal or secret) must have exactly one `exit:` entry and
    // nothing else may, or the file won't reload. Reconcile the touched cell.
    reconcile_exit(level, (tx, ty), default_dest);
    true
}

/// Keep `level.exits` in step with the grid at one cell: add a routed door when
/// the cell becomes an exit door tile, drop its entry when it stops being one.
fn reconcile_exit(level: &mut LevelData, tile: (usize, usize), default_dest: &str) {
    let existing = level.exits.iter().position(|e| e.tile == tile);
    match (is_door(level, tile), existing) {
        (true, None) => level.exits.push(level::ExitDoor {
            tile,
            dest: default_dest.to_string(),
        }),
        (false, Some(i)) => {
            level.exits.remove(i);
        }
        _ => {}
    }
}

/// Whether a screen position is over the decoration sprite-picker overlay.
fn in_deco_picker(x: i32, y: i32) -> bool {
    (PICKER_X..PICKER_X + PICKER_W).contains(&x) && (PICKER_Y..PICKER_Y + PICKER_H).contains(&y)
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

/// Short human-readable name for a decoration layer (for console feedback).
fn layer_name(layer: DecoLayer) -> &'static str {
    match layer {
        DecoLayer::Background => "Background",
        DecoLayer::Foreground => "Foreground",
    }
}

/// Index of the decoration on `layer` snapped to the given tile cell, if any.
/// Decorations placed through the editor are grid-aligned, so a cell holds at
/// most one per layer (background and foreground are tracked independently).
fn deco_index_at(level: &LevelData, layer: DecoLayer, tile: (usize, usize)) -> Option<usize> {
    level.decorations.iter().position(|d| {
        d.layer == layer
            && (d.x / TILE_SIZE).round() as i64 == tile.0 as i64
            && (d.y / TILE_SIZE).round() as i64 == tile.1 as i64
    })
}

/// Place a decoration of `sprite` on `layer` at the grid cell under a screen
/// position, snapped to the grid. Replaces any decoration already on that layer
/// in that cell (leaving the other layer's untouched). Returns whether the level
/// changed.
fn place_deco(
    level: &mut LevelData,
    sprite: u32,
    layer: DecoLayer,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    if let Some(i) = deco_index_at(level, layer, tile) {
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
        layer,
    });
    true
}

/// Remove the decoration on `layer` in the grid cell under a screen position, if
/// any. Returns whether the level changed.
fn erase_deco(
    level: &mut LevelData,
    layer: DecoLayer,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    match deco_index_at(level, layer, tile) {
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
/// Test-play the level currently being edited: write it to a temp directory on
/// its own and launch the game there. Its exit doors may point at levels that
/// aren't in this one-level directory; the game tolerates an unresolvable door
/// target by simply restarting the level.
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
    if !(TILE_AREA_TOP..HUD_TOP).contains(&my) {
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
/// is distinguishable from the real gameplay tiles in the editor. Background
/// decorations are bracketed in magenta, foreground ones in orange. While in
/// decoration mode the brackets brighten, and the layer currently being edited
/// (`active_layer`) is brighter still than the other; outside decoration mode
/// both are dimmed (mirroring the path overlay).
fn draw_decorations(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    deco_mode: bool,
    active_layer: DecoLayer,
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
    for deco in &level.decorations {
        // The active layer reads brightest; the inactive layer is dimmer; both
        // fade further when not in deco mode.
        let alpha = if !deco_mode {
            170
        } else if deco.layer == active_layer {
            255
        } else {
            200
        };
        // Magenta (background) vs orange (foreground) both read distinctly from
        // the path overlay (yellow/cyan) and the hover highlight (yellow).
        let color = match deco.layer {
            DecoLayer::Background => Color::RGBA(255, 80, 245, alpha),
            DecoLayer::Foreground => Color::RGBA(255, 170, 40, alpha),
        };
        canvas.set_draw_color(color);
        let x = deco.x as i32 - camera_x;
        let y = deco.y as i32 - camera_y;
        draw_corner_brackets(canvas, x, y, size);
    }

    canvas.set_clip_rect(prev_clip);
}

/// Draw a bold L-shaped bracket at each corner of the `size`x`size` cell at
/// (x, y), using the canvas's current draw colour. The arms are filled bars
/// (several pixels thick) so the marker reads clearly over busy sprites, without
/// looking like a full tile border.
fn draw_corner_brackets(canvas: &mut WindowCanvas, x: i32, y: i32, size: i32) {
    let b = (size / 4).max(8); // bracket arm length
    let t = 3i32; // bracket arm thickness
    let (l, top, r, bot) = (x, y, x + size, y + size);
    let tu = t as u32;
    let bu = b as u32;
    // Each corner is an L made of one horizontal and one vertical filled bar.
    // Top-left
    let _ = canvas.fill_rect(Rect::new(l, top, bu, tu));
    let _ = canvas.fill_rect(Rect::new(l, top, tu, bu));
    // Top-right
    let _ = canvas.fill_rect(Rect::new(r - b, top, bu, tu));
    let _ = canvas.fill_rect(Rect::new(r - t, top, tu, bu));
    // Bottom-left
    let _ = canvas.fill_rect(Rect::new(l, bot - t, bu, tu));
    let _ = canvas.fill_rect(Rect::new(l, bot - b, tu, bu));
    // Bottom-right
    let _ = canvas.fill_rect(Rect::new(r - b, bot - t, bu, tu));
    let _ = canvas.fill_rect(Rect::new(r - t, bot - b, tu, bu));
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

/// Draw the path-mode action menu in the HUD bar: a "new block" button and a
/// "toggle loop" button. The new-block button shows a path glyph with a `+` and
/// lights up green while a fresh block is armed (the next level click starts it).
/// The loop button's glyph reflects the active block's current state - a full
/// rectangle with corner nodes for a loop, an open "C" for an open path - and
/// lights up while it is a loop. Dimmed when there is no active block to act on.
fn draw_path_menu(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    active_block: Option<usize>,
    start_new: bool,
) {
    // --- New-block button ---
    let nb = PATH_NEW_BTN;
    let nr = to_rect(nb);
    canvas.set_draw_color(if start_new {
        Color::RGB(40, 70, 50)
    } else {
        Color::RGB(45, 45, 58)
    });
    let _ = canvas.fill_rect(nr);
    canvas.set_draw_color(if start_new {
        Color::RGB(120, 220, 150)
    } else {
        Color::RGB(90, 130, 100)
    });
    let _ = canvas.draw_rect(nr);

    // Glyph: a short two-segment path with node dots, plus a `+` marking "add".
    let nglyph = if start_new {
        Color::RGB(150, 230, 175)
    } else {
        Color::RGB(150, 200, 165)
    };
    canvas.set_draw_color(nglyph);
    let ncy = nb.1 + nb.3 as i32 / 2;
    let nodes = [
        (nb.0 + 12, ncy + 5),
        (nb.0 + 22, ncy + 5),
        (nb.0 + 22, ncy - 5),
    ];
    for w in nodes.windows(2) {
        let _ = canvas.draw_line(w[0], w[1]);
    }
    for n in nodes {
        fill_circle(canvas, n.0, n.1, 2);
    }
    // A `+` to the right of the path, signalling a new block.
    let (pcx, pcy) = (nb.0 + 38, ncy);
    let arm = 5;
    let _ = canvas.draw_line((pcx - arm, pcy), (pcx + arm, pcy));
    let _ = canvas.draw_line((pcx - arm, pcy + 1), (pcx + arm, pcy + 1));
    let _ = canvas.draw_line((pcx, pcy - arm), (pcx, pcy + arm));
    let _ = canvas.draw_line((pcx + 1, pcy - arm), (pcx + 1, pcy + arm));

    // --- Toggle-loop button ---
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

/// Draw a rectangular button with a centred text label, lit yellow when active.
fn draw_text_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), label: &str, active: bool) {
    let r = to_rect(b);
    canvas.set_draw_color(if active {
        Color::RGB(80, 75, 40)
    } else {
        Color::RGB(50, 50, 65)
    });
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);
    let tx = b.0 + (b.2 as i32 - font::text_width(label, 1)) / 2;
    let ty = b.1 + (b.3 as i32 - font::line_height(1)) / 2;
    let color = if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(185, 195, 215)
    };
    font::draw_text(canvas, tx, ty, label, color, 1);
}

/// In Exit mode, outline every door and label where it leads. Normal doors are
/// bracketed cyan, secret doors magenta; the selected door is highlighted
/// yellow. Clipped to the play area so labels never bleed into the bars.
fn draw_exits(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    selected: Option<(usize, usize)>,
    camera_x: i32,
    camera_y: i32,
) {
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));
    let size = TILE_SIZE as i32;
    for exit in &level.exits {
        let (tx, ty) = exit.tile;
        let is_sel = selected == Some(exit.tile);
        let secret = level.tiles[ty][tx] == tiles::SECRET_EXIT;
        let x = tx as i32 * size - camera_x;
        let y = ty as i32 * size - camera_y;
        let color = if is_sel {
            Color::RGB(255, 235, 90)
        } else if secret {
            Color::RGB(220, 90, 220)
        } else {
            Color::RGB(90, 200, 235)
        };
        canvas.set_draw_color(color);
        draw_corner_brackets(canvas, x, y, size);

        // Destination label on a dark plate, just below the door.
        let label = format!("-> {}", exit.dest);
        let w = font::text_width(&label, 1);
        let lx = x + (size - w) / 2;
        let ly = y + size + 3;
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 160));
        let _ = canvas.fill_rect(Rect::new(
            lx - 2,
            ly - 1,
            (w + 4) as u32,
            (font::line_height(1) + 2) as u32,
        ));
        let text_color = if is_sel {
            Color::RGB(255, 235, 90)
        } else {
            Color::RGB(220, 220, 232)
        };
        font::draw_text(canvas, lx, ly, &label, text_color, 1);
    }
    canvas.set_clip_rect(prev_clip);
}

/// Draw the Exit-mode HUD: the exit tool palette (doors + coins) on the left, a
/// Set-destination button for the selected door, and a status line naming the
/// current target or prompting the user to paint / route a door.
fn draw_exit_menu(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    level: &LevelData,
    palette: &[Tool],
    selected_tool: usize,
    selected: Option<(usize, usize)>,
) {
    draw_palette_slots(
        canvas,
        tilemap_texture,
        character_texture,
        palette,
        selected_tool,
        HUD_MARGIN_X,
    );

    let secret = selected.is_some_and(|(tx, ty)| level.tiles[ty][tx] == tiles::SECRET_EXIT);
    let dest = selected
        .and_then(|t| level.exits.iter().find(|e| e.tile == t))
        .map(|e| e.dest.as_str());

    draw_text_button(canvas, EXIT_DEST_BTN, "Set dest", selected.is_some());

    let msg = match dest {
        Some(d) => format!("{} door -> {}", if secret { "secret" } else { "normal" }, d),
        None => "paint E/S doors & C/R coins; click a door to route it".to_string(),
    };
    let x = EXIT_DEST_BTN.0 + EXIT_DEST_BTN.2 as i32 + 16;
    let y = HUD_TOP + (HUD_HEIGHT - font::line_height(1)) / 2;
    font::draw_text(canvas, x, y, &msg, Color::RGB(205, 205, 222), 1);
}

/// Draw the full-list level overlay: a dimmed backdrop and a centred panel with
/// one clickable row per level (index, name and id). The current level's row is
/// highlighted. Used both to browse levels and to pick a door's destination.
fn draw_level_overlay(canvas: &mut WindowCanvas, docs: &[Document], current: usize, overlay: Overlay) {
    let count = docs.len();
    let r = overlay_rect(count);

    canvas.set_draw_color(Color::RGBA(0, 0, 0, 150));
    let _ = canvas.fill_rect(Rect::new(0, 0, VIEW_WIDTH, VIEW_HEIGHT));
    canvas.set_draw_color(Color::RGB(28, 28, 38));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(95, 95, 125));
    let _ = canvas.draw_rect(r);

    let title = match overlay {
        Overlay::Jump => "Go to level  (Esc to close)",
        Overlay::PickDest => "Pick destination  (Esc to cancel)",
    };
    font::draw_text(canvas, r.x() + OVERLAY_PAD, r.y() + 8, title, Color::RGB(255, 235, 90), 1);
    canvas.set_draw_color(Color::RGB(70, 70, 95));
    let sep_y = r.y() + OVERLAY_TITLE_H - 2;
    let _ = canvas.draw_line(
        (r.x() + OVERLAY_PAD, sep_y),
        (r.x() + r.width() as i32 - OVERLAY_PAD, sep_y),
    );

    let first = r.y() + OVERLAY_TITLE_H + OVERLAY_PAD;
    for (i, doc) in docs.iter().enumerate() {
        let ry = first + i as i32 * OVERLAY_ROW_H;
        if i == current {
            canvas.set_draw_color(Color::RGB(58, 58, 40));
            let _ = canvas.fill_rect(Rect::new(
                r.x() + OVERLAY_PAD,
                ry,
                r.width() - (2 * OVERLAY_PAD) as u32,
                (OVERLAY_ROW_H - 2) as u32,
            ));
        }
        let name = if doc.level.name.is_empty() {
            doc.level.id.as_str()
        } else {
            doc.level.name.as_str()
        };
        let label = format!("{:>2}  {}  [{}]", i + 1, name, doc.level.id);
        let color = if i == current {
            Color::RGB(255, 235, 90)
        } else {
            Color::RGB(222, 222, 234)
        };
        font::draw_text(canvas, r.x() + OVERLAY_PAD + 4, ry + 4, &label, color, 1);
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
    // modes have their own selectors (the path overlay / the sprite picker / the
    // Exit-mode menu), so the bar is left empty for them.
    if mode != Mode::Normal {
        return;
    }
    draw_palette_slots(
        canvas,
        tilemap_texture,
        character_texture,
        palette,
        selected,
        HUD_MARGIN_X,
    );
}

/// Draw one palette bar's worth of tool slots starting at `x0`, highlighting the
/// selected one. Shared by the Normal-mode palette and the Exit-mode menu.
fn draw_palette_slots(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    palette: &[Tool],
    selected: usize,
    x0: i32,
) {
    let slot_y = HUD_TOP + (HUD_HEIGHT - SLOT) / 2;
    for (i, tool) in palette.iter().enumerate() {
        let slot_x = x0 + i as i32 * (SLOT + SLOT_PAD);
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
fn draw_top_bar(
    canvas: &mut WindowCanvas,
    docs: &[Document],
    current: usize,
    mode: Mode,
    deco_layer: DecoLayer,
) {
    // Background
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, 0, VIEW_WIDTH, TOP_BAR_HEIGHT as u32));
    // Bottom border
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, TOP_BAR_HEIGHT - 1), (VIEW_WIDTH as i32, TOP_BAR_HEIGHT - 1));

    // --- Levels button (shows the current level; opens the browser) ---
    let level_label = format!("Lv {}/{}", current + 1, docs.len());
    draw_text_button(canvas, LEVELS_BTN, &level_label, false);

    // --- Mode-switch buttons (exactly one active) ---
    draw_normal_button(canvas, NORMAL_BTN, mode == Mode::Normal);
    draw_block_button(canvas, BLOCK_BTN, mode == Mode::Path);
    // Pass the active layer only while in deco mode, so the button can show which
    // of the two layers (background / foreground) is currently selected.
    draw_deco_button(
        canvas,
        DECO_BTN,
        (mode == Mode::Deco).then_some(deco_layer),
    );
    // Exit-door mode button.
    draw_text_button(canvas, EXIT_BTN, "EXIT", mode == Mode::Exit);

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

/// Draw the decoration-mode button. While inactive it shows a small "picture"
/// glyph (a framed tile with a sun and mountain). While active it shows a
/// two-square "layers" badge instead - the back square is the background layer,
/// the front square the foreground layer - with the currently selected layer
/// lit in its overlay colour (magenta = background, orange = foreground) so it
/// is clear which of the two deco modes is in effect.
fn draw_deco_button(canvas: &mut WindowCanvas, b: (i32, i32, u32, u32), layer: Option<DecoLayer>) {
    let active = layer.is_some();
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

    let cx = b.0 + b.2 as i32 / 2;
    let cy = b.1 + b.3 as i32 / 2;

    let Some(layer) = layer else {
        // Inactive: a little framed picture (a border, a sun, and a mountain).
        canvas.set_draw_color(Color::RGB(160, 200, 230));
        let frame = Rect::new(cx - 11, cy - 8, 22, 16);
        let _ = canvas.draw_rect(frame);
        fill_circle(canvas, cx - 5, cy - 3, 2);
        let base = cy + 7;
        let _ = canvas.draw_line((cx - 9, base), (cx - 1, base - 9));
        let _ = canvas.draw_line((cx - 1, base - 9), (cx + 9, base));
        return;
    };

    // Active: a two-square "layers" badge. The back square (up-left) is the
    // background layer, the front square (down-right) the foreground layer. The
    // selected one is filled in its overlay colour; the other is a dim outline.
    // Drawn back-then-front so the foreground square really overlaps the back.
    let dim = Color::RGB(120, 120, 140);
    let back = Rect::new(cx - 9, cy - 7, 11, 11);
    let front = Rect::new(cx - 1, cy - 3, 11, 11);

    if layer == DecoLayer::Background {
        canvas.set_draw_color(Color::RGB(255, 80, 245));
        let _ = canvas.fill_rect(back);
        canvas.set_draw_color(Color::RGB(255, 170, 250));
        let _ = canvas.draw_rect(back);
    } else {
        canvas.set_draw_color(dim);
        let _ = canvas.draw_rect(back);
    }

    if layer == DecoLayer::Foreground {
        canvas.set_draw_color(Color::RGB(255, 170, 40));
        let _ = canvas.fill_rect(front);
        canvas.set_draw_color(Color::RGB(255, 210, 150));
        let _ = canvas.draw_rect(front);
    } else {
        canvas.set_draw_color(dim);
        let _ = canvas.draw_rect(front);
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
    println!("  Toolbar resize arrows   : left-click=grow edge, right-click=shrink edge");
    println!("  Arrows / WASD: pan camera              Home        : scroll to start");
    println!("  Ctrl+Arrow  : grow canvas at that edge Ctrl+Shift+Arrow: shrink that edge");
    println!("  G           : toggle grid              Ctrl+S      : save level");
    println!("  Modes (toolbar/keys): F1 Normal tiles | F2 path blocks | F3 decorations | F4 exit doors");
    println!("  Levels      : the 'Lv n/m' button or Tab opens the level browser; click a level to jump");
    println!("  Normal mode : click palette bar to pick a tool, left-click paints, right-click erases");
    println!("                (world tiles only; exit doors and coins moved to Exit mode)");
    println!("  Path mode   : left-click adds points / drags points & edges, right-click deletes");
    println!("                N new block, L open/close loop, Tab cycle, Del remove block");
    println!("                (bottom bar shows New-block and Toggle-loop buttons)");
    println!("  Deco mode   : click picker to choose a sprite, left-click places, right-click erases");
    println!("                deco toolbar button: 1st click = background layer, 2nd = foreground, 3rd = Normal");
    println!("  Exit mode   : palette bar paints E/S exit doors and C/R gold/red coins (right-click erases);");
    println!("                click a door to select it, then Set-dest picks its target level (routing auto-syncs)");
    println!("  Esc / Q     : quit (Esc first closes an open level browser)");
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

    #[test]
    fn painting_a_door_syncs_a_routed_exit_and_erasing_removes_it() {
        let mut level = empty_level();
        let (x, y) = click_at(3, 3);

        // Painting a normal exit door creates its routed exit entry.
        assert!(apply_tool(
            &mut level,
            Tool::Tile(tiles::EXIT),
            "level02",
            x,
            y,
            0.0,
            0.0
        ));
        assert_eq!(
            level.exits,
            vec![level::ExitDoor { tile: (3, 3), dest: "level02".to_string() }]
        );

        // Converting it to a secret door keeps the single exit entry.
        assert!(apply_tool(
            &mut level,
            Tool::Tile(tiles::SECRET_EXIT),
            "level02",
            x,
            y,
            0.0,
            0.0
        ));
        assert_eq!(level.tiles[3][3], tiles::SECRET_EXIT);
        assert_eq!(level.exits.len(), 1);

        // Erasing the door drops its exit entry so the file stays valid.
        assert!(apply_tool(&mut level, Tool::Erase, "level02", x, y, 0.0, 0.0));
        assert!(level.exits.is_empty());
    }
}
