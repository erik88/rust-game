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
    on_ground: bool,
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

        // Move horizontally with collision
        self.x += self.vel_x * delta_time;
        self.resolve_x_collision(tilemap);

        // Move vertically with collision
        self.y += self.vel_y * delta_time;
        self.resolve_y_collision(tilemap);

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
        let player_bottom = self.y + self.height as f32;

        // Check if player is standing on any platform
        let mut platform_to_activate: Option<(i32, i32)> = None;
        let mut platform_vel: Option<(f32, f32)> = None;

        for platform in &tilemap.moving_platforms {
            let platform_left = platform.x;
            let platform_right = platform.x + tilemap.tile_size as f32;
            let platform_top = platform.y;

            // Check if player's feet are touching the top of the platform
            // Use a tighter tolerance to avoid sticking when jumping
            let feet_touching = player_bottom >= platform_top
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
        }

        // Activate platform if needed (outside the loop to avoid borrow issues)
        if let Some((px, py)) = platform_to_activate {
            tilemap.activate_platform(px, py);
        }

        // Move player with platform, but check for collisions separately for X and Y
        if let Some((vel_x, vel_y)) = platform_vel {
            let move_x = vel_x * delta_time;
            let move_y = vel_y * delta_time;

            let old_x = self.x;
            let old_y = self.y;

            // Try horizontal movement
            if move_x.abs() > 0.01 {
                self.x += move_x;

                // Check for collision after X movement
                let mut x_collided = false;
                let player_left = self.x as i32;
                let player_right = (self.x + self.width as f32) as i32;
                let player_top = self.y as i32;
                let player_bottom = (self.y + self.height as f32) as i32;

                for ty in (player_top / tilemap.tile_size as i32)..=((player_bottom - 1) / tilemap.tile_size as i32) {
                    for tx in (player_left / tilemap.tile_size as i32)..=((player_right - 1) / tilemap.tile_size as i32) {
                        if tilemap.is_solid(tx, ty) {
                            x_collided = true;
                            break;
                        }
                    }
                    if x_collided {
                        break;
                    }
                }

                if x_collided {
                    self.x = old_x; // Revert X movement
                }
            }

            // Try vertical movement
            if move_y.abs() > 0.01 {
                self.y += move_y;

                // Check for collision after Y movement
                let mut y_collided = false;
                let player_left = self.x as i32;
                let player_right = (self.x + self.width as f32) as i32;
                let player_top = self.y as i32;
                let player_bottom = (self.y + self.height as f32) as i32;

                for ty in (player_top / tilemap.tile_size as i32)..=((player_bottom - 1) / tilemap.tile_size as i32) {
                    for tx in (player_left / tilemap.tile_size as i32)..=((player_right - 1) / tilemap.tile_size as i32) {
                        if tilemap.is_solid(tx, ty) {
                            y_collided = true;
                            break;
                        }
                    }
                    if y_collided {
                        break;
                    }
                }

                if y_collided {
                    self.y = old_y; // Revert Y movement
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
                    return true;
                }
            }
        }
        false
    }

    fn resolve_x_collision(&mut self, tilemap: &TileMap) {
        let player_left = self.x as i32;
        let player_right = (self.x + self.width as f32) as i32;
        let player_top = self.y as i32;
        let player_bottom = (self.y + self.height as f32) as i32;

        // Check tiles the player overlaps
        let top_tile = (player_top as f32 / tilemap.tile_size as f32).floor() as i32;
        let bottom_tile = ((player_bottom - 1) as f32 / tilemap.tile_size as f32).floor() as i32;
        let left_tile = (player_left as f32 / tilemap.tile_size as f32).floor() as i32;
        let right_tile = ((player_right - 1) as f32 / tilemap.tile_size as f32).floor() as i32;

        let mut collision_resolved = false;
        for ty in top_tile..=bottom_tile {
            for tx in left_tile..=right_tile {
                if tilemap.is_solid(tx, ty) {
                    let tile_left = tx * tilemap.tile_size as i32;
                    let tile_right = tile_left + tilemap.tile_size as i32;

                    // Moving right
                    if self.vel_x > 0.0 {
                        self.x = tile_left as f32 - self.width as f32;
                        self.vel_x = 0.0;
                        collision_resolved = true;
                        break;
                    }
                    // Moving left
                    else if self.vel_x < 0.0 {
                        self.x = tile_right as f32;
                        self.vel_x = 0.0;
                        collision_resolved = true;
                        break;
                    }
                }
            }
            if collision_resolved {
                break;
            }
        }

        // Check collision with moving platforms
        if !collision_resolved {
            let player_left_f = self.x;
            let player_right_f = self.x + self.width as f32;
            let player_top_f = self.y;
            let player_bottom_f = self.y + self.height as f32;

            for platform in &tilemap.moving_platforms {
                let platform_left = platform.x;
                let platform_right = platform.x + tilemap.tile_size as f32;
                let platform_top = platform.y;
                let platform_bottom = platform.y + tilemap.tile_size as f32;

                // Check if player overlaps platform horizontally and vertically
                let overlaps_x = player_right_f > platform_left && player_left_f < platform_right;
                let overlaps_y = player_bottom_f > platform_top && player_top_f < platform_bottom;

                if overlaps_x && overlaps_y {
                    // Moving right
                    if self.vel_x > 0.0 {
                        self.x = platform_left - self.width as f32;
                        self.vel_x = 0.0;
                        break;
                    }
                    // Moving left
                    else if self.vel_x < 0.0 {
                        self.x = platform_right;
                        self.vel_x = 0.0;
                        break;
                    }
                }
            }
        }
    }

    fn resolve_y_collision(&mut self, tilemap: &TileMap) {
        let player_left = self.x as i32;
        let player_right = (self.x + self.width as f32) as i32;
        let player_top = self.y as i32;
        let player_bottom = (self.y + self.height as f32) as i32;

        self.on_ground = false;

        // Check tiles the player overlaps - use ceiling to include partial overlaps
        let top_tile = (player_top as f32 / tilemap.tile_size as f32).floor() as i32;
        let bottom_tile = ((player_bottom - 1) as f32 / tilemap.tile_size as f32).floor() as i32;
        let left_tile = (player_left as f32 / tilemap.tile_size as f32).floor() as i32;
        let right_tile = ((player_right - 1) as f32 / tilemap.tile_size as f32).floor() as i32;

        let mut collision_resolved = false;
        for ty in top_tile..=bottom_tile {
            for tx in left_tile..=right_tile {
                if tilemap.is_solid(tx, ty) {
                    let tile_top = ty * tilemap.tile_size as i32;
                    let tile_bottom = tile_top + tilemap.tile_size as i32;

                    // Moving down or resting on ground
                    if self.vel_y >= 0.0 {
                        self.y = tile_top as f32 - self.height as f32;
                        self.vel_y = 0.0;
                        collision_resolved = true;
                        break;
                    }
                    // Moving up
                    else if self.vel_y < 0.0 {
                        self.y = tile_bottom as f32;
                        self.vel_y = 0.0;
                        collision_resolved = true;
                        break;
                    }
                }
            }
            if collision_resolved {
                break;
            }
        }

        // Check collision with moving platforms
        if !collision_resolved {
            let player_left_f = self.x;
            let player_right_f = self.x + self.width as f32;
            let player_top_f = self.y;
            let player_bottom_f = self.y + self.height as f32;

            for platform in &tilemap.moving_platforms {
                let platform_left = platform.x;
                let platform_right = platform.x + tilemap.tile_size as f32;
                let platform_top = platform.y;
                let platform_bottom = platform.y + tilemap.tile_size as f32;

                // Check if player overlaps platform horizontally and vertically
                let overlaps_x = player_right_f > platform_left && player_left_f < platform_right;
                let overlaps_y = player_bottom_f > platform_top && player_top_f < platform_bottom;

                if overlaps_x && overlaps_y {
                    // Moving down
                    if self.vel_y >= 0.0 {
                        self.y = platform_top - self.height as f32;
                        self.vel_y = 0.0;
                        break;
                    }
                    // Moving up
                    else if self.vel_y < 0.0 {
                        self.y = platform_bottom;
                        self.vel_y = 0.0;
                        break;
                    }
                }
            }
        }

        // Check if player is standing on ground (check tiles just below player's feet)
        let feet_y = player_bottom;
        let tile_below_y = (feet_y as f32 / tilemap.tile_size as f32).floor() as i32;

        for tx in left_tile..=right_tile {
            if tilemap.is_solid(tx, tile_below_y) {
                self.on_ground = true;
                break;
            }
        }

        // Also check if standing on a platform
        let player_left_f = self.x;
        let player_right_f = self.x + self.width as f32;
        let player_bottom_f = self.y + self.height as f32;

        for platform in &tilemap.moving_platforms {
            let platform_left = platform.x;
            let platform_right = platform.x + tilemap.tile_size as f32;
            let platform_top = platform.y;

            // Check if player's feet are on top of the platform
            // Use tighter tolerance to match handle_platforms
            if player_bottom_f >= platform_top
                && player_bottom_f <= platform_top + 2.0
                && player_right_f > platform_left
                && player_left_f < platform_right
            {
                self.on_ground = true;
                break;
            }
        }
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
