/// Size of the game window and of the visible play area, in pixels. The
/// camera clamping in `main` and the render culling in [`tilemap`] both
/// derive from these, so changing the resolution here changes it everywhere.
pub const SCREEN_WIDTH: u32 = 800;
pub const SCREEN_HEIGHT: u32 = 600;

pub mod engine;
pub mod font;
pub mod geometry;
pub mod input;
pub mod level;
pub mod player;
pub mod texture;
pub mod tilemap;
pub mod tiles;
pub mod time;
