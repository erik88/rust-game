use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};
use std::collections::HashMap;

#[derive(Clone)]
pub struct MovingPlatform {
    pub x: f32,
    pub y: f32,
    pub tile_type: u32, // 9=up, 10=right, 11=down, 12=left
    pub active: bool,
    pub vel_x: f32,
    pub vel_y: f32,
    pub original_tile_x: i32,
    pub original_tile_y: i32,
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
}

impl TileMap {
    pub fn new() -> Self {
        // Create a sample level
        // 0 = empty, 1 = solid tile, 2 = different tile type
        let level_data = vec![
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 11, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 11, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0],
            vec![0, 0, 0, 1, 1, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 1, 1, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 12, 0, 0, 0, 0, 9, 0],
            vec![1, 1, 1, 1, 1, 4, 4, 1, 1, 1, 1, 4, 4, 1, 1, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0],
        ];

        let height = level_data.len();
        let width = level_data[0].len();

        // Extract moving platforms (tiles 9, 10, 11, 12)
        let mut platforms = Vec::new();
        let mut tiles = level_data.clone();

        for (y, row) in level_data.iter().enumerate() {
            for (x, &tile) in row.iter().enumerate() {
                if tile >= 9 && tile <= 12 {
                    platforms.push(MovingPlatform {
                        x: (x as i32 * 40) as f32,
                        y: (y as i32 * 40) as f32,
                        tile_type: tile,
                        active: false,
                        vel_x: 0.0,
                        vel_y: 0.0,
                        original_tile_x: x as i32,
                        original_tile_y: y as i32,
                    });
                    // Remove platform from static grid
                    tiles[y][x] = 0;
                }
            }
        }

