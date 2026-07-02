//! All the editor's drawing: the top toolbar and its buttons, the bottom HUD
//! (tool palette / path menu / exit menu), the grid and hover overlays, the
//! path/decoration/exit/selection overlays, and the level-browser panel.

use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

use rustgamex::font;
use rustgamex::level::{DecoLayer, LevelData};
use rustgamex::player::{PLAYER_HEIGHT, PLAYER_WIDTH};
use rustgamex::tilemap::TileMap;
use rustgamex::tiles::{self, TILE_SIZE, TilePos};

use crate::editor::{Document, Mode, Overlay};
use crate::layout::{
    BLOCK_BTN, BTN_H, BTN_Y, ButtonRect, DECO_BTN, EXIT_BTN, EXIT_DEST_BTN, HUD_HEIGHT,
    HUD_MARGIN_X, HUD_TOP, LEVELS_BTN, NORMAL_BTN, OVERLAY_PAD, OVERLAY_ROW_H, OVERLAY_TITLE_H,
    PATH_LOOP_BTN, PATH_NEW_BTN, PICKER_CELL, PICKER_H, PICKER_ROWS, PICKER_W, PICKER_X, PICKER_Y,
    PLAY_BTN, RESIZE_BOT_BTN, RESIZE_LEFT_BTN, RESIZE_RIGHT_BTN, RESIZE_TOP_BTN, SELECT_BTN, SLOT,
    SLOT_PAD, TILE_AREA_TOP, TOP_BAR_HEIGHT, VIEW_HEIGHT, VIEW_WIDTH, cursor_tile_i, overlay_rect,
};
use crate::select::{SelectDrag, Selection, clamp_move_target};
use crate::tools::Tool;

pub fn draw_grid(canvas: &mut WindowCanvas, tilemap: &TileMap, camera_x: i32, camera_y: i32) {
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 60));
    let size = TILE_SIZE as i32;

    let level_w = tilemap.width as i32 * size;
    let level_h = tilemap.height as i32 * size;
    // camera_y is already offset so that row 0 draws at TILE_AREA_TOP when camera scroll=0.
    // y0 must be clamped to TILE_AREA_TOP so grid never bleeds into the top bar.
    let x0 = (-camera_x).max(0);
    let x1 = (level_w - camera_x).min(VIEW_WIDTH as i32);
    let y0 = (-camera_y).max(TILE_AREA_TOP);
    let y1 = (level_h - camera_y).min(HUD_TOP);

    for col in 0..=tilemap.width as i32 {
        let x = col * size - camera_x;
        if (0..=VIEW_WIDTH as i32).contains(&x) {
            let _ = canvas.draw_line((x, y0), (x, y1));
        }
    }
    for row in 0..=tilemap.height as i32 {
        let y = row * size - camera_y;
        if (TILE_AREA_TOP..=HUD_TOP).contains(&y) {
            let _ = canvas.draw_line((x0, y), (x1, y));
        }
    }
}

pub fn draw_hover(
    canvas: &mut WindowCanvas,
    tilemap: &TileMap,
    mouse: (i32, i32),
    camera_x: i32,
    camera_y: i32,
) {
    let (mx, my) = mouse;
    if !(TILE_AREA_TOP..HUD_TOP).contains(&my) {
        return;
    }
    let size = TILE_SIZE as i32;
    let world_x = mx + camera_x;
    // camera_y here is camera_yi - TILE_AREA_TOP, so world tile coords use it directly.
    let world_y = my + camera_y;
    if world_x < 0 || world_y < TILE_AREA_TOP {
        return;
    }
    let tx = world_x / size;
    let ty = world_y / size;
    if tx >= tilemap.width as i32 || ty >= tilemap.height as i32 || ty < 0 {
        return;
    }
    canvas.set_draw_color(Color::RGB(255, 235, 90));
    let _ = canvas.draw_rect(Rect::new(
        tx * size - camera_x,
        ty * size - camera_y,
        size as u32,
        size as u32,
    ));
}

