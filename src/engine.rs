use crate::input::InputState;
use crate::player::Player;
use crate::tilemap::TileMap;

pub enum OnDeath {
    Respawn,
    Stop,
}

/// Core game engine - handles game state and logic independent of rendering
pub struct GameEngine {
    pub player: Player,
    pub tilemap: TileMap,
    pub on_death: OnDeath,
    pub stopped: bool,
}

impl GameEngine {
    /// Create a game engine
    pub fn new_with(player: Player, tilemap: TileMap, on_death: OnDeath) -> Self {
        Self {
            player,
            tilemap,
            on_death,
            stopped: false,
        }
    }

    /// Run one game frame with the given input and delta time
    pub fn step(&mut self, input: &InputState, delta_time: f32) {
        if self.stopped {
            return;
        }

        // Update tilemap (for disappearing tiles and moving platforms)
        self.tilemap.update(delta_time);

        // Update player
        self.player.update(input, &mut self.tilemap, delta_time);

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

    /// Get mutable reference to player (for setup/testing)
    pub fn player_mut(&mut self) -> &mut Player {
        &mut self.player
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
        // Add a small margin, so that while the player is standing with one foot on solid ground,
        // he will not touch the deadly tile.
        let player_bounds = self.player.bounding_rect().shrink(2.0);

        // 3 = deadly tiles
        for tile in self.tilemap.tiles_of_type(3) {
            if player_bounds.intersects(&tile.get_bounding_rect()) {
                return true;
            }
        }
        false
    }
}
