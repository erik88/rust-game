//! Level editor: browse and edit the level files in `levels/`.
//!
//! It renders each level the way the game does (tiles, moving platforms in
//! their starting positions, and the player at the spawn point) and lets you
//! paint tiles onto the grid. Edits are kept in memory per level and are only
//! written to disk when you save, so switching levels never loses work.
//!
//! The editor has five mutually-exclusive modes, switched with the grouped
//! toolbar buttons (or the keys noted below): Normal (paint tiles), Path (edit
//! path blocks), Deco (place decorations), Exit (route exit doors) and Select
//! (mark a rectangular block of tiles and drag it to move or copy it). The
//! bottom tool palette holds the world-building tiles in Normal mode and the
//! exit doors / coins in Exit mode.
//!
//! A level browser overlay (the LVLS toolbar button, or Tab) lists every level
//! by name; click a row to jump to it. The same list is reused in Exit mode to
//! pick a door's destination.
//!
//! The code is split by concern:
//! - [`editor`] - the [`Editor`](editor::Editor) state and the
//!   [`UICommand`](editor::UICommand) every key/button maps to
//! - [`layout`] - window/button layout and screen-to-tile mapping
//! - [`tools`] - the paintable tools and how they mutate the level
//! - [`path_edit`] - path-block (moving block) editing
//! - [`select`] - rectangular region select / move / copy
//! - [`draw`] - all rendering
//!
//! Run `cargo run --bin level_editor` from the project root; controls are
//! printed on startup (see [`print_controls`]).

mod draw;
mod editor;
mod layout;
mod path_edit;
mod select;
#[cfg(test)]
mod testutil;
mod tools;

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::mouse::MouseButton;
use sdl2::pixels::Color;

use rustgamex::level;
use rustgamex::texture::load_png_texture;
use rustgamex::tiles;
use rustgamex::time::{RealTime, TimeProvider};

use draw::{
    draw_deco_picker, draw_decorations, draw_exit_menu, draw_exits, draw_grid, draw_hover,
    draw_hud, draw_level_overlay, draw_path_menu, draw_paths, draw_selection, draw_top_bar,
};
use editor::{
    Document, Editor, Mode, Overlay, UICommand, key_command, path_menu_command, set_title,
    topbar_command,
};
use layout::{
    EXIT_DEST_BTN, HUD_TOP, TILE_AREA_TOP, TOP_BAR_HEIGHT, VIEW_HEIGHT, VIEW_WIDTH, cursor_tile,
    in_deco_picker, palette_slot_at, picker_sprite_at,
};
use select::{SelectDrag, Selection};
use tools::{Tool, apply_tool, erase_deco, is_door, place_deco};

