//! The editor's state and its discrete actions: the [`Editor`] struct bundling
//! all loop-persistent state, the [`UICommand`] every keyboard shortcut and
//! toolbar button maps to, and the level [`Document`]s being edited.

use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::render::WindowCanvas;
use std::path::PathBuf;

use rustgamex::level::{DecoLayer, Edge, LevelData};
use rustgamex::player::Player;
use rustgamex::tilemap::TileMap;
use rustgamex::tiles::{self, TILE_SIZE, TilePos};

use crate::layout::{
    BLOCK_BTN, DECO_BTN, EXIT_BTN, HUD_TOP, LEVELS_BTN, NORMAL_BTN, PATH_LOOP_BTN, PATH_NEW_BTN,
    PLAY_BTN, RESIZE_BOT_BTN, RESIZE_LEFT_BTN, RESIZE_RIGHT_BTN, RESIZE_TOP_BTN, SELECT_BTN,
    TILE_AREA_TOP, VIEW_WIDTH, cursor_tile, cursor_tile_i, in_overlay, overlay_row_at,
};
use crate::layout::PAN_SPEED;
use crate::path_edit::{Drag, toggle_loop};
use crate::select::{SelectDrag, Selection, clamp_move_target, move_selection};
use crate::tools::{Tool, layer_name, reconcile_exit};

/// Which editing mode the editor is in. Exactly one is active at a time; the
/// bottom tool palette is shown only in [`Mode::Normal`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Paint normal tiles from the bottom palette.
    Normal,
    /// Edit path-block control points.
    Path,
    /// Place render-only decorations.
    Deco,
    /// Select exit doors and set where each one leads.
    Exit,
    /// Select a rectangular block of tiles and drag it to move or copy it.
    Select,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "Normal (tiles)",
            Mode::Path => "Path blocks",
            Mode::Deco => "Decorations",
            Mode::Exit => "Exit doors",
            Mode::Select => "Select (region)",
        }
    }
}

/// The full-list level overlay, and what a click on one of its rows does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// Browsing levels: clicking a row switches to that level.
    Jump,
    /// Choosing where the selected exit door leads: clicking a row sets its
    /// destination and closes the overlay.
    PickDest,
}

/// A level being edited, paired with the file it came from.
pub struct Document {
    pub path: PathBuf,
    pub level: LevelData,
    pub modified: bool,
}

