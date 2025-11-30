use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};
use crate::input::InputState;
use crate::tilemap::TileMap;

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub on_ground: bool,
    was_on_ground: bool,

    // Spawn position for reset
    spawn_x: f32,
    spawn_y: f32,

    // Animation
    frame: usize,
    frame_time: f32,
    facing_right: bool,
}

const PLAYER_SPEED: f32 = 150.0;
const JUMP_SPEED: f32 = 400.0;
const GRAVITY: f32 = 1200.0;
const JUMP_HOLD_GRAVITY: f32 = 800.0; // Reduced gravity while holding jump
const JUMP_RELEASE_DAMPING: f32 = 0.5; // Velocity multiplier when jump is released
const FRAME_DURATION: f32 = 0.25;

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            width: 16,
            height: 38,
            vel_x: 0.0,
            vel_y: 0.0,
            on_ground: false,
            was_on_ground: false,
            spawn_x: x,
            spawn_y: y,
            frame: 0,
            frame_time: 0.0,
            facing_right: true,
        }
    }

    pub fn update(&mut self, input: &InputState, tilemap: &mut TileMap, delta_time: f32) {
        // Store previous ground state
        self.was_on_ground = self.on_ground;

        // Horizontal movement
        self.vel_x = 0.0;
        if input.left {
            self.vel_x = -PLAYER_SPEED;
            self.facing_right = false;
        }
        if input.right {
            self.vel_x = PLAYER_SPEED;
            self.facing_right = true;
        }

        // Variable jump height: if player releases jump while going up, cut the jump short
        if !input.jump && self.vel_y < 0.0 {
            self.vel_y *= JUMP_RELEASE_DAMPING;
        }

        // Apply gravity (reduced while holding jump and moving upward)
        let current_gravity = if input.jump && self.vel_y < 0.0 {
            JUMP_HOLD_GRAVITY
        } else {
            GRAVITY
        };
        self.vel_y += current_gravity * delta_time;

        // Calculate potential new position
        let mut new_x = self.x + self.vel_x * delta_time;
        let mut new_y = self.y + self.vel_y * delta_time;

        // Check and adjust horizontal movement
        if self.check_collision_at(new_x, self.y, tilemap) {
            // Collision detected - try to slide to the edge of the obstacle
            new_x = self.resolve_x_position(new_x, tilemap);
            self.vel_x = 0.0;
        }

        // Check and adjust vertical movement
        if self.check_collision_at(new_x, new_y, tilemap) {
            // Collision detected - try to slide to the edge of the obstacle
            new_y = self.resolve_y_position(new_x, new_y, tilemap);
            self.vel_y = 0.0;
        }

        // Apply the adjusted position
        self.x = new_x;
        self.y = new_y;

        // Check if player is on ground (tiles)
        self.on_ground = self.check_collision_at(self.x, self.y + 1.0, tilemap);

        // Also check if standing on a platform
        if !self.on_ground {
            let player_left = self.x;
            let player_right = self.x + self.width as f32;
            let player_bottom = self.y + self.height as f32;

            for platform in &tilemap.moving_platforms {
                let platform_left = platform.x;
                let platform_right = platform.x + tilemap.tile_size as f32;
                let platform_top = platform.y;

                // Check if player's feet are on top of the platform
                // Use wider tolerance to catch player positioned slightly above platform
                let feet_on_platform = player_bottom >= platform_top - 3.0
                    && player_bottom <= platform_top + 2.0
                    && player_right > platform_left
                    && player_left < platform_right;

                if feet_on_platform && self.vel_y >= 0.0 {
                    self.on_ground = true;
                    break;
                }
            }
        }

        // Touch all tiles the player is currently overlapping
        self.touch_tiles(tilemap);

        // Handle platform interactions (includes collision checking)
        self.handle_platforms(tilemap, delta_time);

        // Clamp player to level bounds
        let level_width = (tilemap.width as f32) * (tilemap.tile_size as f32);
        self.x = self.x.max(0.0).min(level_width - self.width as f32);

        // Jump logic (after collision so we know if we just landed)
        let just_landed = !self.was_on_ground && self.on_ground;

        // Jump if: just pressed jump while grounded, OR just landed while holding jump
        if self.on_ground && (input.jump_pressed || (just_landed && input.jump)) {
            self.vel_y = -JUMP_SPEED;
            self.on_ground = false;
        }

        // Update animation
        if self.vel_x.abs() > 0.1 && self.on_ground {
            // If we just started walking (frame is 0), set it to 1
            if self.frame == 0 {
                self.frame = 1;
            }

            self.frame_time += delta_time;
            if self.frame_time >= FRAME_DURATION {
                self.frame_time = 0.0;
                // Alternate between frames 1 and 2 for walking
                if self.frame == 1 {
                    self.frame = 2;
                } else {
                    self.frame = 1;
                }
            }
        } else {
            // Frame 0 is idle
            self.frame = 0;
            self.frame_time = 0.0;
        }
    }

    fn resolve_x_position(&self, target_x: f32, tilemap: &TileMap) -> f32 {
        // Binary search to find the closest valid X position
        let start_x = self.x;
        let mut low = start_x.min(target_x);
        let mut high = start_x.max(target_x);

        // Use binary search to find the exact collision point
        for _ in 0..10 {
            let mid = (low + high) / 2.0;
            if self.check_collision_at(mid, self.y, tilemap) {
                if target_x > start_x {
                    high = mid; // Moving right, collision ahead
                } else {
                    low = mid; // Moving left, collision ahead
                }
            } else {
                if target_x > start_x {
                    low = mid; // Moving right, no collision yet
                } else {
                    high = mid; // Moving left, no collision yet
                }
            }
        }

        // Return the safe position
        if target_x > start_x {
            low // Moving right, return leftmost safe position
        } else {
            high // Moving left, return rightmost safe position
        }
    }

    fn resolve_y_position(&self, x: f32, target_y: f32, tilemap: &TileMap) -> f32 {
        // Binary search to find the closest valid Y position
        let start_y = self.y;
        let mut low = start_y.min(target_y);
        let mut high = start_y.max(target_y);

        // Use binary search to find the exact collision point
        for _ in 0..10 {
            let mid = (low + high) / 2.0;
            if self.check_collision_at(x, mid, tilemap) {
                if target_y > start_y {
                    high = mid; // Moving down, collision below
                } else {
                    low = mid; // Moving up, collision above
                }
            } else {
                if target_y > start_y {
                    low = mid; // Moving down, no collision yet
                } else {
                    high = mid; // Moving up, no collision yet
                }
            }
        }

        // Return the safe position
        if target_y > start_y {
            low // Moving down, return highest safe position
        } else {
            high // Moving up, return lowest safe position
        }
    }

    fn check_collision_at(&self, x: f32, y: f32, tilemap: &TileMap) -> bool {
        // Define the player's corners at the given position
        let corners = [
            (x, y),                                           // Top-left
            (x + self.width as f32 - 1.0, y),                 // Top-right
            (x, y + self.height as f32 - 1.0),                // Bottom-left
            (x + self.width as f32 - 1.0, y + self.height as f32 - 1.0), // Bottom-right
        ];

        // Check if any corner is inside a solid tile
        for &(corner_x, corner_y) in &corners {
            let tile_x = (corner_x / tilemap.tile_size as f32).floor() as i32;
            let tile_y = (corner_y / tilemap.tile_size as f32).floor() as i32;

            if tilemap.is_solid(tile_x, tile_y) {
                return true;
            }
        }

        // Check collision with moving platforms (they are solid obstacles)
        let player_left = x;
        let player_right = x + self.width as f32;
        let player_top = y;
        let player_bottom = y + self.height as f32;

        for platform in &tilemap.moving_platforms {
            let platform_left = platform.x;
            let platform_right = platform.x + tilemap.tile_size as f32;
            let platform_top = platform.y;
            let platform_bottom = platform.y + tilemap.tile_size as f32;

            // Check if player overlaps platform
            let overlaps_x = player_right > platform_left && player_left < platform_right;
            let overlaps_y = player_bottom > platform_top && player_top < platform_bottom;

            if overlaps_x && overlaps_y {
                return true;
            }
        }

        false
    }

    fn touch_tiles(&self, tilemap: &mut TileMap) {
        let player_left = self.x as i32;
        let player_right = (self.x + self.width as f32) as i32;
        let player_top = self.y as i32;
        let player_bottom = (self.y + self.height as f32) as i32;

        // Touch all tiles the player overlaps or is standing on
        // Don't subtract 1 here so we include tiles the player is exactly touching
        let top_tile = player_top / tilemap.tile_size as i32;
        let bottom_tile = player_bottom / tilemap.tile_size as i32;
        let left_tile = player_left / tilemap.tile_size as i32;
        let right_tile = player_right / tilemap.tile_size as i32;

        for ty in top_tile..=bottom_tile {
            for tx in left_tile..=right_tile {
                tilemap.touch_tile(tx, ty);
            }
        }
    }

    fn handle_platforms(&mut self, tilemap: &mut TileMap, delta_time: f32) {
        let player_left = self.x;
        let player_right = self.x + self.width as f32;
        let player_top = self.y;
        let player_bottom = self.y + self.height as f32;

        // Check if player is standing on any platform
        let mut platform_to_activate: Option<(i32, i32)> = None;
        let mut platform_vel: Option<(f32, f32)> = None;
        let mut platform_push: Option<f32> = None; // Horizontal push from platform beside player

        for platform in &tilemap.moving_platforms {
            let platform_left = platform.x;
            let platform_right = platform.x + tilemap.tile_size as f32;
            let platform_top = platform.y;
            let platform_bottom = platform.y + tilemap.tile_size as f32;

            // Check if player's feet are touching the top of the platform
            // Use wider tolerance to catch player positioned slightly above platform
            let feet_touching = player_bottom >= platform_top - 3.0
                && player_bottom <= platform_top + 2.0
                && player_right > platform_left
                && player_left < platform_right;

            if feet_touching && self.vel_y >= 0.0 {
                // Player is standing on this platform
                // Mark platform for activation if not already active
                if !platform.active {
                    let px = (platform.x / tilemap.tile_size as f32) as i32;
                    let py = (platform.y / tilemap.tile_size as f32) as i32;
                    platform_to_activate = Some((px, py));
                }

                // Store platform velocity to move player
                platform_vel = Some((platform.vel_x, platform.vel_y));
                break;
            }

            // Check if horizontally moving platform is beside the player and should push them
            if platform.active && platform.vel_x.abs() > 0.01 {
                // Check vertical alignment - player and platform must overlap vertically
                let vertical_overlap = player_bottom > platform_top && player_top < platform_bottom;

                if vertical_overlap {
                    // Platform moving right, check if it's to the left of player
                    if platform.vel_x > 0.0 && platform_right >= player_left - 2.0 && platform_right <= player_left + 2.0 {
                        platform_push = Some(platform.vel_x);
                    }
                    // Platform moving left, check if it's to the right of player
                    else if platform.vel_x < 0.0 && platform_left <= player_right + 2.0 && platform_left >= player_right - 2.0 {
                        platform_push = Some(platform.vel_x);
                    }
                }
            }
        }

        // Activate platform if needed (outside the loop to avoid borrow issues)
        if let Some((px, py)) = platform_to_activate {
            tilemap.activate_platform(px, py);
        }

        // Handle platform pushing from the side
        if let Some(push_vel_x) = platform_push {
            let move_x = push_vel_x * delta_time;
            let new_x = self.x + move_x;
            if self.check_collision_at(new_x, self.y, tilemap) {
                // Platform is pushing player into obstacle - resolve to edge
                self.x = self.resolve_x_position(new_x, tilemap);
            } else {
                self.x = new_x;
            }
        }

        // Move player with platform - platform carries the player
        if let Some((vel_x, vel_y)) = platform_vel {
            let move_x = vel_x * delta_time;
            let move_y = vel_y * delta_time;

            // Try horizontal movement
            if move_x.abs() > 0.01 {
                let new_x = self.x + move_x;
                if self.check_collision_at(new_x, self.y, tilemap) {
                    // Platform is pushing player into obstacle - resolve to edge
                    self.x = self.resolve_x_position(new_x, tilemap);
                } else {
                    self.x = new_x;
                }
            }

            // Try vertical movement
            if move_y.abs() > 0.01 {
                let new_y = self.y + move_y;
                if self.check_collision_at(self.x, new_y, tilemap) {
                    // Platform is pushing player into obstacle - resolve to edge
                    self.y = self.resolve_y_position(self.x, new_y, tilemap);
                } else {
                    self.y = new_y;
                }
            }
        }
    }

    pub fn is_touching_deadly_tile(&self, tilemap: &TileMap) -> bool {
        let player_left = self.x as i32;
        let player_right = (self.x + self.width as f32) as i32;
        let player_top = self.y as i32;
        let player_bottom = (self.y + self.height as f32) as i32;

        // Check if touching any deadly tiles
        let top_tile = player_top / tilemap.tile_size as i32;
        let bottom_tile = player_bottom / tilemap.tile_size as i32;
        let left_tile = player_left / tilemap.tile_size as i32;
        let right_tile = player_right / tilemap.tile_size as i32;

        for ty in top_tile..=bottom_tile {
            for tx in left_tile..=right_tile {
                if tilemap.is_deadly(tx, ty) {
                    // If player is on ground and this deadly tile is at their feet level,
                    // it's safe (standing on top of deadly tile)
                    if self.on_ground && ty == bottom_tile {
                        // Check if player's feet are actually on top of this tile
                        let tile_top = (ty as f32) * (tilemap.tile_size as f32);
                        let feet_y = self.y + self.height as f32;

                        // If feet are within a few pixels of the tile top, they're standing on it
                        if (feet_y - tile_top).abs() < 3.0 {
                            continue; // Safe to stand on top of deadly tile
                        }
                    }

                    // Otherwise, touching deadly tile is fatal
                    return true;
                }
            }
        }
        false
    }

    pub fn is_dead(&self, screen_height: u32) -> bool {
        // Player is dead if they fall below the screen
        self.y > screen_height as f32 + 100.0
    }

    pub fn reset(&mut self) {
        self.x = self.spawn_x;
        self.y = self.spawn_y;
        self.vel_x = 0.0;
        self.vel_y = 0.0;
        self.on_ground = false;
        self.was_on_ground = false;
        self.frame = 0;
        self.frame_time = 0.0;
        self.facing_right = true;
    }

    pub fn render(&self, canvas: &mut WindowCanvas, texture: &Texture, camera_x: i32) {
        let src_rect = Rect::new(
            (self.frame * self.width as usize) as i32,
            0,
            self.width,
            self.height,
        );

        let dst_rect = Rect::new(
            self.x as i32 - camera_x,
            self.y as i32,
            self.width,
            self.height,
        );

        canvas
            .copy_ex(
                texture,
                Some(src_rect),
                Some(dst_rect),
                0.0,
                None,
                !self.facing_right,
                false,
            )
            .unwrap();
    }
}
