use crate::geometry::rect::Rect;
use crate::geometry::vec2d::Vec2d;
use crate::input::InputState;
use crate::tilemap::{MovingPlatform, TileMap};
use sdl2::render::{Texture, WindowCanvas};

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub on_ground: bool,
    was_on_ground: bool,

    // Spawn and death
    pub is_dead: bool,
    spawn_x: f32,
    spawn_y: f32,

    // Animation
    frame: usize,
    frame_time: f32,
    facing_right: bool,
}

pub const PLAYER_WIDTH: u32 = 16;
pub const PLAYER_HEIGHT: u32 = 38;

const PLAYER_SPEED: f32 = 150.0;
const JUMP_SPEED: f32 = 400.0;
const GRAVITY: f32 = 1200.0;
const JUMP_HOLD_GRAVITY: f32 = 800.0; // Reduced gravity while holding jump
const JUMP_RELEASE_DAMPING: f32 = 0.5; // Velocity multiplier when jump is released
const FRAME_DURATION: f32 = 0.25;
// Vertical tolerance for treating the player's feet as standing on a platform.
// Platforms move before the player each frame, so this must exceed the distance
// a platform travels in one frame (100 px/s * delta_time), or the player loses
// his footing on a slow frame and the platform passes through him.
const PLATFORM_RIDE_TOLERANCE: f32 = 6.0;

/// True if feet at `player_bottom` are within riding tolerance of a
/// platform's top edge
fn near_platform_top(player_bottom: f32, platform_top: f32) -> bool {
    player_bottom >= platform_top - PLATFORM_RIDE_TOLERANCE
        && player_bottom <= platform_top + PLATFORM_RIDE_TOLERANCE
}

