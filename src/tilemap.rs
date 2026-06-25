use crate::geometry::rect::Rect;
use crate::geometry::vec2d::Vec2d;
use crate::level::{Decoration, LevelData};
use crate::tiles::{self, TILE_SIZE};
use sdl2::render::{Texture, WindowCanvas};
use std::collections::{HashMap, HashSet};

// How long each phase of a periodic tile (7/8) lasts
const PERIODIC_TILE_INTERVAL: f32 = 1.0;

// Remaining lifetime of a crumbling tile in each decay state. A freshly
// touched tile (4) lives 1.0s in total, passing through the cracked states
// (5, 6) on the way to disappearing.
const CRUMBLE_TIME_FRESH: f32 = 1.0;
const CRUMBLE_TIME_CRACKED: f32 = 0.6;
const CRUMBLE_TIME_VERY_CRACKED: f32 = 0.3;

// Speed of activated moving platforms, in pixels per second
const PLATFORM_SPEED: f32 = 100.0;

// How long a moving platform lingers after it collides and stops before it is
// destroyed
const PLATFORM_DESTROY_DELAY: f32 = 1.0;

// Coins idle most of the time, then play a quick two-frame shimmer. The cycle
// repeats every COIN_ANIM_INTERVAL seconds, with each extra sprite shown for
// COIN_ANIM_FRAME seconds before the coin lands back on its normal graphic.
const COIN_ANIM_INTERVAL: f32 = 5.0;
const COIN_ANIM_FRAME: f32 = 0.25;

// The coin graphic is a circle centered in its 40x40 tile; collision uses a
// square of this size inscribed in that circle rather than the full tile, so
// the player only collects it when actually overlapping the coin.
const COIN_DIAMETER: f32 = 24.0;

// When a coin is collected it plays a brief two-frame sparkle in its place.
// The frames live at these pixel coordinates in tilemap.png and each shows for
// COIN_COLLECT_FRAME seconds.
const COIN_COLLECT_FRAME: f32 = 0.2;
const COIN_COLLECT_SPRITES: [(i32, i32); 2] = [(80, 160), (120, 160)];

// Pixel coordinates in tilemap.png of the animation-only sprites that do not
// correspond to a tile id.
//
// The two extra coin sprites shown during the idle shimmer.
const COIN_SHIMMER_SPRITES: [(i32, i32); 2] = [(40, 160), (40, 200)];
// The angry-face sprite a death tile shows after it has killed the player.
const DEATH_HIT_SPRITE: (i32, i32) = (40, 0);

#[derive(Clone)]
pub struct MovingPlatform {
    pub x: f32,
    pub y: f32,
    pub tile_type: u32, // one of tiles::MOVE_UP/RIGHT/DOWN/LEFT
    pub active: bool,
    pub vel_x: f32,
    pub vel_y: f32,
    pub original_tile_x: i32,
    pub original_tile_y: i32,
    // Time left before a stopped platform is destroyed; `None` until it
    // collides with something
    pub destroy_timer: Option<f32>,
    // When set, the platform endlessly follows this fixed path instead of being
    // a one-shot platform that activates on touch and self-destructs. Path
    // blocks are always `active`, never collide-stop, and have no destroy_timer.
    pub path: Option<PathState>,
}

impl MovingPlatform {
    pub fn rect(&self) -> Rect {
        Rect::new(Vec2d::new(self.x, self.y), Vec2d::new(TILE_SIZE, TILE_SIZE))
    }
}

/// The traversal state of a path-following block: the ordered control points
/// (in pixel top-left coordinates) and where along them the block currently is.
#[derive(Clone)]
pub struct PathState {
    /// Control points in path order, as pixel top-left positions.
    points: Vec<Vec2d>,
    /// Whether the path wraps from the last point back to the first.
    closed: bool,
    /// Index of the control point the block is currently moving toward.
    target: usize,
    /// For open paths, whether we are travelling toward higher indices. Unused
    /// for closed loops, which always advance forward with wraparound.
    forward: bool,
}

