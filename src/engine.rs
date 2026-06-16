use crate::input::InputState;
use crate::level::LevelData;
use crate::player::Player;
use crate::tilemap::TileMap;
use crate::tiles;

pub enum OnDeath {
    Respawn,
    Stop,
}

// How long the game freezes after the player reaches an exit tile, before the
// next level is loaded
const LEVEL_TRANSITION_TIME: f32 = 0.7;

/// Core game engine - handles game state and logic independent of rendering
pub struct GameEngine {
    pub player: Player,
    pub tilemap: TileMap,
    pub on_death: OnDeath,
    pub stopped: bool,
    levels: Vec<LevelData>,
    current_level: usize,
    transition_timer: Option<f32>,
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
            current_level: 0,
            transition_timer: None,
        }
    }

    /// Create a game engine that plays through a sequence of levels,
    /// looping back to the first one after the last.
    pub fn from_levels(levels: Vec<LevelData>, on_death: OnDeath) -> Result<Self, String> {
        let first = levels.first().ok_or("no levels given")?;
        let mut engine = Self::new_with(
            Player::new(first.spawn.0, first.spawn.1),
            TileMap::from_data(first.tiles.clone()),
            on_death,
        );
        engine.levels = levels;
        Ok(engine)
    }

    /// Run one game frame with the given input and delta time
    pub fn step(&mut self, input: &InputState, delta_time: f32) {
        if self.stopped {
            return;
        }

        // During a level transition the world is frozen
        if let Some(timer) = &mut self.transition_timer {
            *timer -= delta_time;
            if *timer <= 0.0 {
                self.transition_timer = None;
                self.advance_level();
            }
            return;
        }

        // Update tilemap (for disappearing tiles and moving platforms)
        self.tilemap.update(delta_time);

        // Update player
        self.player.update(input, &mut self.tilemap, delta_time);

        // Pick up any coins the player is overlapping; the exit stays shut
        // until they are all gone.
        self.tilemap.collect_coins(&self.player.bounding_rect());

        // Check if player fell off the screen or touched deadly tile and reset if needed
        if self.fallen_outside_playable_area(&self.player) || self.is_player_touching_deadly_tile()
        {
            self.player.is_dead = true;
            match self.on_death {
                OnDeath::Stop => {
                    self.stopped = true;
                }
                OnDeath::Respawn => {
                    self.player.reset();
                    self.tilemap.reset();
                }
            }
            return;
        }

        // Check if player reached an open exit tile - start the level
        // transition. A closed door (coins still uncollected) does nothing.
        if self.tilemap.doors_open() && self.is_player_touching_tile_of_type(tiles::EXIT) {
            self.transition_timer = Some(LEVEL_TRANSITION_TIME);
        }
    }

    /// Index of the level currently being played
    pub fn current_level(&self) -> usize {
        self.current_level
    }

    /// True while the post-exit freeze is running
    pub fn is_transitioning(&self) -> bool {
        self.transition_timer.is_some()
    }

    fn advance_level(&mut self) {
        if self.levels.is_empty() {
            // Single-level mode: restart the level
            self.player.reset();
            self.tilemap.reset();
            return;
        }
        self.current_level = (self.current_level + 1) % self.levels.len();
        let level = &self.levels[self.current_level];
        self.player = Player::new(level.spawn.0, level.spawn.1);
        self.tilemap = TileMap::from_data(level.tiles.clone());
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

    fn is_player_touching_deadly_tile(&self) -> bool {
        self.is_player_touching_tile_of_type(tiles::DEATH)
    }

    fn is_player_touching_tile_of_type(&self, tile_type: u32) -> bool {
        // Add a small margin, so that while the player is standing with one foot on solid ground,
        // he will not touch the deadly tile.
        let player_bounds = self.player.bounding_rect().shrink(2.0);

        for tile in self.tilemap.tiles_of_type(tile_type) {
            if player_bounds.intersects(&tile.get_bounding_rect()) {
                return true;
            }
        }
        false
    }
}