fn main() -> Result<(), String> {
    let docs: Vec<Document> = level::load_dir_entries("levels")?
        .into_iter()
        .map(|(path, level)| Document {
            path,
            level,
            modified: false,
        })
        .collect();
    if docs.is_empty() {
        return Err("no level files found in levels/".to_string());
    }

    // Normal-mode palette: the world-building tiles. Exit doors and coins live in
    // the Exit-mode menu (`exit_palette`) instead, since they are edited there
    // alongside the door routing.
    let palette: Vec<Tool> = std::iter::once(Tool::Erase)
        .chain(std::iter::once(Tool::Spawn))
        // Tile id 2 is unused, so skip it when listing the paintable tiles.
        .chain(std::iter::once(Tool::Tile(tiles::SOLID)))
        .chain((tiles::DEATH..=tiles::MOVE_LEFT).map(Tool::Tile))
        .collect();

    // Exit-mode palette: the exit doors (normal / secret) and the coins that gate
    // them. Painting a door here auto-syncs its `exit:` routing.
    let exit_palette: Vec<Tool> = vec![
        Tool::Erase,
        Tool::Tile(tiles::EXIT),
        Tool::Tile(tiles::SECRET_EXIT),
        Tool::Tile(tiles::COIN),
        Tool::Tile(tiles::RED_COIN),
    ];

    print_controls();

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let window = video_subsystem
        .window("Level Editor", VIEW_WIDTH, VIEW_HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;

    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);

    let texture_creator = canvas.texture_creator();
    let character_texture = load_png_texture(&texture_creator, "character.png")?;
    let mut tilemap_texture = load_png_texture(&texture_creator, "tilemap.png")?;

    let mut event_pump = sdl_context.event_pump()?;
    let mut time_provider = RealTime::new();

    let mut editor = Editor::new(docs, palette, exit_palette);
    set_title(
        &mut canvas,
        &editor.docs,
        editor.current,
        editor.palette[editor.selected],
    );

    'running: loop {
        let delta_time = time_provider.delta_time();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => editor.quit = true,

                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } => {
                    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
                    let shift = keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
                    // Track Ctrl so a Select-mode drag copies instead of moves.
                    editor.copy_mod = ctrl;
                    if key == Keycode::Escape && editor.overlay.is_some() {
                        // Esc dismisses an open overlay before it means "quit".
                        editor.execute(UICommand::CloseOverlay);
                    } else if let Some(cmd) = key_command(key, ctrl, shift, editor.mode) {
                        editor.execute(cmd);
                    } else {
                        // Keys that aren't commands drive camera panning.
                        match key {
                            Keycode::Left | Keycode::A => editor.pan.left = true,
                            Keycode::Right | Keycode::D => editor.pan.right = true,
                            Keycode::Up | Keycode::W => editor.pan.up = true,
                            Keycode::Down | Keycode::S => editor.pan.down = true,
                            _ => {}
                        }
                    }
                }

                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Left | Keycode::A => editor.pan.left = false,
                    Keycode::Right | Keycode::D => editor.pan.right = false,
                    Keycode::Up | Keycode::W => editor.pan.up = false,
                    Keycode::Down | Keycode::S => editor.pan.down = false,
                    Keycode::LCtrl | Keycode::RCtrl => editor.copy_mod = false,
                    _ => {}
                },

                Event::MouseMotion {
                    x, y, mousestate, ..
                } => {
                    editor.mouse = (x, y);
                    if editor.overlay.is_some() {
                        // An open overlay captures the view; no painting beneath it.
                    } else if editor.mode == Mode::Path {
                        if mousestate.left()
                            && path_edit::drag_path(
                                &mut editor.docs[editor.current].level,
                                editor.active_block,
                                editor.dragging,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Deco {
                        // Drag to paint or erase a run of decorations, but never
                        // while the cursor is over the picker overlay.
                        let changed = if in_deco_picker(x, y) {
                            false
                        } else if mousestate.left() {
                            place_deco(
                                &mut editor.docs[editor.current].level,
                                editor.deco_sprite,
                                editor.deco_layer,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        } else if mousestate.right() {
                            erase_deco(
                                &mut editor.docs[editor.current].level,
                                editor.deco_layer,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        } else {
                            false
                        };
                        if changed {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Normal
                        && editor.overlay.is_none()
                        && (TILE_AREA_TOP..HUD_TOP).contains(&y)
                    {
                        let dest = editor.default_exit_dest();
                        if mousestate.left() {
                            if apply_tool(
                                &mut editor.docs[editor.current].level,
                                editor.palette[editor.selected],
                                &dest,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            ) {
                                editor.mark_changed();
                            }
                        } else if mousestate.right()
                            && apply_tool(
                                &mut editor.docs[editor.current].level,
                                Tool::Erase,
                                &dest,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Exit
                        && editor.overlay.is_none()
                        && (TILE_AREA_TOP..HUD_TOP).contains(&y)
                    {
                        // Drag to paint a run of coins/doors, mirroring Normal mode.
                        let dest = editor.default_exit_dest();
                        let tool = if mousestate.left() {
                            Some(editor.exit_palette[editor.exit_tool])
                        } else if mousestate.right() {
                            Some(Tool::Erase)
                        } else {
                            None
                        };
                        if let Some(tool) = tool
                            && apply_tool(
                                &mut editor.docs[editor.current].level,
                                tool,
                                &dest,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Select {
                        // Rubber-band: grow the selection to the cursor. A move
                        // drag needs no state here — its preview follows the mouse.
                        if let Some(SelectDrag::Marquee { anchor }) = editor.select_drag
                            && let Some(tile) = cursor_tile(
                                &editor.docs[editor.current].level,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            )
                        {
                            editor.selection = Some(Selection::from_corners(anchor, tile));
                        }
                    }
                }

                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    editor.dragging = None;
                    if editor.mode == Mode::Select {
                        editor.finish_select_drag();
                    }
                }

                Event::MouseButtonDown {
                    mouse_btn, x, y, ..
                } => {
                    if editor.overlay.is_some() {
                        // While the level overlay is up it captures every click.
                        editor.overlay_click(mouse_btn, x, y);
                    } else if y < TOP_BAR_HEIGHT {
                        if let Some(cmd) = topbar_command(mouse_btn, x, y) {
                            editor.execute(cmd);
                        }
                    } else if y >= HUD_TOP {
                        // The HUD bar holds the tool palette in Normal mode and a
                        // small action menu in Path and Exit modes.
                        match editor.mode {
                            Mode::Normal => {
                                if mouse_btn == MouseButton::Left
                                    && let Some(slot) = palette_slot_at(x, editor.palette.len())
                                {
                                    editor.selected = slot;
                                    editor.retitle = true;
                                }
                            }
                            Mode::Path => {
                                if let Some(cmd) = path_menu_command(mouse_btn, x, y) {
                                    editor.execute(cmd);
                                }
                            }
                            Mode::Exit => {
                                // The exit bar holds a tool palette (doors + coins)
                                // on the left; the Set-dest button routes whichever
                                // door is currently selected.
                                if mouse_btn == MouseButton::Left {
                                    if let Some(slot) =
                                        palette_slot_at(x, editor.exit_palette.len())
                                    {
                                        editor.exit_tool = slot;
                                    } else if editor.selected_door.is_some()
                                        && EXIT_DEST_BTN.contains(x, y)
                                    {
                                        editor.overlay = Some(Overlay::PickDest);
                                    }
                                }
                            }
                            // Deco mode's layer is chosen from the toolbar button,
                            // so its bottom bar stays empty.
                            Mode::Deco => {}
                            // Select mode has no bottom-bar tools.
                            Mode::Select => {}
                        }
                    } else if editor.mode == Mode::Exit {
                        // Paint the selected exit tool (left) or erase (right); the
                        // reconcile in apply_tool keeps `exit:` routing in step.
                        // Whatever door ends up under the cursor is then selected so
                        // its destination can be set from the bottom bar.
                        let tool = match mouse_btn {
                            MouseButton::Left => editor.exit_palette[editor.exit_tool],
                            MouseButton::Right => Tool::Erase,
                            _ => continue,
                        };
                        let dest = editor.default_exit_dest();
                        if apply_tool(
                            &mut editor.docs[editor.current].level,
                            tool,
                            &dest,
                            x,
                            y,
                            editor.camera_x,
                            editor.camera_y,
                        ) {
                            editor.mark_changed();
                        }
                        let tile = cursor_tile(
                            &editor.docs[editor.current].level,
                            x,
                            y,
                            editor.camera_x,
                            editor.camera_y,
                        );
                        editor.selected_door =
                            tile.filter(|&t| is_door(&editor.docs[editor.current].level, t));
                    } else if editor.mode == Mode::Deco {
                        // Clicking the sprite picker selects a sprite; clicking
                        // the level places (left) or erases (right) a decoration.
                        if in_deco_picker(x, y) {
                            if mouse_btn == MouseButton::Left
                                && let Some(s) = picker_sprite_at(x, y)
                            {
                                editor.deco_sprite = s;
                            }
                        } else {
                            let changed = match mouse_btn {
                                MouseButton::Left => place_deco(
                                    &mut editor.docs[editor.current].level,
                                    editor.deco_sprite,
                                    editor.deco_layer,
                                    x,
                                    y,
                                    editor.camera_x,
                                    editor.camera_y,
                                ),
                                MouseButton::Right => erase_deco(
                                    &mut editor.docs[editor.current].level,
                                    editor.deco_layer,
                                    x,
                                    y,
                                    editor.camera_x,
                                    editor.camera_y,
                                ),
                                _ => false,
                            };
                            if changed {
                                editor.mark_changed();
                            }
                        }
                    } else if editor.mode == Mode::Path {
                        let changed = match mouse_btn {
                            MouseButton::Left => path_edit::path_left_click(
                                &mut editor.docs[editor.current].level,
                                &mut editor.active_block,
                                &mut editor.dragging,
                                &mut editor.start_new,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            ),
                            MouseButton::Right => path_edit::path_right_click(
                                &mut editor.docs[editor.current].level,
                                &mut editor.active_block,
                                x,
                                y,
                                editor.camera_x,
                                editor.camera_y,
                            ),
                            _ => false,
                        };
                        if changed {
                            editor.mark_changed();
                        }
                    } else if editor.mode == Mode::Select {
                        editor.select_press(mouse_btn, x, y);
                    } else {
                        // Normal mode: paint the selected tool (left) or erase (right).
                        let tool = match mouse_btn {
                            MouseButton::Left => editor.palette[editor.selected],
                            MouseButton::Right => Tool::Erase,
                            _ => continue,
                        };
                        let dest = editor.default_exit_dest();
                        if apply_tool(
                            &mut editor.docs[editor.current].level,
                            tool,
                            &dest,
                            x,
                            y,
                            editor.camera_x,
                            editor.camera_y,
                        ) {
                            editor.mark_changed();
                        }
                    }
                }

                _ => {}
            }
        }

        if editor.quit {
            break 'running;
        }

        editor.apply_switch();
        editor.rebuild_if_dirty();
        editor.update_camera(delta_time);

        if editor.retitle {
            set_title(
                &mut canvas,
                &editor.docs,
                editor.current,
                editor.palette[editor.selected],
            );
            editor.retitle = false;
        }

        let camera_xi = editor.camera_x as i32;
        let camera_yi = editor.camera_y as i32;

        // Effective camera_y passed to render functions shifts tiles down by
        // TILE_AREA_TOP so they appear below the top toolbar.
        let render_cam_y = camera_yi - TILE_AREA_TOP;

        canvas.set_draw_color(Color::RGB(135, 206, 235));
        canvas.clear();
        editor
            .tilemap
            .render(&mut canvas, &tilemap_texture, camera_xi, render_cam_y);
        editor
            .player
            .render(&mut canvas, &character_texture, camera_xi, render_cam_y);
        // Foreground decorations draw over the player, just as in the game.
        editor
            .tilemap
            .render_foreground(&mut canvas, &tilemap_texture, camera_xi, render_cam_y);

        if editor.show_grid {
            draw_grid(&mut canvas, &editor.tilemap, camera_xi, render_cam_y);
        }
        draw_paths(
            &mut canvas,
            &editor.docs[editor.current].level,
            editor.active_block,
            editor.mode == Mode::Path,
            camera_xi,
            render_cam_y,
        );
        draw_decorations(
            &mut canvas,
            &editor.docs[editor.current].level,
            editor.mode == Mode::Deco,
            editor.deco_layer,
            camera_xi,
            render_cam_y,
        );
        draw_hover(&mut canvas, &editor.tilemap, editor.mouse, camera_xi, render_cam_y);
        if editor.mode == Mode::Select {
            draw_selection(
                &mut canvas,
                &mut tilemap_texture,
                &editor.docs[editor.current].level,
                editor.selection,
                editor.select_drag,
                editor.mouse,
                camera_xi,
                render_cam_y,
            );
        }
        if editor.mode == Mode::Exit {
            draw_exits(
                &mut canvas,
                &editor.docs[editor.current].level,
                editor.selected_door,
                camera_xi,
                render_cam_y,
            );
        }
        if editor.mode == Mode::Deco {
            draw_deco_picker(&mut canvas, &tilemap_texture, editor.deco_sprite);
        }
        draw_hud(
            &mut canvas,
            &tilemap_texture,
            &character_texture,
            &editor.palette,
            editor.selected,
            editor.mode,
        );
        if editor.mode == Mode::Path {
            draw_path_menu(
                &mut canvas,
                &editor.docs[editor.current].level,
                editor.active_block,
                editor.start_new,
            );
        }
        if editor.mode == Mode::Exit {
            draw_exit_menu(
                &mut canvas,
                &tilemap_texture,
                &character_texture,
                &editor.docs[editor.current].level,
                &editor.exit_palette,
                editor.exit_tool,
                editor.selected_door,
            );
        }
        draw_top_bar(
            &mut canvas,
            &editor.docs,
            editor.current,
            editor.mode,
            editor.deco_layer,
        );
        if let Some(overlay) = editor.overlay {
            draw_level_overlay(&mut canvas, &editor.docs, editor.current, overlay);
        }

        canvas.present();
        time_provider.wait_for_next_frame();
    }

    Ok(())
}

fn print_controls() {
    println!("Level Editor controls:");
    println!("  Left mouse  : paint selected tool      Right mouse : erase");
    println!("  Click palette bar to pick a tool       , / . (PgUp/PgDn): prev/next level");
    println!("  Toolbar resize arrows   : left-click=grow edge, right-click=shrink edge");
    println!("  Arrows / WASD: pan camera              Home        : scroll to start");
    println!("  Ctrl+Arrow  : grow canvas at that edge Ctrl+Shift+Arrow: shrink that edge");
    println!("  G           : toggle grid              Ctrl+S      : save level");
    println!("  Modes (toolbar/keys): F1 Normal tiles | F2 path blocks | F3 decorations | F4 exit doors | F5 select");
    println!("  Levels      : the 'Lv n/m' button or Tab opens the level browser; click a level to jump");
    println!("  Normal mode : click palette bar to pick a tool, left-click paints, right-click erases");
    println!("                (world tiles only; exit doors and coins moved to Exit mode)");
    println!("  Path mode   : left-click adds points / drags points & edges, right-click deletes");
    println!("                N new block, L open/close loop, Tab cycle, Del remove block");
    println!("                (bottom bar shows New-block and Toggle-loop buttons)");
    println!("  Deco mode   : click picker to choose a sprite, left-click places, right-click erases");
    println!("                deco toolbar button: 1st click = background layer, further clicks toggle bg/fg");
    println!("  Exit mode   : palette bar paints E/S exit doors and C/R gold/red coins (right-click erases);");
    println!("                click a door to select it, then Set-dest picks its target level (routing auto-syncs)");
    println!("  Select mode : left-drag marks a tile block, drag from inside it to move (Ctrl-drag copies);");
    println!("                right-click clears the selection, Delete erases the marked tiles");
    println!("  Esc / Q     : quit (Esc first closes an open level browser)");
}