impl PathState {
    /// Pick the next control point to head for once the current target is
    /// reached. Closed paths wrap around; open paths reverse at each end.
    fn advance(&mut self) {
        let last = self.points.len() - 1;
        if self.closed {
            self.target = (self.target + 1) % self.points.len();
        } else if self.forward {
            if self.target == last {
                self.forward = false;
                self.target = last - 1;
            } else {
                self.target += 1;
            }
        } else if self.target == 0 {
            self.forward = true;
            self.target = 1;
        } else {
            self.target -= 1;
        }
    }
}

/// A transient sparkle played at a coin's tile right after it is collected.
#[derive(Clone)]
struct CoinEffect {
    tile_x: usize,
    tile_y: usize,
    timer: f32,
}

pub struct TileMap {
    pub width: usize,
    pub height: usize,
    pub tile_size: u32,
    pub tiles: Vec<Vec<u32>>,
    original_tiles: Vec<Vec<u32>>, // Store original state for reset
    disappearing_tiles: HashMap<(usize, usize), f32>, // (x, y) -> time remaining
    pub moving_platforms: Vec<MovingPlatform>,
    original_platforms: Vec<MovingPlatform>, // Store original platform positions for reset
    path_blocks: Vec<MovingPlatform>,        // Blocks that endlessly follow a fixed path
    original_path_blocks: Vec<MovingPlatform>, // Original path-block state for reset
    periodic_timer: f32,                     // Shared clock for periodic tiles (7/8)
    coin_timer: f32,                         // Shared clock for the coin shimmer animation
    triggered_death_tiles: HashSet<(usize, usize)>, // Death tiles the player has hit this life
    coin_effects: Vec<CoinEffect>,           // Sparkles playing where coins were collected
    decorations: Vec<Decoration>,            // Render-only sprites; never affect gameplay
}

pub struct Tile {
    pub tile_type: u32,
    pub x: usize,
    pub y: usize,
}

impl Tile {
    pub fn get_bounding_rect(&self) -> Rect {
        Rect::new(
            Vec2d::new(self.x as f32 * TILE_SIZE, self.y as f32 * TILE_SIZE),
            Vec2d::new(TILE_SIZE, TILE_SIZE),
        )
    }
}

impl TileMap {
    /// Build a tilemap from a tile grid. Moving platform tiles (9-12) are
    /// extracted from the grid into dynamic platforms.
    pub fn from_data(level_data: Vec<Vec<u32>>) -> Self {
        let height = level_data.len();
        let width = level_data.first().map_or(0, |row| row.len());

        // Extract moving platforms from the static grid
        let mut platforms = Vec::new();
        let mut tiles = level_data.clone();

        for (y, row) in level_data.iter().enumerate() {
            for (x, &tile) in row.iter().enumerate() {
                if tiles::is_moving(tile) {
                    platforms.push(MovingPlatform {
                        x: x as f32 * TILE_SIZE,
                        y: y as f32 * TILE_SIZE,
                        tile_type: tile,
                        active: false,
                        vel_x: 0.0,
                        vel_y: 0.0,
                        original_tile_x: x as i32,
                        original_tile_y: y as i32,
                        destroy_timer: None,
                        path: None,
                    });
                    tiles[y][x] = tiles::EMPTY;
                }
            }
        }

        Self {
            width,
            height,
            tile_size: TILE_SIZE as u32,
            original_tiles: level_data,
            tiles,
            disappearing_tiles: HashMap::new(),
            moving_platforms: platforms.clone(),
            original_platforms: platforms,
            path_blocks: Vec::new(),
            original_path_blocks: Vec::new(),
            periodic_timer: 0.0,
            coin_timer: 0.0,
            triggered_death_tiles: HashSet::new(),
            coin_effects: Vec::new(),
            decorations: Vec::new(),
        }
    }

