//! Level data and the ASCII level file format.
//!
//! A level file consists of an optional header of `key: value` lines,
//! a blank line, and then the tile grid. Example:
//!
//! ```text
//! name: First Steps
//! block: 1,1 4,1 4,3 1,3 loop
//!
//! ..........
//! .P.....E..
//! 1111111111
//! ```
//!
//! A `block:` header defines a platform that moves along a path of control
//! points (`x,y` tile coordinates). Consecutive points must be strictly
//! horizontal or vertical neighbours. A trailing `loop` makes the path a closed
//! cycle; without it the block reverses direction at each end. There may be any
//! number of `block:` lines, one per moving block.
//!
//! A `deco:` header places a purely decorative sprite, e.g. `deco: 40,80 27`.
//! The position is in *pixel* (world) coordinates - not tile coordinates - so
//! decorations can be placed and (eventually) sized freely; the trailing number
//! is a 1-based index into the tilemap sprite sheet (see [`tiles::sheet_src_xy`]),
//! letting a level use any sprite. Decorations are render-only: they never affect
//! collision, coins, or the exit, which makes them suitable both for scenery and
//! for "hidden paths" (e.g. a wall-looking sprite over empty space). There may be
//! any number of `deco:` lines.
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

/// A platform that travels along a fixed path of control points. Defined by a
/// `block:` header line rather than a grid character, because the path is an
/// ordered sequence the flat tile grid cannot express.
#[derive(Clone, Debug, PartialEq)]
pub struct PathBlock {
    /// Control points in tile coordinates, in path order. Consecutive points
    /// (including the wrap from last to first when `closed`) are always strictly
    /// horizontal or vertical neighbours.
    pub points: Vec<(usize, usize)>,
    /// `true` for a closed loop; `false` for an open path the block bounces back
    /// and forth along.
    pub closed: bool,
}

/// A render-only sprite placed somewhere in the level. Decorations carry no
/// gameplay meaning at all - they are never consulted by collision, coins, or
/// the exit - which is what lets them serve both as scenery and as "hidden
/// paths" (a sprite drawn over otherwise-empty space, or omitted over solid
/// ground). Defined by a `deco:` header line.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoration {
    /// Position of the sprite's top-left corner, in pixel (world) coordinates.
    /// Stored in pixels rather than tile coordinates so placement (and, later,
    /// size) is not locked to the grid; the editor snaps to the grid by default.
    pub x: f32,
    pub y: f32,
    /// 1-based index into the tilemap sprite sheet (see [`tiles::sheet_src_xy`]).
    pub sprite: u32,
}

/// Parsed level, independent of how it was stored on disk.
#[derive(Clone, Debug)]
pub struct LevelData {
    pub name: String,
    /// Player spawn position in pixels (top-left corner of the player).
    pub spawn: (f32, f32),
    pub tiles: Vec<Vec<u32>>,
    /// Path-following moving blocks defined by `block:` header lines.
    pub path_blocks: Vec<PathBlock>,
    /// Render-only decorative sprites defined by `deco:` header lines.
    pub decorations: Vec<Decoration>,
}

impl LevelData {
    pub fn parse(text: &str) -> Result<LevelData, String> {
        let mut name = String::new();
        let mut path_blocks: Vec<PathBlock> = Vec::new();
        let mut decorations: Vec<Decoration> = Vec::new();
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
                        "block" => path_blocks.push(parse_path_block(value.trim())?),
                        "deco" => decorations.push(parse_decoration(value.trim())?),
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

        Ok(LevelData {
            name,
            spawn,
            tiles,
            path_blocks,
            decorations,
        })
    }

