//! Shared helpers for the editor's unit tests.

use rustgamex::level::LevelData;
use rustgamex::tiles::TILE_SIZE;

use crate::layout::TILE_AREA_TOP;

/// A 10x10 empty level with a spawn in the bottom-left.
pub fn empty_level() -> LevelData {
    let mut rows = vec![".".repeat(10); 10];
    rows[9].replace_range(0..1, "P");
    LevelData::parse(&rows.join("\n")).unwrap()
}

/// Screen position at the centre of tile (tx, ty) with the camera at origin,
/// the inverse of `layout::screen_to_tile`.
pub fn click_at(tx: i32, ty: i32) -> (i32, i32) {
    let s = TILE_SIZE as i32;
    (tx * s + s / 2, TILE_AREA_TOP + ty * s + s / 2)
}
