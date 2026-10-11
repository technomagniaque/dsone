#!/usr/bin/env bash
# Usage: ./build-scripts/build/pacman.sh [--no-cache]
# Output: ./dist/pacman/dsone-<version>-1-x86_64.pkg.tar.zst
set -euo pipefail

cd "$(dirname "$0")/../.."

export DOCKER_BUILDKIT=1
IMAGE="sone-pacman-builder"
OUTDIR="dist/pacman"
DOCKER_ARGS=()

if [[ "${1:-}" == "--no-cache" ]]; then
    DOCKER_ARGS+=(--no-cache)
fi

mkdir -p "$OUTDIR"

echo "=== Building .pkg.tar.zst in Docker (Arch Linux) ==="
echo ""

echo "Building Docker image..."
docker build ${DOCKER_ARGS[@]+"${DOCKER_ARGS[@]}"} -f build-scripts/build/Dockerfile.pacman -t "$IMAGE" .

echo ""
echo "Extracting .pkg.tar.zst..."
CONTAINER=$(docker create "$IMAGE")
docker cp "$CONTAINER:/output/." "$OUTDIR/"
docker rm "$CONTAINER" > /dev/null

# Post-build check
echo ""
PKG=$(ls "$OUTDIR"/dsone-*.pkg.tar.zst 2>/dev/null | head -1)
if [[ -z "$PKG" ]]; then
    echo "ERROR: No .pkg.tar.zst found in $OUTDIR after build."
    exit 1
fi

echo ""
echo "=== Build complete ==="
ls -lh "$PKG"
