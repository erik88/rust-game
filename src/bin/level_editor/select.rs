//! Region select: marking a rectangular block of tiles and moving or copying
//! it elsewhere in the grid, keeping exit routing consistent for any doors
//! that travel with the block.

use rustgamex::level::LevelData;
use rustgamex::tiles::{self, TilePos};

use crate::tools::reconcile_exit;

/// A rectangular block of tile cells selected in select mode, as inclusive
/// tile bounds: `min` is the top-left corner, `max` the bottom-right.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Selection {
    pub min: TilePos,
    pub max: TilePos,
}

impl Selection {
    /// The rectangle spanning two (possibly unordered) corner cells.
    pub fn from_corners(a: TilePos, b: TilePos) -> Selection {
        Selection {
            min: (a.0.min(b.0), a.1.min(b.1)),
            max: (a.0.max(b.0), a.1.max(b.1)),
        }
    }

    /// A `w`x`h` selection with its top-left at `min`.
    pub fn at(min: TilePos, w: usize, h: usize) -> Selection {
        Selection {
            min,
            max: (min.0 + w - 1, min.1 + h - 1),
        }
    }

    pub fn width(&self) -> usize {
        self.max.0 - self.min.0 + 1
    }

    pub fn height(&self) -> usize {
        self.max.1 - self.min.1 + 1
    }

    pub fn contains(&self, t: TilePos) -> bool {
        (self.min.0..=self.max.0).contains(&t.0) && (self.min.1..=self.max.1).contains(&t.1)
    }
}

/// The in-progress pointer interaction while in select mode.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SelectDrag {
    /// Rubber-band selecting: `anchor` is the fixed corner; the moving corner is
    /// the cell under the cursor.
    Marquee { anchor: TilePos },
    /// Moving (or, when `copy`, copying) the current selection. `grab` is the
    /// offset from the selection's top-left to the grabbed cell, so the block
    /// tracks the cursor.
    Move { grab: TilePos, copy: bool },
}

/// Where a selection's top-left should land when its `grab` cell follows the
/// cursor, clamped so the whole block stays inside a `width`x`height` grid.
pub fn clamp_move_target(
    width: usize,
    height: usize,
    sel: Selection,
    grab: TilePos,
    cursor: (i32, i32),
) -> TilePos {
    let max_x = (width as i32 - sel.width() as i32).max(0);
    let max_y = (height as i32 - sel.height() as i32).max(0);
    let tx = (cursor.0 - grab.0 as i32).clamp(0, max_x);
    let ty = (cursor.1 - grab.1 as i32).clamp(0, max_y);
    (tx as usize, ty as usize)
}

