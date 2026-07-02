//! Screen layout of the editor UI: the window dimensions, the position of
//! every toolbar/menu button, and the helpers that map screen positions to
//! UI elements or tile cells.

use rustgamex::level::LevelData;
use rustgamex::tiles::{self, TILE_SIZE};
use rustgamex::{SCREEN_HEIGHT, SCREEN_WIDTH};
use sdl2::rect::Rect;

/// A rectangular UI button on the screen, with hit-testing. Kept as a distinct
/// type (rather than a bare `(x, y, w, h)` tuple) so button positions can't be
/// mixed up with other coordinate quadruples.
#[derive(Clone, Copy)]
pub struct ButtonRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl ButtonRect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// Whether a screen position falls inside the button.
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w as i32 && y >= self.y && y < self.y + self.h as i32
    }

    /// The button as an SDL rect, for drawing.
    pub fn rect(self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    /// The button's centre point, where glyphs are drawn around.
    pub fn center(self) -> (i32, i32) {
        (self.x + self.w as i32 / 2, self.y + self.h as i32 / 2)
    }
}

pub const VIEW_WIDTH: u32 = SCREEN_WIDTH;
// The visible play area should match the game's so levels look the same in the
// editor. The window is taller to fit the UI controls above and below it.
pub const PLAY_HEIGHT: u32 = SCREEN_HEIGHT;
pub const VIEW_HEIGHT: u32 = PLAY_HEIGHT + TOP_BAR_HEIGHT as u32 + HUD_HEIGHT as u32;
pub const PAN_SPEED: f32 = 600.0;

// Top toolbar
pub const TOP_BAR_HEIGHT: i32 = 36;
pub const BTN_Y: i32 = 4;
pub const BTN_H: u32 = 28;

// Palette HUD layout
pub const HUD_HEIGHT: i32 = 56;
pub const HUD_TOP: i32 = VIEW_HEIGHT as i32 - HUD_HEIGHT;
pub const SLOT: i32 = 40;
pub const SLOT_PAD: i32 = 6;
pub const HUD_MARGIN_X: i32 = 8;

// Path-mode action menu, drawn in the (otherwise empty) HUD bar while in path
// mode. It holds a "new block" button and a "toggle loop" button.
pub const PATH_MENU_BTN_H: i32 = 36;
pub const PATH_NEW_BTN: ButtonRect = ButtonRect::new(
    HUD_MARGIN_X,
    HUD_TOP + (HUD_HEIGHT - PATH_MENU_BTN_H) / 2,
    52,
    PATH_MENU_BTN_H as u32,
);
pub const PATH_LOOP_BTN: ButtonRect = ButtonRect::new(
    PATH_NEW_BTN.x + PATH_NEW_BTN.w as i32 + 8,
    PATH_NEW_BTN.y,
    52,
    PATH_MENU_BTN_H as u32,
);

// Tile area is between top bar and palette HUD
pub const TILE_AREA_TOP: i32 = TOP_BAR_HEIGHT;

// Decoration sprite picker: the whole 240x240 tilemap sheet (6x6 sprites at full
// scale) drawn as an overlay in the top-right of the tile area while in
// decoration mode. Clicking a cell selects that sprite.
pub const PICKER_ROWS: i32 = 6;
pub const PICKER_CELL: i32 = TILE_SIZE as i32;
pub const PICKER_W: i32 = tiles::SHEET_COLUMNS as i32 * PICKER_CELL;
pub const PICKER_H: i32 = PICKER_ROWS * PICKER_CELL;
pub const PICKER_X: i32 = VIEW_WIDTH as i32 - PICKER_W;
pub const PICKER_Y: i32 = TILE_AREA_TOP;

