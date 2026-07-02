//! Path-block editing: creating, extending, dragging and deleting the control
//! points of the moving blocks declared with `block:` header lines. Every
//! operation keeps the path valid by construction (all segments strictly
//! horizontal or vertical), matching the rule the level parser enforces.

use rustgamex::level::{LevelData, PathBlock};
use rustgamex::tiles::TilePos;

use crate::layout::cursor_tile;

/// What the cursor is currently dragging within the active path block.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drag {
    /// A single control point (only open-path endpoints are draggable this way).
    Point(usize),
    /// A whole edge `points[i] -> points[(i+1)%n]`, moved perpendicular to itself
    /// so it carries both endpoints and keeps the path orthogonal.
    Segment(usize),
}

/// Snap `to` so it lies strictly horizontal or vertical from `from`, by keeping
/// whichever axis moves the most and locking the other to `from`.
pub fn snap_axis(from: TilePos, to: TilePos) -> TilePos {
    let dx = (to.0 as i32 - from.0 as i32).abs();
    let dy = (to.1 as i32 - from.1 as i32).abs();
    if dx >= dy {
        (to.0, from.1) // horizontal move
    } else {
        (from.0, to.1) // vertical move
    }
}

/// True if `a` and `b` are strictly horizontal or vertical neighbours (share
/// exactly one coordinate), matching the rule the level parser enforces.
pub fn axis_aligned(a: TilePos, b: TilePos) -> bool {
    (a.0 == b.0) != (a.1 == b.1)
}

/// Find the (block index, point index) of a control point sitting on the given
/// tile, if any.
fn point_at_tile(level: &LevelData, tile: TilePos) -> Option<(usize, usize)> {
    for (b, block) in level.path_blocks.iter().enumerate() {
        if let Some(p) = block.points.iter().position(|&pt| pt == tile) {
            return Some((b, p));
        }
    }
    None
}

/// True if `tile` lies strictly between the endpoints of the axis-aligned edge
/// `a`-`b` (endpoints excluded - those are handled as control points).
fn on_segment(a: TilePos, b: TilePos, tile: TilePos) -> bool {
    if a.1 == b.1 && tile.1 == a.1 {
        return tile.0 > a.0.min(b.0) && tile.0 < a.0.max(b.0);
    }
    if a.0 == b.0 && tile.0 == a.0 {
        return tile.1 > a.1.min(b.1) && tile.1 < a.1.max(b.1);
    }
    false
}

/// Find the (block index, segment index) of an edge passing through `tile`. The
/// segment index `i` refers to the edge `points[i] -> points[(i+1) % n]`.
pub fn segment_at_tile(level: &LevelData, tile: TilePos) -> Option<(usize, usize)> {
    for (b, block) in level.path_blocks.iter().enumerate() {
        let n = block.points.len();
        if n < 2 {
            continue;
        }
        let segments = if block.closed { n } else { n - 1 };
        for i in 0..segments {
            if on_segment(block.points[i], block.points[(i + 1) % n], tile) {
                return Some((b, i));
            }
        }
    }
    None
}

/// Whether every edge of the block (including the closing wrap when looped) is a
/// valid strictly horizontal/vertical segment.
pub fn block_is_valid(block: &PathBlock) -> bool {
    let n = block.points.len();
    if n < 2 {
        return false;
    }
    let segments = if block.closed { n } else { n - 1 };
    (0..segments).all(|i| axis_aligned(block.points[i], block.points[(i + 1) % n]))
}

/// Handle a left-click while in path mode: select/drag an existing point, or
/// append a new one to (or start) the active block. Returns whether the level
/// changed.
#[allow(clippy::too_many_arguments)]
pub fn path_left_click(
    level: &mut LevelData,
    active_block: &mut Option<usize>,
    dragging: &mut Option<Drag>,
    start_new: &mut bool,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };

    // Clicking an existing point selects its block; an open path's endpoint can
    // then be dragged.
    if let Some((b, p)) = point_at_tile(level, tile) {
        *active_block = Some(b);
        let block = &level.path_blocks[b];
        let is_endpoint = !block.closed && (p == 0 || p == block.points.len() - 1);
        *dragging = is_endpoint.then_some(Drag::Point(p));
        return false;
    }

    // Clicking on an edge (between two points) selects its block and starts
    // dragging that whole edge.
    if let Some((b, s)) = segment_at_tile(level, tile) {
        *active_block = Some(b);
        *dragging = Some(Drag::Segment(s));
        return false;
    }

    // Start a new block when asked, or when there is none active yet.
    if *start_new || active_block.is_none() {
        level.path_blocks.push(PathBlock {
            points: vec![tile],
            closed: false,
        });
        *active_block = Some(level.path_blocks.len() - 1);
        *start_new = false;
        return true;
    }

    // Otherwise append a snapped point to the active (open) block.
    let block = &mut level.path_blocks[active_block.unwrap()];
    if block.closed {
        return false; // press L to open the loop before extending it
    }
    let last = *block.points.last().unwrap();
    let next = snap_axis(last, tile);
    if next == last {
        return false;
    }
    block.points.push(next);
    true
}

