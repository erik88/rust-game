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
| 7           | A periodic tile, it will turn into tile 8 after 1 second     |
| 8           | A periodic tile, it will turn into tile 7 after 1 second     |
| 9           | A moving tile. Goes upwards.                                 |
| 10          | A moving tile. Goes right.                                   |
| 11          | A moving tile. Goes down.                                    |
| 12          | A moving tile. Goes left.                                    |

## Moving tiles
- Solid, the player cannot be inside them
- Activate when the player steps on top of them
- Will push the player ahead of them
- If the player is standing on a horizontally moving block, it will carry him (unless other collisions occur)