    /// Build a tilemap from a parsed level: its tile grid plus any path-
    /// following blocks declared in `block:` headers and decorative sprites
    /// declared in `deco:` headers.
    pub fn from_level(level: &LevelData) -> Self {
        let mut map = Self::from_data(level.tiles.clone());
        map.decorations = level.decorations.clone();
        for def in &level.path_blocks {
            if def.points.len() < 2 {
                continue;
            }
            let points: Vec<Vec2d> = def
                .points
                .iter()
                .map(|&(x, y)| Vec2d::new(x as f32 * TILE_SIZE, y as f32 * TILE_SIZE))
                .collect();
            let start = points[0];
            map.path_blocks.push(MovingPlatform {
                x: start.x,
                y: start.y,
                tile_type: tiles::PATH_BLOCK,
                active: true,
                vel_x: 0.0,
                vel_y: 0.0,
                original_tile_x: def.points[0].0 as i32,
                original_tile_y: def.points[0].1 as i32,
                destroy_timer: None,
                path: Some(PathState {
                    points,
                    closed: def.closed,
                    target: 1,
                    forward: true,
                }),
            });
        }
        map.original_path_blocks = map.path_blocks.clone();
        map
    }

    pub fn reset(&mut self) {
        // Restore all tiles to original state (but platforms are not in the grid)
        self.tiles = self.original_tiles.clone();
        for platform in &self.original_platforms {
            self.tiles[platform.original_tile_y as usize][platform.original_tile_x as usize] =
                tiles::EMPTY;
        }
        self.disappearing_tiles.clear();
        self.moving_platforms = self.original_platforms.clone();
        self.path_blocks = self.original_path_blocks.clone();
        self.periodic_timer = 0.0;
        self.coin_timer = 0.0;
        self.triggered_death_tiles.clear();
        self.coin_effects.clear();
    }

    pub fn tiles_of_type(&self, t: u32) -> Vec<Tile> {
        self.tiles
            .iter()
            .flatten()
            .enumerate()
            .filter(|(_, tile_type)| **tile_type == t)
            .map(|(index, &tile_type)| Tile {
                tile_type,
                x: index % self.width,
                y: index / self.width,
            })
            .collect::<Vec<Tile>>()
    }

    /// Every block the player physically interacts with: the one-shot moving
    /// platforms and the path-following blocks. They share the same struct and
    /// the player treats them identically (ride on top, get pushed/crushed).
    pub fn platforms(&self) -> impl Iterator<Item = &MovingPlatform> {
        self.moving_platforms.iter().chain(self.path_blocks.iter())
    }

    pub fn update(&mut self, delta_time: f32) {
        self.update_periodic_tiles(delta_time);
        self.update_crumbling_tiles(delta_time);
        self.update_moving_platforms(delta_time);
        self.update_path_blocks(delta_time);
        self.coin_timer = (self.coin_timer + delta_time) % COIN_ANIM_INTERVAL;

        // Advance coin-collection sparkles and drop them once both frames played
        let total = COIN_COLLECT_FRAME * COIN_COLLECT_SPRITES.len() as f32;
        for effect in &mut self.coin_effects {
            effect.timer += delta_time;
        }
        self.coin_effects.retain(|effect| effect.timer < total);
    }

    /// All periodic tiles swap between their solid and ghost phase on a
    /// shared clock, so they blink in a predictable rhythm
    fn update_periodic_tiles(&mut self, delta_time: f32) {
        self.periodic_timer += delta_time;
        if self.periodic_timer < PERIODIC_TILE_INTERVAL {
            return;
        }
        self.periodic_timer -= PERIODIC_TILE_INTERVAL;

        for row in &mut self.tiles {
            for tile in row.iter_mut() {
                match *tile {
                    tiles::PERIODIC_SOLID => *tile = tiles::PERIODIC_GHOST,
                    tiles::PERIODIC_GHOST => *tile = tiles::PERIODIC_SOLID,
                    _ => {}
                }
            }
        }
    }

    /// Touched crumbling tiles decay through their cracked states and
    /// finally disappear
    fn update_crumbling_tiles(&mut self, delta_time: f32) {
        let mut to_remove = Vec::new();

        for ((x, y), timer) in self.disappearing_tiles.iter_mut() {
            *timer -= delta_time;

            if *timer <= 0.0 {
                self.tiles[*y][*x] = tiles::EMPTY;
                to_remove.push((*x, *y));
            } else if *timer <= CRUMBLE_TIME_VERY_CRACKED {
                self.tiles[*y][*x] = tiles::CRUMBLE_VERY_CRACKED;
            } else if *timer <= CRUMBLE_TIME_CRACKED {
                self.tiles[*y][*x] = tiles::CRUMBLE_CRACKED;
            }
            // Above CRUMBLE_TIME_CRACKED the tile stays in its fresh state
        }

        for key in to_remove {
            self.disappearing_tiles.remove(&key);
        }
    }

