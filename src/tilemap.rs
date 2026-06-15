use crate::geometry::rect::Rect;
use crate::geometry::vec2d::Vec2d;
use crate::tiles::{self, TILE_SIZE};
use sdl2::render::{Texture, WindowCanvas};
use std::collections::HashMap;

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
}

impl MovingPlatform {
    pub fn rect(&self) -> Rect {
        Rect::new(Vec2d::new(self.x, self.y), Vec2d::new(TILE_SIZE, TILE_SIZE))
    }
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
    periodic_timer: f32,                     // Shared clock for periodic tiles (7/8)
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
            periodic_timer: 0.0,
        }
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
        self.periodic_timer = 0.0;
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

    pub fn update(&mut self, delta_time: f32) {
        self.update_periodic_tiles(delta_time);
        self.update_crumbling_tiles(delta_time);
        self.update_moving_platforms(delta_time);
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
            }
        }

        // Remove in reverse to preserve indices
        for i in platforms_to_remove.iter().rev() {
            self.moving_platforms.remove(*i);
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

    /// Source rectangle of a tile's graphic inside tilemap.png
    fn tile_src_rect(&self, tile_id: u32) -> sdl2::rect::Rect {
        const TILES_PER_ROW: u32 = 6;
        let src_x = ((tile_id - 1) % TILES_PER_ROW) * self.tile_size;
        let src_y = ((tile_id - 1) / TILES_PER_ROW) * self.tile_size;
        sdl2::rect::Rect::new(src_x as i32, src_y as i32, self.tile_size, self.tile_size)
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

        for row in 0..self.height {
            for col in start_col..end_col {
                let tile_id = self.tiles[row][col];
                if tile_id == tiles::EMPTY {
                    continue;
                }

                let dst_rect = sdl2::rect::Rect::new(
                    (col as i32 * self.tile_size as i32) - camera_x,
                    (row as i32 * self.tile_size as i32) - camera_y,
                    self.tile_size,
                    self.tile_size,
                );

                canvas
                    .copy(texture, Some(self.tile_src_rect(tile_id)), Some(dst_rect))
                    .unwrap();
            }
        }

        for platform in &self.moving_platforms {
            let dst_rect = sdl2::rect::Rect::new(
                platform.x as i32 - camera_x,
                platform.y as i32 - camera_y,
                self.tile_size,
                self.tile_size,
            );

            canvas
                .copy(
                    texture,
                    Some(self.tile_src_rect(platform.tile_type)),
                    Some(dst_rect),
                )
                .unwrap();
        }
    }
}