/// Draw the Select-mode overlay: the current selection as a translucent blue
/// rectangle, plus, while a move/copy drag is in progress, a 50%-transparent
/// preview of the selected tiles at their drop location and an outline showing
/// where the block will land (green for copy, yellow for move). `camera_x` is the
/// truncated horizontal camera and `camera_y` is `camera_yi - TILE_AREA_TOP`, so
/// the math mirrors [`draw_hover`]. Clipped to the play area.
#[allow(clippy::too_many_arguments)]
pub fn draw_selection(
    canvas: &mut WindowCanvas,
    tilemap_texture: &mut Texture,
    level: &LevelData,
    selection: Option<Selection>,
    select_drag: Option<SelectDrag>,
    mouse: (i32, i32),
    camera_x: i32,
    camera_y: i32,
) {
    let Some(sel) = selection else {
        return;
    };
    let size = TILE_SIZE as i32;
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));

    let rect_for = |min: TilePos, w: usize, h: usize| {
        Rect::new(
            min.0 as i32 * size - camera_x,
            min.1 as i32 * size - camera_y,
            (w * TILE_SIZE as usize) as u32,
            (h * TILE_SIZE as usize) as u32,
        )
    };

    let r = rect_for(sel.min, sel.width(), sel.height());
    canvas.set_draw_color(Color::RGBA(90, 170, 255, 60));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(120, 200, 255));
    let _ = canvas.draw_rect(r);

    // While dragging the block, show where it will land.
    if let Some(SelectDrag::Move { grab, copy }) = select_drag
        && let Some(cursor) = cursor_tile_i(mouse, camera_x, camera_y)
    {
        let target = clamp_move_target(level.width(), level.height(), sel, grab, cursor);

        // Render the picked-up tiles at 50% transparency at the drop location so
        // you can see exactly what is being placed.
        tilemap_texture.set_alpha_mod(128);
        for r in 0..sel.height() {
            for c in 0..sel.width() {
                let tile = level.tiles[sel.min.1 + r][sel.min.0 + c];
                if tile == tiles::EMPTY {
                    continue;
                }
                let (sx, sy) = tiles::tile_src_xy(tile);
                let src = Rect::new(sx, sy, TILE_SIZE as u32, TILE_SIZE as u32);
                let dst = Rect::new(
                    (target.0 + c) as i32 * size - camera_x,
                    (target.1 + r) as i32 * size - camera_y,
                    TILE_SIZE as u32,
                    TILE_SIZE as u32,
                );
                let _ = canvas.copy(tilemap_texture, Some(src), Some(dst));
            }
        }
        tilemap_texture.set_alpha_mod(255);

        let pr = rect_for(target, sel.width(), sel.height());
        let color = if copy {
            Color::RGB(120, 235, 140)
        } else {
            Color::RGB(255, 235, 90)
        };
        canvas.set_draw_color(color);
        let _ = canvas.draw_rect(pr);
        let _ = canvas.draw_rect(Rect::new(
            pr.x() + 1,
            pr.y() + 1,
            pr.width().saturating_sub(2),
            pr.height().saturating_sub(2),
        ));
    }

    canvas.set_clip_rect(prev_clip);
}

/// Draw the path-block overlay: each block's control points joined by lines,
/// with a green start marker and direction arrows. The active block is drawn in
/// yellow, others in cyan; everything is dimmed when not in path-edit mode.
pub fn draw_paths(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    active_block: Option<usize>,
    path_mode: bool,
    camera_x: i32,
    camera_y: i32,
) {
    if level.path_blocks.is_empty() {
        return;
    }
    let size = TILE_SIZE as i32;
    // Keep the overlay inside the play area so lines never bleed into the bars.
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));

    let alpha = if path_mode { 235 } else { 110 };
    let center = |pt: TilePos| -> (i32, i32) {
        (
            pt.0 as i32 * size + size / 2 - camera_x,
            pt.1 as i32 * size + size / 2 - camera_y,
        )
    };

    for (b, block) in level.path_blocks.iter().enumerate() {
        let is_active = path_mode && active_block == Some(b);
        let line_color = if is_active {
            Color::RGBA(255, 235, 90, alpha)
        } else {
            Color::RGBA(100, 200, 235, alpha)
        };

        // Segments between consecutive points (plus the closing wrap if looped).
        let n = block.points.len();
        let last_seg = if block.closed { n } else { n.saturating_sub(1) };
        for i in 0..last_seg {
            let a = center(block.points[i]);
            let c = center(block.points[(i + 1) % n]);
            canvas.set_draw_color(line_color);
            draw_thick_line(canvas, a, c);
            if path_mode {
                draw_seg_arrow(canvas, a, c, line_color);
            }
        }

        // Control points: the start (index 0, the resting position) is a larger
        // green dot; the rest are smaller dots in the block's colour.
        for (i, &pt) in block.points.iter().enumerate() {
            let (px, py) = center(pt);
            if i == 0 {
                canvas.set_draw_color(Color::RGBA(90, 220, 120, alpha));
                fill_circle(canvas, px, py, 6);
            } else {
                canvas.set_draw_color(line_color);
                fill_circle(canvas, px, py, 4);
            }
        }
    }

    canvas.set_clip_rect(prev_clip);
}

/// Mark every decoration with a corner-bracket outline so its render-only sprite
/// is distinguishable from the real gameplay tiles in the editor. Background
/// decorations are bracketed in magenta, foreground ones in orange. While in
/// decoration mode the brackets brighten, and the layer currently being edited
/// (`active_layer`) is brighter still than the other; outside decoration mode
/// both are dimmed (mirroring the path overlay).
pub fn draw_decorations(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    deco_mode: bool,
    active_layer: DecoLayer,
    camera_x: i32,
    camera_y: i32,
) {
    if level.decorations.is_empty() {
        return;
    }
    // Keep the overlay inside the play area so brackets never bleed into the bars.
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));

    let size = TILE_SIZE as i32;
    for deco in &level.decorations {
        // The active layer reads brightest; the inactive layer is dimmer; both
        // fade further when not in deco mode.
        let alpha = if !deco_mode {
            170
        } else if deco.layer == active_layer {
            255
        } else {
            200
        };
        // Magenta (background) vs orange (foreground) both read distinctly from
        // the path overlay (yellow/cyan) and the hover highlight (yellow).
        let color = match deco.layer {
            DecoLayer::Background => Color::RGBA(255, 80, 245, alpha),
            DecoLayer::Foreground => Color::RGBA(255, 170, 40, alpha),
        };
        canvas.set_draw_color(color);
        let x = deco.x as i32 - camera_x;
        let y = deco.y as i32 - camera_y;
        draw_corner_brackets(canvas, x, y, size);
    }

    canvas.set_clip_rect(prev_clip);
}