    /// Serialize the level back into the ASCII file format that [`parse`]
    /// accepts. Round-trips: `parse(level.to_text())` reproduces the level.
    ///
    /// [`parse`]: LevelData::parse
    pub fn to_text(&self) -> String {
        let (spawn_x, spawn_y) = self.spawn_tile();

        let mut out = String::new();
        let mut has_header = false;
        if !self.name.is_empty() {
            out.push_str("name: ");
            out.push_str(&self.name);
            out.push('\n');
            has_header = true;
        }
        for block in &self.path_blocks {
            // A block needs at least two points to be a path; skip any partial
            // block (e.g. one still being drawn in the editor) so the output
            // always parses back.
            if block.points.len() < 2 {
                continue;
            }
            out.push_str("block:");
            for (x, y) in &block.points {
                out.push_str(&format!(" {},{}", x, y));
            }
            if block.closed {
                out.push_str(" loop");
            }
            out.push('\n');
            has_header = true;
        }
        for deco in &self.decorations {
            out.push_str(&format!(
                "deco: {},{} {}\n",
                fmt_coord(deco.x),
                fmt_coord(deco.y),
                deco.sprite
            ));
            has_header = true;
        }
        // Blank line separating the header from the tile grid.
        if has_header {
            out.push('\n');
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
                self.shift_decorations(0.0, TILE_SIZE);
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
                self.shift_decorations(TILE_SIZE, 0.0);
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
                self.shift_decorations(0.0, -TILE_SIZE);
            }
            Edge::Bottom => {
                self.tiles.pop();
            }
            Edge::Left => {
                for row in &mut self.tiles {
                    row.remove(0);
                }
                self.spawn.0 -= TILE_SIZE;
                self.shift_decorations(-TILE_SIZE, 0.0);
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

    /// Offset every decoration by the given pixel delta. Used when growing or
    /// shrinking the top/left edge shifts the rest of the level's content.
    fn shift_decorations(&mut self, dx: f32, dy: f32) {
        for deco in &mut self.decorations {
            deco.x += dx;
            deco.y += dy;
        }
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

/// Parse the value of a `block:` header into a [`PathBlock`]. The value is a
/// whitespace-separated list of `x,y` control points, optionally ending with the
/// word `loop` to close the path. Consecutive points (and the closing wrap, when
/// looped) must be strictly horizontal or vertical neighbours.
fn parse_path_block(spec: &str) -> Result<PathBlock, String> {
    let mut tokens: Vec<&str> = spec.split_whitespace().collect();
    let closed = tokens.last() == Some(&"loop");
    if closed {
        tokens.pop();
    }
    if tokens.len() < 2 {
        return Err("block path needs at least two control points".to_string());
    }

    let mut points = Vec::with_capacity(tokens.len());
    for tok in tokens {
        let (xs, ys) = tok
            .split_once(',')
            .ok_or_else(|| format!("invalid control point '{}', expected 'x,y'", tok))?;
        let x = xs
            .parse::<usize>()
            .map_err(|_| format!("invalid x in control point '{}'", tok))?;
        let y = ys
            .parse::<usize>()
            .map_err(|_| format!("invalid y in control point '{}'", tok))?;
        points.push((x, y));
    }

    // Every segment (including the closing wrap) must move along exactly one
    // axis: same x or same y, but not both (which would be a zero-length
    // segment) and not neither (a diagonal).
    let n = points.len();
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments {
        let a = points[i];
        let b = points[(i + 1) % n];
        if (a.0 == b.0) == (a.1 == b.1) {
            return Err(format!(
                "control points {},{} and {},{} must be strictly horizontal or vertical neighbours",
                a.0, a.1, b.0, b.1
            ));
        }
    }

    Ok(PathBlock { points, closed })
}

/// Parse the value of a `deco:` header into a [`Decoration`]. The value is an
/// `x,y` pixel position followed by a 1-based sprite-sheet index, e.g.
/// `40,80 27`.
fn parse_decoration(spec: &str) -> Result<Decoration, String> {
    let mut tokens = spec.split_whitespace();
    let pos = tokens
        .next()
        .ok_or("decoration needs an 'x,y' position and a sprite index")?;
    let sprite_tok = tokens
        .next()
        .ok_or("decoration needs a sprite index after its position")?;
    if tokens.next().is_some() {
        return Err(format!(
            "decoration '{}' has too many fields, expected 'x,y index'",
            spec
        ));
    }

    let (xs, ys) = pos
        .split_once(',')
        .ok_or_else(|| format!("invalid decoration position '{}', expected 'x,y'", pos))?;
    let x = xs
        .parse::<f32>()
        .map_err(|_| format!("invalid x in decoration position '{}'", pos))?;
    let y = ys
        .parse::<f32>()
        .map_err(|_| format!("invalid y in decoration position '{}'", pos))?;
    let sprite = sprite_tok
        .parse::<u32>()
        .map_err(|_| format!("invalid sprite index '{}'", sprite_tok))?;
    if sprite < 1 {
        return Err("decoration sprite index must be 1 or greater".to_string());
    }

    Ok(Decoration { x, y, sprite })
}

/// Format a pixel coordinate for the level file, dropping the decimal point for
/// whole numbers (grid-snapped decorations) so output stays tidy and stable.
fn fmt_coord(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
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
        let original = LevelData::parse(
            "name: Round Trip\nblock: 1,1 4,1 4,3 1,3 loop\nblock: 2,5 2,7\ndeco: 40,80 27\ndeco: 12.5,0 3\n\n.P1345678\n^>v<E.1.E",
        )
        .unwrap();
        let reparsed = LevelData::parse(&original.to_text()).unwrap();

        assert_eq!(reparsed.name, original.name);
        assert_eq!(reparsed.spawn, original.spawn);
        assert_eq!(reparsed.tiles, original.tiles);
        assert_eq!(reparsed.path_blocks, original.path_blocks);
        assert_eq!(reparsed.decorations, original.decorations);
    }

    #[test]
    fn parses_decorations_with_pixel_coordinates() {
        let level = LevelData::parse("deco: 40,80 27\ndeco: 12.5,0 3\n\nP.\n11").unwrap();
        assert_eq!(
            level.decorations,
            vec![
                Decoration { x: 40.0, y: 80.0, sprite: 27 },
                Decoration { x: 12.5, y: 0.0, sprite: 3 },
            ]
        );
    }

    #[test]
    fn rejects_decoration_with_bad_sprite_index() {
        assert!(LevelData::parse("deco: 0,0 0\n\nP.\n11").is_err());
        assert!(LevelData::parse("deco: 0,0 x\n\nP.\n11").is_err());
        assert!(LevelData::parse("deco: 00 1\n\nP.\n11").is_err());
        assert!(LevelData::parse("deco: 0,0\n\nP.\n11").is_err());
    }

    #[test]
    fn grow_and_shrink_shift_decorations_with_content() {
        let mut level = LevelData::parse("deco: 40,40 5\n\n.P.\n111").unwrap();
        // Growing the top and left edges pushes content (and decorations) by one
        // tile down and right.
        level.grow(Edge::Top);
        level.grow(Edge::Left);
        assert_eq!(level.decorations[0], Decoration { x: 80.0, y: 80.0, sprite: 5 });
        // Shrinking those edges again restores the original placement.
        level.shrink(Edge::Top);
        level.shrink(Edge::Left);
        assert_eq!(level.decorations[0], Decoration { x: 40.0, y: 40.0, sprite: 5 });
    }

    #[test]
    fn parses_open_and_closed_path_blocks() {
        let level =
            LevelData::parse("block: 1,1 4,1 4,3 1,3 loop\nblock: 2,5 2,7\n\nP.\n11").unwrap();
        assert_eq!(level.path_blocks.len(), 2);
        assert_eq!(level.path_blocks[0].points, vec![(1, 1), (4, 1), (4, 3), (1, 3)]);
        assert!(level.path_blocks[0].closed);
        assert_eq!(level.path_blocks[1].points, vec![(2, 5), (2, 7)]);
        assert!(!level.path_blocks[1].closed);
    }

    #[test]
    fn rejects_diagonal_path_segment() {
        let err = LevelData::parse("block: 1,1 3,3\n\nP.\n11").unwrap_err();
        assert!(err.contains("horizontal or vertical"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_closing_wrap_that_is_not_axis_aligned() {
        // 1,1 -> 4,1 -> 4,3 are fine, but the wrap 4,3 -> 1,1 is diagonal.
        let err = LevelData::parse("block: 1,1 4,1 4,3 loop\n\nP.\n11").unwrap_err();
        assert!(err.contains("horizontal or vertical"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_zero_length_path_segment() {
        let err = LevelData::parse("block: 2,2 2,2\n\nP.\n11").unwrap_err();
        assert!(err.contains("horizontal or vertical"), "unexpected error: {}", err);
    }

    #[test]
    fn rejects_path_block_with_too_few_points() {
        let err = LevelData::parse("block: 2,2\n\nP.\n11").unwrap_err();
        assert!(err.contains("at least two"), "unexpected error: {}", err);
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