    /// Active platforms travel in their direction until they hit a solid
    /// tile; platforms that leave the playable area are removed
    fn update_moving_platforms(&mut self, delta_time: f32) {
        let mut platforms_to_remove = Vec::new();

        let grid = &self.tiles;
        let width = self.width;
        let height = self.height;

        for (i, platform) in self.moving_platforms.iter_mut().enumerate() {
            if !platform.active {
                continue;
            }

            // A platform that already collided is winding down to destruction;
            // count it down and remove it once the delay elapses.
            if let Some(timer) = platform.destroy_timer.as_mut() {
                *timer -= delta_time;
                if *timer <= 0.0 {
                    platforms_to_remove.push(i);
                }
                continue;
            }

            platform.x += platform.vel_x * delta_time;
            platform.y += platform.vel_y * delta_time;

            // Remove platforms that are completely outside the playable area
            if platform.x + TILE_SIZE < 0.0
                || platform.x > width as f32 * TILE_SIZE
                || platform.y + TILE_SIZE < 0.0
                || platform.y > height as f32 * TILE_SIZE
            {
                platforms_to_remove.push(i);
                continue;
            }

            // Stop the platform when it overlaps a solid tile
            let tile_size = TILE_SIZE as i32;
            let left = platform.x as i32 / tile_size;
            let right = ((platform.x + TILE_SIZE) as i32 - 1) / tile_size;
            let top = platform.y as i32 / tile_size;
            let bottom = ((platform.y + TILE_SIZE) as i32 - 1) / tile_size;

            let mut collided = false;
            for ty in top..=bottom {
                for tx in left..=right {
                    if tx >= 0
                        && ty >= 0
                        && tx < width as i32
                        && ty < height as i32
                        && tiles::is_solid(grid[ty as usize][tx as usize])
                    {
                        collided = true;
                        break;
                    }
                }
                if collided {
                    break;
                }
            }

            if collided {
                platform.vel_x = 0.0;
                platform.vel_y = 0.0;
                platform.destroy_timer = Some(PLATFORM_DESTROY_DELAY);
            }
        }

        // Remove in reverse to preserve indices
        for i in platforms_to_remove.iter().rev() {
            self.moving_platforms.remove(*i);
        }
    }

    /// Advance each path-following block along its control points at a constant
    /// speed, turning corners and reversing or looping at the ends. `vel_x`/
    /// `vel_y` are set to the block's velocity on its current segment so the
    /// player's carry/push logic (which reads them) works just as it does for
    /// the one-shot moving platforms.
    fn update_path_blocks(&mut self, delta_time: f32) {
        for block in &mut self.path_blocks {
            let Some(path) = block.path.as_mut() else {
                continue;
            };

            let mut pos = Vec2d::new(block.x, block.y);
            let mut remaining = PLATFORM_SPEED * delta_time;
            // Direction of the last segment travelled this frame, used to expose
            // the block's velocity for the player carry/push logic.
            let mut dir = Vec2d::new(0.0, 0.0);

            // A frame may cross several short segments; keep moving until the
            // budget is spent. The bound guards against spinning forever should a
            // degenerate (zero-length) segment ever slip through validation.
            for _ in 0..path.points.len() {
                if remaining <= 0.0 {
                    break;
                }
                let target = path.points[path.target];
                let to = target - pos;
                let dist = (to.x * to.x + to.y * to.y).sqrt();

                if dist <= remaining {
                    pos = target;
                    remaining -= dist;
                    if dist > 0.0 {
                        dir = Vec2d::new(to.x / dist, to.y / dist);
                    }
                    path.advance();
                } else {
                    dir = Vec2d::new(to.x / dist, to.y / dist);
                    pos.x += dir.x * remaining;
                    pos.y += dir.y * remaining;
                    remaining = 0.0;
                }
            }

            block.x = pos.x;
            block.y = pos.y;
            block.vel_x = dir.x * PLATFORM_SPEED;
            block.vel_y = dir.y * PLATFORM_SPEED;
        }
    }

