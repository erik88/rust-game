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
                    '1'..='8' => ch.to_digit(10).unwrap(),
                    '^' => tiles::MOVE_UP,
                    '>' => tiles::MOVE_RIGHT,
                    'v' => tiles::MOVE_DOWN,
                    '<' => tiles::MOVE_LEFT,
                    'E' => tiles::EXIT,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grid_with_all_tile_characters() {
        let level = LevelData::parse("P.12345678\n^>v<E.....").unwrap();
        assert_eq!(level.tiles[0], vec![0, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(level.tiles[1], vec![9, 10, 11, 12, 13, 0, 0, 0, 0, 0]);
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
}