impl Player {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            width: PLAYER_WIDTH,
            height: PLAYER_HEIGHT,
            vel_x: 0.0,
            vel_y: 0.0,
            on_ground: false,
            was_on_ground: false,
            spawn_x: x,
            spawn_y: y,
            is_dead: false,
            frame: 0,
            frame_time: 0.0,
            facing_right: true,
        }
    }

    pub fn position(&self) -> Vec2d {
        Vec2d::new(self.x, self.y)
    }

    pub fn bounding_rect(&self) -> Rect {
        Rect::new(
            Vec2d::new(self.x, self.y),
            Vec2d::new(self.width as f32, self.height as f32),
        )
    }

    /// If the player is slightly inside a solid tile (e.g. a periodic tile that
    /// just flipped from ghost to solid), push them out along the axis of least
    /// penetration. Only acts when the penetration depth is small enough that
    /// the player clearly just grazed the edge; larger overlaps are left alone.
    pub fn try_unstick(&mut self, tilemap: &TileMap) {
        const MAX_UNSTICK_DEPTH: f32 = 8.0;

        if !self.check_collision_at(self.x, self.y, tilemap) {
            return;
        }

        let tile_size = tilemap.tile_size as f32;
        let player_right = self.x + self.width as f32;
        let player_bottom = self.y + self.height as f32;

        let left_tile = (self.x / tile_size).floor() as i32;
        let right_tile = ((player_right - 1.0) / tile_size).floor() as i32;
        let top_tile = (self.y / tile_size).floor() as i32;
        let bottom_tile = ((player_bottom - 1.0) / tile_size).floor() as i32;

        let mut best_depth = f32::INFINITY;
        let mut best_dx = 0.0f32;
        let mut best_dy = 0.0f32;

        for ty in top_tile..=bottom_tile {
            for tx in left_tile..=right_tile {
                if !tilemap.is_solid(tx, ty) {
                    continue;
                }

                let tile_left = tx as f32 * tile_size;
                let tile_right = tile_left + tile_size;
                let tile_top = ty as f32 * tile_size;
                let tile_bottom = tile_top + tile_size;

                // Penetration depth in each direction
                let candidates = [
                    (player_right - tile_left, -(player_right - tile_left), 0.0f32),
                    (tile_right - self.x, tile_right - self.x, 0.0f32),
                    (player_bottom - tile_top, 0.0f32, -(player_bottom - tile_top)),
                    (tile_bottom - self.y, 0.0f32, tile_bottom - self.y),
                ];

                if let Some(&(depth, dx, dy)) = candidates
                    .iter()
                    .filter(|&&(d, _, _)| d > 0.0)
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                {
                    if depth < best_depth {
                        best_depth = depth;
                        best_dx = dx;
                        best_dy = dy;
                    }
                }
            }
        }

        if best_depth <= MAX_UNSTICK_DEPTH {
            let new_x = self.x + best_dx;
            let new_y = self.y + best_dy;
            if !self.check_collision_at(new_x, new_y, tilemap) {
                self.x = new_x;
                self.y = new_y;
            }
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
        if !self.on_ground && self.vel_y >= 0.0 {
            self.on_ground = tilemap
                .moving_platforms
                .iter()
                .any(|platform| self.feet_on_platform(platform));
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

        self.update_animation(delta_time);
    }

    fn update_animation(&mut self, delta_time: f32) {
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

    /// True if the player's feet rest on top of the platform: vertically
    /// within the riding tolerance and horizontally overlapping it
    fn feet_on_platform(&self, platform: &MovingPlatform) -> bool {
        let rect = platform.rect();
        near_platform_top(self.y + self.height as f32, rect.position.y)
            && self.x + self.width as f32 > rect.position.x
            && self.x < rect.position.x + rect.size.x
    }

    /// True if the player's whole width lies within the platform's, i.e. he is
    /// standing entirely on it rather than overhanging an edge.
    fn fully_on_platform(&self, platform: &MovingPlatform) -> bool {
        let rect = platform.rect();
        self.x >= rect.position.x && self.x + self.width as f32 <= rect.position.x + rect.size.x
    }

    /// True if a solid tile sits directly beneath the player's feet, i.e. solid
    /// ground (not a platform) is helping hold him up. A player still propped up
    /// by solid ground while stepping onto a platform hasn't committed to it yet.
    fn on_solid_ground(&self, tilemap: &TileMap) -> bool {
        let tile_size = tilemap.tile_size as f32;
        // Probe the tile row immediately beneath the player's feet
        let tile_y = ((self.y + self.height as f32) / tile_size).floor() as i32;
        let left_tile = (self.x / tile_size).floor() as i32;
        let right_tile = ((self.x + self.width as f32 - 1.0) / tile_size).floor() as i32;
        (left_tile..=right_tile).any(|tx| tilemap.is_solid(tx, tile_y))
    }

    fn resolve_x_position(&self, target_x: f32, tilemap: &TileMap) -> f32 {
        // Binary search to find the closest valid X position
        let start_x = self.x;
        let mut low = start_x.min(target_x);
        let mut high = start_x.max(target_x);

        // Use binary search to find the exact collision geometry
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

        // Use binary search to find the exact collision geometry
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
            (x, y),                                                      // Top-left
            (x + self.width as f32 - 1.0, y),                            // Top-right
            (x, y + self.height as f32 - 1.0),                           // Bottom-left
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
        let player_rect = Rect::new(
            Vec2d::new(x, y),
            Vec2d::new(self.width as f32, self.height as f32),
        );

        for platform in &tilemap.moving_platforms {
            // Exception: if player's feet are on or near the top surface of the platform,
            // don't treat it as a collision (allows walking onto platform from the side)
            let feet_on_top = near_platform_top(y + self.height as f32, platform.y);

            if !feet_on_top && player_rect.intersects(&platform.rect()) {
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

        // Whether solid ground (not a platform) is currently supporting the
        // player. While true, stepping onto a platform must not start it - the
        // player has somewhere else to stand and hasn't committed to it.
        let supported_by_ground = self.on_solid_ground(tilemap);

        // Check if player is standing on any platform
        let mut platform_to_activate: Option<(i32, i32)> = None;
        // (vel_x, platform_top) of the platform the player is riding
        let mut riding: Option<(f32, f32)> = None;
        let mut platform_push: Option<f32> = None; // Horizontal push from platform beside player

        for platform in &tilemap.moving_platforms {
            // Check if player's feet are touching the top of the platform
            if self.feet_on_platform(platform) && self.vel_y >= 0.0 {
                // Player is standing on this platform. Activate it once he is
                // entirely on top, or as soon as it is his only support (he is
                // not also resting on solid ground) - otherwise a player still
                // straddling solid ground keeps it dormant until fully aboard.
                if !platform.active
                    && (self.fully_on_platform(platform) || !supported_by_ground)
                {
                    let px = (platform.x / tilemap.tile_size as f32) as i32;
                    let py = (platform.y / tilemap.tile_size as f32) as i32;
                    platform_to_activate = Some((px, py));
                }

                // Store platform info to move player
                riding = Some((platform.vel_x, platform.y));
                break;
            }

            // Check if horizontally moving platform is beside the player and should push them
            if platform.active && platform.vel_x.abs() > 0.01 {
                let platform_rect = platform.rect();
                let platform_left = platform_rect.position.x;
                let platform_right = platform_rect.position.x + platform_rect.size.x;

                // Check vertical alignment - player and platform must overlap vertically
                let vertical_overlap = player_bottom > platform_rect.position.y
                    && player_top < platform_rect.position.y + platform_rect.size.y;

                if vertical_overlap {
                    // Platform moving right, check if it's to the left of player
                    if platform.vel_x > 0.0
                        && platform_right >= player_left - 2.0
                        && platform_right <= player_left + 2.0
                    {
                        platform_push = Some(platform.vel_x);
                    }
                    // Platform moving left, check if it's to the right of player
                    else if platform.vel_x < 0.0
                        && platform_left <= player_right + 2.0
                        && platform_left >= player_right - 2.0
                    {
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
        if let Some((vel_x, platform_top)) = riding {
            let move_x = vel_x * delta_time;

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

            // Vertical carry: the platform already moved this frame (the tilemap
            // updates before the player), so place the player's feet directly on
            // its top instead of integrating the platform velocity. Gravity must
            // also be cancelled here - the platform never registers as a vertical
            // collision while riding (the feet_on_top exception), so vel_y would
            // otherwise grow each frame until the player sinks out of the riding
            // tolerance and gets trapped inside upward-moving platforms.
            let snap_y = platform_top - self.height as f32;
            if self.check_collision_at(self.x, snap_y, tilemap) {
                // Platform is pushing player into obstacle - resolve to edge
                self.y = self.resolve_y_position(self.x, snap_y, tilemap);
            } else {
                self.y = snap_y;
            }
            self.vel_y = 0.0;
        }
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
        self.is_dead = false;
    }

    pub fn render(
        &self,
        canvas: &mut WindowCanvas,
        texture: &Texture,
        camera_x: i32,
        camera_y: i32,
    ) {
        let src_rect = sdl2::rect::Rect::new(
            (self.frame * self.width as usize) as i32,
            0,
            self.width,
            self.height,
        );

        let dst_rect = sdl2::rect::Rect::new(
            self.x as i32 - camera_x,
            self.y as i32 - camera_y,
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