/// Draw a bold L-shaped bracket at each corner of the `size`x`size` cell at
/// (x, y), using the canvas's current draw colour. The arms are filled bars
/// (several pixels thick) so the marker reads clearly over busy sprites, without
/// looking like a full tile border.
fn draw_corner_brackets(canvas: &mut WindowCanvas, x: i32, y: i32, size: i32) {
    let b = (size / 4).max(8); // bracket arm length
    let t = 3i32; // bracket arm thickness
    let (l, top, r, bot) = (x, y, x + size, y + size);
    let tu = t as u32;
    let bu = b as u32;
    // Each corner is an L made of one horizontal and one vertical filled bar.
    // Top-left
    let _ = canvas.fill_rect(Rect::new(l, top, bu, tu));
    let _ = canvas.fill_rect(Rect::new(l, top, tu, bu));
    // Top-right
    let _ = canvas.fill_rect(Rect::new(r - b, top, bu, tu));
    let _ = canvas.fill_rect(Rect::new(r - t, top, tu, bu));
    // Bottom-left
    let _ = canvas.fill_rect(Rect::new(l, bot - t, bu, tu));
    let _ = canvas.fill_rect(Rect::new(l, bot - b, tu, bu));
    // Bottom-right
    let _ = canvas.fill_rect(Rect::new(r - b, bot - t, bu, tu));
    let _ = canvas.fill_rect(Rect::new(r - t, bot - b, tu, bu));
}

/// Draw a 3px-wide line by stacking three 1px lines.
fn draw_thick_line(canvas: &mut WindowCanvas, a: (i32, i32), b: (i32, i32)) {
    let _ = canvas.draw_line(a, b);
    let _ = canvas.draw_line((a.0 + 1, a.1), (b.0 + 1, b.1));
    let _ = canvas.draw_line((a.0, a.1 + 1), (b.0, b.1 + 1));
}

/// Draw a small chevron at the midpoint of an axis-aligned segment, pointing
/// from `a` toward `b` to show the block's travel direction.
fn draw_seg_arrow(canvas: &mut WindowCanvas, a: (i32, i32), b: (i32, i32), color: Color) {
    canvas.set_draw_color(color);
    let mx = (a.0 + b.0) / 2;
    let my = (a.1 + b.1) / 2;
    let dx = (b.0 - a.0).signum();
    let dy = (b.1 - a.1).signum();
    let s = 5;
    if dx != 0 {
        let _ = canvas.draw_line((mx, my), (mx - dx * s, my - s));
        let _ = canvas.draw_line((mx, my), (mx - dx * s, my + s));
    } else if dy != 0 {
        let _ = canvas.draw_line((mx, my), (mx - s, my - dy * s));
        let _ = canvas.draw_line((mx, my), (mx + s, my - dy * s));
    }
}

/// Draw the decoration sprite picker: the whole tilemap sheet at full scale with
/// a grid and the selected cell highlighted. Shown only in decoration mode.
pub fn draw_deco_picker(canvas: &mut WindowCanvas, tilemap_texture: &Texture, selected: u32) {
    // Backdrop framing the sheet so it reads as a panel over the level.
    let frame = Rect::new(
        PICKER_X - 4,
        PICKER_Y - 4,
        PICKER_W as u32 + 8,
        PICKER_H as u32 + 8,
    );
    canvas.set_draw_color(Color::RGB(20, 20, 28));
    let _ = canvas.fill_rect(frame);
    canvas.set_draw_color(Color::RGB(90, 90, 110));
    let _ = canvas.draw_rect(frame);

    // The sheet itself, drawn 1:1 (it is exactly PICKER_W x PICKER_H).
    let dst = Rect::new(PICKER_X, PICKER_Y, PICKER_W as u32, PICKER_H as u32);
    let _ = canvas.copy(tilemap_texture, None, Some(dst));

    // Cell grid.
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 40));
    for col in 0..=tiles::SHEET_COLUMNS as i32 {
        let x = PICKER_X + col * PICKER_CELL;
        let _ = canvas.draw_line((x, PICKER_Y), (x, PICKER_Y + PICKER_H));
    }
    for row in 0..=PICKER_ROWS {
        let y = PICKER_Y + row * PICKER_CELL;
        let _ = canvas.draw_line((PICKER_X, y), (PICKER_X + PICKER_W, y));
    }

    // Highlight the selected cell.
    let idx = selected.saturating_sub(1) as i32;
    let col = idx % tiles::SHEET_COLUMNS as i32;
    let row = idx / tiles::SHEET_COLUMNS as i32;
    if row < PICKER_ROWS {
        canvas.set_draw_color(Color::RGB(255, 235, 90));
        let cell = Rect::new(
            PICKER_X + col * PICKER_CELL,
            PICKER_Y + row * PICKER_CELL,
            PICKER_CELL as u32,
            PICKER_CELL as u32,
        );
        let _ = canvas.draw_rect(cell);
        let _ = canvas.draw_rect(Rect::new(
            cell.x() - 1,
            cell.y() - 1,
            PICKER_CELL as u32 + 2,
            PICKER_CELL as u32 + 2,
        ));
    }
}

