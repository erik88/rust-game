//! Tile types and their semantics. The visual layout of these tiles in
//! tilemap.png is documented in CLAUDE.md.

/// Width and height of a tile in pixels
pub const TILE_SIZE: f32 = 40.0;

pub const EMPTY: u32 = 0;
pub const SOLID: u32 = 1;
// Tile id 2 is unused (formerly a dark solid variant).
/// The player dies when touching this tile
pub const DEATH: u32 = 3;
/// Crumbling tile: starts decaying when touched (4 -> 5 -> 6 -> gone)
pub const CRUMBLE: u32 = 4;
pub const CRUMBLE_CRACKED: u32 = 5;
pub const CRUMBLE_VERY_CRACKED: u32 = 6;
/// Periodic tile, solid phase. Swaps with PERIODIC_GHOST every second.
pub const PERIODIC_SOLID: u32 = 7;
/// Periodic tile, phased-out (non-solid) phase
pub const PERIODIC_GHOST: u32 = 8;
/// Moving tiles: activate when stepped on, then travel in their direction
pub const MOVE_UP: u32 = 9;
pub const MOVE_RIGHT: u32 = 10;
pub const MOVE_DOWN: u32 = 11;
pub const MOVE_LEFT: u32 = 12;
/// Exit door, closed phase. Rendered as the CLOSED door sprite; the level can
/// only be completed once every coin has been collected, at which point the
/// door is drawn with the EXIT_OPEN sprite and touching it completes the level.
pub const EXIT: u32 = 13;
/// A collectible coin. Every coin in a level must be collected before its
/// exit door opens. Never stored in a level file as a separate tile - it is
/// placed with the `C` character.
pub const COIN: u32 = 14;
/// A path-following block. Render-only id: path blocks are declared with a
/// `block:` header, never stored in the tile grid, so this id exists purely to
/// select their sprite.
pub const PATH_BLOCK: u32 = 15;
/// Exit door, open phase. This is purely a render substitution for an [`EXIT`]
/// tile once all coins are gone; it never appears in the tile grid itself.
pub const EXIT_OPEN: u32 = 19;

/// Number of 40px sprites per row in tilemap.png.
pub const SHEET_COLUMNS: u32 = 6;

/// Top-left pixel coordinate of the sprite at a 1-based sheet index, laid out
/// left-to-right then top-to-bottom (index 1 = (0,0), index 7 = (0,40)). Unlike
/// [`tile_src_xy`], which maps gameplay tile ids to their (possibly irregular)
/// sprite positions, this addresses the sheet purely by position so decorations
/// can reference *any* sprite regardless of gameplay meaning.
pub fn sheet_src_xy(index: u32) -> (i32, i32) {
    let i = index.saturating_sub(1);
    let col = (i % SHEET_COLUMNS) as i32;
    let row = (i / SHEET_COLUMNS) as i32;
    (col * TILE_SIZE as i32, row * TILE_SIZE as i32)
}

/// Top-left pixel coordinate of a tile's graphic within tilemap.png. The sheet
/// is 240x240 (six 40px tiles per row); these are the literal positions of each
/// sprite rather than offsets derived from the tile index.
pub fn tile_src_xy(tile_id: u32) -> (i32, i32) {
    match tile_id {
        SOLID => (0, 0),
        DEATH => (80, 0),
        CRUMBLE => (120, 0),
        CRUMBLE_CRACKED => (160, 0),
        CRUMBLE_VERY_CRACKED => (200, 0),
        PERIODIC_SOLID => (0, 40),
        PERIODIC_GHOST => (40, 40),
        MOVE_UP => (80, 40),
        MOVE_RIGHT => (120, 40),
        MOVE_DOWN => (160, 40),
        MOVE_LEFT => (200, 40),
        EXIT => (0, 120),
        COIN => (40, 120),
        PATH_BLOCK => (0, 80),
        EXIT_OPEN => (0, 160),
        _ => (0, 0),
    }
}

/// Whether the player and moving platforms collide with this tile type
pub fn is_solid(tile_type: u32) -> bool {
    !matches!(tile_type, EMPTY | DEATH | PERIODIC_GHOST | EXIT | COIN)
}

/// Whether this tile type is extracted from the grid as a moving platform
pub fn is_moving(tile_type: u32) -> bool {
    matches!(tile_type, MOVE_UP | MOVE_RIGHT | MOVE_DOWN | MOVE_LEFT)
}