/// Move (or, when `copy`, copy) the block of tiles in `sel` so its top-left lands
/// at `target`. Both `sel` and `target` must lie fully inside the grid. Exit
/// routing for every touched cell is reconciled afterwards so the level stays
/// loadable; a moved door keeps its cell's routing only if it does not leave that
/// cell, otherwise it is re-routed to `default_dest`.
pub fn move_selection(
    level: &mut LevelData,
    sel: Selection,
    target: TilePos,
    copy: bool,
    default_dest: &str,
) {
    let (w, h) = (sel.width(), sel.height());
    // Snapshot the block first: source and target may overlap.
    let mut cells = vec![vec![tiles::EMPTY; w]; h];
    for (r, row) in cells.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = level.tiles[sel.min.1 + r][sel.min.0 + c];
        }
    }

    let mut touched: Vec<TilePos> = Vec::new();
    if !copy {
        for r in 0..h {
            for c in 0..w {
                let cell = (sel.min.0 + c, sel.min.1 + r);
                level.tiles[cell.1][cell.0] = tiles::EMPTY;
                touched.push(cell);
            }
        }
    }
    for (r, row) in cells.iter().enumerate() {
        for (c, &val) in row.iter().enumerate() {
            let cell = (target.0 + c, target.1 + r);
            level.tiles[cell.1][cell.0] = val;
            touched.push(cell);
        }
    }

    touched.sort_unstable();
    touched.dedup();
    for cell in touched {
        reconcile_exit(level, cell, default_dest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{click_at, empty_level};
    use crate::tools::{Tool, apply_tool};
    use rustgamex::level;

    #[test]
    fn selection_from_corners_normalises_and_measures() {
        let sel = Selection::from_corners((5, 6), (2, 3));
        assert_eq!(sel.min, (2, 3));
        assert_eq!(sel.max, (5, 6));
        assert_eq!((sel.width(), sel.height()), (4, 4));
        assert!(sel.contains((3, 4)));
        assert!(sel.contains((2, 3)));
        assert!(sel.contains((5, 6)));
        assert!(!sel.contains((6, 6)));
    }

    #[test]
    fn clamp_move_target_keeps_the_block_inside_the_grid() {
        let sel = Selection::at((0, 0), 3, 2); // 3x2 block, grab its top-left
        // Cursor at (8,9) in a 10x10 grid: a 3x2 block can start at most at (7,8).
        assert_eq!(clamp_move_target(10, 10, sel, (0, 0), (8, 9)), (7, 8));
        // Negative reach clamps to the origin.
        assert_eq!(clamp_move_target(10, 10, sel, (0, 0), (-4, -4)), (0, 0));
        // A grab offset shifts where the top-left lands.
        assert_eq!(clamp_move_target(10, 10, sel, (1, 1), (5, 5)), (4, 4));
    }

    #[test]
    fn move_selection_relocates_the_block_and_clears_the_source() {
        let mut level = empty_level();
        level.tiles[1][1] = tiles::SOLID;
        level.tiles[1][2] = tiles::DEATH;

        let sel = Selection::at((1, 1), 2, 1);
        move_selection(&mut level, sel, (5, 5), false, "level02");

        // Source cleared, block stamped at the destination.
        assert_eq!(level.tiles[1][1], tiles::EMPTY);
        assert_eq!(level.tiles[1][2], tiles::EMPTY);
        assert_eq!(level.tiles[5][5], tiles::SOLID);
        assert_eq!(level.tiles[5][6], tiles::DEATH);
    }

    #[test]
    fn move_selection_copy_leaves_the_source_intact() {
        let mut level = empty_level();
        level.tiles[2][2] = tiles::SOLID;

        let sel = Selection::at((2, 2), 1, 1);
        move_selection(&mut level, sel, (4, 4), true, "level02");

        assert_eq!(level.tiles[2][2], tiles::SOLID);
        assert_eq!(level.tiles[4][4], tiles::SOLID);
    }

    #[test]
    fn move_selection_handles_overlapping_source_and_target() {
        let mut level = empty_level();
        level.tiles[0][0] = tiles::SOLID;
        level.tiles[0][1] = tiles::DEATH;

        // Shift a 2x1 block one cell right; the target overlaps the source.
        let sel = Selection::at((0, 0), 2, 1);
        move_selection(&mut level, sel, (1, 0), false, "level02");

        assert_eq!(level.tiles[0][0], tiles::EMPTY);
        assert_eq!(level.tiles[0][1], tiles::SOLID);
        assert_eq!(level.tiles[0][2], tiles::DEATH);
    }

    #[test]
    fn move_selection_reconciles_exits_for_moved_doors() {
        let mut level = empty_level();
        // A routed door at (3,3), as apply_tool would leave it.
        assert!(apply_tool(
            &mut level,
            Tool::Tile(tiles::EXIT),
            "level02",
            click_at(3, 3).0,
            click_at(3, 3).1,
            0.0,
            0.0
        ));

        let sel = Selection::at((3, 3), 1, 1);
        move_selection(&mut level, sel, (6, 6), false, "level09");

        // The exit follows the door to its new cell, re-routed to the default.
        assert_eq!(
            level.exits,
            vec![level::ExitDoor {
                tile: (6, 6),
                dest: "level09".to_string()
            }]
        );
    }
}