    /// Activate the (inactive) platform occupying the given tile position
    pub fn activate_platform(&mut self, tile_x: i32, tile_y: i32) {
        for platform in &mut self.moving_platforms {
            if platform.active {
                continue;
            }
            let px = (platform.x / TILE_SIZE) as i32;
            let py = (platform.y / TILE_SIZE) as i32;
            if px != tile_x || py != tile_y {
                continue;
            }

            platform.active = true;
            let (vel_x, vel_y) = match platform.tile_type {
                tiles::MOVE_UP => (0.0, -PLATFORM_SPEED),
                tiles::MOVE_RIGHT => (PLATFORM_SPEED, 0.0),
                tiles::MOVE_DOWN => (0.0, PLATFORM_SPEED),
                tiles::MOVE_LEFT => (-PLATFORM_SPEED, 0.0),
                _ => (0.0, 0.0),
            };
            platform.vel_x = vel_x;
            platform.vel_y = vel_y;
            break;
        }
    }

    /// Start the decay of a crumbling tile at the given position (no-op for
    /// other tile types or tiles already decaying)
    pub fn touch_tile(&mut self, tile_x: i32, tile_y: i32) {
        if tile_x < 0 || tile_y < 0 || tile_x >= self.width as i32 || tile_y >= self.height as i32 {
            return;
        }

        let x = tile_x as usize;
        let y = tile_y as usize;
        let remaining = match self.tiles[y][x] {
            tiles::CRUMBLE => CRUMBLE_TIME_FRESH,
            tiles::CRUMBLE_CRACKED => CRUMBLE_TIME_CRACKED,
            tiles::CRUMBLE_VERY_CRACKED => CRUMBLE_TIME_VERY_CRACKED,
            _ => return,
        };
        self.disappearing_tiles.entry((x, y)).or_insert(remaining);
    }

    /// Remove every coin the player's bounding box overlaps. Returns the number
    /// collected this frame.
    pub fn collect_coins(&mut self, player_bounds: &Rect) -> u32 {
        let mut collected = 0;
        let inset = (TILE_SIZE - COIN_DIAMETER) / 2.0;
        for tile in self.tiles_of_type(tiles::COIN) {
            if player_bounds.intersects(&tile.get_bounding_rect().shrink(inset)) {
                self.tiles[tile.y][tile.x] = tiles::EMPTY;
                self.coin_effects.push(CoinEffect {
                    tile_x: tile.x,
                    tile_y: tile.y,
                    timer: 0.0,
                });
                collected += 1;
            }
        }
        collected
    }

    /// Mark every death tile the player's bounds overlap as "triggered" so it
    /// renders its hit sprite until the level resets. Returns whether any death
    /// tile was hit.
    pub fn trigger_death_tiles(&mut self, player_bounds: &Rect) -> bool {
        let mut hit = false;
        for tile in self.tiles_of_type(tiles::DEATH) {
            if player_bounds.intersects(&tile.get_bounding_rect()) {
                self.triggered_death_tiles.insert((tile.x, tile.y));
                hit = true;
            }
        }
        hit
    }

    /// How many uncollected coins remain in the level.
    pub fn coins_remaining(&self) -> usize {
        self.tiles
            .iter()
            .flatten()
            .filter(|&&t| t == tiles::COIN)
            .count()
    }

    /// Exit doors stay shut until every coin has been collected.
    pub fn doors_open(&self) -> bool {
        self.coins_remaining() == 0
    }

    pub fn is_solid(&self, tile_x: i32, tile_y: i32) -> bool {
        if tile_x < 0 || tile_y < 0 || tile_x >= self.width as i32 || tile_y >= self.height as i32 {
            return false;
        }
        tiles::is_solid(self.tiles[tile_y as usize][tile_x as usize])
    }

    /// Get the tile type at a specific tile position
    pub fn get_tile(&self, tile_x: i32, tile_y: i32) -> u32 {
        if tile_x < 0 || tile_y < 0 || tile_x >= self.width as i32 || tile_y >= self.height as i32 {
            return tiles::EMPTY;
        }
        self.tiles[tile_y as usize][tile_x as usize]
    }