/// Draw the path-mode action menu in the HUD bar: a "new block" button and a
/// "toggle loop" button. The new-block button shows a path glyph with a `+` and
/// lights up green while a fresh block is armed (the next level click starts it).
/// The loop button's glyph reflects the active block's current state - a full
/// rectangle with corner nodes for a loop, an open "C" for an open path - and
/// lights up while it is a loop. Dimmed when there is no active block to act on.
pub fn draw_path_menu(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    active_block: Option<usize>,
    start_new: bool,
) {
    // --- New-block button ---
    let nb = PATH_NEW_BTN;
    let nr = nb.rect();
    canvas.set_draw_color(if start_new {
        Color::RGB(40, 70, 50)
    } else {
        Color::RGB(45, 45, 58)
    });
    let _ = canvas.fill_rect(nr);
    canvas.set_draw_color(if start_new {
        Color::RGB(120, 220, 150)
    } else {
        Color::RGB(90, 130, 100)
    });
    let _ = canvas.draw_rect(nr);

    // Glyph: a short two-segment path with node dots, plus a `+` marking "add".
    let nglyph = if start_new {
        Color::RGB(150, 230, 175)
    } else {
        Color::RGB(150, 200, 165)
    };
    canvas.set_draw_color(nglyph);
    let (_, ncy) = nb.center();
    let nodes = [
        (nb.x + 12, ncy + 5),
        (nb.x + 22, ncy + 5),
        (nb.x + 22, ncy - 5),
    ];
    for w in nodes.windows(2) {
        let _ = canvas.draw_line(w[0], w[1]);
    }
    for n in nodes {
        fill_circle(canvas, n.0, n.1, 2);
    }
    // A `+` to the right of the path, signalling a new block.
    let (pcx, pcy) = (nb.x + 38, ncy);
    let arm = 5;
    let _ = canvas.draw_line((pcx - arm, pcy), (pcx + arm, pcy));
    let _ = canvas.draw_line((pcx - arm, pcy + 1), (pcx + arm, pcy + 1));
    let _ = canvas.draw_line((pcx, pcy - arm), (pcx, pcy + arm));
    let _ = canvas.draw_line((pcx + 1, pcy - arm), (pcx + 1, pcy + arm));

    // --- Toggle-loop button ---
    let closed = active_block
        .and_then(|b| level.path_blocks.get(b))
        .map(|blk| blk.closed);
    let lit = closed == Some(true);
    let has_active = closed.is_some();

    let b = PATH_LOOP_BTN;
    let r = b.rect();
    canvas.set_draw_color(if lit {
        Color::RGB(80, 75, 40)
    } else {
        Color::RGB(45, 45, 58)
    });
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if lit {
        Color::RGB(255, 235, 90)
    } else if has_active {
        Color::RGB(100, 200, 235)
    } else {
        Color::RGB(80, 80, 100)
    });
    let _ = canvas.draw_rect(r);

    // Loop glyph: top, left and bottom edges of a small rectangle are always
    // drawn; the right edge is only closed when the active block is a loop.
    let glyph = if lit {
        Color::RGB(255, 235, 90)
    } else if has_active {
        Color::RGB(160, 200, 230)
    } else {
        Color::RGB(110, 110, 130)
    };
    canvas.set_draw_color(glyph);
    let (cx, cy) = b.center();
    let (gw, gh) = (16, 12);
    let (l, t, ri, bo) = (cx - gw / 2, cy - gh / 2, cx + gw / 2, cy + gh / 2);
    let _ = canvas.draw_line((l, t), (ri, t));
    let _ = canvas.draw_line((l, t), (l, bo));
    let _ = canvas.draw_line((l, bo), (ri, bo));
    if lit {
        // Closed loop: complete the rectangle and mark the corners like the
        // path overlay's control points.
        let _ = canvas.draw_line((ri, t), (ri, bo));
        for (px, py) in [(l, t), (ri, t), (ri, bo), (l, bo)] {
            fill_circle(canvas, px, py, 2);
        }
    } else {
        // Open path: leave the right side open, capping the two free ends.
        fill_circle(canvas, ri, t, 2);
        fill_circle(canvas, ri, bo, 2);
    }
}