// "Levels" button (top-left): shows the current level and opens the level
// browser overlay, which is the sole way to switch levels with the mouse.
pub const LEVELS_BTN: ButtonRect = ButtonRect::new(8, BTN_Y, 76, BTN_H);
// Mode-switch buttons, grouped left-to-right: place normal tiles, edit path
// blocks, place decorations, route exit doors, select regions. Exactly one
// mode is active at a time.
pub const MODE_BTN_W: u32 = 52;
pub const MODE_BTN_X0: i32 = 116;
pub const MODE_BTN_STEP: i32 = MODE_BTN_W as i32 + 6;
pub const NORMAL_BTN: ButtonRect = ButtonRect::new(MODE_BTN_X0, BTN_Y, MODE_BTN_W, BTN_H);
pub const BLOCK_BTN: ButtonRect =
    ButtonRect::new(MODE_BTN_X0 + MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
pub const DECO_BTN: ButtonRect =
    ButtonRect::new(MODE_BTN_X0 + 2 * MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
pub const EXIT_BTN: ButtonRect =
    ButtonRect::new(MODE_BTN_X0 + 3 * MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
pub const SELECT_BTN: ButtonRect =
    ButtonRect::new(MODE_BTN_X0 + 4 * MODE_BTN_STEP, BTN_Y, MODE_BTN_W, BTN_H);
// Resize buttons: left-click = grow, right-click = shrink
pub const RESIZE_TOP_BTN: ButtonRect = ButtonRect::new(580, BTN_Y, 36, BTN_H);
pub const RESIZE_BOT_BTN: ButtonRect = ButtonRect::new(620, BTN_Y, 36, BTN_H);
pub const RESIZE_LEFT_BTN: ButtonRect = ButtonRect::new(660, BTN_Y, 36, BTN_H);
pub const RESIZE_RIGHT_BTN: ButtonRect = ButtonRect::new(700, BTN_Y, 36, BTN_H);
// Play button
pub const PLAY_BTN: ButtonRect = ButtonRect::new(748, BTN_Y, 44, BTN_H);

// Exit-mode HUD: a small tool palette (exit doors + coins) on the left, then a
// button that routes the selected door's destination. The palette occupies the
// first `EXIT_PALETTE_SLOTS` slots from the left margin, so the Set-dest button
// clears them.
pub const EXIT_MENU_BTN_H: i32 = 36;
pub const EXIT_PALETTE_SLOTS: i32 = 5;
pub const EXIT_DEST_BTN: ButtonRect = ButtonRect::new(
    HUD_MARGIN_X + EXIT_PALETTE_SLOTS * (SLOT + SLOT_PAD) + 12,
    HUD_TOP + (HUD_HEIGHT - EXIT_MENU_BTN_H) / 2,
    130,
    EXIT_MENU_BTN_H as u32,
);

// Level-browser overlay layout (a centred panel listing every level).
pub const OVERLAY_W: i32 = 440;
pub const OVERLAY_ROW_H: i32 = 24;
pub const OVERLAY_PAD: i32 = 10;
pub const OVERLAY_TITLE_H: i32 = 26;

/// Bounding rect of the level-browser overlay panel for `count` levels.
pub fn overlay_rect(count: usize) -> Rect {
    let h = OVERLAY_TITLE_H + count as i32 * OVERLAY_ROW_H + OVERLAY_PAD * 2;
    let x = (VIEW_WIDTH as i32 - OVERLAY_W) / 2;
    let avail = HUD_TOP - TILE_AREA_TOP;
    let y = (TILE_AREA_TOP + (avail - h) / 2).max(TILE_AREA_TOP + 8);
    Rect::new(x, y, OVERLAY_W as u32, h as u32)
}

/// Index of the overlay list row under a screen position, if any.
pub fn overlay_row_at(count: usize, x: i32, y: i32) -> Option<usize> {
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
pub fn in_overlay(count: usize, x: i32, y: i32) -> bool {
    let r = overlay_rect(count);
    x >= r.x() && x < r.x() + r.width() as i32 && y >= r.y() && y < r.y() + r.height() as i32
}

/// Index of the HUD palette slot under an x position, for a bar of `count`
/// slots starting at the left margin.
pub fn palette_slot_at(x: i32, count: usize) -> Option<usize> {
    for i in 0..count {
        let slot_x = HUD_MARGIN_X + i as i32 * (SLOT + SLOT_PAD);
        if x >= slot_x && x < slot_x + SLOT {
            return Some(i);
        }
    }
    None
}

/// Whether a screen position is over the decoration sprite-picker overlay.
pub fn in_deco_picker(x: i32, y: i32) -> bool {
    (PICKER_X..PICKER_X + PICKER_W).contains(&x) && (PICKER_Y..PICKER_Y + PICKER_H).contains(&y)
}

/// The 1-based sprite-sheet index for the picker cell under a screen position,
/// or `None` if the position is outside the picker.
pub fn picker_sprite_at(x: i32, y: i32) -> Option<u32> {
    if !in_deco_picker(x, y) {
        return None;
    }
    let col = (x - PICKER_X) / PICKER_CELL;
    let row = (y - PICKER_Y) / PICKER_CELL;
    Some((row * tiles::SHEET_COLUMNS as i32 + col + 1) as u32)
}

/// Convert a screen position to the tile it sits over, or `None` if it is
/// outside the tile-editing area.
pub fn screen_to_tile(
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

/// Tile coordinate under the cursor, clamped to the level bounds. `None` if the
/// cursor is outside the tile area entirely.
pub fn cursor_tile(
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

/// Tile coordinates (possibly outside the grid) under a screen position, using
/// the same integer render camera the tiles are drawn with — `camera_x` is the
/// truncated horizontal camera and `render_cam_y` is `camera_y - TILE_AREA_TOP`.
/// Kept int-for-int consistent with rendering so the Select-mode move preview and
/// the committed move always land on the same cell. `None` outside the tile area.
pub fn cursor_tile_i(mouse: (i32, i32), camera_x: i32, render_cam_y: i32) -> Option<(i32, i32)> {
    let (mx, my) = mouse;
    if !(TILE_AREA_TOP..HUD_TOP).contains(&my) {
        return None;
    }
    let size = TILE_SIZE as i32;
    let world_x = mx + camera_x;
    let world_y = my + render_cam_y;
    if world_x < 0 || world_y < 0 {
        return None;
    }
    Some((world_x / size, world_y / size))
}