/// Camera-panning keys currently held. Grouped so they can all be released in
/// one place (e.g. after launching the game, which can swallow the key-up
/// events that would otherwise clear them).
#[derive(Default)]
pub struct Pan {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl Pan {
    pub fn clear(&mut self) {
        *self = Pan::default();
    }
}

/// A discrete editor action that can be triggered from more than one input: a
/// toolbar/menu button click and/or a keyboard shortcut. Both input paths map
/// their event to a `UICommand` and run it through [`Editor::execute`], so each
/// action's behaviour lives in exactly one place instead of being duplicated
/// between the key and mouse handlers.
#[derive(Clone, Copy)]
pub enum UICommand {
    /// Move to the previous / next level (wrapping).
    PrevLevel,
    NextLevel,
    /// Switch to a specific mode (the F1/F2 keys and the Normal button).
    SetMode(Mode),
    /// Path button: toggle between Path mode and Normal.
    ToggleBlockMode,
    /// Deco button: enter deco mode on background, then toggle background <-> foreground.
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
    /// Erase every tile in the current Select-mode selection.
    EraseSelection,
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
pub struct Editor {
    pub docs: Vec<Document>,
    pub palette: Vec<Tool>,
    /// Exit-mode tool palette: the exit doors (normal / secret) and the coins
    /// that gate them, painted from the bottom bar while in [`Mode::Exit`].
    pub exit_palette: Vec<Tool>,
    pub current: usize,
    pub camera_x: f32,
    pub camera_y: f32,
    pub selected: usize,
    /// Selected slot into [`Editor::exit_palette`] (the Exit-mode tool).
    pub exit_tool: usize,
    pub show_grid: bool,
    pub pan: Pan,
    pub mouse: (i32, i32),
    /// The active editing mode (normal tiles / path blocks / decorations).
    pub mode: Mode,
    /// Path-block editing state (used while in `Mode::Path`).
    pub active_block: Option<usize>,
    /// The point or edge currently being dragged within the active block.
    pub dragging: Option<Drag>,
    /// When set, the next left-click starts a fresh block instead of appending.
    pub start_new: bool,
    /// Which sprite-sheet index the next decoration placement uses, and which
    /// layer it lands on (while in `Mode::Deco`).
    pub deco_sprite: u32,
    pub deco_layer: DecoLayer,
    /// The exit door currently selected for editing (grid coords), in `Mode::Exit`.
    pub selected_door: Option<TilePos>,
    /// The current rectangular tile selection, in `Mode::Select`.
    pub selection: Option<Selection>,
    /// The in-progress selection drag (rubber-band or move/copy), in `Mode::Select`.
    pub select_drag: Option<SelectDrag>,
    /// Whether a Ctrl key is currently held, so a Select-mode drag copies instead
    /// of moves. Tracked from key events since mouse events carry no modifiers.
    pub copy_mod: bool,
    /// The open full-list level overlay, if any (level browser / destination picker).
    pub overlay: Option<Overlay>,
    /// Rendered view of the current level, rebuilt from it whenever `dirty`.
    pub tilemap: TileMap,
    pub player: Player,
    pub dirty: bool,
    /// A pending level switch, applied once per frame after event handling so
    /// the index never changes mid-iteration.
    pub switch_to: Option<usize>,
    /// Set by [`UICommand::Quit`]; breaks the main loop after the frame.
    pub quit: bool,
    /// Set whenever the window title needs refreshing (level/tool/modified
    /// state changed); `main` re-applies it once per frame.
    pub retitle: bool,
}

impl Editor {
    pub fn new(docs: Vec<Document>, palette: Vec<Tool>, exit_palette: Vec<Tool>) -> Self {
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
            selection: None,
            select_drag: None,
            copy_mod: false,
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
    pub fn execute(&mut self, cmd: UICommand) {
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
                // button: the first click enters background; further clicks just
                // toggle between background and foreground (never back to Normal,
                // use another mode button for that).
                if self.mode != Mode::Deco {
                    self.deco_layer = DecoLayer::Background;
                } else if self.deco_layer == DecoLayer::Background {
                    self.deco_layer = DecoLayer::Foreground;
                } else {
                    self.deco_layer = DecoLayer::Background;
                }
                self.set_mode(Mode::Deco);
                println!("Deco layer: {}", layer_name(self.deco_layer));
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
            UICommand::EraseSelection => {
                if let Some(sel) = self.selection {
                    let dest = self.default_exit_dest();
                    let level = &mut self.docs[self.current].level;
                    for ty in sel.min.1..=sel.max.1 {
                        for tx in sel.min.0..=sel.max.0 {
                            level.tiles[ty][tx] = tiles::EMPTY;
                            reconcile_exit(level, (tx, ty), &dest);
                        }
                    }
                    self.mark_changed();
                }
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
        // The tile selection only makes sense in Select mode; drop it otherwise.
        if mode != Mode::Select {
            self.selection = None;
        }
        self.select_drag = None;
        println!("Mode: {}", mode.name());
    }

    /// The level id a newly painted exit door is routed to by default: the next
    /// level in the list (matching the old linear progression), or this level
    /// when it is the only one.
    pub fn default_exit_dest(&self) -> String {
        let n = self.docs.len();
        let next = (self.current + 1) % n;
        self.docs[next].level.id.clone()
    }

    /// Handle a click while the level overlay is open: a row either jumps to that
    /// level or assigns it as the selected door's destination; a click off the
    /// rows dismisses the overlay.
    pub fn overlay_click(&mut self, btn: MouseButton, x: i32, y: i32) {
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

    /// Handle a mouse-button press in the tile area while in [`Mode::Select`].
    /// Left inside the current selection starts a move (a copy while Ctrl is
    /// held); left elsewhere starts a fresh rubber-band selection; right clears
    /// the selection.
    pub fn select_press(&mut self, btn: MouseButton, x: i32, y: i32) {
        let Some(tile) = cursor_tile(
            &self.docs[self.current].level,
            x,
            y,
            self.camera_x,
            self.camera_y,
        ) else {
            return;
        };
        match btn {
            MouseButton::Left => {
                if let Some(sel) = self.selection.filter(|s| s.contains(tile)) {
                    let grab = (tile.0 - sel.min.0, tile.1 - sel.min.1);
                    self.select_drag = Some(SelectDrag::Move {
                        grab,
                        copy: self.copy_mod,
                    });
                } else {
                    self.selection = Some(Selection::from_corners(tile, tile));
                    self.select_drag = Some(SelectDrag::Marquee { anchor: tile });
                }
            }
            MouseButton::Right => {
                self.selection = None;
                self.select_drag = None;
            }
            _ => {}
        }
    }

    /// End a Select-mode pointer drag: commit a pending move/copy of the block to
    /// wherever the cursor released, then clear the drag. A no-op for a rubber-band
    /// drag (the selection was already updated as the cursor moved).
    pub fn finish_select_drag(&mut self) {
        let Some(SelectDrag::Move { grab, copy }) = self.select_drag.take() else {
            return;
        };
        let Some(sel) = self.selection else {
            return;
        };
        let camera_x = self.camera_x as i32;
        let render_cam_y = self.camera_y as i32 - TILE_AREA_TOP;
        let Some(cursor) = cursor_tile_i(self.mouse, camera_x, render_cam_y) else {
            return;
        };
        let (w, h) = {
            let level = &self.docs[self.current].level;
            (level.width(), level.height())
        };
        let target = clamp_move_target(w, h, sel, grab, cursor);
        if target == sel.min {
            return;
        }
        let dest = self.default_exit_dest();
        move_selection(&mut self.docs[self.current].level, sel, target, copy, &dest);
        self.selection = Some(Selection::at(target, sel.width(), sel.height()));
        self.mark_changed();
    }

    /// Queue a move by `delta` levels (negative = previous), wrapping around.
    fn switch_level(&mut self, delta: isize) {
        let n = self.docs.len();
        let next = (self.current as isize + delta).rem_euclid(n as isize) as usize;
        self.switch_to = Some(next);
    }

    /// Mark the current level edited: flag it for redraw and a title refresh.
    pub fn mark_changed(&mut self) {
        self.docs[self.current].modified = true;
        self.dirty = true;
        self.retitle = true;
    }

    /// Apply a pending level switch, resetting the per-level view state.
    pub fn apply_switch(&mut self) {
        if let Some(index) = self.switch_to.take() {
            self.current = index;
            self.camera_x = 0.0;
            self.camera_y = 0.0;
            self.pan.clear();
            self.active_block = None;
            self.dragging = None;
            self.start_new = false;
            self.selected_door = None;
            self.selection = None;
            self.select_drag = None;
            self.dirty = true;
            self.retitle = true;
        }
    }

    /// Rebuild the rendered view from the level data after an edit.
    pub fn rebuild_if_dirty(&mut self) {
        if self.dirty {
            self.tilemap = TileMap::from_level(&self.docs[self.current].level);
            self.player = spawn_player(&self.docs[self.current].level);
            self.dirty = false;
        }
    }

    /// Advance the camera by the held pan keys and clamp it to the level.
    pub fn update_camera(&mut self, delta_time: f32) {
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
        let level_width = self.tilemap.width as f32 * TILE_SIZE;
        let level_height = self.tilemap.height as f32 * TILE_SIZE;
        let max_camera_x = (level_width - VIEW_WIDTH as f32).max(0.0);
        let max_camera_y = (level_height - tile_area_h).max(0.0);
        self.camera_x = self.camera_x.clamp(0.0, max_camera_x);
        self.camera_y = self.camera_y.clamp(0.0, max_camera_y);
    }
}

/// Map a key press (with its modifiers) to the [`UICommand`] it triggers, if
/// any. Returns `None` for keys handled directly in the loop (panning) or that
/// are unbound, so the caller can fall through to those.
pub fn key_command(key: Keycode, ctrl: bool, shift: bool, mode: Mode) -> Option<UICommand> {
    // Ctrl+Arrow resizes the canvas (Ctrl+Shift+Arrow shrinks instead of grows).
    let resize_edge = match key {
        Keycode::Up if ctrl => Some(Edge::Top),
        Keycode::Down if ctrl => Some(Edge::Bottom),
        Keycode::Left if ctrl => Some(Edge::Left),
        Keycode::Right if ctrl => Some(Edge::Right),
        _ => None,
    };
    if let Some(edge) = resize_edge {
        return Some(UICommand::Resize {
            edge,
            shrink: shift,
        });
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
        Keycode::F5 => UICommand::SetMode(Mode::Select),
        Keycode::N if mode == Mode::Path => UICommand::NewBlock,
        Keycode::Tab if mode == Mode::Path => UICommand::CycleBlock,
        Keycode::L if mode == Mode::Path => UICommand::ToggleLoop,
        Keycode::Delete | Keycode::Backspace if mode == Mode::Path => UICommand::DeleteBlock,
        Keycode::Delete | Keycode::Backspace if mode == Mode::Select => UICommand::EraseSelection,
        // Tab opens the level browser everywhere except path mode (where it
        // cycles blocks).
        Keycode::Tab => UICommand::OpenLevels,
        _ => return None,
    })
}

/// Map a mouse-button press in the top toolbar to the [`UICommand`] it
/// triggers, if it hit a button. Left-click on a resize button grows, right
/// shrinks; the other toolbar buttons only respond to the left button.
pub fn topbar_command(btn: MouseButton, x: i32, y: i32) -> Option<UICommand> {
    if btn == MouseButton::Left {
        if LEVELS_BTN.contains(x, y) {
            return Some(UICommand::OpenLevels);
        } else if NORMAL_BTN.contains(x, y) {
            return Some(UICommand::SetMode(Mode::Normal));
        } else if BLOCK_BTN.contains(x, y) {
            return Some(UICommand::ToggleBlockMode);
        } else if DECO_BTN.contains(x, y) {
            return Some(UICommand::CycleDecoMode);
        } else if EXIT_BTN.contains(x, y) {
            return Some(UICommand::SetMode(Mode::Exit));
        } else if SELECT_BTN.contains(x, y) {
            return Some(UICommand::SetMode(Mode::Select));
        } else if PLAY_BTN.contains(x, y) {
            return Some(UICommand::Play);
        }
    }
    // Resize buttons respond to either button: left grows, right shrinks.
    let edge = if RESIZE_TOP_BTN.contains(x, y) {
        Some(Edge::Top)
    } else if RESIZE_BOT_BTN.contains(x, y) {
        Some(Edge::Bottom)
    } else if RESIZE_LEFT_BTN.contains(x, y) {
        Some(Edge::Left)
    } else if RESIZE_RIGHT_BTN.contains(x, y) {
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
pub fn path_menu_command(btn: MouseButton, x: i32, y: i32) -> Option<UICommand> {
    if btn != MouseButton::Left {
        return None;
    }
    if PATH_NEW_BTN.contains(x, y) {
        Some(UICommand::NewBlock)
    } else if PATH_LOOP_BTN.contains(x, y) {
        Some(UICommand::ToggleLoop)
    } else {
        None
    }
}

pub fn spawn_player(level: &LevelData) -> Player {
    Player::new(level.spawn.0, level.spawn.1)
}

/// Test-play the level currently being edited: write it to a temp directory on
/// its own and launch the game there. The temp dir contains only one level, and
/// its exit doors may point at levels that aren't in it; the game tolerates an
/// unresolvable door target by simply restarting the level. Blocks until the
/// game window closes.
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

pub fn set_title(canvas: &mut WindowCanvas, docs: &[Document], current: usize, tool: Tool) {
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
