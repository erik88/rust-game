//! Level data and the ASCII level file format.
//!
//! A level file consists of an optional header of `key: value` lines,
//! a blank line, and then the tile grid. Example:
//!
//! ```text
//! name: First Steps
//!
//! ..........
//! .P.....E..
//! 1111111111
//! ```
//!
//! Grid characters:
//!
//! | Char    | Tile                                  |
//! |---------|---------------------------------------|
//! | `.`     | 0 - empty space                       |
//! | `1`-`8` | tile types 1-8 (solid/death/crumbling/periodic) |
//! | `^`     | 9 - moving tile, goes up              |
//! | `>`     | 10 - moving tile, goes right          |
//! | `v`     | 11 - moving tile, goes down           |
//! | `<`     | 12 - moving tile, goes left           |
//! | `E`     | 13 - exit tile                        |
//! | `C`     | 14 - coin (collect all to open exits) |
//! | `P`     | player spawn point (empty space)      |

use crate::player::{PLAYER_HEIGHT, PLAYER_WIDTH};
use crate::tiles::{self, TILE_SIZE};

/// Parsed level, independent of how it was stored on disk.
#[derive(Clone, Debug)]
pub struct LevelData {
    pub name: String,
    /// Player spawn position in pixels (top-left corner of the player).
    pub spawn: (f32, f32),
    pub tiles: Vec<Vec<u32>>,
}

impl LevelData {
    pub fn parse(text: &str) -> Result<LevelData, String> {
        let mut name = String::new();
        let mut grid_lines: Vec<&str> = Vec::new();
        let mut in_grid = false;

        for line in text.lines() {
            if !in_grid {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some((key, value)) = trimmed.split_once(':') {
                    match key.trim() {
                        "name" => name = value.trim().to_string(),
                        other => return Err(format!("unknown header key '{}'", other)),
                    }
                    continue;
                }
                in_grid = true;
            }
            grid_lines.push(line.trim_end());
        }

        // Drop trailing empty lines
        while grid_lines.last().is_some_and(|l| l.is_empty()) {
            grid_lines.pop();
        }
        if grid_lines.is_empty() {
            return Err("level has no tile grid".to_string());
        }

        let width = grid_lines.iter().map(|l| l.chars().count()).max().unwrap();
        let mut tiles = Vec::with_capacity(grid_lines.len());
        let mut spawn: Option<(f32, f32)> = None;

        for (y, line) in grid_lines.iter().enumerate() {
            let mut row = Vec::with_capacity(width);
            for (x, ch) in line.chars().enumerate() {
                let tile = match ch {
                    '.' | ' ' | '0' => tiles::EMPTY,
                    // Tile id 2 is unused, so '2' is not a valid tile character.
                    '1' | '3'..='8' => ch.to_digit(10).unwrap(),
                    '^' => tiles::MOVE_UP,
                    '>' => tiles::MOVE_RIGHT,
                    'v' => tiles::MOVE_DOWN,
                    '<' => tiles::MOVE_LEFT,
                    'E' => tiles::EXIT,
                    'C' => tiles::COIN,
                    'P' => {
                        if spawn.is_some() {
                            return Err("level has more than one spawn point 'P'".to_string());
                        }
                        // Center the player horizontally in the tile, feet on its floor
                        spawn = Some((
                            x as f32 * TILE_SIZE + (TILE_SIZE - PLAYER_WIDTH as f32) / 2.0,
                            y as f32 * TILE_SIZE + TILE_SIZE - PLAYER_HEIGHT as f32,
                        ));
                        tiles::EMPTY
                    }
                    other => {
                        return Err(format!(
                            "unknown tile character '{}' at line {}, column {}",
                            other,
                            y + 1,
                            x + 1
                        ));
                    }
                };
                row.push(tile);
            }
            // Pad short rows with empty space
            row.resize(width, 0);
            tiles.push(row);
        }

        let spawn = spawn.ok_or("level has no spawn point 'P'")?;

        Ok(LevelData { name, spawn, tiles })
    }

    /// Serialize the level back into the ASCII file format that [`parse`]
    /// accepts. Round-trips: `parse(level.to_text())` reproduces the level.
    ///
    /// [`parse`]: LevelData::parse
    pub fn to_text(&self) -> String {
        let (spawn_x, spawn_y) = self.spawn_tile();

        let mut out = String::new();
        if !self.name.is_empty() {
            out.push_str("name: ");
            out.push_str(&self.name);
            out.push_str("\n\n");
        }
        for (y, row) in self.tiles.iter().enumerate() {
            for (x, &tile) in row.iter().enumerate() {
                let ch = if (x, y) == (spawn_x, spawn_y) {
                    'P'
                } else {
                    tile_to_char(tile)
                };
                out.push(ch);
            }
            out.push('\n');
        }
        out
    }

