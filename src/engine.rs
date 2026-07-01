use crate::input::InputState;
use crate::level::LevelData;
use crate::player::Player;
use crate::tilemap::TileMap;
use crate::tiles;
use std::collections::HashMap;

pub enum OnDeath {
    Respawn,
    Stop,
}

// How far below a door's base the player's feet may sit and still enter it.
// Just enough to absorb the ~1px gap the collision resolver leaves between the
// feet and the ground the door rests on; anything more (poking into the door
// from below) is rejected.
const EXIT_FEET_TOLERANCE: f32 = 4.0;

/// Core game engine - handles game state and logic independent of rendering
pub struct GameEngine {
    pub player: Player,
    pub tilemap: TileMap,
    pub on_death: OnDeath,
    pub stopped: bool,
    levels: Vec<LevelData>,
    /// Map from each level's `id` to its index in `levels`, so an exit door can
    /// route to its destination by name.
    level_index: HashMap<String, usize>,
    current_level: usize,
    /// Destination id of the door the player is currently entering, captured
    /// when the exit animation starts and consumed when it finishes.
    pending_dest: Option<String>,
}

impl GameEngine {
    /// Create a game engine with a single, externally constructed level.
    /// Completing this level restarts it.
    pub fn new_with(player: Player, tilemap: TileMap, on_death: OnDeath) -> Self {
        Self {
            player,
            tilemap,
            on_death,
            stopped: false,
            levels: Vec::new(),
            level_index: HashMap::new(),
            current_level: 0,
            pending_dest: None,
        }
    }

    /// Create a game engine over a set of levels. Progression is driven entirely
    /// by each exit door's destination (see [`LevelData::exits`]); the order the
    /// levels are passed in only decides which one is played first.
    pub fn from_levels(levels: Vec<LevelData>, on_death: OnDeath) -> Result<Self, String> {
        Self::from_levels_at(levels, 0, on_death)
    }

    pub fn from_levels_at(
        levels: Vec<LevelData>,
        start: usize,
        on_death: OnDeath,
    ) -> Result<Self, String> {
        let start = start.min(levels.len().saturating_sub(1));
        let first = levels.get(start).ok_or("no levels given")?;
        let mut engine = Self::new_with(
            Player::new(first.spawn.0, first.spawn.1),
            TileMap::from_level(first),
            on_death,
        );
        engine.levels = levels;
        engine.level_index = engine
            .levels
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
        engine.current_level = start;
        Ok(engine)
    }

    /// Run one game frame with the given input and delta time
    pub fn step(&mut self, input: &InputState, delta_time: f32) {
        if self.stopped {
            return;
        }

        // If the player is mid-death-animation, advance it and wait for it to finish
        if self.player.is_dead {
            self.player.update_death_animation(delta_time);
            if self.player.death_anim_done {
                match self.on_death {
                    OnDeath::Stop => self.stopped = true,
                    OnDeath::Respawn => {
                        self.player.reset();
                        self.tilemap.reset();
                    }
                }
            }
            return;
        }

        // While the exit animation plays the world is frozen; only the player's
        // walk-into-the-door animation advances. The next level loads once it
        // finishes.
        if self.player.is_exiting {
            self.player.update_exit_animation(delta_time);
            if self.player.exit_anim_done {
                self.advance_level();
            }
            return;
        }

        // Update tilemap (for disappearing tiles and moving platforms)
        self.tilemap.update(delta_time);

        // Eject player if a tile that just became solid (e.g. periodic tile
        // flipping ghost → solid) is now slightly overlapping them
        self.player.try_unstick(&self.tilemap);

        // Update player
        self.player.update(input, &mut self.tilemap, delta_time);

        // Pick up any coins the player is overlapping; the exit stays shut
        // until they are all gone.
        self.tilemap.collect_coins(&self.player.bounding_rect());

        // Check if player fell off the screen or touched deadly tile and reset
        // if needed. Touching a death tile marks it so it shows its hit sprite
        // until the player revives. The margin keeps a player standing with one
        // foot on solid ground from triggering an adjacent death tile.
        let death_bounds = self.player.bounding_rect().shrink(2.0);
        if self.fallen_outside_playable_area(&self.player)
            || self.tilemap.trigger_death_tiles(&death_bounds)
        {
            self.player.is_dead = true;
            return;
        }

        // Check if player reached an open exit tile - start the exit animation.
        // A closed door (its coins still uncollected) is skipped by exit_target.
        if let Some((target_x, target_y, door_tile)) = self.exit_target() {
            self.pending_dest = self.dest_for_door(door_tile);
            self.player.start_exit(target_x, target_y);
        }
    }