        Self {
            width,
            height,
            tile_size: 40,
            original_tiles: level_data,
            tiles,
            disappearing_tiles: HashMap::new(),
            moving_platforms: platforms.clone(),
            original_platforms: platforms,
        }
    }

    pub fn reset(&mut self) {
        // Restore all tiles to original state (but platforms are not in the grid)
        self.tiles = self.original_tiles.clone();
        // Remove platforms from the grid again
        for platform in &self.original_platforms {
            self.tiles[platform.original_tile_y as usize][platform.original_tile_x as usize] = 0;
        }
        // Clear all disappearing tile timers
        self.disappearing_tiles.clear();
        // Reset platforms to original state
        self.moving_platforms = self.original_platforms.clone();
    }

    pub fn update(&mut self, delta_time: f32) {
        // Update disappearing tiles
        let mut to_remove = Vec::new();

        for ((x, y), timer) in self.disappearing_tiles.iter_mut() {
            *timer -= delta_time;

            // Animate tile decay based on remaining time
            if *timer <= 0.0 {
                // Fully disappeared
                self.tiles[*y][*x] = 0;
                to_remove.push((*x, *y));
            } else if *timer <= 0.3 {
                // Very cracked (type 6)
                self.tiles[*y][*x] = 6;
            } else if *timer <= 0.6 {
                // Cracked (type 5)
                self.tiles[*y][*x] = 5;
            }
            // If timer > 0.6, tile stays as type 4
        }

        // Clean up finished timers
        for key in to_remove {
            self.disappearing_tiles.remove(&key);
        }

        // Update moving platforms
        let mut platforms_to_remove = Vec::new();

        // Check collision for each platform
        let tiles = &self.tiles;
        let width = self.width;
        let height = self.height;
        let tile_size = self.tile_size;

        for (i, platform) in self.moving_platforms.iter_mut().enumerate() {
            if platform.active {
                // Move platform
                platform.x += platform.vel_x * delta_time;
                platform.y += platform.vel_y * delta_time;

                // Check if platform is completely outside playable area
                let platform_right = platform.x + tile_size as f32;
                let platform_bottom = platform.y + tile_size as f32;

                if platform_right < 0.0
                    || platform.x > (width as f32 * tile_size as f32)
                    || platform_bottom < 0.0
                    || platform.y > (height as f32 * tile_size as f32)
                {
                    platforms_to_remove.push(i);
                    continue;
                }

                // Check collision with static tiles
                let platform_left = platform.x as i32;
                let platform_right = (platform.x + tile_size as f32) as i32;
                let platform_top = platform.y as i32;
                let platform_bottom = (platform.y + tile_size as f32) as i32;

                let mut collided = false;

                for ty in (platform_top / tile_size as i32)
                    ..=((platform_bottom - 1) / tile_size as i32)
                {
                    for tx in (platform_left / tile_size as i32)
                        ..=((platform_right - 1) / tile_size as i32)
                    {
                        // Check if tile is solid directly
                        if tx >= 0 && ty >= 0 && tx < width as i32 && ty < height as i32 {
                            let tile_type = tiles[ty as usize][tx as usize];
                            if tile_type != 0 && tile_type != 3 {
                                collided = true;
                                break;
                            }
                        }
                    }
                    if collided {
                        break;
                    }
                }

                if collided {
                    // Stop the platform
                    platform.vel_x = 0.0;
                    platform.vel_y = 0.0;
                }
            }
        }

        // Remove platforms that went out of bounds (in reverse to preserve indices)
        for i in platforms_to_remove.iter().rev() {
            self.moving_platforms.remove(*i);
        }
    }

    pub fn activate_platform(&mut self, tile_x: i32, tile_y: i32) {
        const PLATFORM_SPEED: f32 = 100.0;

        for platform in &mut self.moving_platforms {
            if !platform.active {
                let px = (platform.x / self.tile_size as f32) as i32;
                let py = (platform.y / self.tile_size as f32) as i32;

                if px == tile_x && py == tile_y {
                    platform.active = true;
                    // Set velocity based on tile type
                    match platform.tile_type {
                        9 => {
                            // Up
                            platform.vel_x = 0.0;
                            platform.vel_y = -PLATFORM_SPEED;
                        }
                        10 => {
                            // Right
                            platform.vel_x = PLATFORM_SPEED;
                            platform.vel_y = 0.0;
                        }
                        11 => {
                            // Down
                            platform.vel_x = 0.0;
                            platform.vel_y = PLATFORM_SPEED;
                        }
                        12 => {
                            // Left
                            platform.vel_x = -PLATFORM_SPEED;
                            platform.vel_y = 0.0;
                        }
                        _ => {}
                    }
                    break;
                }
            }
        }
    }

    pub fn touch_tile(&mut self, tile_x: i32, tile_y: i32) {
        if tile_x < 0 || tile_y < 0 || tile_x >= self.width as i32 || tile_y >= self.height as i32 {
            return;
        }

        let x = tile_x as usize;
        let y = tile_y as usize;
        let tile_type = self.tiles[y][x];

        // Start disappearing timer if not already started
        if !self.disappearing_tiles.contains_key(&(x, y)) {
            match tile_type {
                4 => {
                    // Type 4: full 1.0 second decay (4 -> 5 -> 6 -> 0)
                    self.disappearing_tiles.insert((x, y), 1.0);
                }
                5 => {
                    // Type 5: 0.6 seconds remaining (5 -> 6 -> 0)
                    self.disappearing_tiles.insert((x, y), 0.6);
                }
                6 => {
                    // Type 6: 0.3 seconds remaining (6 -> 0)
                    self.disappearing_tiles.insert((x, y), 0.3);
                }
                _ => {}
            }
        }
    }

    pub fn is_solid(&self, tile_x: i32, tile_y: i32) -> bool {
        if tile_x < 0 || tile_y < 0 || tile_x >= self.width as i32 || tile_y >= self.height as i32 {
            return false;
        }
        let tile_type = self.tiles[tile_y as usize][tile_x as usize];
        // Type 3 (deadly) tiles are not solid, type 0 is empty
        // Types 4, 5, 6 are all solid (different decay states)
        tile_type != 0 && tile_type != 3
    }

    pub fn is_deadly(&self, tile_x: i32, tile_y: i32) -> bool {
        if tile_x < 0 || tile_y < 0 || tile_x >= self.width as i32 || tile_y >= self.height as i32 {
            return false;
        }
        self.tiles[tile_y as usize][tile_x as usize] == 3
    }

    pub fn render(&self, canvas: &mut WindowCanvas, texture: &Texture, camera_x: i32) {
        // Calculate which tiles are visible
        let start_col = (camera_x / self.tile_size as i32).max(0) as usize;
        let end_col = ((camera_x + 800) / self.tile_size as i32 + 1).min(self.width as i32) as usize;

        for row in 0..self.height {
            for col in start_col..end_col {
                let tile_id = self.tiles[row][col];
                if tile_id > 0 {
                    // Calculate source rectangle from tilemap texture
                    // Tiles are arranged in rows of 6
                    let tiles_per_row = 6;
                    let src_x = ((tile_id - 1) % tiles_per_row) * self.tile_size;
                    let src_y = ((tile_id - 1) / tiles_per_row) * self.tile_size;

                    let src_rect = Rect::new(
                        src_x as i32,
                        src_y as i32,
                        self.tile_size,
                        self.tile_size,
                    );

                    let dst_rect = Rect::new(
                        (col as i32 * self.tile_size as i32) - camera_x,
                        row as i32 * self.tile_size as i32,
                        self.tile_size,
                        self.tile_size,
                    );

                    canvas.copy(texture, Some(src_rect), Some(dst_rect)).unwrap();
                }
            }
        }

        // Render moving platforms
        for platform in &self.moving_platforms {
            let tiles_per_row = 6;
            let tile_id = platform.tile_type;
            let src_x = ((tile_id - 1) % tiles_per_row) * self.tile_size;
            let src_y = ((tile_id - 1) / tiles_per_row) * self.tile_size;

            let src_rect = Rect::new(
                src_x as i32,
                src_y as i32,
                self.tile_size,
                self.tile_size,
            );

            let dst_rect = Rect::new(
                platform.x as i32 - camera_x,
                platform.y as i32,
                self.tile_size,
                self.tile_size,
            );

            canvas.copy(texture, Some(src_rect), Some(dst_rect)).unwrap();
        }
    }
}
