use crate::geometry::rect::Rect;
use crate::geometry::vec2d::Vec2d;
use crate::input::InputState;
use crate::tilemap::{MovingPlatform, TileMap};
use crate::tiles::TILE_SIZE;
use sdl2::render::{Texture, WindowCanvas};

/// Phases of the exit sequence the player plays after reaching an open door.
#[derive(Clone, Copy, PartialEq)]
enum ExitPhase {
    /// Airborne when the door was reached: fall until the feet reach the door's
    /// base height before walking in.
    Landing,
    /// Walk horizontally towards the middle of the door at walking speed.
    Walking,
    /// Play the "stepping into the door" frames (third sprite-sheet row).
    Entering,
}

/// Progress of the death animation.
#[derive(Clone, Copy)]
struct DeathAnim {
    frame: usize,
    timer: f32,
    done: bool,
}

/// Progress of the exit sequence: the player is first eased into alignment
/// with the door, then plays the "stepping into the door" animation (third
/// sprite-sheet row) where he shrinks inwards.
#[derive(Clone, Copy)]
struct ExitAnim {
    phase: ExitPhase,
    /// Where the player's bounding box eases to: centred on the door with his
    /// feet resting on its base.
    target: Vec2d,
    frame: usize,
    timer: f32,
    done: bool,
}

/// What the player is currently doing. Exactly one of these holds at a time,
/// and each animation's transient state lives only inside its variant, so
/// impossible combinations (dead *and* exiting, exit fields read outside an
/// exit) cannot be expressed.
#[derive(Clone, Copy)]
enum PlayerState {
    /// Normal play: input, physics and collisions apply.
    Alive,
    /// Playing the death animation; the world is frozen around him.
    Dying(DeathAnim),
    /// Playing the exit sequence into a door; the world is frozen around him.
    Exiting(ExitAnim),
}

/// Which ground cycle the moving frames are drawn from. Tracked rather than
/// derived in `render` because the exit sequence walks the player towards a
/// door without touching `vel_x`, so speed alone can't be trusted there.
#[derive(Clone, Copy, PartialEq)]
enum Gait {
    /// The two-frame cycle in the death row's spare cells.
    Walking,
    /// The original, longer-strided cycle in the first row.
    Running,
}

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub on_ground: bool,

    state: PlayerState,

    // Spawn position, for reset()
    spawn_x: f32,
    spawn_y: f32,

    /// Steps since the last jump press, so `<= JUMP_BUFFER_STEPS` means a press
    /// is still pending. `WINDOW_SPENT` once a jump has consumed it.
    steps_since_press: u8,
    /// Steps since he was last on the ground, so `<= COYOTE_STEPS` means a jump
    /// is still allowed. `WINDOW_SPENT` once a jump has consumed it.
    airborne_steps: u8,

    // Animation
    frame: usize,
    frame_time: f32,
    gait: Gait,
    facing_right: bool,
}

pub const PLAYER_WIDTH: u32 = 16;
pub const PLAYER_HEIGHT: u32 = 38;

const PLAYER_SPEED: f32 = 150.0;
// Top speed while the run button is held. Starting, stopping and turning stay
// instant at PLAYER_SPEED; only the stretch above walking pace ramps, so the
// character keeps its tight feel and running adds momentum on top rather than
// replacing the handling.
const RUN_SPEED: f32 = 260.0;
// Build-up is slower than bleed-off, so reaching top speed feels earned but
// letting go hands control straight back.
const RUN_ACCELERATION: f32 = 500.0;
const RUN_DECELERATION: f32 = 800.0;
const JUMP_SPEED: f32 = 400.0;
// Both jump-forgiveness windows, in fixed physics steps (the step is always
// FIXED_DT, so steps and time are interchangeable here; 3 steps is 50 ms).
//
// A press is remembered for this long, so pressing a hair *early* - just
// before touching down - still jumps on landing instead of being swallowed.
const JUMP_BUFFER_STEPS: u8 = 3;
// A jump stays allowed for this long after he leaves the ground, so pressing a
// hair *late* - just after walking off a lip - still jumps instead of nothing.
const COYOTE_STEPS: u8 = 3;
// Parks either counter past its window. Jumping spends both, so that one press
// and one lift-off can never be cashed a second time in mid-air.
const WINDOW_SPENT: u8 = u8::MAX;
const GRAVITY: f32 = 1200.0;
const JUMP_HOLD_GRAVITY: f32 = 800.0; // Reduced gravity while holding jump
const JUMP_RELEASE_DAMPING: f32 = 0.5; // Velocity multiplier when jump is released
const FRAME_DURATION: f32 = 0.25;
// First frame of the running cycle, held as the pose while rising.
const JUMP_FRAME: usize = 1;
// The two walking frames sit in the death row's spare cells, just right of its
// three frames; the running cycle is frames 1 and 2 of the first row. Frame 0
// there is the idle pose, shared by both gaits.
const WALK_SRC_X: i32 = (DEATH_FRAMES * PLAYER_WIDTH as usize) as i32;
// Walking pins `vel_x` to exactly PLAYER_SPEED, so anything above it is the run
// ramping in. The margin only absorbs float drift, not a real speed difference.
const RUN_POSE_MARGIN: f32 = 1.0;
// The falling pose lives in the top-right corner of the sheet. Its white cape
// streams out behind the character, so the frame is wider than all the others:
// only the drawn sprite grows, the collision box stays PLAYER_WIDTH.
const FALL_SRC_X: i32 = 60;
const FALL_FRAME_WIDTH: u32 = 20;
// How fast he must be dropping before the cape streams out. Swapping the pose
// the instant `vel_y` turns positive makes the cape flick out at the apex of
// every little hop; waiting for real downward speed keeps the rising pose
// through the top of the arc and lets the cape catch as the fall builds. At
// GRAVITY this is a touch over 0.1 s (and ~9 px) past the apex.
const FALL_POSE_SPEED: f32 = 150.0;
// Distance from a frame's left edge to the centre of the head, for the normal
// frames and for the wider falling one. The falling frame is placed by its head
// rather than by its left edge, so the body stays over the collision box and
// only the cape hangs off the side.
const HEAD_CENTER_X: i32 = 7;
const FALL_HEAD_CENTER_X: i32 = 11;
const DEATH_FRAME_DURATION: f32 = 0.15;
const DEATH_FRAMES: usize = 3;
// Each "stepping into the door" frame plays for this long. Deliberately slower
// than the walk/death frames so the shrink-inwards effect reads clearly.
const EXIT_FRAME_DURATION: f32 = 0.225;
const EXIT_FRAMES: usize = 3;
// Vertical tolerance for treating the player's feet as standing on a platform.
// Platforms move before the player each frame, so this must exceed the distance
// a platform travels in one frame (100 px/s * delta_time), or the player loses
// his footing on a slow frame and the platform passes through him. `time`'s
// MAX_DELTA caps delta_time at 50 ms to keep that true (100 * 0.05 = 5 px).
const PLATFORM_RIDE_TOLERANCE: f32 = 6.0;