    /// The destination level id of the exit door at the given grid position, if
    /// the current level declares one for it.
    fn dest_for_door(&self, tile: (usize, usize)) -> Option<String> {
        self.levels
            .get(self.current_level)
            .and_then(|level| level.exits.iter().find(|e| e.tile == tile))
            .map(|e| e.dest.clone())
    }

    /// If the player is touching an open exit door, the position his bounding
    /// box should ease to so he is centred horizontally on the door with his
    /// feet resting on its base, together with that door's grid coordinates.
    fn exit_target(&self) -> Option<(f32, f32, (usize, usize))> {
        // Same margin as `is_player_touching_tile_of_type`, so a player merely
        // grazing an adjacent door doesn't trigger the exit.
        let player_bounds = self.player.bounding_rect().shrink(2.0);
        let feet = self.player.y + self.player.height as f32;
        let tile_size = self.tilemap.tile_size as f32;
        // Normal doors open on the gold coins, secret doors on the red ones; a
        // door whose coins are still uncollected is not enterable.
        let door_types = [
            (tiles::EXIT, self.tilemap.doors_open()),
            (tiles::SECRET_EXIT, self.tilemap.secret_doors_open()),
        ];
        for (door_type, open) in door_types {
            if !open {
                continue;
            }
            for tile in self.tilemap.tiles_of_type(door_type) {
                let rect = tile.get_bounding_rect();
                let door_base = rect.position.y + rect.size.y;
                // Only enter when the feet are level with the door's base or
                // above it; poking into the door from below cannot enter.
                if player_bounds.intersects(&rect) && feet <= door_base + EXIT_FEET_TOLERANCE {
                    let target_x = rect.position.x + (tile_size - self.player.width as f32) / 2.0;
                    let target_y = rect.position.y + tile_size - self.player.height as f32;
                    return Some((target_x, target_y, (tile.x, tile.y)));
                }
            }
        }
        None
    }

    /// Index of the level currently being played
    pub fn current_level(&self) -> usize {
        self.current_level
    }

    /// Name of the level currently being played, if any (empty in single-level mode)
    pub fn current_level_name(&self) -> Option<&str> {
        self.levels.get(self.current_level).map(|l| l.name.as_str())
    }

    /// True while the player is playing the exit animation, before the next
    /// level loads
    pub fn is_transitioning(&self) -> bool {
        self.player.is_exiting
    }

    fn advance_level(&mut self) {
        // Route to the level the entered door points at. With no resolvable
        // destination (single-level mode, or a door whose target isn't in this
        // set) the level simply restarts.
        let next = self
            .pending_dest
            .take()
            .and_then(|dest| self.level_index.get(&dest).copied());
        match next {
            Some(index) => {
                self.current_level = index;
                let level = &self.levels[index];
                self.player = Player::new(level.spawn.0, level.spawn.1);
                self.tilemap = TileMap::from_level(level);
            }
            None => {
                self.player.reset();
                self.tilemap.reset();
            }
        }
    }

    pub fn fallen_outside_playable_area(&self, p: &Player) -> bool {
        // Player is dead if they fall below the screen
        let screen_height = self.tilemap.height * 40;
        p.y > screen_height as f32 + 100.0
    }

    /// Get read-only reference to player
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// Get read-only reference to tilemap
    pub fn tilemap(&self) -> &TileMap {
        &self.tilemap
    }

    /// Get mutable reference to tilemap (for setup/testing)
    pub fn tilemap_mut(&mut self) -> &mut TileMap {
        &mut self.tilemap
    }

    /// Check if player is on ground (convenience helper for tests)
    pub fn is_player_on_ground(&self) -> bool {
        self.player.on_ground
    }
}