    /// Source rectangle for a 40x40 sprite at the given pixel coordinate in
    /// tilemap.png.
    fn sprite_rect(&self, (x, y): (i32, i32)) -> sdl2::rect::Rect {
        sdl2::rect::Rect::new(x, y, self.tile_size, self.tile_size)
    }

    /// Pixel coordinate of the coin sprite to draw this frame: the idle shimmer
    /// flashes its two extra sprites at the start of each cycle, then rests on
    /// the normal coin graphic for the remainder.
    fn coin_sprite(&self) -> (i32, i32) {
        if self.coin_timer < COIN_ANIM_FRAME {
            COIN_SHIMMER_SPRITES[0]
        } else if self.coin_timer < COIN_ANIM_FRAME * 2.0 {
            COIN_SHIMMER_SPRITES[1]
        } else {
            tiles::tile_src_xy(tiles::COIN)
        }
    }

    /// Pixel coordinate of a destroying platform's sprite for the given decay
    /// frame (0 = just collided, 1 = final stretch before removal).
    fn platform_destroy_sprite(tile_type: u32, frame: usize) -> (i32, i32) {
        match (tile_type, frame) {
            (tiles::MOVE_UP, 0) => (80, 80),
            (tiles::MOVE_UP, _) => (80, 120),
            (tiles::MOVE_RIGHT, 0) => (120, 80),
            (tiles::MOVE_RIGHT, _) => (120, 120),
            (tiles::MOVE_DOWN, 0) => (160, 80),
            (tiles::MOVE_DOWN, _) => (160, 120),
            (tiles::MOVE_LEFT, 0) => (200, 80),
            (tiles::MOVE_LEFT, _) => (200, 120),
            _ => (0, 0),
        }
    }

