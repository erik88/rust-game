//! The paintable tools (tiles, spawn, eraser, decorations) and how applying
//! one mutates the level. Painting exit doors keeps the level's `exit:`
//! routing reconciled so the file always stays loadable.

use rustgamex::level::{self, DecoLayer, Decoration, LevelData};
use rustgamex::player::{PLAYER_HEIGHT, PLAYER_WIDTH};
use rustgamex::tiles::{self, TILE_SIZE, TilePos};

use crate::layout::{HUD_TOP, TILE_AREA_TOP, cursor_tile};

/// A tool the user can paint with.
#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Erase,
    Spawn,
    Tile(u32),
}

impl Tool {
    pub fn name(self) -> String {
        match self {
            Tool::Erase => "Erase".to_string(),
            Tool::Spawn => "Spawn".to_string(),
            Tool::Tile(n) => format!("Tile {}", n),
        }
    }
}

/// Whether the cell at `tile` holds an exit door, normal or secret.
pub fn is_door(level: &LevelData, tile: TilePos) -> bool {
    matches!(
        level.tiles[tile.1][tile.0],
        tiles::EXIT | tiles::SECRET_EXIT
    )
}

/// Apply a tool to the tile under a screen position. Returns true if the level
/// changed. `default_dest` is the level id a freshly painted exit door is routed
/// to until the user reassigns it in Exit mode.
pub fn apply_tool(
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
pub fn reconcile_exit(level: &mut LevelData, tile: TilePos, default_dest: &str) {
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

/// Short human-readable name for a decoration layer (for console feedback).
pub fn layer_name(layer: DecoLayer) -> &'static str {
    match layer {
        DecoLayer::Background => "Background",
        DecoLayer::Foreground => "Foreground",
    }
}

/// Index of the decoration on `layer` snapped to the given tile cell, if any.
/// Decorations placed through the editor are grid-aligned, so a cell holds at
/// most one per layer (background and foreground are tracked independently).
fn deco_index_at(level: &LevelData, layer: DecoLayer, tile: TilePos) -> Option<usize> {
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
pub fn place_deco(
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
pub fn erase_deco(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{click_at, empty_level};

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
            vec![level::ExitDoor {
                tile: (3, 3),
                dest: "level02".to_string()
            }]
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
