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

    // Death animation
    death_frame: usize,
    death_anim_timer: f32,
    pub death_anim_done: bool,
}

pub const PLAYER_WIDTH: u32 = 16;
pub const PLAYER_HEIGHT: u32 = 38;

const PLAYER_SPEED: f32 = 150.0;
const JUMP_SPEED: f32 = 400.0;
const GRAVITY: f32 = 1200.0;
const JUMP_HOLD_GRAVITY: f32 = 800.0; // Reduced gravity while holding jump
const JUMP_RELEASE_DAMPING: f32 = 0.5; // Velocity multiplier when jump is released
const FRAME_DURATION: f32 = 0.25;
const DEATH_FRAME_DURATION: f32 = 0.15;
const DEATH_FRAMES: usize = 3;
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
            death_frame: 0,
            death_anim_timer: 0.0,
            death_anim_done: false,
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

    pub fn update_death_animation(&mut self, delta_time: f32) {
        if self.death_anim_done {
            return;
        }
        self.death_anim_timer += delta_time;
        if self.death_anim_timer >= DEATH_FRAME_DURATION {
            self.death_anim_timer -= DEATH_FRAME_DURATION;
            if self.death_frame < DEATH_FRAMES - 1 {
                self.death_frame += 1;
            } else {
                self.death_anim_done = true;
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

        // Check and adjust vertical movement. Skip this when the player is
        // embedded in the *side* of a horizontally-moving platform: the tilemap
        // updates before the player, so a right/left-moving platform shoves into
        // the player's body before he gets a chance to move. Treating that side
        // overlap as a vertical collision would zero vel_y and hold the player
        // aloft ("surfing" in the air). Let gravity keep pulling him down and
        // leave the horizontal ejection to handle_platforms().
        // A descending block overlapping the player from above must not block
        // his downward motion: it presses down on him, it cannot hold him up.
        // Resolving it as a floor would zero vel_y and trap him inside the
        // block. Moving *up* into it still resolves normally, so jumping into
        // its underside instantly kills the jump. The matching downward push is
        // applied in handle_platforms().
        let falling_under_block =
            self.vel_y >= 0.0 && self.pressed_from_above_by_block(new_x, new_y, tilemap);
        if self.check_collision_at(new_x, new_y, tilemap)
            && !self.embedded_in_moving_platform(new_x, self.y, tilemap)
            && !falling_under_block
        {
            // Collision detected - try to slide to the edge of the obstacle
            new_y = self.resolve_y_position(new_x, new_y, tilemap);
            self.vel_y = 0.0;
        }

        // Apply the adjusted position
        self.x = new_x;
        self.y = new_y;

        // Check if player is on ground. Probe solid *tiles* only, one pixel
        // beneath the feet. A moving platform or path block must never count
        // here: an overhead descending block would masquerade as ground (and,
        // while holding jump, auto-jump him into its underside), and a block
        // overlapping his side would let him jump off its flank in mid-air.
        // Platforms genuinely supporting him from below are handled by the
        // `feet_on_platform` check immediately after.
        self.on_ground = self.solid_tile_at(self.x, self.y + 1.0, tilemap);

        // Also check if standing on a platform
        if !self.on_ground && self.vel_y >= 0.0 {
            self.on_ground = tilemap
                .platforms()
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

    /// True if the player at `(x, y)` overlaps a horizontally-moving platform
    /// from the side rather than resting on its top. This happens because the
    /// platform moves before the player each frame and shoves into his body; the
    /// overlap must not be mistaken for a floor (see the vertical-collision check
    /// in `update`), or the player surfs in the air instead of falling.
    fn embedded_in_moving_platform(&self, x: f32, y: f32, tilemap: &TileMap) -> bool {
        let player_rect = Rect::new(
            Vec2d::new(x, y),
            Vec2d::new(self.width as f32, self.height as f32),
        );
        tilemap.platforms().any(|platform| {
            platform.active
                && platform.vel_x.abs() > 0.01
                && !near_platform_top(y + self.height as f32, platform.y)
                && player_rect.intersects(&platform.rect())
        })
    }

    /// True if at `(x, y)` the player is overlapped from above by an active,
    /// downward-moving block: the block's top sits at or above the player's top
    /// and its underside has cut into the player's body. Such a block presses
    /// *down* on him - it must never arrest his fall (that would glue him to the
    /// block's speed or trap him inside it). The downward push is applied
    /// separately in `handle_platforms`.
    fn pressed_from_above_by_block(&self, x: f32, y: f32, tilemap: &TileMap) -> bool {
        let player_rect = Rect::new(
            Vec2d::new(x, y),
            Vec2d::new(self.width as f32, self.height as f32),
        );
        tilemap.platforms().any(|platform| {
            platform.active
                && platform.vel_y > 0.0
                && platform.y <= y
                && player_rect.intersects(&platform.rect())
        })
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

    /// True if any of the player's corners at `(x, y)` lie inside a solid tile.
    /// Unlike [`check_collision_at`](Self::check_collision_at) this ignores moving
    /// platforms and path blocks, so it is the correct probe for detecting solid
    /// ground beneath the feet: a block merely overlapping the player from the
    /// *side* must never read as ground (that would let him jump off a block's
    /// flank in mid-air). Platforms supporting him from below are handled
    /// separately via [`feet_on_platform`](Self::feet_on_platform).
    fn solid_tile_at(&self, x: f32, y: f32, tilemap: &TileMap) -> bool {
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

        false
    }

    fn check_collision_at(&self, x: f32, y: f32, tilemap: &TileMap) -> bool {
        // Solid tiles first
        if self.solid_tile_at(x, y, tilemap) {
            return true;
        }

        // Check collision with moving platforms (they are solid obstacles)
        let player_rect = Rect::new(
            Vec2d::new(x, y),
            Vec2d::new(self.width as f32, self.height as f32),
        );

        for platform in tilemap.platforms() {
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

        for platform in tilemap.platforms() {
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
                // Platform squeezing player into a wall = crush death
                self.is_dead = true;
                return;
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
                // Upward-moving platform squeezing player into solid above = crush death
                if snap_y < self.y {
                    self.is_dead = true;
                    return;
                }
                // Platform is pushing player into obstacle - resolve to edge
                self.y = self.resolve_y_position(self.x, snap_y, tilemap);
            } else {
                self.y = snap_y;
            }
            self.vel_y = 0.0;
        }

        // A downward-moving block pressing on the player from above. The block
        // can never hold him up: it shoves him down while it descends faster
        // than he is falling, and otherwise he simply falls away from it under
        // gravity - whichever is the greater downward motion wins. If solid
        // support sits beneath him so he cannot be pushed clear, he is crushed.
        for platform in tilemap.platforms() {
            if !platform.active || platform.vel_y <= 0.0 {
                continue;
            }
            let rect = platform.rect();
            let h_overlap = self.x + self.width as f32 > rect.position.x
                && self.x < rect.position.x + rect.size.x;
            if !h_overlap {
                continue;
            }
            // Only act when the block is above the player with its underside
            // cutting into him; a block at or below his feet is a ride/up-push
            // case handled earlier.
            let block_bottom = rect.position.y + rect.size.y;
            if rect.position.y <= self.y && block_bottom > self.y {
                // Push him down flush with the block's underside, unless solid
                // ground blocks the way - then he is crushed against it.
                if self.check_collision_at(self.x, block_bottom, tilemap) {
                    self.is_dead = true;
                    return;
                }
                self.y = block_bottom;
                // Hand the block's downward speed to the player so he is flung
                // off its underside instead of being carried along at the
                // block's pace. Gravity then accelerates him clear of the block
                // within a frame - he can never ride beneath it, not even while
                // holding jump (which only matters while moving upward). This
                // also discards any leftover upward jump velocity.
                self.vel_y = self.vel_y.max(platform.vel_y);
            }
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
        self.death_frame = 0;
        self.death_anim_timer = 0.0;
        self.death_anim_done = false;
    }

    pub fn render(
        &self,
        canvas: &mut WindowCanvas,
        texture: &Texture,
        camera_x: i32,
        camera_y: i32,
    ) {
        let (src_x, src_y) = if self.is_dead {
            ((self.death_frame * self.width as usize) as i32, self.height as i32)
        } else {
            ((self.frame * self.width as usize) as i32, 0)
        };
        let src_rect = sdl2::rect::Rect::new(src_x, src_y, self.width, self.height);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputState;
    use crate::level::LevelData;
    use crate::tiles;

    /// Build a 10x10 level of open air with a single right-moving platform
    /// (tile 10) occupying tile (2, 2) — its rect spans x in [80, 120],
    /// y in [80, 120]. The platform is activated and starts moving right.
    fn map_with_active_right_platform() -> TileMap {
        let mut grid = vec![vec![tiles::EMPTY; 10]; 10];
        grid[2][2] = tiles::MOVE_RIGHT;
        let mut tilemap = TileMap::from_data(grid);
        tilemap.activate_platform(2, 2);
        tilemap
    }

    /// Regression test: a player pressing *into* a horizontally-moving platform
    /// while airborne must keep falling. The platform moves before the player
    /// each frame and shoves into his side; that side overlap previously got
    /// mistaken for a floor, zeroing vel_y and letting the player "surf" in the
    /// air. See `embedded_in_moving_platform` and the vertical-collision check.
    #[test]
    fn player_falls_while_pushed_by_horizontal_platform() {
        let mut tilemap = map_with_active_right_platform();

        // Spawn the player airborne, flush against the platform's right edge
        // (platform right edge = 120) with no ground anywhere beneath him.
        let mut player = Player::new(120.0, 85.0);
        let start_y = player.y;

        // Hold left, i.e. push against the oncoming right-moving platform.
        let input = InputState {
            left: true,
            ..Default::default()
        };

        let dt = 1.0 / 60.0;
        // Mirror the engine loop: the tilemap (platforms) updates before the player.
        // After a few frames, while the player still overlaps the platform
        // vertically, he should have been carried sideways (the intended "pushed
        // ahead" behaviour) *and* dropped from gravity rather than hovering.
        for _ in 0..5 {
            tilemap.update(dt);
            player.update(&input, &mut tilemap, dt);
        }
        assert!(
            player.x > 120.0,
            "player should be pushed right by the platform, but x = {}",
            player.x
        );
        assert!(
            player.y > start_y,
            "player should already be falling while pushed, but y went {} -> {}",
            start_y,
            player.y
        );

        // Letting it run on, he keeps accelerating downward instead of surfing.
        for _ in 0..25 {
            tilemap.update(dt);
            player.update(&input, &mut tilemap, dt);
        }
        assert!(
            player.y > start_y + 20.0,
            "player should keep falling, but y went {} -> {}",
            start_y,
            player.y
        );
    }

    /// Regression test: a player who jumps upward into the underside of a
    /// downward-moving path block must not die. The block descends onto his head
    /// in mid-air with nothing solid beneath him, so it can only block/shove him,
    /// never crush him. The crush death only applies when a descending block
    /// pins the player against solid ground below; this guards that distinction.
    #[test]
    fn player_survives_jumping_up_into_a_descending_path_block() {
        // A vertical path block in column 3 travelling from tile row 1 down to
        // row 8. It starts on its top point (pixel rect x in [120,160],
        // y in [40,80]) and moves downward. The grid is otherwise empty air, so
        // there is no floor for the block to crush the player against.
        let level = LevelData::parse(
            "block: 3,1 3,8\n\n........\n........\n........\n........\n........\n........\n........\n........\n........\nP.......",
        )
        .unwrap();
        let mut tilemap = TileMap::from_level(&level);

        // Spawn the player just below the block's bottom edge (y = 80) and
        // horizontally inside its span, then launch him upward into it. Nothing
        // solid sits beneath him.
        let mut player = Player::new(130.0, 82.0);
        player.vel_y = -JUMP_SPEED;

        let input = InputState::default();
        let dt = 1.0 / 60.0;

        // Run a full second: long enough for the descending block to travel down
        // through the player's starting region and shove against him repeatedly.
        for _ in 0..60 {
            // Mirror the engine loop: the tilemap (path blocks) updates before
            // the player.
            tilemap.update(dt);
            player.update(&input, &mut tilemap, dt);
            assert!(
                !player.is_dead,
                "player must not die when a descending block pushes him in mid-air \
                 (player at y = {})",
                player.y
            );
        }
    }

    /// Regression test: standing beside a path block and merely pushing into its
    /// side must not let the player jump. A horizontally-moving block overlaps the
    /// player's torso before he resolves each frame; that side overlap previously
    /// read as "ground" (via `check_collision_at`), so holding/pressing jump while
    /// shoved against a block let him re-jump off its flank and climb far higher
    /// than a normal jump. The block touches his side, not his feet, so he must
    /// stay airborne.
    #[test]
    fn cannot_jump_off_the_side_of_a_path_block() {
        // A horizontal path block starting at tile (2,2) — rect x in [80,120],
        // y in [80,120] — moving right. The level is otherwise empty air, so
        // there is no floor anywhere beneath the player.
        let level = LevelData::parse(
            "block: 2,2 5,2\n\n........\n........\n........\n........\n........\n........\n........\nP.......",
        )
        .unwrap();
        let mut tilemap = TileMap::from_level(&level);

        // Player hanging against the block's right flank: his left edge overlaps
        // the block's right portion, his feet (y+38 = 123) hang well below the
        // block's underside (120), so he is plainly not standing on top.
        let mut player = Player::new(115.0, 85.0);
        let start_y = player.y;

        // Press jump hard against the block (held + freshly pressed) and push
        // left into it, the worst case for spuriously re-jumping.
        let input = InputState {
            jump: true,
            jump_pressed: true,
            left: true,
            ..Default::default()
        };
        let dt = 1.0 / 60.0;

        for frame in 0..120 {
            tilemap.update(dt);
            player.update(&input, &mut tilemap, dt);

            // He is never grounded by the block alone, so the jump never fires:
            // he only ever falls away from his start, never rises above it.
            assert!(
                !player.on_ground,
                "frame {frame}: a block at the player's side must not count as ground"
            );
            assert!(
                player.y >= start_y - 1.0,
                "frame {frame}: player jumped off the block's side and rose to y = {}",
                player.y
            );
        }
    }

    /// A player who jumps up into the underside of a descending path block must
    /// instantly lose his jump and be flung downward - never *ride* flush along
    /// the block's underside. Once he is hit he must fall strictly faster than
    /// the block descends, pulling away from it under gravity. This must hold
    /// even while *holding jump*: a block overhead must not register as ground
    /// (which would auto-jump him every frame and glue him to the underside).
    #[test]
    fn cannot_ride_below_a_descending_block_even_holding_jump() {
        for hold_jump in [false, true] {
            let level = LevelData::parse(
                "block: 3,1 3,8\n\n........\n........\n........\n........\n........\n........\n........\n........\n........\nP.......",
            )
            .unwrap();
            let mut tilemap = TileMap::from_level(&level);

            // Player just below the block's underside (y = 80), inside its
            // column, launched straight up into it.
            let mut player = Player::new(130.0, 82.0);
            player.vel_y = -JUMP_SPEED;
            let start_y = player.y;

            let input = InputState {
                jump: hold_jump,
                ..Default::default()
            };
            let dt = 1.0 / 60.0;

            let mut prev_gap = f32::NEG_INFINITY;
            for frame in 0..30 {
                tilemap.update(dt);
                // The block's underside and descent speed *after* this frame's
                // move.
                let (block_bottom, block_speed) = {
                    let b = tilemap.platforms().next().unwrap();
                    (b.y + b.rect().size.y, b.vel_y)
                };

                player.update(&input, &mut tilemap, dt);
                let gap = player.y - block_bottom; // player's top below the block

                assert!(!player.is_dead, "hold={hold_jump} frame {frame}: must not die");
                // The upward jump is gone immediately and never returns.
                assert!(
                    player.vel_y >= -0.01,
                    "hold={hold_jump} frame {frame}: jump should be lost, vel_y = {}",
                    player.vel_y
                );
                // He never wedges up inside the block (stays at/below its
                // underside) and never surfs back above his start.
                assert!(
                    gap >= -0.01,
                    "hold={hold_jump} frame {frame}: player wedged into block (gap {gap})"
                );
                assert!(
                    player.y >= start_y - 1.0,
                    "hold={hold_jump} frame {frame}: player rode back up past start (y {})",
                    player.y
                );

                // Past the initial contact frames he must be outrunning the
                // block: falling faster than it descends, with the gap to its
                // underside strictly growing - i.e. not riding it.
                if frame >= 2 {
                    assert!(
                        player.vel_y > block_speed,
                        "hold={hold_jump} frame {frame}: player should fall faster than \
                         the block ({} vs {block_speed})",
                        player.vel_y
                    );
                    assert!(
                        gap > prev_gap + 0.01,
                        "hold={hold_jump} frame {frame}: gap to the block should keep \
                         growing ({prev_gap} -> {gap})"
                    );
                }
                prev_gap = gap;
            }

            // He ends up well clear below the block, not stuck at the spawn.
            assert!(
                player.y > start_y + 100.0,
                "hold={hold_jump}: player should have fallen well clear, only reached y = {}",
                player.y
            );
        }
    }
}