    pub fn render(
        &self,
        canvas: &mut WindowCanvas,
        texture: &Texture,
        camera_x: i32,
        camera_y: i32,
    ) {
        // Calculate which tiles are visible
        let start_col = (camera_x / self.tile_size as i32).max(0) as usize;
        let end_col =
            ((camera_x + 800) / self.tile_size as i32 + 1).min(self.width as i32) as usize;

        // Once all coins are collected, closed exit doors show the open sprite
        let doors_open = self.doors_open();

        for row in 0..self.height {
            for col in start_col..end_col {
                let tile_id = self.tiles[row][col];
                if tile_id == tiles::EMPTY {
                    continue;
                }

                // The grid only ever stores EXIT; swap in the open sprite for
                // rendering when the door is open.
                let sprite_id = if tile_id == tiles::EXIT && doors_open {
                    tiles::EXIT_OPEN
                } else {
                    tile_id
                };

                let dst_rect = sdl2::rect::Rect::new(
                    (col as i32 * self.tile_size as i32) - camera_x,
                    (row as i32 * self.tile_size as i32) - camera_y,
                    self.tile_size,
                    self.tile_size,
                );

                // Pick the source sprite: coins shimmer through their extra
                // sprites, and a death tile the player just hit shows its
                // angry-face sprite until the level resets on respawn.
                let sprite = if sprite_id == tiles::COIN {
                    self.coin_sprite()
                } else if sprite_id == tiles::DEATH
                    && self.triggered_death_tiles.contains(&(col, row))
                {
                    DEATH_HIT_SPRITE
                } else {
                    tiles::tile_src_xy(sprite_id)
                };

                canvas
                    .copy(texture, Some(self.sprite_rect(sprite)), Some(dst_rect))
                    .unwrap();
            }
        }

        // Decorations draw on top of the base tile grid but beneath the moving
        // platforms (and the player, which the caller draws last). They are
        // render-only, so they are positioned straight from their pixel
        // coordinates without any grid snapping.
        for deco in &self.decorations {
            let dst_rect = sdl2::rect::Rect::new(
                deco.x as i32 - camera_x,
                deco.y as i32 - camera_y,
                self.tile_size,
                self.tile_size,
            );
            let sprite = tiles::sheet_src_xy(deco.sprite);
            canvas
                .copy(texture, Some(self.sprite_rect(sprite)), Some(dst_rect))
                .unwrap();
        }

        for platform in self.platforms() {
            let dst_rect = sdl2::rect::Rect::new(
                platform.x as i32 - camera_x,
                platform.y as i32 - camera_y,
                self.tile_size,
                self.tile_size,
            );

            // A collided platform animates toward destruction, flashing its
            // first decay sprite the moment it collides and the second for the
            // final stretch, telegraphing its imminent removal.
            let sprite = match platform.destroy_timer {
                Some(timer) => {
                    let frame = if timer <= PLATFORM_DESTROY_DELAY / 2.0 { 1 } else { 0 };
                    Self::platform_destroy_sprite(platform.tile_type, frame)
                }
                None => tiles::tile_src_xy(platform.tile_type),
            };

            canvas
                .copy(texture, Some(self.sprite_rect(sprite)), Some(dst_rect))
                .unwrap();
        }

        // Coin-collection sparkles play their frames in sequence at the tile
        // the coin occupied.
        for effect in &self.coin_effects {
            let frame = (effect.timer / COIN_COLLECT_FRAME) as usize;
            let Some(&sprite) = COIN_COLLECT_SPRITES.get(frame) else {
                continue;
            };
            let src_rect = self.sprite_rect(sprite);
            let dst_rect = sdl2::rect::Rect::new(
                (effect.tile_x as i32 * self.tile_size as i32) - camera_x,
                (effect.tile_y as i32 * self.tile_size as i32) - camera_y,
                self.tile_size,
                self.tile_size,
            );
            canvas.copy(texture, Some(src_rect), Some(dst_rect)).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LevelData;

    /// Position of the single path block in a test map (it is the only platform).
    fn block_pos(map: &TileMap) -> (f32, f32) {
        let b = map.platforms().next().unwrap();
        (b.x, b.y)
    }

    #[test]
    fn open_path_block_moves_then_reverses() {
        // Horizontal open path from tile (1,1) to (3,1): 80px (0.8s) one way.
        let level = LevelData::parse("block: 1,1 3,1\n\nP.\n11").unwrap();
        let mut map = TileMap::from_level(&level);
        assert_eq!(block_pos(&map), (TILE_SIZE, TILE_SIZE), "starts on first point");

        map.update(0.5); // 50px toward the far end
        assert!((block_pos(&map).0 - (TILE_SIZE + 50.0)).abs() < 0.01);

        // Reaches the far end at 0.8s, then reverses for 0.2s (20px back).
        map.update(0.5);
        assert!((block_pos(&map).0 - (3.0 * TILE_SIZE - 20.0)).abs() < 0.01);
        assert!(
            map.platforms().next().unwrap().vel_x < 0.0,
            "velocity should point back toward the start after reversing"
        );
    }

    #[test]
    fn closed_loop_block_returns_to_start() {
        // A square loop with 80px sides: perimeter 320px = 3.2s at 100px/s.
        let level = LevelData::parse("block: 1,1 3,1 3,3 1,3 loop\n\nP.\n11").unwrap();
        let mut map = TileMap::from_level(&level);
        let start = block_pos(&map);

        map.update(3.2);
        let end = block_pos(&map);
        assert!(
            (end.0 - start.0).abs() < 0.01 && (end.1 - start.1).abs() < 0.01,
            "loop should return to start, got {:?} vs {:?}",
            end,
            start
        );
    }

    #[test]
    fn path_block_resets_to_start() {
        let level = LevelData::parse("block: 1,1 3,1\n\nP.\n11").unwrap();
        let mut map = TileMap::from_level(&level);
        map.update(0.5);
        map.reset();
        assert_eq!(block_pos(&map), (TILE_SIZE, TILE_SIZE));
    }

    #[test]
    fn decorations_are_carried_into_the_tilemap_without_touching_the_grid() {
        let level = LevelData::parse("deco: 40,80 27\n\nP.\n11").unwrap();
        let map = TileMap::from_level(&level);
        assert_eq!(map.decorations, level.decorations);
        // Decorations are render-only: the tile grid is unaffected.
        assert_eq!(map.tiles, vec![vec![0, 0], vec![1, 1]]);
    }
}
