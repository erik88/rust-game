# Tilemap

The tiles are located inside "tilemap.png". They are 40x40 pixels.
Tile with index 1 is located at (0,0), tile with index 2 is located at (40,0), etc.
There are six tiles per row, so tile 7 is located at (0,40);

| Tile number | Description                                                  |
|-------------|--------------------------------------------------------------|
| 0           | Technically not a tile - 0 just describes empty space.       |
| 1           | A solid tile                                                 |
| 2           | A solid tile, dark variation                                 |
| 3           | A death tile, the player dies when colliding with this       |
| 4           | A crumbling tile, it will turn into tile 5 after 0.4 seconds |
| 5           | A crumbling tile, it will turn into tile 6 after 0.3 seconds |
| 6           | A crumbling tile, it will disappear after 0.3 seconds        |
| 7           | A periodic tile, solid. It will turn into tile 8 after 1 second |
| 8           | A periodic tile, NOT solid. It will turn into tile 7 after 1 second |
| 9           | A moving tile. Goes upwards.                                 |
| 10          | A moving tile. Goes right.                                   |
| 11          | A moving tile. Goes down.                                    |
| 12          | A moving tile. Goes left.                                    |
| 13          | An exit tile (door), CLOSED. Not solid. Opens once all GOLD coins are collected; touching it while open completes the level. |
| 14          | A gold coin. Not solid. Collected on touch. All gold coins must be collected before the normal exit doors open. |
| 15          | A path block. Render-only sprite for the moving blocks declared with `block:` headers; never stored in the tile grid. |
| 16          | A secret exit tile (door), CLOSED. Like tile 13, but opens once all RED coins are collected. Sprite at (0,200), just below the normal door. |
| 17          | A red coin. Not solid. Collected on touch. All red coins must be collected before the secret exit doors open. Sprite at (200,160). |
| 19          | An exit tile (door), OPEN. Render-only sprite shown for tile 13 once all gold coins are collected; never stored in a level file. |
| 20          | A secret exit tile (door), OPEN. Render-only sprite shown for tile 16 once all red coins are collected; never stored in a level file. Sprite at (0,240). |

## Coins and the exits
There are two independent coin/door currencies:
- **Gold coins** (`C`, tile 14) gate the **normal** exit doors (`E`, tile 13).
- **Red coins** (`R`, tile 17) gate the **secret** exit doors (`S`, tile 16).

For each kind: while any of its coins remain, that door type renders with its
CLOSED sprite and touching it does nothing. Once every coin of that kind is
collected, its doors render with the OPEN sprite (tile 19 / 20) and touching one
completes the level. A level with no red coins leaves its secret doors open from
the start (and likewise for gold coins and normal doors).

## Moving tiles
- Solid, the player cannot be inside them.
- Activate when the player steps on top of them, and start moving in their respective direction.
- Horizontally moving tiles (left/right) will
  - Push the player ahead of them if he is in the way 
  - Carry the player forward, the player is standing on top of them.

## Path blocks
- A solid block (rendered with its own sprite, tile id 15 at (0,80)) that endlessly travels a
  fixed path of control points, set by a `block:` header line rather than a grid
  character (see "Levels" below). They reuse the same carry/push physics as the
  moving tiles above: the player can ride on top and is pushed/crushed when the
  block moves into him.
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
