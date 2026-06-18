#!/usr/bin/env bash
#
# Build a self-contained macOS .app bundle (and a .dmg) for the game.
#
#   ./packaging/macos/build.sh            # build for the host architecture
#   ./packaging/macos/build.sh --universal  # build a universal (arm64 + x86_64) binary
#
# SDL2 is compiled from source and statically linked (see the macOS dependency
# section in Cargo.toml), so the bundle has no external library dependencies.
# This requires `cmake` at build time:  brew install cmake
#
# The resulting app is UNSIGNED. On first launch recipients must right-click the
# app and choose "Open" (or run: xattr -dr com.apple.quarantine RustGame.app).
set -euo pipefail

APP_NAME="RustGame"
BIN_NAME="rustgame"
BUNDLE_ID="com.themerion.rustgame"

# Locate the project root (two levels up from this script).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

if ! command -v cmake >/dev/null 2>&1; then
    echo "error: cmake is required to build the bundled SDL2." >&2
    echo "       install it with:  brew install cmake" >&2
    exit 1
fi

# --- Build the binary -------------------------------------------------------

# The vendored SDL2 sources ship a CMakeLists.txt that requests compatibility
# with CMake < 3.5, which CMake 4 refuses by default. Allow it (honoured by
# CMake >= 3.31) so the bundled static build configures cleanly.
export CMAKE_POLICY_VERSION_MINIMUM=3.5

BINARY=""   # set to the path of the final (possibly merged) executable
if [[ "${1:-}" == "--universal" ]]; then
    echo ">> Building universal (arm64 + x86_64) release binary"
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
    cargo build --release --features bundle-sdl2 --bin "$BIN_NAME" --target aarch64-apple-darwin
    cargo build --release --features bundle-sdl2 --bin "$BIN_NAME" --target x86_64-apple-darwin
    BINARY="target/${BIN_NAME}-universal"
    lipo -create -output "$BINARY" \
        "target/aarch64-apple-darwin/release/$BIN_NAME" \
        "target/x86_64-apple-darwin/release/$BIN_NAME"
else
    echo ">> Building release binary for the host architecture"
    cargo build --release --features bundle-sdl2 --bin "$BIN_NAME"
    BINARY="target/release/$BIN_NAME"
fi

# --- Assemble the .app bundle ----------------------------------------------

DIST="$ROOT/dist"
APP="$DIST/$APP_NAME.app"
CONTENTS="$APP/Contents"
echo ">> Assembling $APP"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

cp "$BINARY" "$CONTENTS/MacOS/$BIN_NAME"
chmod +x "$CONTENTS/MacOS/$BIN_NAME"

# Assets are loaded by relative path; main.rs chdirs into Resources at startup.
cp "$ROOT/character.png" "$ROOT/tilemap.png" "$CONTENTS/Resources/"
cp -R "$ROOT/levels" "$CONTENTS/Resources/levels"

# --- App icon (.icns from the 256x256 PNG) ---------------------------------

ICON_SRC="$ROOT/packaging/linux/rustgame.png"
if [[ -f "$ICON_SRC" ]]; then
    echo ">> Generating app icon"
    ICONSET="$(mktemp -d)/icon.iconset"
    mkdir -p "$ICONSET"
    for size in 16 32 64 128 256 512; do
        sips -z "$size" "$size" "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
        sips -z $((size*2)) $((size*2)) "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$ICONSET" -o "$CONTENTS/Resources/$APP_NAME.icns"
    rm -rf "$(dirname "$ICONSET")"
    ICON_PLIST="    <key>CFBundleIconFile</key>
    <string>$APP_NAME</string>"
else
    ICON_PLIST=""
fi

# --- Info.plist -------------------------------------------------------------

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')"
cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleExecutable</key>
    <string>$BIN_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>NSHighResolutionCapable</key>
    <true/>
$ICON_PLIST
</dict>
</plist>
PLIST

# --- Seal the bundle (ad-hoc) ----------------------------------------------

# The linker already ad-hoc-signs the inner Mach-O, but the .app as a whole has
# no resource seal (_CodeSignature/CodeResources). macOS treats that as an
# invalid/inconsistent signature and refuses to launch the app. Signing the
# bundle ad-hoc seals the Info.plist + Resources so the seal is consistent.
# This is NOT notarization: recipients still clear quarantine on first launch
# (right-click -> Open), but the app will actually run.
echo ">> Sealing bundle (ad-hoc signature)"
codesign --force --sign - --timestamp=none "$APP"
codesign --verify --deep --strict "$APP" && echo "   signature verified ✓"

echo ">> Built $APP"

# --- Package into a .dmg ----------------------------------------------------

DMG="$DIST/$APP_NAME.dmg"
echo ">> Creating $DMG"
rm -f "$DMG"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo ""
echo "Done:"
echo "  $APP"
echo "  $DMG"
echo ""
echo "The app is unsigned. Recipients should right-click -> Open on first launch,"
echo "or run: xattr -dr com.apple.quarantine '$APP_NAME.app'"