    /// The spawn point expressed in tile coordinates (the inverse of the pixel
    /// placement [`parse`] computes for a `P`).
    ///
    /// [`parse`]: LevelData::parse
    pub fn spawn_tile(&self) -> (usize, usize) {
        let x = (self.spawn.0 - (TILE_SIZE - PLAYER_WIDTH as f32) / 2.0) / TILE_SIZE;
        let y = (self.spawn.1 - (TILE_SIZE - PLAYER_HEIGHT as f32)) / TILE_SIZE;
        (x.round().max(0.0) as usize, y.round().max(0.0) as usize)
    }

    /// Grid width in tiles.
    pub fn width(&self) -> usize {
        self.tiles.first().map_or(0, |row| row.len())
    }

    /// Grid height in tiles.
    pub fn height(&self) -> usize {
        self.tiles.len()
    }

    /// Add a row or column of empty tiles along the given edge. Inserting at the
    /// top or left shifts the existing content (and the spawn) to keep it put.
    pub fn grow(&mut self, edge: Edge) {
        match edge {
            Edge::Top => {
                let width = self.width();
                self.tiles.insert(0, vec![tiles::EMPTY; width]);
                self.spawn.1 += TILE_SIZE;
            }
            Edge::Bottom => {
                let width = self.width();
                self.tiles.push(vec![tiles::EMPTY; width]);
            }
            Edge::Left => {
                for row in &mut self.tiles {
                    row.insert(0, tiles::EMPTY);
                }
                self.spawn.0 += TILE_SIZE;
            }
            Edge::Right => {
                for row in &mut self.tiles {
                    row.push(tiles::EMPTY);
                }
            }
        }
    }

    /// Remove the row or column along the given edge. Returns false (leaving the
    /// level untouched) when it would shrink the grid below 1x1. The spawn is
    /// clamped back inside the grid if its row or column is removed.
    pub fn shrink(&mut self, edge: Edge) -> bool {
        match edge {
            Edge::Top | Edge::Bottom if self.height() <= 1 => return false,
            Edge::Left | Edge::Right if self.width() <= 1 => return false,
            _ => {}
        }
        match edge {
            Edge::Top => {
                self.tiles.remove(0);
                self.spawn.1 -= TILE_SIZE;
            }
            Edge::Bottom => {
                self.tiles.pop();
            }
            Edge::Left => {
                for row in &mut self.tiles {
                    row.remove(0);
                }
                self.spawn.0 -= TILE_SIZE;
            }
            Edge::Right => {
                for row in &mut self.tiles {
                    row.pop();
                }
            }
        }
        self.clamp_spawn();
        true
    }

    /// Snap the spawn back inside the current grid bounds.
    fn clamp_spawn(&mut self) {
        let (sx, sy) = self.spawn_tile();
        let x = sx.min(self.width().saturating_sub(1));
        let y = sy.min(self.height().saturating_sub(1));
        self.spawn = (
            x as f32 * TILE_SIZE + (TILE_SIZE - PLAYER_WIDTH as f32) / 2.0,
            y as f32 * TILE_SIZE + TILE_SIZE - PLAYER_HEIGHT as f32,
        );
    }
}

/// One edge of the level grid, used by [`LevelData::grow`] and
/// [`LevelData::shrink`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Map a tile code back to its level-file character (inverse of the match in
/// [`LevelData::parse`]).
fn tile_to_char(tile: u32) -> char {
    match tile {
        tiles::MOVE_UP => '^',
        tiles::MOVE_RIGHT => '>',
        tiles::MOVE_DOWN => 'v',
        tiles::MOVE_LEFT => '<',
        tiles::EXIT => 'E',
        tiles::COIN => 'C',
        n @ (1 | 3..=8) => char::from_digit(n, 10).unwrap(),
        _ => '.', // EMPTY and anything unexpected
    }
}

/// Load and parse every `.txt` level file in a directory, in filename order.
/// Errors are prefixed with the offending file path.
pub fn load_dir(dir: &str) -> Result<Vec<LevelData>, String> {
    Ok(load_dir_entries(dir)?
        .into_iter()
        .map(|(_, level)| level)
        .collect())
}