/// Draw a rectangular button with a centred text label, lit yellow when active.
fn draw_text_button(canvas: &mut WindowCanvas, b: ButtonRect, label: &str, active: bool) {
    let r = b.rect();
    canvas.set_draw_color(if active {
        Color::RGB(80, 75, 40)
    } else {
        Color::RGB(50, 50, 65)
    });
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);
    let tx = b.x + (b.w as i32 - font::text_width(label, 1)) / 2;
    let ty = b.y + (b.h as i32 - font::line_height(1)) / 2;
    let color = if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(185, 195, 215)
    };
    font::draw_text(canvas, tx, ty, label, color, 1);
}

/// In Exit mode, outline every door and label where it leads. Normal doors are
/// bracketed cyan, secret doors magenta; the selected door is highlighted
/// yellow. Clipped to the play area so labels never bleed into the bars.
pub fn draw_exits(
    canvas: &mut WindowCanvas,
    level: &LevelData,
    selected: Option<TilePos>,
    camera_x: i32,
    camera_y: i32,
) {
    let prev_clip = canvas.clip_rect();
    canvas.set_clip_rect(Rect::new(
        0,
        TILE_AREA_TOP,
        VIEW_WIDTH,
        (HUD_TOP - TILE_AREA_TOP) as u32,
    ));
    let size = TILE_SIZE as i32;
    for exit in &level.exits {
        let (tx, ty) = exit.tile;
        let is_sel = selected == Some(exit.tile);
        let secret = level.tiles[ty][tx] == tiles::SECRET_EXIT;
        let x = tx as i32 * size - camera_x;
        let y = ty as i32 * size - camera_y;
        let color = if is_sel {
            Color::RGB(255, 235, 90)
        } else if secret {
            Color::RGB(220, 90, 220)
        } else {
            Color::RGB(90, 200, 235)
        };
        canvas.set_draw_color(color);
        draw_corner_brackets(canvas, x, y, size);

        // Destination label on a dark plate, just below the door.
        let label = format!("-> {}", exit.dest);
        let w = font::text_width(&label, 1);
        let lx = x + (size - w) / 2;
        let ly = y + size + 3;
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 160));
        let _ = canvas.fill_rect(Rect::new(
            lx - 2,
            ly - 1,
            (w + 4) as u32,
            (font::line_height(1) + 2) as u32,
        ));
        let text_color = if is_sel {
            Color::RGB(255, 235, 90)
        } else {
            Color::RGB(220, 220, 232)
        };
        font::draw_text(canvas, lx, ly, &label, text_color, 1);
    }
    canvas.set_clip_rect(prev_clip);
}

/// Draw the Exit-mode HUD: the exit tool palette (doors + coins) on the left, a
/// Set-destination button for the selected door, and a status line naming the
/// current target or prompting the user to paint / route a door.
pub fn draw_exit_menu(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    level: &LevelData,
    palette: &[Tool],
    selected_tool: usize,
    selected: Option<TilePos>,
) {
    draw_palette_slots(
        canvas,
        tilemap_texture,
        character_texture,
        palette,
        selected_tool,
        HUD_MARGIN_X,
    );

    let secret = selected.is_some_and(|(tx, ty)| level.tiles[ty][tx] == tiles::SECRET_EXIT);
    let dest = selected
        .and_then(|t| level.exits.iter().find(|e| e.tile == t))
        .map(|e| e.dest.as_str());

    draw_text_button(canvas, EXIT_DEST_BTN, "Set dest", selected.is_some());

    let msg = match dest {
        Some(d) => format!("{} door -> {}", if secret { "secret" } else { "normal" }, d),
        None => "paint E/S doors & C/R coins; click a door to route it".to_string(),
    };
    let x = EXIT_DEST_BTN.x + EXIT_DEST_BTN.w as i32 + 16;
    let y = HUD_TOP + (HUD_HEIGHT - font::line_height(1)) / 2;
    font::draw_text(canvas, x, y, &msg, Color::RGB(205, 205, 222), 1);
}

