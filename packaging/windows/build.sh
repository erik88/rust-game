#!/usr/bin/env bash
#
# Cross-compile the game for Windows (64-bit) from Linux and assemble a
# ready-to-ship folder in dist/windows/.
#
# Requirements (Fedora):
#   rustup target add x86_64-pc-windows-gnu
#   sudo dnf install mingw64-gcc mingw64-gcc-c++ mingw64-winpthreads-static
#
# SDL2 is linked dynamically against the vendored import lib in
# packaging/windows/libs/ (see .cargo/config.toml); the matching SDL2.dll is
# copied into the output folder so the game runs on a stock Windows machine
# with no installs.

set -euo pipefail
# Run from the repository root (this script lives in packaging/windows/).
cd "$(dirname "$0")/../.."

TARGET=x86_64-pc-windows-gnu
OUT=dist/windows

echo ">> Building rustgame.exe for ${TARGET} ..."
cargo build --release --target "${TARGET}" --bin rustgame

echo ">> Assembling ${OUT}/ ..."
rm -rf "${OUT}"
mkdir -p "${OUT}/levels"

# Executable
cp "target/${TARGET}/release/rustgame.exe" "${OUT}/"
# Runtime DLL
cp packaging/windows/libs/bin/SDL2.dll "${OUT}/"
# Game assets
cp tilemap.png character.png "${OUT}/"
cp levels/*.txt "${OUT}/levels/"

echo ">> Done. Contents of ${OUT}:"
find "${OUT}" -type f | sort | sed 's/^/   /'
echo
echo "Ship the entire '${OUT}' folder. Run rustgame.exe inside it."