/// True if feet at `player_bottom` are within riding tolerance of a
/// platform's top edge
fn near_platform_top(player_bottom: f32, platform_top: f32) -> bool {
    player_bottom >= platform_top - PLATFORM_RIDE_TOLERANCE
        && player_bottom <= platform_top + PLATFORM_RIDE_TOLERANCE
}

/// How the moving platforms touch the player this frame, gathered by
/// [`Player::scan_platform_contacts`].
#[derive(Default)]
struct PlatformContacts {
    /// Tile position of a dormant platform the player just committed to,
    /// which should start moving.
    activate: Option<(i32, i32)>,
    /// `(vel_x, platform_top)` of the platform the player is riding.
    riding: Option<(f32, f32)>,
    /// Horizontal velocity of a platform shoving the player at his side.
    side_push: Option<f32>,
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
            steps_since_press: WINDOW_SPENT,
            // He spawns in mid-air above the ground, and must not get a free
            // jump out of it before his feet have touched anything.
            airborne_steps: WINDOW_SPENT,
            state: PlayerState::Alive,
            spawn_x: x,
            spawn_y: y,
            frame: 0,
            frame_time: 0.0,
            gait: Gait::Walking,
            facing_right: true,
        }
    }

    /// True while the player is playing his death animation.
    pub fn is_dead(&self) -> bool {
        matches!(self.state, PlayerState::Dying(_))
    }

    /// True while the player is playing the exit sequence into a door.
    pub fn is_exiting(&self) -> bool {
        matches!(self.state, PlayerState::Exiting(_))
    }

    /// Start the death animation. Only an alive player can die; killing him
    /// twice in one frame (e.g. two overlapping death tiles) is a no-op.
    pub fn kill(&mut self) {
        if matches!(self.state, PlayerState::Alive) {
            self.state = PlayerState::Dying(DeathAnim {
                frame: 0,
                timer: 0.0,
                done: false,
            });
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

        let tile_size = TILE_SIZE;
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

    /// Advance the death animation. Returns true once its final frame has been
    /// shown (and keeps returning true after that). Returns false when the
    /// player is not dying at all.
    pub fn update_death_animation(&mut self, delta_time: f32) -> bool {
        let PlayerState::Dying(mut anim) = self.state else {
            return false;
        };
        if !anim.done {
            anim.timer += delta_time;
            if anim.timer >= DEATH_FRAME_DURATION {
                anim.timer -= DEATH_FRAME_DURATION;
                if anim.frame < DEATH_FRAMES - 1 {
                    anim.frame += 1;
                } else {
                    anim.done = true;
                }
            }
            self.state = PlayerState::Dying(anim);
        }
        anim.done
    }

    /// Begin the exit sequence towards the door at `(target_x, target_y)`. The
    /// player walks into the middle of the door (landing first if airborne),
    /// then plays the "stepping inside" animation. No-op unless he is alive.
    pub fn start_exit(&mut self, target_x: f32, target_y: f32) {
        if !matches!(self.state, PlayerState::Alive) {
            return;
        }
        self.state = PlayerState::Exiting(ExitAnim {
            // If he is already on the ground he can walk straight in; otherwise
            // he first falls to the door's base height.
            phase: if self.on_ground {
                ExitPhase::Walking
            } else {
                ExitPhase::Landing
            },
            target: Vec2d::new(target_x, target_y),
            frame: 0,
            timer: 0.0,
            done: false,
        });
        // vel_x is intentionally kept: an airborne entry carries its horizontal
        // momentum through the fall (see the Landing phase).
    }

    /// Advance the exit sequence: land (if airborne), walk to the middle of the
    /// door, then play the 3-frame "entering the door" animation. Returns true
    /// once the final frame has been shown (and keeps returning true after
    /// that). Returns false when the player is not exiting at all.
    pub fn update_exit_animation(&mut self, delta_time: f32) -> bool {
        let PlayerState::Exiting(mut anim) = self.state else {
            return false;
        };
        if anim.done {
            return true;
        }

        match anim.phase {
            ExitPhase::Landing => {
                // Keep whatever horizontal momentum the jump had while falling to
                // the door's base height, but stop the moment he reaches the
                // door's middle so a sideways jump doesn't overshoot and then
                // have to walk back in.
                let before_x = self.x;
                self.x += self.vel_x * delta_time;
                if (before_x - anim.target.x).signum() != (self.x - anim.target.x).signum() {
                    self.x = anim.target.x;
                    self.vel_x = 0.0;
                }
                self.vel_y += GRAVITY * delta_time;
                self.y += self.vel_y * delta_time;
                if self.y >= anim.target.y {
                    self.y = anim.target.y;
                    self.vel_y = 0.0;
                    anim.phase = ExitPhase::Walking;
                }
            }
            ExitPhase::Walking => {
                // Step horizontally towards the door's middle at walking speed,
                // playing the walk cycle, then begin the entering animation.
                let dx = anim.target.x - self.x;
                let step = PLAYER_SPEED * delta_time;
                if dx.abs() <= step {
                    self.x = anim.target.x;
                    // Snap flush onto the door base (the ground rest position can
                    // sit a pixel off) so the entering animation is centred.
                    self.y = anim.target.y;
                    anim.phase = ExitPhase::Entering;
                    self.frame = 0;
                    self.frame_time = 0.0;
                } else {
                    self.facing_right = dx > 0.0;
                    self.x += step.copysign(dx);
                    // `step` is PLAYER_SPEED, so he strolls into the door.
                    self.gait = Gait::Walking;
                    self.step_walk_animation(delta_time);
                }
            }
            ExitPhase::Entering => {
                anim.timer += delta_time;
                if anim.timer >= EXIT_FRAME_DURATION {
                    anim.timer -= EXIT_FRAME_DURATION;
                    if anim.frame < EXIT_FRAMES - 1 {
                        anim.frame += 1;
                    } else {
                        anim.done = true;
                    }
                }
            }
        }

        self.state = PlayerState::Exiting(anim);
        anim.done
    }

    pub fn update(&mut self, input: &InputState, tilemap: &mut TileMap, delta_time: f32) {
        // Age the remembered press, resetting to zero on a fresh one. A press
        // made *this* step therefore reads as zero steps old at the jump check
        // at the end of it, so buffering never delays an ordinary jump.
        self.steps_since_press = if input.jump_pressed {
            0
        } else {
            self.steps_since_press.saturating_add(1)
        };

        // Horizontal movement. Releasing the stick still stops dead, and both
        // directions at once still resolves to right, exactly as before.
        if input.left {
            self.facing_right = false;
        }
        if input.right {
            self.facing_right = true;
        }
        let direction = if input.right {
            1.0
        } else if input.left {
            -1.0
        } else {
            0.0
        };

        if direction == 0.0 {
            self.vel_x = 0.0;
        } else {
            let target = if input.run { RUN_SPEED } else { PLAYER_SPEED };
            // Speed measured along the direction being held. A standing start
            // reads as zero and a turn reads as negative, so both floor to
            // walking pace and the character never slides through a reversal.
            let speed = (self.vel_x * direction).max(PLAYER_SPEED);
            let speed = if speed < target {
                (speed + RUN_ACCELERATION * delta_time).min(target)
            } else {
                (speed - RUN_DECELERATION * delta_time).max(target)
            };
            self.vel_x = direction * speed;
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

        // Coyote time is measured from the ground state resolved just above, so
        // it counts real support - a block at his side or overhead never
        // refreshes it, for the same reasons that keep it out of `on_ground`.
        self.airborne_steps = if self.on_ground {
            0
        } else {
            self.airborne_steps.saturating_add(1)
        };

        // Touch all tiles the player is currently overlapping
        self.touch_tiles(tilemap);

        // Handle platform interactions (includes collision checking)
        self.handle_platforms(tilemap, delta_time);

        // Clamp player to level bounds
        let level_width = tilemap.width as f32 * TILE_SIZE;
        self.x = self.x.max(0.0).min(level_width - self.width as f32);

        // Jump logic (after collision so we know whether he is grounded)

        // Both sides of the press are forgiven: `steps_since_press` remembers a
        // press made just before landing, `airborne_steps` keeps the jump
        // available just after walking off an edge. Only a fresh press ever
        // resets the former, so holding the button down still cannot bounce
        // him - he has to release and press again.
        if self.airborne_steps <= COYOTE_STEPS && self.steps_since_press <= JUMP_BUFFER_STEPS {
            self.vel_y = -JUMP_SPEED;
            self.on_ground = false;
            // Spend both windows. Leaving either armed would let a second press
            // moments later cash the same lift-off again as a mid-air jump.
            self.steps_since_press = WINDOW_SPENT;
            self.airborne_steps = WINDOW_SPENT;
        }

        self.update_animation(delta_time);
    }

    fn update_animation(&mut self, delta_time: f32) {
        if !self.on_ground {
            // Airborne: hold the first running frame (legs apart) as the jump
            // pose. Once he is dropping fast enough, `render` swaps in the
            // falling pose instead, so the arc reads as rise-then-fall.
            self.gait = Gait::Running;
            self.frame = JUMP_FRAME;
            self.frame_time = 0.0;
        } else if self.vel_x.abs() > 0.1 {
            // Pick the gait from how fast he is actually moving rather than
            // from the run button: the run ramps in over PLAYER_SPEED, so his
            // legs stay in step with his speed through the acceleration and
            // through the coast back down after the button is let go.
            self.gait = if self.vel_x.abs() > PLAYER_SPEED + RUN_POSE_MARGIN {
                Gait::Running
            } else {
                Gait::Walking
            };
            // Cycle the legs in proportion to ground speed, so a run doesn't
            // read as a moonwalk. Scaled here rather than inside
            // `step_walk_animation` because the exit animation drives that
            // directly while moving the player without touching `vel_x`.
            self.step_walk_animation(delta_time * self.vel_x.abs() / PLAYER_SPEED);
        } else {
            // Frame 0 is idle
            self.frame = 0;
            self.frame_time = 0.0;
        }
    }

    /// Advance the two-frame walk cycle (frames 1 and 2), seeding from the idle
    /// frame on the first step.
    fn step_walk_animation(&mut self, delta_time: f32) {
        // If we just started walking (frame is 0), set it to 1
        if self.frame == 0 {
            self.frame = 1;
        }

        self.frame_time += delta_time;
        if self.frame_time >= FRAME_DURATION {
            self.frame_time = 0.0;
            // Alternate between frames 1 and 2 for walking
            self.frame = if self.frame == 1 { 2 } else { 1 };
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
            platform.is_active()
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
            platform.is_active()
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
        let tile_size = TILE_SIZE;
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
            let tile_x = (corner_x / TILE_SIZE).floor() as i32;
            let tile_y = (corner_y / TILE_SIZE).floor() as i32;

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
        let tile_size = TILE_SIZE as i32;
        let top_tile = player_top / tile_size;
        let bottom_tile = player_bottom / tile_size;
        let left_tile = player_left / tile_size;
        let right_tile = player_right / tile_size;

        for ty in top_tile..=bottom_tile {
            for tx in left_tile..=right_tile {
                tilemap.touch_tile(tx, ty);
            }
        }
    }

    /// Resolve all interactions with moving platforms and path blocks for this
    /// frame, in order: activate/ride the platform under the feet, take a
    /// horizontal push from a platform at the side, then take a downward push
    /// from a block overhead. Any of the pushes can crush (kill) the player,
    /// which ends the handling for the frame.
    fn handle_platforms(&mut self, tilemap: &mut TileMap, delta_time: f32) {
        let contacts = self.scan_platform_contacts(tilemap);

        // Activate platform if needed (outside the scan to avoid borrow issues)
        if let Some((px, py)) = contacts.activate {
            tilemap.activate_platform(px, py);
        }

        if let Some(push_vel_x) = contacts.side_push {
            self.take_side_push(push_vel_x, tilemap, delta_time);
            if self.is_dead() {
                return;
            }
        }

        if let Some((vel_x, platform_top)) = contacts.riding {
            self.ride_platform(vel_x, platform_top, tilemap, delta_time);
            if self.is_dead() {
                return;
            }
        }

        self.take_overhead_push(tilemap);
    }

    /// One pass over all platforms, classifying how each touches the player.
    /// The scan stops at the first platform the player is riding; a side push
    /// found before it is kept.
    fn scan_platform_contacts(&self, tilemap: &TileMap) -> PlatformContacts {
        let player_left = self.x;
        let player_right = self.x + self.width as f32;
        let player_top = self.y;
        let player_bottom = self.y + self.height as f32;

        // Whether solid ground (not a platform) is currently supporting the
        // player. While true, stepping onto a platform must not start it - the
        // player has somewhere else to stand and hasn't committed to it.
        let supported_by_ground = self.on_solid_ground(tilemap);

        let mut contacts = PlatformContacts::default();

        for platform in tilemap.platforms() {
            // Check if player's feet are touching the top of the platform
            if self.feet_on_platform(platform) && self.vel_y >= 0.0 {
                // Player is standing on this platform. Activate it once he is
                // entirely on top, or as soon as it is his only support (he is
                // not also resting on solid ground) - otherwise a player still
                // straddling solid ground keeps it dormant until fully aboard.
                if !platform.is_active()
                    && (self.fully_on_platform(platform) || !supported_by_ground)
                {
                    let px = (platform.x / TILE_SIZE) as i32;
                    let py = (platform.y / TILE_SIZE) as i32;
                    contacts.activate = Some((px, py));
                }

                // Store platform info to move player
                contacts.riding = Some((platform.vel_x, platform.y));
                break;
            }

            // Check if horizontally moving platform is beside the player and should push them
            if platform.is_active() && platform.vel_x.abs() > 0.01 {
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
                        contacts.side_push = Some(platform.vel_x);
                    }
                    // Platform moving left, check if it's to the right of player
                    else if platform.vel_x < 0.0
                        && platform_left <= player_right + 2.0
                        && platform_left >= player_right - 2.0
                    {
                        contacts.side_push = Some(platform.vel_x);
                    }
                }
            }
        }

        contacts
    }

    /// Shoved sideways by a horizontally moving platform. Being squeezed into
    /// a wall crushes (kills) the player.
    fn take_side_push(&mut self, push_vel_x: f32, tilemap: &TileMap, delta_time: f32) {
        let new_x = self.x + push_vel_x * delta_time;
        if self.check_collision_at(new_x, self.y, tilemap) {
            // Platform squeezing player into a wall = crush death
            self.kill();
        } else {
            self.x = new_x;
        }
    }

    /// Carried by the platform under the player's feet: follow it horizontally
    /// and keep the feet snapped to its top. Being squeezed into solid ground
    /// above crushes (kills) the player.
    fn ride_platform(&mut self, vel_x: f32, platform_top: f32, tilemap: &TileMap, delta_time: f32) {
        // Try horizontal movement
        let move_x = vel_x * delta_time;
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
                self.kill();
                return;
            }
            // Platform is pushing player into obstacle - resolve to edge
            self.y = self.resolve_y_position(self.x, snap_y, tilemap);
        } else {
            self.y = snap_y;
        }
        self.vel_y = 0.0;
    }

    /// A downward-moving block pressing on the player from above. The block
    /// can never hold him up: it shoves him down while it descends faster
    /// than he is falling, and otherwise he simply falls away from it under
    /// gravity - whichever is the greater downward motion wins. If solid
    /// support sits beneath him so he cannot be pushed clear, he is crushed.
    fn take_overhead_push(&mut self, tilemap: &TileMap) {
        for platform in tilemap.platforms() {
            if !platform.is_active() || platform.vel_y <= 0.0 {
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
                    self.kill();
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
        // Respawn owes him nothing: a press buffered as he died must not fire,
        // and he gets no coyote jump until his feet touch down again.
        self.steps_since_press = WINDOW_SPENT;
        self.airborne_steps = WINDOW_SPENT;
        self.frame = 0;
        self.frame_time = 0.0;
        self.gait = Gait::Walking;
        self.facing_right = true;
        self.state = PlayerState::Alive;
    }

    /// Sprite-sheet position of the current body frame.
    ///
    /// The idle frame and the whole running cycle come from the first row; only
    /// the two moving frames of a walk come from the walking cycle, which lives
    /// in the death row's spare cells.
    fn body_frame_src(&self) -> (i32, i32) {
        let cell = |x: i32, frame: usize| x + (frame * self.width as usize) as i32;
        if self.gait == Gait::Walking && self.frame > 0 {
            (cell(WALK_SRC_X, self.frame - 1), self.height as i32)
        } else {
            (cell(0, self.frame), 0)
        }
    }

    /// Whether the wide cape-trailing pose should be drawn.
    ///
    /// Not simply "moving downward": he has to be dropping at
    /// `FALL_POSE_SPEED` before the cape catches, so the rising pose carries
    /// through the apex of the arc instead of flicking over the moment
    /// `vel_y` crosses zero.
    fn uses_fall_pose(&self) -> bool {
        !self.on_ground && self.vel_y >= FALL_POSE_SPEED
    }

    pub fn render(
        &self,
        canvas: &mut WindowCanvas,
        texture: &Texture,
        camera_x: i32,
        camera_y: i32,
    ) {
        // The source frame, plus its width and the distance from its left edge
        // to the centre of the head — the falling pose is wider than the rest,
        // so those two can't be assumed to be the player's own.
        let (src_x, src_y, frame_width, head_center) = match self.state {
            // Third sprite-sheet row: the "stepping into the door" frames.
            PlayerState::Exiting(anim) if anim.phase == ExitPhase::Entering => (
                (anim.frame * self.width as usize) as i32,
                (self.height * 2) as i32,
                self.width,
                HEAD_CENTER_X,
            ),
            // Second row: the death frames.
            PlayerState::Dying(anim) => (
                (anim.frame * self.width as usize) as i32,
                self.height as i32,
                self.width,
                HEAD_CENTER_X,
            ),
            // Dropping fast enough for the cape to catch (including the fall
            // onto a door's base height): the wide top-right pose, cape
            // trailing behind him.
            _ if self.uses_fall_pose() => {
                (FALL_SRC_X, 0, FALL_FRAME_WIDTH, FALL_HEAD_CENTER_X)
            }
            // Idle, or one of the two ground cycles (also used while rising,
            // and while walking towards a door during the exit sequence).
            _ => {
                let (src_x, src_y) = self.body_frame_src();
                (src_x, src_y, self.width, HEAD_CENTER_X)
            }
        };
        let src_rect = sdl2::rect::Rect::new(src_x, src_y, frame_width, self.height);

        // Line the drawn head up with where the head sits in a normal frame, so
        // a wider frame adds cape without shifting the character off his
        // collision box. `copy_ex` mirrors inside the destination rect, which
        // puts the head `width - 1 - centre` from its left edge when facing
        // left — and swings the cape over to the other side with it.
        let head_offset = |width: u32, center: i32| {
            if self.facing_right {
                center
            } else {
                width as i32 - 1 - center
            }
        };
        let dst_rect = sdl2::rect::Rect::new(
            self.x as i32 - camera_x + head_offset(self.width, HEAD_CENTER_X)
                - head_offset(frame_width, head_center),
            self.y as i32 - camera_y,
            frame_width,
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
    use crate::time::FIXED_DT;

    /// Flat ground across the whole map, with his feet resting on it.
    fn on_flat_ground() -> (Player, TileMap) {
        let mut grid = vec![vec![tiles::EMPTY; 10]; 4];
        grid[1] = vec![tiles::SOLID; 10];
        let mut player = Player::new(40.0, TILE_SIZE - PLAYER_HEIGHT as f32);
        let mut tilemap = TileMap::from_data(grid);
        player.update(&InputState::new(), &mut tilemap, FIXED_DT);
        assert!(player.on_ground, "player should start grounded");
        (player, tilemap)
    }

    /// Ground for the first three columns, then a drop, so he can walk off a
    /// lip. His feet rest on the ledge at the start.
    fn on_a_ledge() -> (Player, TileMap) {
        let mut grid = vec![vec![tiles::EMPTY; 10]; 4];
        grid[1][..3].fill(tiles::SOLID);
        let mut player = Player::new(0.0, TILE_SIZE - PLAYER_HEIGHT as f32);
        let mut tilemap = TileMap::from_data(grid);
        player.update(&InputState::new(), &mut tilemap, FIXED_DT);
        assert!(player.on_ground, "player should start on the ledge");
        (player, tilemap)
    }

    fn press() -> InputState {
        InputState {
            jump: true,
            jump_pressed: true,
            ..InputState::new()
        }
    }

    /// The whole point of the buffer: a press made just before touchdown is
    /// remembered and fires on landing instead of being swallowed.
    #[test]
    fn a_press_just_before_landing_is_buffered() {
        // How many steps a clean fall takes, as the reference to press against.
        let landing_step = {
            let (mut player, mut tilemap) = on_flat_ground();
            player.y -= 60.0; // lift him into the air
            player.on_ground = false;
            let mut step = 0;
            while !player.on_ground {
                player.update(&InputState::new(), &mut tilemap, FIXED_DT);
                step += 1;
                assert!(step < 200, "player never landed");
            }
            step
        };
        assert!(
            landing_step > JUMP_BUFFER_STEPS as usize + 2,
            "the fall must be long enough to press early in it"
        );

        // Pressing within the window before touchdown jumps on landing;
        // pressing well outside it is forgotten, as it was before.
        for (press_step, should_jump) in [
            (landing_step - 1, true),
            (landing_step - JUMP_BUFFER_STEPS as usize, true),
            (landing_step - JUMP_BUFFER_STEPS as usize - 3, false),
        ] {
            let (mut player, mut tilemap) = on_flat_ground();
            player.y -= 60.0;
            player.on_ground = false;

            let mut jumped = false;
            for step in 1..=landing_step + 2 {
                let input = if step == press_step {
                    press()
                } else {
                    InputState::new()
                };
                player.update(&input, &mut tilemap, FIXED_DT);
                if player.vel_y < 0.0 {
                    jumped = true;
                }
            }
            assert_eq!(
                jumped, should_jump,
                "press at step {press_step} of a {landing_step}-step fall: \
                 expected jumped = {should_jump}"
            );
        }
    }

    /// Coyote time: a press a few steps *after* walking off the lip still
    /// jumps, and one past the window does not. `nth_airborne_step` is which
    /// step off the ledge the press lands on - the step he leaves the ground
    /// counts as the first, so the boundary sits exactly at COYOTE_STEPS.
    #[test]
    fn a_press_just_after_leaving_the_ledge_still_jumps() {
        for nth_airborne_step in 2..=COYOTE_STEPS + 1 {
            let (mut player, mut tilemap) = on_a_ledge();
            let walk_right = InputState {
                right: true,
                ..InputState::new()
            };

            // Walk off the lip. That last step is airborne step 1.
            let mut steps = 0;
            while player.on_ground {
                player.update(&walk_right, &mut tilemap, FIXED_DT);
                steps += 1;
                assert!(steps < 200, "player never left the ledge");
            }
            assert!(player.vel_y > 0.0, "he should be falling off the ledge");
            assert_eq!(player.airborne_steps, 1);

            // Coast to the step before the one being tested, then press.
            while player.airborne_steps < nth_airborne_step - 1 {
                player.update(&walk_right, &mut tilemap, FIXED_DT);
            }
            let mut input = press();
            input.right = true;
            player.update(&input, &mut tilemap, FIXED_DT);

            let should_jump = nth_airborne_step <= COYOTE_STEPS;
            assert_eq!(
                player.vel_y < 0.0,
                should_jump,
                "pressing on airborne step {nth_airborne_step} \
                 (window is {COYOTE_STEPS}): expected jumped = {should_jump}, \
                 vel_y = {}",
                player.vel_y
            );
        }
    }

    /// The two windows must not combine into a mid-air double jump: jumping
    /// spends the coyote window, so a second press moments later does nothing.
    #[test]
    fn coyote_time_cannot_be_cashed_twice_for_a_double_jump() {
        let (mut player, mut tilemap) = on_flat_ground();

        player.update(&press(), &mut tilemap, FIXED_DT);
        assert!(player.vel_y < 0.0, "the first press should jump");
        assert!(!player.on_ground);

        // Press again on every step of the rise - the whole coyote window and
        // well past it. He must only ever slow under gravity, never re-launch.
        let mut previous = player.vel_y;
        for step in 1..=COYOTE_STEPS as usize + 6 {
            player.update(&press(), &mut tilemap, FIXED_DT);
            assert!(
                player.vel_y > previous,
                "step {step}: a second press re-launched him \
                 ({previous} -> {})",
                player.vel_y
            );
            previous = player.vel_y;
        }
    }

    /// Buffering must not put latency into an ordinary jump: a press made while
    /// standing on the ground still fires on that very step.
    #[test]
    fn a_grounded_press_jumps_on_the_same_step() {
        let (mut player, mut tilemap) = on_flat_ground();
        player.update(&press(), &mut tilemap, FIXED_DT);

        assert_eq!(
            player.vel_y, -JUMP_SPEED,
            "a grounded press should launch on the same step it is made"
        );
        assert!(!player.on_ground);
    }

    /// A moving platform underfoot is real support, so it must refresh coyote
    /// time and a press while riding must jump exactly as it does off a tile.
    #[test]
    fn riding_a_platform_refreshes_coyote_time() {
        let mut tilemap = map_with_active_right_platform();
        // Standing on top of the platform, whose rect spans y 80..120.
        let mut player = Player::new(90.0, 80.0 - PLAYER_HEIGHT as f32);

        // The engine updates the tilemap before the player; mirror that here.
        for step in 0..5 {
            tilemap.update(FIXED_DT);
            player.update(&InputState::new(), &mut tilemap, FIXED_DT);
            assert!(player.on_ground, "step {step}: should be riding the platform");
            assert_eq!(
                player.airborne_steps, 0,
                "step {step}: a platform underfoot should count as ground"
            );
        }

        tilemap.update(FIXED_DT);
        player.update(&press(), &mut tilemap, FIXED_DT);
        assert_eq!(player.vel_y, -JUMP_SPEED, "should jump off the platform");
        assert!(!player.on_ground);
    }

    /// Dying must not hand him a jump on respawn: a press buffered as he died,
    /// and the ground he died on, are both forgotten by `reset`.
    #[test]
    fn respawn_owes_no_buffered_jump() {
        let (mut player, mut tilemap) = on_flat_ground();

        // Buffer a press, then die and respawn before it could be spent.
        player.steps_since_press = 0;
        player.kill();
        player.reset();

        assert_eq!(player.steps_since_press, WINDOW_SPENT, "respawn kept a press");
        assert_eq!(player.airborne_steps, WINDOW_SPENT, "respawn kept coyote time");

        // The first step after respawning must not launch him upward.
        player.update(&InputState::new(), &mut tilemap, FIXED_DT);
        assert!(player.vel_y >= 0.0, "player jumped on respawn");
    }

    /// Walking and running must draw from different cycles, and each frame of
    /// each cycle must land on its own sprite-sheet cell.
    #[test]
    fn walking_and_running_use_separate_sprite_cycles() {
        let mut player = Player::new(0.0, 0.0);
        let w = PLAYER_WIDTH as i32;
        let row1 = PLAYER_HEIGHT as i32;

        // Idle is the shared first cell of the top row, whatever the gait.
        for gait in [Gait::Walking, Gait::Running] {
            player.gait = gait;
            player.frame = 0;
            assert_eq!(player.body_frame_src(), (0, 0), "idle must be shared");
        }

        // Running: frames 1 and 2 of the top row, exactly as before.
        player.gait = Gait::Running;
        player.frame = 1;
        assert_eq!(player.body_frame_src(), (w, 0));
        player.frame = 2;
        assert_eq!(player.body_frame_src(), (2 * w, 0));

        // Walking: the death row's two spare cells, right of its three frames.
        player.gait = Gait::Walking;
        player.frame = 1;
        assert_eq!(player.body_frame_src(), (WALK_SRC_X, row1));
        player.frame = 2;
        assert_eq!(player.body_frame_src(), (WALK_SRC_X + w, row1));

        // Those cells must start past the death frames and stay on the sheet.
        assert_eq!(WALK_SRC_X, DEATH_FRAMES as i32 * w);
        assert!(WALK_SRC_X + 2 * w <= 80, "walk cycle runs off the sheet");
    }

    /// The gait follows actual speed, not the run button: he keeps walking
    /// legs until the run has actually ramped him past walking pace.
    #[test]
    fn gait_follows_speed_not_the_run_button() {
        let mut tilemap = TileMap::from_data(vec![
            vec![tiles::EMPTY; 10],
            vec![tiles::SOLID; 10],
        ]);
        // Stand him on the solid row so he is grounded and can move.
        let mut player = Player::new(16.0, TILE_SIZE - PLAYER_HEIGHT as f32);

        let run_right = InputState {
            right: true,
            run: true,
            ..InputState::new()
        };
        let walk_right = InputState {
            right: true,
            ..InputState::new()
        };

        // Settle on the ground.
        player.update(&InputState::new(), &mut tilemap, FIXED_DT);
        assert!(player.on_ground, "player should be standing on the ground");

        // Plain walking never reaches the running cycle.
        for _ in 0..30 {
            player.update(&walk_right, &mut tilemap, FIXED_DT);
            assert_eq!(player.vel_x, PLAYER_SPEED, "walking should pin vel_x");
            assert!(player.gait == Gait::Walking, "walking used the run cycle");
        }

        // Holding run: the very first step is still at walking pace, so the
        // legs must not snap to the run cycle before he has actually sped up.
        player.update(&run_right, &mut tilemap, FIXED_DT);
        assert!(
            player.vel_x > PLAYER_SPEED,
            "run should start ramping immediately"
        );

        // Once up to speed he is unambiguously running.
        for _ in 0..30 {
            player.update(&run_right, &mut tilemap, FIXED_DT);
        }
        assert_eq!(player.vel_x, RUN_SPEED, "should have reached top speed");
        assert!(player.gait == Gait::Running, "running used the walk cycle");

        // Letting go of run coasts back down, and the legs return to walking
        // only once he is actually back at walking pace.
        loop {
            player.update(&walk_right, &mut tilemap, FIXED_DT);
            if player.vel_x <= PLAYER_SPEED + RUN_POSE_MARGIN {
                break;
            }
            assert!(
                player.gait == Gait::Running,
                "still faster than walking pace ({}), so the legs should run",
                player.vel_x
            );
        }
        assert_eq!(player.vel_x, PLAYER_SPEED);
        assert!(player.gait == Gait::Walking, "should be walking again");

        // Standing still falls back to the idle frame.
        player.update(&InputState::new(), &mut tilemap, FIXED_DT);
        assert_eq!(player.frame, 0, "idle frame expected when stopped");
    }

    /// The cape pose must not flick out at the apex of the arc: a player who
    /// has only just started dropping still uses the rising pose, and the
    /// swap happens once he is falling at `FALL_POSE_SPEED`.
    #[test]
    fn cape_pose_waits_for_real_falling_speed() {
        let mut player = Player::new(0.0, 0.0);
        player.on_ground = false;

        // Rising, and the moment at the very top of the arc.
        for vel_y in [-JUMP_SPEED, -1.0, 0.0] {
            player.vel_y = vel_y;
            assert!(
                !player.uses_fall_pose(),
                "vel_y = {vel_y}: should still hold the rising pose"
            );
        }

        // Just barely dropping - still too slow for the cape to catch.
        player.vel_y = FALL_POSE_SPEED - 1.0;
        assert!(!player.uses_fall_pose(), "cape caught too early");

        // At and beyond the threshold the cape streams out.
        for vel_y in [FALL_POSE_SPEED, FALL_POSE_SPEED + 200.0] {
            player.vel_y = vel_y;
            assert!(player.uses_fall_pose(), "vel_y = {vel_y}: cape should be out");
        }

        // Grounded never uses the falling pose, however large vel_y is.
        player.on_ground = true;
        player.vel_y = FALL_POSE_SPEED + 200.0;
        assert!(!player.uses_fall_pose(), "grounded must not use the fall pose");
    }

    /// The threshold is reached a short way into the fall, not many frames
    /// later: after a full jump the cape is out well before he lands.
    #[test]
    fn cape_pose_engages_shortly_after_the_apex() {
        let mut player = Player::new(0.0, 0.0);
        player.on_ground = false;
        player.vel_y = 0.0;

        let mut frames = 0;
        while !player.uses_fall_pose() {
            player.vel_y += GRAVITY * FIXED_DT;
            frames += 1;
            assert!(frames < 60, "cape pose never engaged while falling");
        }

        assert!(
            (4..=10).contains(&frames),
            "cape should catch a few frames past the apex, took {frames}"
        );
    }

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

    /// A player standing on a wide strip of solid ground with open air above,
    /// for testing horizontal movement with nothing to collide with.
    fn standing() -> (Player, TileMap) {
        let mut grid = vec![vec![tiles::EMPTY; 40]; 12];
        grid[4] = vec![tiles::SOLID; 40];
        (Player::new(400.0, 122.0), TileMap::from_data(grid))
    }

    /// Hold right (optionally with run) for `frames` and return the new `vel_x`.
    fn hold_right(frames: usize, run: bool, player: &mut Player, tilemap: &mut TileMap) -> f32 {
        let input = InputState {
            right: true,
            run,
            ..Default::default()
        };
        for _ in 0..frames {
            player.update(&input, tilemap, FIXED_DT);
        }
        player.vel_x
    }

    #[test]
    fn walking_is_unchanged_when_run_is_not_held() {
        // Whatever the run mechanic does, plain walking must still be the flat
        // instant-on PLAYER_SPEED the existing levels were built around.
        for frames in [1, 2, 10, 120] {
            let (mut player, mut tilemap) = standing();
            assert_eq!(
                hold_right(frames, false, &mut player, &mut tilemap),
                PLAYER_SPEED
            );
        }
    }

    #[test]
    fn holding_run_builds_speed_and_caps_at_run_speed() {
        let (mut player, mut tilemap) = standing();

        // The first frame is walking pace plus one step of acceleration, so the
        // character responds instantly rather than easing up from a standstill.
        let first = hold_right(1, true, &mut player, &mut tilemap);
        assert!(
            first > PLAYER_SPEED && first < PLAYER_SPEED + 20.0,
            "run should start from walking pace, got {first}"
        );

        // It converges on RUN_SPEED and never overshoots.
        assert_eq!(hold_right(120, true, &mut player, &mut tilemap), RUN_SPEED);
    }

    #[test]
    fn reaching_top_speed_takes_about_a_fifth_of_a_second() {
        // Long enough that the build-up reads as momentum, short enough to be
        // usable in a corridor. Guards the tuning against accidental edits.
        let frames_to_top = (1..=200)
            .find(|&f| {
                let (mut player, mut tilemap) = standing();
                hold_right(f, true, &mut player, &mut tilemap) >= RUN_SPEED
            })
            .expect("should reach top speed");
        let secs = frames_to_top as f32 * FIXED_DT;
        assert!(
            (0.15..=0.30).contains(&secs),
            "time to top speed was {secs}s"
        );
    }

    #[test]
    fn releasing_run_bleeds_back_down_to_walking_speed() {
        let (mut player, mut tilemap) = standing();
        hold_right(120, true, &mut player, &mut tilemap);

        // Speed comes off over several frames rather than snapping, so letting
        // go of run keeps the momentum readable.
        let after_one = hold_right(1, false, &mut player, &mut tilemap);
        assert!(
            after_one > PLAYER_SPEED && after_one < RUN_SPEED,
            "expected a gradual bleed, got {after_one}"
        );
        assert_eq!(
            hold_right(120, false, &mut player, &mut tilemap),
            PLAYER_SPEED
        );
    }

    #[test]
    fn turning_around_while_running_snaps_to_walking_speed() {
        let (mut player, mut tilemap) = standing();
        assert_eq!(hold_right(120, true, &mut player, &mut tilemap), RUN_SPEED);

        // A reversal must not carry the run's momentum through the turn, or the
        // character would skate past the spot the player aimed at. Run is still
        // held here, so the new direction restarts from walking pace and begins
        // building again — it must not inherit the old direction's top speed.
        let input = InputState {
            left: true,
            run: true,
            ..Default::default()
        };
        player.update(&input, &mut tilemap, FIXED_DT);
        assert!(
            player.vel_x < 0.0 && player.vel_x.abs() < PLAYER_SPEED + 20.0,
            "turn should restart from walking pace, got {}",
            player.vel_x
        );
    }

    #[test]
    fn releasing_the_direction_still_stops_dead_even_at_run_speed() {
        let (mut player, mut tilemap) = standing();
        hold_right(120, true, &mut player, &mut tilemap);

        player.update(&InputState::default(), &mut tilemap, FIXED_DT);
        assert_eq!(player.vel_x, 0.0);
    }

    #[test]
    fn the_run_ramp_is_expressed_per_second_not_per_frame() {
        // Physics is pinned to FIXED_DT today, but the ramp must not silently
        // depend on that the way the jump-cut damping does.
        let sample = |dt: f32| {
            let (mut player, mut tilemap) = standing();
            let input = InputState {
                right: true,
                run: true,
                ..Default::default()
            };
            for _ in 0..(0.1 / dt).round() as usize {
                player.update(&input, &mut tilemap, dt);
            }
            player.vel_x
        };
        let coarse = sample(1.0 / 60.0);
        let fine = sample(1.0 / 240.0);
        assert!(
            (coarse - fine).abs() < 1.0,
            "ramp differed with step size: {coarse} vs {fine}"
        );
    }

    /// An airborne player who reaches a door must first fall to the door's base
    /// height and only then walk horizontally towards its middle - never slide
    /// sideways through the air. He ends flush with the door.
    #[test]
    fn airborne_exit_lands_before_walking_in() {
        // Player above and to the left of the door target, airborne.
        let mut player = Player::new(100.0, 0.0);
        player.on_ground = false;

        let target_x = 200.0;
        let target_y = 120.0;
        let start_x = player.x;
        player.start_exit(target_x, target_y);

        let dt = 1.0 / 60.0;
        let mut started_walking = false;
        let mut finished = false;
        for _ in 0..600 {
            finished = player.update_exit_animation(dt);

            // The instant he first moves horizontally he must already be at the
            // door's base height (i.e. he landed first).
            if !started_walking && (player.x - start_x).abs() > 0.001 {
                started_walking = true;
                assert!(
                    player.y >= target_y - 0.001,
                    "player walked in mid-air before landing (y = {})",
                    player.y
                );
                assert!(player.facing_right, "should face towards the door");
            }

            if finished {
                break;
            }
        }

        assert!(started_walking, "player never walked towards the door");
        assert!(finished, "exit animation never finished");
        assert!(
            (player.x - target_x).abs() < 0.001 && (player.y - target_y).abs() < 0.001,
            "player should finish flush with the door ({}, {})",
            player.x,
            player.y
        );
    }

    /// An airborne entry drifts sideways while falling, but stops the moment it
    /// reaches the door's middle so a sideways jump doesn't overshoot and then
    /// walk back. It still ends flush with the door.
    #[test]
    fn airborne_exit_stops_drifting_at_the_door_middle() {
        let mut player = Player::new(40.0, 0.0);
        player.on_ground = false;
        player.vel_x = PLAYER_SPEED; // drifting right towards the door

        let target_x = 100.0;
        let target_y = 120.0;
        player.start_exit(target_x, target_y);

        let dt = 1.0 / 60.0;

        // First frame: still high above the door, yet already carried sideways.
        player.update_exit_animation(dt);
        assert!(player.y < target_y, "should still be landing");
        assert!(
            player.x > 40.0,
            "horizontal momentum should carry him while airborne (x = {})",
            player.x
        );

        // He must never overshoot the door's middle while drifting.
        for _ in 0..600 {
            let done = player.update_exit_animation(dt);
            assert!(
                player.x <= target_x + 0.001,
                "drift should stop at the door middle, not overshoot (x = {})",
                player.x
            );
            if done {
                break;
            }
        }

        // Run to completion; he still ends flush with the door.
        assert!(
            (player.x - target_x).abs() < 0.001 && (player.y - target_y).abs() < 0.001,
            "player should finish flush with the door ({}, {})",
            player.x,
            player.y
        );
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
                !player.is_dead(),
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

                assert!(!player.is_dead(), "hold={hold_jump} frame {frame}: must not die");
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