/// Draw the full-list level overlay: a dimmed backdrop and a centred panel with
/// one clickable row per level (index, name and id). The current level's row is
/// highlighted. Used both to browse levels and to pick a door's destination.
pub fn draw_level_overlay(
    canvas: &mut WindowCanvas,
    docs: &[Document],
    current: usize,
    overlay: Overlay,
) {
    let count = docs.len();
    let r = overlay_rect(count);

    canvas.set_draw_color(Color::RGBA(0, 0, 0, 150));
    let _ = canvas.fill_rect(Rect::new(0, 0, VIEW_WIDTH, VIEW_HEIGHT));
    canvas.set_draw_color(Color::RGB(28, 28, 38));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(95, 95, 125));
    let _ = canvas.draw_rect(r);

    let title = match overlay {
        Overlay::Jump => "Go to level  (Esc to close)",
        Overlay::PickDest => "Pick destination  (Esc to cancel)",
    };
    font::draw_text(
        canvas,
        r.x() + OVERLAY_PAD,
        r.y() + 8,
        title,
        Color::RGB(255, 235, 90),
        1,
    );
    canvas.set_draw_color(Color::RGB(70, 70, 95));
    let sep_y = r.y() + OVERLAY_TITLE_H - 2;
    let _ = canvas.draw_line(
        (r.x() + OVERLAY_PAD, sep_y),
        (r.x() + r.width() as i32 - OVERLAY_PAD, sep_y),
    );

    let first = r.y() + OVERLAY_TITLE_H + OVERLAY_PAD;
    for (i, doc) in docs.iter().enumerate() {
        let ry = first + i as i32 * OVERLAY_ROW_H;
        if i == current {
            canvas.set_draw_color(Color::RGB(58, 58, 40));
            let _ = canvas.fill_rect(Rect::new(
                r.x() + OVERLAY_PAD,
                ry,
                r.width() - (2 * OVERLAY_PAD) as u32,
                (OVERLAY_ROW_H - 2) as u32,
            ));
        }
        let name = if doc.level.name.is_empty() {
            doc.level.id.as_str()
        } else {
            doc.level.name.as_str()
        };
        let label = format!("{:>2}  {}  [{}]", i + 1, name, doc.level.id);
        let color = if i == current {
            Color::RGB(255, 235, 90)
        } else {
            Color::RGB(222, 222, 234)
        };
        font::draw_text(canvas, r.x() + OVERLAY_PAD + 4, ry + 4, &label, color, 1);
    }
}

pub fn draw_hud(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    palette: &[Tool],
    selected: usize,
    mode: Mode,
) {
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, HUD_TOP, VIEW_WIDTH, HUD_HEIGHT as u32));
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, HUD_TOP), (VIEW_WIDTH as i32, HUD_TOP));

    // The tool palette is only relevant when painting normal tiles; the other
    // modes have their own selectors (the path overlay / the sprite picker / the
    // Exit-mode menu), so the bar is left empty for them.
    if mode != Mode::Normal {
        return;
    }
    draw_palette_slots(
        canvas,
        tilemap_texture,
        character_texture,
        palette,
        selected,
        HUD_MARGIN_X,
    );
}

/// Draw one palette bar's worth of tool slots starting at `x0`, highlighting the
/// selected one. Shared by the Normal-mode palette and the Exit-mode menu.
fn draw_palette_slots(
    canvas: &mut WindowCanvas,
    tilemap_texture: &Texture,
    character_texture: &Texture,
    palette: &[Tool],
    selected: usize,
    x0: i32,
) {
    let slot_y = HUD_TOP + (HUD_HEIGHT - SLOT) / 2;
    for (i, tool) in palette.iter().enumerate() {
        let slot_x = x0 + i as i32 * (SLOT + SLOT_PAD);
        let dst = Rect::new(slot_x, slot_y, SLOT as u32, SLOT as u32);

        canvas.set_draw_color(Color::RGB(55, 55, 70));
        let _ = canvas.fill_rect(dst);

        match *tool {
            Tool::Tile(n) => {
                let (sx, sy) = tiles::tile_src_xy(n);
                let src = Rect::new(sx, sy, TILE_SIZE as u32, TILE_SIZE as u32);
                let _ = canvas.copy(tilemap_texture, Some(src), Some(dst));
            }
            Tool::Spawn => {
                let src = Rect::new(0, 0, PLAYER_WIDTH, PLAYER_HEIGHT);
                let player_dst = Rect::new(
                    slot_x + (SLOT - PLAYER_WIDTH as i32) / 2,
                    slot_y + (SLOT - PLAYER_HEIGHT as i32) / 2,
                    PLAYER_WIDTH,
                    PLAYER_HEIGHT,
                );
                let _ = canvas.copy(character_texture, Some(src), Some(player_dst));
            }
            Tool::Erase => {
                canvas.set_draw_color(Color::RGB(220, 80, 80));
                let _ = canvas
                    .draw_line((slot_x + 8, slot_y + 8), (slot_x + SLOT - 8, slot_y + SLOT - 8));
                let _ = canvas
                    .draw_line((slot_x + SLOT - 8, slot_y + 8), (slot_x + 8, slot_y + SLOT - 8));
            }
        }

        if i == selected {
            canvas.set_draw_color(Color::RGB(255, 235, 90));
            let _ = canvas.draw_rect(dst);
            let _ = canvas.draw_rect(Rect::new(
                slot_x - 1,
                slot_y - 1,
                SLOT as u32 + 2,
                SLOT as u32 + 2,
            ));
        } else {
            canvas.set_draw_color(Color::RGB(90, 90, 110));
            let _ = canvas.draw_rect(dst);
        }
    }
}