/// Handle a right-click while in path mode: delete the control point under the
/// cursor when doing so keeps the path valid. Returns whether the level changed.
pub fn path_right_click(
    level: &mut LevelData,
    active_block: &mut Option<usize>,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    let Some((b, p)) = point_at_tile(level, tile) else {
        return false;
    };

    let block = &mut level.path_blocks[b];
    let n = block.points.len();
    // Removing a point is allowed when it is an open endpoint, or when its two
    // neighbours stay axis-aligned once it is gone (a redundant point on a
    // straight run). This keeps every path valid by construction.
    let removable = if block.closed {
        n > 2 && axis_aligned(block.points[(p + n - 1) % n], block.points[(p + 1) % n])
    } else if p == 0 || p == n - 1 {
        true
    } else {
        axis_aligned(block.points[p - 1], block.points[p + 1])
    };
    if !removable {
        return false;
    }

    block.points.remove(p);
    if block.points.len() < 2 {
        level.path_blocks.remove(b);
        *active_block = None;
    } else {
        *active_block = Some(b);
    }
    true
}

/// Move whatever is being dragged (a point or a whole edge) to follow the
/// cursor, keeping the path orthogonal. Returns whether the level changed.
pub fn drag_path(
    level: &mut LevelData,
    active_block: Option<usize>,
    dragging: Option<Drag>,
    screen_x: i32,
    screen_y: i32,
    camera_x: f32,
    camera_y: f32,
) -> bool {
    let (Some(b), Some(drag)) = (active_block, dragging) else {
        return false;
    };
    let Some(tile) = cursor_tile(level, screen_x, screen_y, camera_x, camera_y) else {
        return false;
    };
    let block = &mut level.path_blocks[b];
    match drag {
        Drag::Point(p) => {
            // Endpoints have a single neighbour; snap to keep that segment valid.
            let neighbor = if p == 0 {
                block.points.get(1).copied()
            } else {
                block.points.get(p - 1).copied()
            };
            let new_pt = neighbor.map_or(tile, |n| snap_axis(n, tile));
            if block.points[p] == new_pt {
                return false;
            }
            block.points[p] = new_pt;
            true
        }
        Drag::Segment(s) => drag_segment(block, s, tile),
    }
}

/// Slide edge `s` perpendicular to itself onto `tile`, carrying both its
/// endpoints. The move is reverted if it would collapse an adjacent edge to zero
/// length (which would break the path). Returns whether the block changed.
pub fn drag_segment(block: &mut PathBlock, s: usize, tile: TilePos) -> bool {
    let n = block.points.len();
    let j = (s + 1) % n;
    let (a, c) = (block.points[s], block.points[j]);

    let (new_a, new_c) = if a.1 == c.1 {
        // Horizontal edge: move both endpoints' row to the cursor's row.
        if tile.1 == a.1 {
            return false;
        }
        ((a.0, tile.1), (c.0, tile.1))
    } else if a.0 == c.0 {
        // Vertical edge: move both endpoints' column to the cursor's column.
        if tile.0 == a.0 {
            return false;
        }
        ((tile.0, a.1), (tile.0, c.1))
    } else {
        return false;
    };

    block.points[s] = new_a;
    block.points[j] = new_c;
    if !block_is_valid(block) {
        block.points[s] = a;
        block.points[j] = c;
        return false;
    }
    true
}

