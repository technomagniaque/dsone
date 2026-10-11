#!/usr/bin/env bash
# Usage: ./build-scripts/test/pacman.sh [path/to/dsone.pkg.tar.zst]
# Auto-finds in dist/pacman/
set -euo pipefail

cd "$(dirname "$0")/../.."
source build-scripts/test/common.sh

# ── Find .pkg.tar.zst ────────────────────────────────────────────────────────
PKG="${1:-}"
if [[ -z "$PKG" ]]; then
    candidate=$(ls dist/pacman/dsone-*.pkg.tar.zst 2>/dev/null | head -1 || true)
    if [[ -n "$candidate" ]]; then
        PKG="$candidate"
    fi
fi

if [[ -z "$PKG" || ! -f "$PKG" ]]; then
    echo "ERROR: No .pkg.tar.zst found. Build first or pass path as argument."
    exit 1
fi

PKG=$(realpath "$PKG")

echo "=== SONE .pkg.tar.zst Arch Linux Test ==="
echo "Package: $PKG"
echo ""
echo "Running smoke test on Arch Linux..."
echo "─────────────────────────────────────────────────"

# ── Distro definitions ────────────────────────────────────────────────────────
# Install runtime deps first (pacman -U doesn't resolve from repos),
# then install the package itself.
DISTROS=(
    "Arch Linux|archlinux:latest|pacman -Syu --noconfirm && pacman -S --noconfirm webkit2gtk-4.1 gtk3 gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav libsecret alsa-lib librsvg libayatana-appindicator xorg-server-xvfb dbus xdotool && pacman -U --noconfirm /pkg/*.pkg.tar.zst"
)

# Write native-package test script with pacman registry check
TEST_SCRIPT=$(mktemp /tmp/sone-test-XXXXXX.sh)
trap 'rm -f "$TEST_SCRIPT"' EXIT
write_test_script "$TEST_SCRIPT" "/usr/bin/dsone"

# Prepend package registry check
ORIG=$(cat "$TEST_SCRIPT")
cat > "$TEST_SCRIPT" << 'PREPEND'
#!/usr/bin/env bash
set -e

# Check: package installed in registry
if pacman -Qi dsone > /dev/null 2>&1; then
    echo "CHECK:pkg_installed:PASS:in_pacman_registry"
else
    echo "CHECK:pkg_installed:FAIL:not_in_registry"
fi

PREPEND
echo "$ORIG" | tail -n +3 >> "$TEST_SCRIPT"
chmod +x "$TEST_SCRIPT"

# ── Run all tests ─────────────────────────────────────────────────────────────
for distro in "${DISTROS[@]}"; do
    IFS='|' read -r label image install_cmd <<< "$distro"

    if ! docker pull "$image" > /dev/null 2>&1; then
        printf "%-26s %s\n" "$label" "SKIP"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    run_test "$label" "$image" "$install_cmd" "$TEST_SCRIPT" \
        -v "$PKG:/pkg/$(basename "$PKG"):ro"
done

print_summary
