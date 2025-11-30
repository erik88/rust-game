#!/bin/bash
# Visualize a specific test by name
#
# Usage: ./visualize_test.sh test_name
#
# Example: ./visualize_test.sh test_downward_platform_stays_inactive_while_player_on_solid_ground

if [ -z "$1" ]; then
    echo "Usage: $0 <test_name>"
    echo ""
    echo "Available tests:"
    echo "  - test_player_falls_and_lands_on_tile"
    echo "  - test_player_rides_horizontally_moving_platform"
    echo "  - test_player_on_decaying_block_falls_through"
    echo "  - test_moving_platform_pushes_player"
    echo "  - test_player_standing_on_solid_and_death_block_survives"
    echo "  - test_player_intersecting_death_block_dies"
    echo "  - test_holding_jump_produces_high_jump"
    echo "  - test_player_jumps_against_platform_direction_while_being_pushed"
    echo "  - test_downward_platform_stays_inactive_while_player_on_solid_ground"
    echo ""
    echo "Or run all tests:"
    echo "  $0 all"
    exit 1
fi

if [ "$1" = "all" ]; then
    echo "Running all tests with visualization..."
    VISUALIZE_TEST=1 cargo test --test game_tests -- --test-threads=1 --nocapture
else
    echo "Running test: $1"
    VISUALIZE_TEST=1 cargo test "$1" -- --nocapture
fi