/// Toggle the active block between an open path and a closed loop. Closing
/// requires at least three points and an axis-aligned wrap from the last point
/// back to the first. Returns whether anything changed.
pub fn toggle_loop(level: &mut LevelData, active_block: Option<usize>) -> bool {
    let Some(b) = active_block else {
        return false;
    };
    let block = &mut level.path_blocks[b];
    if block.closed {
        block.closed = false;
        return true;
    }
    let first = block.points[0];
    let last = *block.points.last().unwrap();
    if block.points.len() >= 3 && axis_aligned(last, first) {
        block.closed = true;
        return true;
    }
    println!("Can't close this path: need 3+ points and an aligned closing segment");
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{click_at, empty_level};

    #[test]
    fn snap_axis_locks_to_dominant_axis() {
        assert_eq!(snap_axis((2, 2), (6, 3)), (6, 2)); // horizontal wins
        assert_eq!(snap_axis((2, 2), (3, 6)), (2, 6)); // vertical wins
    }

    #[test]
    fn axis_aligned_detects_orthogonal_neighbours() {
        assert!(axis_aligned((1, 1), (1, 5)));
        assert!(axis_aligned((1, 1), (4, 1)));
        assert!(!axis_aligned((1, 1), (2, 2)));
        assert!(!axis_aligned((1, 1), (1, 1)));
    }

    #[test]
    fn left_click_builds_an_axis_aligned_path() {
        let mut level = empty_level();
        let (mut active, mut drag, mut new) = (None, None, false);

        let (x, y) = click_at(1, 1);
        assert!(path_left_click(
            &mut level, &mut active, &mut drag, &mut new, x, y, 0.0, 0.0
        ));
        // A diagonal target snaps to a horizontal segment from the last point.
        let (x, y) = click_at(4, 3);
        assert!(path_left_click(
            &mut level, &mut active, &mut drag, &mut new, x, y, 0.0, 0.0
        ));

        assert_eq!(level.path_blocks[0].points, vec![(1, 1), (4, 1)]);
        assert_eq!(active, Some(0));
    }

    #[test]
    fn toggle_loop_requires_a_valid_closing_segment() {
        let mut level = empty_level();
        // Open L-shape: closing (4,4) -> (1,1) would be diagonal, so it can't close.
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4)],
            closed: false,
        });
        assert!(!toggle_loop(&mut level, Some(0)));
        assert!(!level.path_blocks[0].closed);

        // A rectangle closes cleanly.
        level.path_blocks[0].points = vec![(1, 1), (4, 1), (4, 4), (1, 4)];
        assert!(toggle_loop(&mut level, Some(0)));
        assert!(level.path_blocks[0].closed);
    }

    #[test]
    fn right_click_deletes_endpoint_and_drops_degenerate_block() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (4, 1)],
            closed: false,
        });
        let mut active = Some(0);

        let (x, y) = click_at(4, 1);
        assert!(path_right_click(&mut level, &mut active, x, y, 0.0, 0.0));
        // Down to one point, so the whole block is removed.
        assert!(level.path_blocks.is_empty());
        assert_eq!(active, None);
    }

    #[test]
    fn right_click_keeps_corner_points_of_a_loop() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4), (1, 4)],
            closed: true,
        });
        let mut active = Some(0);
        // Deleting a corner would leave a diagonal join, so it is refused.
        let (x, y) = click_at(4, 1);
        assert!(!path_right_click(&mut level, &mut active, x, y, 0.0, 0.0));
        assert_eq!(level.path_blocks[0].points.len(), 4);
    }

    #[test]
    fn segment_at_tile_finds_edges_not_endpoints() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (5, 1)],
            closed: false,
        });
        // A tile between the endpoints hits the edge...
        assert_eq!(segment_at_tile(&level, (3, 1)), Some((0, 0)));
        // ...but the endpoints themselves and off-line tiles do not.
        assert_eq!(segment_at_tile(&level, (1, 1)), None);
        assert_eq!(segment_at_tile(&level, (3, 2)), None);
    }

    #[test]
    fn drag_segment_slides_a_horizontal_edge_keeping_neighbours_valid() {
        // A square loop; dragging the top edge down carries both its corners.
        let mut block = PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4), (1, 4)],
            closed: true,
        };
        assert!(drag_segment(&mut block, 0, (99, 2)));
        assert_eq!(block.points, vec![(1, 2), (4, 2), (4, 4), (1, 4)]);
        assert!(block_is_valid(&block));
    }

    #[test]
    fn drag_segment_slides_a_vertical_edge() {
        let mut block = PathBlock {
            points: vec![(1, 1), (1, 5)],
            closed: false,
        };
        // Vertical edge: only the column moves; both endpoints follow.
        assert!(drag_segment(&mut block, 0, (3, 99)));
        assert_eq!(block.points, vec![(3, 1), (3, 5)]);
    }

    #[test]
    fn drag_segment_reverts_when_it_would_collapse_a_neighbour() {
        // Open L-shape: dragging the top edge down onto row 4 would make its
        // right corner coincide with the next point, so the move is refused.
        let mut block = PathBlock {
            points: vec![(1, 1), (4, 1), (4, 4)],
            closed: false,
        };
        assert!(!drag_segment(&mut block, 0, (99, 4)));
        assert_eq!(block.points, vec![(1, 1), (4, 1), (4, 4)]);
    }

    #[test]
    fn left_click_on_edge_starts_a_segment_drag() {
        let mut level = empty_level();
        level.path_blocks.push(PathBlock {
            points: vec![(1, 1), (5, 1)],
            closed: false,
        });
        let (mut active, mut drag, mut new) = (None, None, false);
        let (x, y) = click_at(3, 1); // a tile on the edge
        assert!(!path_left_click(
            &mut level, &mut active, &mut drag, &mut new, x, y, 0.0, 0.0
        ));
        assert_eq!(active, Some(0));
        assert_eq!(drag, Some(Drag::Segment(0)));
    }
}