/// Draw the top toolbar: the Levels button, mode-switch buttons, resize
/// buttons and the Play button.
pub fn draw_top_bar(
    canvas: &mut WindowCanvas,
    docs: &[Document],
    current: usize,
    mode: Mode,
    deco_layer: DecoLayer,
) {
    // Background
    canvas.set_draw_color(Color::RGB(30, 30, 40));
    let _ = canvas.fill_rect(Rect::new(0, 0, VIEW_WIDTH, TOP_BAR_HEIGHT as u32));
    // Bottom border
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line((0, TOP_BAR_HEIGHT - 1), (VIEW_WIDTH as i32, TOP_BAR_HEIGHT - 1));

    // --- Levels button (shows the current level; opens the browser) ---
    let level_label = format!("Lv {}/{}", current + 1, docs.len());
    draw_text_button(canvas, LEVELS_BTN, &level_label, false);

    // --- Mode-switch buttons (exactly one active) ---
    draw_normal_button(canvas, NORMAL_BTN, mode == Mode::Normal);
    draw_block_button(canvas, BLOCK_BTN, mode == Mode::Path);
    // Pass the active layer only while in deco mode, so the button can show which
    // of the two layers (background / foreground) is currently selected.
    draw_deco_button(canvas, DECO_BTN, (mode == Mode::Deco).then_some(deco_layer));
    // Exit-door mode button.
    draw_text_button(canvas, EXIT_BTN, "EXIT", mode == Mode::Exit);
    // Region select / move mode button.
    draw_text_button(canvas, SELECT_BTN, "SEL", mode == Mode::Select);

    // --- Separator before resize ---
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line(
        (RESIZE_TOP_BTN.x - 8, BTN_Y + 4),
        (RESIZE_TOP_BTN.x - 8, BTN_Y + BTN_H as i32 - 4),
    );

    // --- Resize buttons ---
    draw_resize_button(canvas, RESIZE_TOP_BTN, ArrowDir::Up);
    draw_resize_button(canvas, RESIZE_BOT_BTN, ArrowDir::Down);
    draw_resize_button(canvas, RESIZE_LEFT_BTN, ArrowDir::Left);
    draw_resize_button(canvas, RESIZE_RIGHT_BTN, ArrowDir::Right);

    // --- Separator before play ---
    canvas.set_draw_color(Color::RGB(80, 80, 100));
    let _ = canvas.draw_line(
        (PLAY_BTN.x - 8, BTN_Y + 4),
        (PLAY_BTN.x - 8, BTN_Y + BTN_H as i32 - 4),
    );

    // --- Play button ---
    draw_play_button(canvas, PLAY_BTN);
}

#[derive(Clone, Copy)]
enum ArrowDir {
    Left,
    Right,
    Up,
    Down,
}

/// Draw a resize button (grow on left-click, shrink on right-click).
fn draw_resize_button(canvas: &mut WindowCanvas, b: ButtonRect, dir: ArrowDir) {
    let r = b.rect();
    canvas.set_draw_color(Color::RGB(45, 55, 65));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(80, 100, 110));
    let _ = canvas.draw_rect(r);
    draw_arrow_shape(canvas, b, dir, Color::RGB(120, 200, 160));
}

/// Draw a chevron arrow inside a button rect.
fn draw_arrow_shape(canvas: &mut WindowCanvas, b: ButtonRect, dir: ArrowDir, color: Color) {
    canvas.set_draw_color(color);
    let (cx, cy) = b.center();
    let half = 7i32;
    let tip = 7i32;
    match dir {
        ArrowDir::Left => {
            let _ = canvas.draw_line((cx + tip, cy - half), (cx - tip, cy));
            let _ = canvas.draw_line((cx - tip, cy), (cx + tip, cy + half));
        }
        ArrowDir::Right => {
            let _ = canvas.draw_line((cx - tip, cy - half), (cx + tip, cy));
            let _ = canvas.draw_line((cx + tip, cy), (cx - tip, cy + half));
        }
        ArrowDir::Up => {
            let _ = canvas.draw_line((cx - half, cy + tip), (cx, cy - tip));
            let _ = canvas.draw_line((cx, cy - tip), (cx + half, cy + tip));
        }
        ArrowDir::Down => {
            let _ = canvas.draw_line((cx - half, cy - tip), (cx, cy + tip));
            let _ = canvas.draw_line((cx, cy + tip), (cx + half, cy - tip));
        }
    }
}

/// Draw a green play (triangle) button.
fn draw_play_button(canvas: &mut WindowCanvas, b: ButtonRect) {
    let r = b.rect();
    canvas.set_draw_color(Color::RGB(40, 80, 50));
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(Color::RGB(80, 160, 100));
    let _ = canvas.draw_rect(r);

    // Filled triangle pointing right
    let (cx, cy) = b.center();
    let half_h = 8i32;
    let depth = 9i32;
    canvas.set_draw_color(Color::RGB(100, 220, 130));
    for dy in -half_h..=half_h {
        let width = depth * (half_h - dy.abs()) / half_h;
        let _ = canvas.draw_line((cx - width, cy + dy), (cx + depth - width, cy + dy));
    }
}

