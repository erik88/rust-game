#!/usr/bin/env bash
#
# Build a self-contained Linux AppImage of the game.
#
# The resulting dist/linux/rustgame-x86_64.AppImage bundles the binary, the
# game assets, and SDL2, so it runs on most x86-64 desktop distros without
# installing anything. SDL2 dlopen()s the actual video/GL/audio drivers from
# the host at runtime, so we deliberately bundle only libSDL2 itself and let
# the host provide everything below it.
#
# We assemble the AppDir by hand and pack it with appimagetool rather than
# using linuxdeploy: linuxdeploy rewrites each bundled library's rpath with
# patchelf, and the patchelf it ships is too old to understand the RELR
# (.relr.dyn) sections in Fedora's libraries, silently corrupting libSDL2 so it
# crashes on load. appimagetool only packs the AppDir; it never touches the
# libraries.
#
# Requirements (Fedora): the SDL2 runtime lib the binary links against must be
# installed so it can be copied in:
#   sudo dnf install SDL2
# appimagetool is downloaded automatically into tools/ on first run.

set -euo pipefail
# Run from the repository root (this script lives in packaging/linux/).
cd "$(dirname "$0")/../.."

APP=rustgame
TARGET_BIN=target/release/${APP}
APPDIR=dist/linux/${APP}.AppDir
OUT=dist/linux/${APP}-x86_64.AppImage
APPIMAGETOOL=tools/appimagetool-x86_64.AppImage
APPIMAGETOOL_URL=https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage

# 1. Build the release binary.
echo ">> Building ${APP} (release) ..."
cargo build --release --bin "${APP}"

# 2. Fetch appimagetool if we don't have it yet.
if [ ! -x "${APPIMAGETOOL}" ]; then
    echo ">> Downloading appimagetool ..."
    mkdir -p tools
    curl -fL -o "${APPIMAGETOOL}" "${APPIMAGETOOL_URL}"
    chmod +x "${APPIMAGETOOL}"
fi

# 3. Locate the system libSDL2 to bundle (resolve the symlink to the real file).
SDL2_LIB=$(ldconfig -p | awk '/libSDL2-2.0.so.0/{print $NF; exit}')
if [ -z "${SDL2_LIB}" ]; then
    echo "!! libSDL2-2.0.so.0 not found. Install it with: sudo dnf install SDL2" >&2
    exit 1
fi

# 4. Lay out the AppDir.
echo ">> Assembling AppDir (bundling $(realpath "${SDL2_LIB}")) ..."
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin" \
         "${APPDIR}/usr/lib" \
         "${APPDIR}/usr/share/${APP}/levels" \
         "${APPDIR}/usr/share/applications" \
         "${APPDIR}/usr/share/icons/hicolor/256x256/apps"

cp "${TARGET_BIN}" "${APPDIR}/usr/bin/"
cp -L "${SDL2_LIB}" "${APPDIR}/usr/lib/libSDL2-2.0.so.0"
cp tilemap.png character.png "${APPDIR}/usr/share/${APP}/"
cp levels/*.txt "${APPDIR}/usr/share/${APP}/levels/"

# AppRun launcher (sets LD_LIBRARY_PATH and cd's into the asset dir).
cp packaging/linux/AppRun "${APPDIR}/AppRun"
chmod +x "${APPDIR}/AppRun"

# Desktop file + icon. appimagetool wants both at the AppDir root; we also place
# them under usr/share for desktops that integrate the AppImage into a menu.
cp packaging/linux/${APP}.desktop "${APPDIR}/${APP}.desktop"
cp packaging/linux/${APP}.desktop "${APPDIR}/usr/share/applications/${APP}.desktop"
cp packaging/linux/${APP}.png "${APPDIR}/${APP}.png"
cp packaging/linux/${APP}.png "${APPDIR}/usr/share/icons/hicolor/256x256/apps/${APP}.png"

# 5. Pack the AppImage.
echo ">> Packing the AppImage ..."
export APPIMAGE_EXTRACT_AND_RUN=1   # avoids needing a FUSE mount for the tool
"${APPIMAGETOOL}" "${APPDIR}" "${OUT}"

echo
echo ">> Done: ${OUT}"
echo "   Run it with:  ./${OUT}"
