# Test Visualization Guide

The test suite supports optional visualization to help debug and understand game behavior.

## Quick Start

### Visualize a Single Test

```bash
VISUALIZE_TEST=1 cargo test test_downward_platform_stays_inactive_while_player_on_solid_ground -- --nocapture
```

### Visualize All Tests

```bash
VISUALIZE_TEST=1 cargo test --test game_tests -- --test-threads=1 --nocapture
```

## How It Works

When `VISUALIZE_TEST=1` is set:

1. **Window Opens**: A 800x600 window titled "Test Visualization" appears
2. **Real-time Rendering**: Each test frame is rendered in real-time (at ~60 FPS)
3. **Pause at End**: When the test completes, the window stays open showing the final state
4. **Close to Continue**: Close the window or press ESC to proceed to the next test

## Available Tests

- `test_player_falls_and_lands_on_tile` - Watch player fall and land
- `test_player_rides_horizontally_moving_platform` - See platform movement
- `test_player_on_decaying_block_falls_through` - Observe block decay
- `test_moving_platform_pushes_player` - See platform pushing mechanics
- `test_player_standing_on_solid_and_death_block_survives` - Test death block edge cases
- `test_player_intersecting_death_block_dies` - See death and respawn
- `test_holding_jump_produces_high_jump` - Watch variable jump height
- `test_player_jumps_against_platform_direction_while_being_pushed` - Complex platform interaction
- `test_downward_platform_stays_inactive_while_player_on_solid_ground` - Platform activation logic

## Tips

- Use `--nocapture` to see console output while visualizing
- Use `--test-threads=1` when running multiple tests to avoid window conflicts
- The window pause at the end is automatic - no need to add delays to your tests
- Press ESC for quick window dismissal

## Example Session

```bash
# Test the downward platform behavior and see it in action
VISUALIZE_TEST=1 cargo test test_downward_platform_stays_inactive -- --nocapture

# Output:
# running 1 test
# test tests::test_downward_platform_stays_inactive_while_player_on_solid_ground ...
# Test visualization paused. Close the window to continue...
# ok
```

## Troubleshooting

- **Window doesn't appear**: Make sure `character.png` and `tilemap.png` are in the project root
- **Tests fail with visualization**: Run without `VISUALIZE_TEST=1` to see pure test output
- **Window closes immediately**: Add `--nocapture` flag to prevent test output buffering
