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
| 13          | An exit tile (door), CLOSED. Not solid. Opens once all coins are collected; touching it while open completes the level. |
| 14          | A coin. Not solid. Collected on touch. All coins must be collected before exit doors open. |
| 19          | An exit tile (door), OPEN. Render-only sprite shown for tile 13 once all coins are collected; never stored in a level file. |

## Coins and the exit
- Place coins with the `C` character in level files (tile 14).
- While any coin remains, exit doors (tile 13) render with the CLOSED sprite and touching them does nothing.
- Once every coin is collected, doors render with the OPEN sprite (tile 19) and touching one completes the level.

## Moving tiles
- Solid, the player cannot be inside them.
- Activate when the player steps on top of them, and start moving in their respective direction.
- Horizontally moving tiles (left/right) will
  - Push the player ahead of them if he is in the way 
  - Carry the player forward, the player is standing on top of them.

## Path blocks
- A solid block (rendered with the solid tile sprite) that endlessly travels a
  fixed path of control points, set by a `block:` header line rather than a grid
  character (see "Levels" below). They reuse the same carry/push physics as the
  moving tiles above: the player can ride on top and is pushed/crushed when the
  block moves into him.
- Consecutive control points must be strictly horizontal or vertical neighbours.
- A trailing `loop` makes the path a closed cycle; without it the block reverses
  direction at each end (an open, back-and-forth path).

# Levels

Levels are ASCII text files in the "levels/" directory, loaded in filename
order. The format is documented in `src/level.rs`: an optional `key: value`
header (e.g. `name: My Level`), a blank line, then the tile grid using
characters `.` (empty), `1`-`8` (tile types), `^` `>` `v` `<` (moving tiles
9-12), `E` (exit, 13) and `P` (player spawn). Touching an exit tile completes
the level; after a short transition pause the next level is loaded (looping
back to the first level after the last).

Path blocks are declared with one `block:` header line each, listing ordered
`x,y` control points and an optional trailing `loop`, e.g.
`block: 5,11 12,11` (open) or `block: 7,3 10,3 10,6 7,6 loop` (closed). See the
"Path blocks" section above.
