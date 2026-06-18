# rust-game

A small 2D platform game built with Rust and SDL2.

## Development

### Prerequisites

The game links against SDL2. On macOS, install it via Homebrew:

```bash
brew install sdl2
```

On Apple Silicon, Homebrew installs libraries under `/opt/homebrew/lib`, which is
**not** on the linker's default search path. If `cargo build`/`cargo run` fails
with `ld: library 'SDL2' not found`, add that directory to `LIBRARY_PATH`:

```bash
export LIBRARY_PATH="/opt/homebrew/lib:$LIBRARY_PATH"
```

Add that line to your shell profile (`~/.zshrc`) to make it permanent.

### Build and run

```bash
cargo run                 # run the game
cargo run --bin level_editor   # run the level editor
cargo test                # run the test suite
```

Assets (`character.png`, `tilemap.png`) and the `levels/` directory are loaded
from the current working directory, so run cargo from the project root.

## Packaging for distribution

Platform packaging scripts live under `packaging/`.

### macOS (`.app` + `.dmg`)

```bash
brew install cmake                      # one-time: needed to build SDL2 from source
./packaging/macos/build.sh              # build for the host architecture
./packaging/macos/build.sh --universal  # arm64 + x86_64, runs on any modern Mac
```

This produces `dist/RustGame.app` and `dist/RustGame.dmg`. SDL2 is compiled from
source and statically linked (via the `bundle-sdl2` cargo feature), so the bundle
has no external library dependency and runs on any Mac without Homebrew.

The app is **ad-hoc signed, not notarized**. It will run, but recipients must
clear macOS quarantine on first launch:

- Right-click `RustGame.app` → **Open** → **Open**, once, **or**
- `xattr -dr com.apple.quarantine /Applications/RustGame.app`

Send the `.dmg` only — it already contains the app.

### Linux / Windows

See `packaging/linux/` and `packaging/windows/` for their respective build
scripts.
