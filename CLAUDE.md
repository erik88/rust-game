# Working on this repo

## Commands

```bash
cargo run                      # run the game
cargo run --bin level_editor   # run the level editor
cargo test                     # run the whole test suite
```

The game accepts `--start-level <index>` (0-based, in filename order) and
`--levels-dir <dir>` (default `levels/`). Assets (`character.png`,
`tilemap.png`) and `levels/` are loaded from the current working directory, so
run cargo from the project root.

Engine tests can be watched visually: `VISUALIZE_TEST=1 cargo test <name> --
--nocapture` opens a window and plays the test frame by frame (see
`TEST_VISUALIZATION.md` / `visualize_test.sh`).

## Module map

- `src/engine.rs` — `GameEngine`: orchestrates one frame (`step`), level
  progression through exit doors, death/respawn.
- `src/tilemap.rs` — `TileMap`: the tile grid plus all dynamic world state
  (crumbling/periodic tiles, moving platforms, path blocks, coin bookkeeping,
  effects) and world rendering.
- `src/player.rs` — `Player`: input-driven physics, collision resolution,
  platform riding/pushing, and the death/exit animation state machine.
- `src/level.rs` — the ASCII level file format (`LevelData::parse` /
  `to_text`) and directory loading. The format is documented in its module doc.
- `src/tiles.rs` — **canonical tile ids, semantics and sprite positions.**
- `src/input.rs`, `src/time.rs`, `src/texture.rs`, `src/font.rs` — SDL glue,
  each with a test-friendly abstraction (`InputSource`, `TimeProvider`).
- `src/bin/level_editor/` — the level editor binary, split into `editor`
  (state + commands), `layout`, `tools`, `path_edit`, `select`, `draw`.
- `tests/game_tests.rs` — engine-level integration tests (`TestRunner`).

## Invariants worth knowing

- **Update order:** each frame the tilemap updates before the player
  (`GameEngine::step`). Platform carry/push physics and several regression
  tests depend on this ordering; tests reproduce it manually as
  `tilemap.update(dt); player.update(...)`.
- The window size is `SCREEN_WIDTH`/`SCREEN_HEIGHT` in `src/lib.rs`; render
  culling and camera clamping derive from it.
- Coin tiles must only be removed via `TileMap::collect_coins`, which keeps
  the cached coin counts (and door-open state) in sync with the grid.

# Tilemap

The sprites are in `tilemap.png`, 40x40 pixels each, six per row. Counting
1-based, sprite 1 is at (0,0), sprite 2 at (40,0), sprite 7 at (0,40), etc.

**The canonical list of tile ids, their gameplay semantics and their sprite
positions lives in `src/tiles.rs`** — consult and update it there. Design
intent that the constants can't express:

## Coins and the exits
There are two independent coin/door currencies:
- **Gold coins** (`C`, tile 14) gate the **normal** exit doors (`E`, tile 13).
- **Red coins** (`R`, tile 17) gate the **secret** exit doors (`S`, tile 16).

For each kind: while any of its coins remain, that door type renders with its
CLOSED sprite and touching it does nothing. Once every coin of that kind is
collected, its doors render with the OPEN sprite and touching one completes
the level. A level with no red coins leaves its secret doors open from the
start (and likewise for gold coins and normal doors).

## Moving tiles (9-12)
- Solid, the player cannot be inside them.
- Activate when the player steps on top of them, and start moving in their
  respective direction (9 up, 10 right, 11 down, 12 left).
- Horizontally moving tiles (left/right) will
  - Push the player ahead of them if he is in the way
  - Carry the player forward, the player is standing on top of them.

## Path blocks
- A solid block (tile id 15) that endlessly travels a fixed path of control
  points, set by a `block:` header line rather than a grid character (see
  "Levels" below). They reuse the same carry/push physics as the moving tiles
  above: the player can ride on top and is pushed/crushed when the block moves
  into him.
- Consecutive control points must be strictly horizontal or vertical neighbours.
- A trailing `loop` makes the path a closed cycle; without it the block reverses
  direction at each end (an open, back-and-forth path).

## Decorations
- A purely decorative layer: any sprite from tilemap.png can be placed anywhere,
  set by a `deco:` (background) or `fgdeco:` (foreground) header line rather than
  a grid character (see "Levels" below).
- **Render-only** - decorations never affect gameplay (no collision, coins, or
  exit logic ever consults them). This makes them suitable both for scenery and
  for "hidden paths" (e.g. a solid-looking sprite drawn over empty space, or
  ground left visually bare over a solid tile).
- Position is in **pixel (world) coordinates**, not tile coordinates, so
  placement (and, later, size) is not locked to the grid. The level editor snaps
  to the grid by default.
- The sprite is a 1-based index into the sprite sheet using the raw layout from
  the top of this file (index 1 = (0,0), six per row), so any sprite is reachable
  regardless of its gameplay meaning.
- **Two layers:**
  - **Background** (`deco:`) draws behind the player, coins and moving
    tiles/blocks.
  - **Foreground** (`fgdeco:`) draws in front of all of them, so it can hide the
    player, coins and platforms passing behind it.
- Render order: base tiles, background decorations, moving tiles/blocks, the
  player, then foreground decorations.

# Levels

Levels are ASCII text files in the "levels/" directory, loaded in filename
order. The format is documented in `src/level.rs`: an optional `key: value`
header (e.g. `name: My Level`), a blank line, then the tile grid using
characters `.` (empty), `1`-`8` (tile types), `^` `>` `v` `<` (moving tiles
9-12), `E` (normal exit, 13), `S` (secret exit, 16), `C` (gold coin, 14),
`R` (red coin, 17) and `P` (player spawn). Touching an open exit tile completes
the level; after a short transition pause the level its door points at is loaded.

Levels are linked explicitly by id, not by filename order. An `id:` header names
a level (defaulting to the file stem, e.g. `level01`, when omitted), and **every
door tile (`E` or `S`) must be routed by an `exit:` header line** — there is no
implicit "next level". An `exit:` line gives the door's `x,y` tile position,
`->`, and the destination level id, e.g. `exit: 8,9 -> level02`. Whether a door
is secret (drawn with the secret sprite, gated on red coins) is determined by
its grid tile (`E` vs `S`), not by the exit line. Destinations are checked at
load time, so a door that points at a missing level is a hard error.

Path blocks are declared with one `block:` header line each, listing ordered
`x,y` control points and an optional trailing `loop`, e.g.
`block: 5,11 12,11` (open) or `block: 7,3 10,3 10,6 7,6 loop` (closed). See the
"Path blocks" section above.

Decorations are declared with one `deco:` (background) or `fgdeco:` (foreground)
header line each, giving an `x,y` **pixel** position and a 1-based sprite-sheet
index, e.g. `deco: 200,440 27` or `fgdeco: 200,440 27`. See the "Decorations"
section above.