/// Draw the normal-mode button: a small brick/tile glyph, lit up yellow while
/// normal (tile-painting) mode is active.
fn draw_normal_button(canvas: &mut WindowCanvas, b: ButtonRect, active: bool) {
    let r = b.rect();
    if active {
        canvas.set_draw_color(Color::RGB(80, 75, 40));
    } else {
        canvas.set_draw_color(Color::RGB(50, 50, 65));
    }
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);

    // A little brick wall: an outer block with a couple of mortar lines.
    let glyph = if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(160, 200, 230)
    };
    canvas.set_draw_color(glyph);
    let (cx, cy) = b.center();
    let block = Rect::new(cx - 11, cy - 7, 22, 14);
    let _ = canvas.draw_rect(block);
    // Horizontal mortar line splitting the two courses.
    let _ = canvas.draw_line((cx - 11, cy), (cx + 11, cy));
    // Staggered vertical mortar lines (offset between courses, like real bricks).
    let _ = canvas.draw_line((cx, cy - 7), (cx, cy));
    let _ = canvas.draw_line((cx - 6, cy), (cx - 6, cy + 7));
    let _ = canvas.draw_line((cx + 6, cy), (cx + 6, cy + 7));
}

/// Draw the path-edit toggle button: a little zig-zag path with dotted nodes,
/// lit up yellow while path mode is active.
fn draw_block_button(canvas: &mut WindowCanvas, b: ButtonRect, active: bool) {
    let r = b.rect();
    if active {
        canvas.set_draw_color(Color::RGB(80, 75, 40));
    } else {
        canvas.set_draw_color(Color::RGB(50, 50, 65));
    }
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);

    // A small zig-zag path with node dots, matching the overlay's look.
    let glyph = if active {
        Color::RGB(255, 235, 90)
    } else {
        Color::RGB(160, 200, 230)
    };
    canvas.set_draw_color(glyph);
    let (_, cy) = b.center();
    let nodes = [
        (b.x + 12, cy + 6),
        (b.x + 26, cy + 6),
        (b.x + 26, cy - 6),
        (b.x + 40, cy - 6),
    ];
    for w in nodes.windows(2) {
        let _ = canvas.draw_line(w[0], w[1]);
    }
    for n in nodes {
        fill_circle(canvas, n.0, n.1, 2);
    }
}

/// Draw the decoration-mode button. While inactive it shows a small "picture"
/// glyph (a framed tile with a sun and mountain). While active it shows a
/// two-square "layers" badge instead - the back square is the background layer,
/// the front square the foreground layer - with the currently selected layer
/// lit in its overlay colour (magenta = background, orange = foreground) so it
/// is clear which of the two deco modes is in effect.
fn draw_deco_button(canvas: &mut WindowCanvas, b: ButtonRect, layer: Option<DecoLayer>) {
    let active = layer.is_some();
    let r = b.rect();
    if active {
        canvas.set_draw_color(Color::RGB(40, 70, 55));
    } else {
        canvas.set_draw_color(Color::RGB(50, 50, 65));
    }
    let _ = canvas.fill_rect(r);
    canvas.set_draw_color(if active {
        Color::RGB(120, 220, 150)
    } else {
        Color::RGB(90, 90, 110)
    });
    let _ = canvas.draw_rect(r);

    let (cx, cy) = b.center();

    let Some(layer) = layer else {
        // Inactive: a little framed picture (a border, a sun, and a mountain).
        canvas.set_draw_color(Color::RGB(160, 200, 230));
        let frame = Rect::new(cx - 11, cy - 8, 22, 16);
        let _ = canvas.draw_rect(frame);
        fill_circle(canvas, cx - 5, cy - 3, 2);
        let base = cy + 7;
        let _ = canvas.draw_line((cx - 9, base), (cx - 1, base - 9));
        let _ = canvas.draw_line((cx - 1, base - 9), (cx + 9, base));
        return;
    };

    // Active: a two-square "layers" badge. The back square (up-left) is the
    // background layer, the front square (down-right) the foreground layer. The
    // selected one is filled in its overlay colour; the other is a dim outline.
    // Drawn back-then-front so the foreground square really overlaps the back.
    let dim = Color::RGB(120, 120, 140);
    let back = Rect::new(cx - 9, cy - 7, 11, 11);
    let front = Rect::new(cx - 1, cy - 3, 11, 11);

    if layer == DecoLayer::Background {
        canvas.set_draw_color(Color::RGB(255, 80, 245));
        let _ = canvas.fill_rect(back);
        canvas.set_draw_color(Color::RGB(255, 170, 250));
        let _ = canvas.draw_rect(back);
    } else {
        canvas.set_draw_color(dim);
        let _ = canvas.draw_rect(back);
    }

    if layer == DecoLayer::Foreground {
        canvas.set_draw_color(Color::RGB(255, 170, 40));
        let _ = canvas.fill_rect(front);
        canvas.set_draw_color(Color::RGB(255, 210, 150));
        let _ = canvas.draw_rect(front);
    } else {
        canvas.set_draw_color(dim);
        let _ = canvas.draw_rect(front);
    }
}

/// Approximate a filled circle using horizontal lines.
fn fill_circle(canvas: &mut WindowCanvas, cx: i32, cy: i32, r: i32) {
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt() as i32;
        let _ = canvas.draw_line((cx - dx, cy + dy), (cx + dx, cy + dy));
    }
}
