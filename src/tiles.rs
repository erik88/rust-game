//! Tile types and their semantics. The visual layout of these tiles in
//! tilemap.png is documented in CLAUDE.md.

/// Width and height of a tile in pixels
pub const TILE_SIZE: f32 = 40.0;

pub const EMPTY: u32 = 0;
pub const SOLID: u32 = 1;
pub const SOLID_DARK: u32 = 2;
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
/// Exit door, open phase. This is purely a render substitution for an [`EXIT`]
/// tile once all coins are gone; it never appears in the tile grid itself.
pub const EXIT_OPEN: u32 = 19;

/// Whether the player and moving platforms collide with this tile type
pub fn is_solid(tile_type: u32) -> bool {
    !matches!(tile_type, EMPTY | DEATH | PERIODIC_GHOST | EXIT | COIN)
}

/// Whether this tile type is extracted from the grid as a moving platform
pub fn is_moving(tile_type: u32) -> bool {
    matches!(tile_type, MOVE_UP | MOVE_RIGHT | MOVE_DOWN | MOVE_LEFT)
}
