use crate::input::InputState;
use crate::player::Player;
use crate::tilemap::TileMap;

/// Core game engine - handles game state and logic independent of rendering
pub struct GameEngine {
    pub player: Player,
    pub tilemap: TileMap,
    frame_count: usize,
}

impl GameEngine {
    /// Create a new game engine with default state
    pub fn new() -> Self {
        let player = Player::new(100.0, 300.0);
        let tilemap = TileMap::new();

        Self {
            player,
            tilemap,
            frame_count: 0,
        }
    }

    /// Create a game engine with custom initial state (useful for testing)
    pub fn new_with(player: Player, tilemap: TileMap) -> Self {
        Self {
            player,
            tilemap,
            frame_count: 0,
        }
    }

    /// Run one game frame with the given input and delta time
    pub fn step(&mut self, input: &InputState, delta_time: f32) {
        // Update tilemap (for disappearing tiles and moving platforms)
        self.tilemap.update(delta_time);

        // Update player
        self.player.update(input, &mut self.tilemap, delta_time);

        // Check if player fell off the screen or touched deadly tile and reset if needed
        if self.player.is_dead(600) || self.player.is_touching_deadly_tile(&self.tilemap) {
            self.player.reset();
            self.tilemap.reset();
        }

        self.frame_count += 1;
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

    /// Get current frame count
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    /// Check if player is on ground (convenience helper for tests)
    pub fn is_player_on_ground(&self) -> bool {
        self.player.on_ground
    }
}