/// Like [`load_dir`], but keeps each level's source path - editors need it to
/// write changes back to the right file.
pub fn load_dir_entries(dir: &str) -> Result<Vec<(std::path::PathBuf, LevelData)>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("failed to read levels directory '{}': {}", dir, e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "txt"))
        .collect();
    paths.sort();

    let mut levels = Vec::new();
    for path in paths {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let level = LevelData::parse(&text).map_err(|e| format!("{}: {}", path.display(), e))?;
        levels.push((path, level));
    }
    Ok(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grid_with_all_tile_characters() {
        let level = LevelData::parse("P.1345678\n^>v<E....").unwrap();
        assert_eq!(level.tiles[0], vec![0, 0, 1, 3, 4, 5, 6, 7, 8]);
        assert_eq!(level.tiles[1], vec![9, 10, 11, 12, 13, 0, 0, 0, 0]);
    }

    #[test]
    fn parses_header_and_spawn() {
        let level = LevelData::parse("name: My Level\n\n...\n.P.\n111").unwrap();
        assert_eq!(level.name, "My Level");
        // P is at tile (1, 1): centered horizontally, feet on the tile floor
        assert_eq!(level.spawn, (52.0, 42.0));
        // The spawn tile itself is empty space
        assert_eq!(level.tiles[1][1], 0);
    }

    #[test]
    fn pads_short_rows_to_uniform_width() {
        let level = LevelData::parse("P....\n11").unwrap();
        assert_eq!(level.tiles[1], vec![1, 1, 0, 0, 0]);
    }

    #[test]
    fn header_is_optional() {
        let level = LevelData::parse("P.E\n111").unwrap();
        assert_eq!(level.name, "");
    }

    #[test]
    fn rejects_unknown_character() {
        let err = LevelData::parse("P?\n11").unwrap_err();
        assert!(err.contains("'?'"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_missing_spawn() {
        let err = LevelData::parse("...\n111").unwrap_err();
        assert!(err.contains("spawn"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_duplicate_spawn() {
        let err = LevelData::parse("P.P\n111").unwrap_err();
        assert!(
            err.contains("more than one spawn"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn rejects_empty_input() {
        assert!(LevelData::parse("").is_err());
        assert!(LevelData::parse("name: x\n\n").is_err());
    }

    #[test]
    fn to_text_round_trips_through_parse() {
        let original = LevelData::parse("name: Round Trip\n\n.P1345678\n^>v<E.1.E").unwrap();
        let reparsed = LevelData::parse(&original.to_text()).unwrap();

        assert_eq!(reparsed.name, original.name);
        assert_eq!(reparsed.spawn, original.spawn);
        assert_eq!(reparsed.tiles, original.tiles);
    }

    #[test]
    fn spawn_tile_inverts_pixel_placement() {
        let level = LevelData::parse("name: x\n\n...\n.P.\n111").unwrap();
        assert_eq!(level.spawn_tile(), (1, 1));
    }

    #[test]
    fn grow_right_and_bottom_extends_without_moving_content() {
        let mut level = LevelData::parse(".P.\n111").unwrap();
        level.grow(Edge::Right);
        level.grow(Edge::Bottom);

        assert_eq!(level.width(), 4);
        assert_eq!(level.height(), 3);
        assert_eq!(level.spawn_tile(), (1, 0), "spawn should not move");
        assert_eq!(level.tiles[0], vec![0, 0, 0, 0]);
        assert_eq!(level.tiles[1], vec![1, 1, 1, 0]);
        assert_eq!(level.tiles[2], vec![0, 0, 0, 0], "new empty bottom row");
    }

    #[test]
    fn grow_top_and_left_shifts_content_and_spawn() {
        let mut level = LevelData::parse(".P.\n111").unwrap();
        level.grow(Edge::Top);
        level.grow(Edge::Left);

        assert_eq!(level.width(), 4);
        assert_eq!(level.height(), 3);
        // Spawn was at (1,0); a top row and a left column push it to (2,1)
        assert_eq!(level.spawn_tile(), (2, 1));
        assert_eq!(level.tiles[0], vec![0, 0, 0, 0], "new empty top row");
        assert_eq!(level.tiles[2], vec![0, 1, 1, 1], "ground shifted right");
    }

    #[test]
    fn shrink_clamps_spawn_and_refuses_below_minimum() {
        let mut level = LevelData::parse("..\nP1").unwrap();
        // Spawn sits in the bottom-left; removing the bottom row must pull it up
        assert_eq!(level.spawn_tile(), (0, 1));
        assert!(level.shrink(Edge::Bottom));
        assert_eq!(level.height(), 1);
        assert_eq!(level.spawn_tile(), (0, 0), "spawn clamped into remaining row");

        // Now 2 wide, 1 tall: the last row can't be removed
        assert!(!level.shrink(Edge::Bottom));
        assert!(level.shrink(Edge::Right));
        assert!(!level.shrink(Edge::Right), "1x1 grid cannot shrink further");
        assert_eq!(level.width(), 1);
        assert_eq!(level.height(), 1);
    }
}
