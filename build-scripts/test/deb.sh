#!/usr/bin/env bash
# Usage: ./build-scripts/test/deb.sh [path/to/dsone.deb]
# Auto-finds in dist/deb/ or src-tauri/target/release/bundle/deb/
set -euo pipefail

cd "$(dirname "$0")/../.."
source build-scripts/test/common.sh

# ── Find .deb ─────────────────────────────────────────────────────────────────
DEB="${1:-}"
if [[ -z "$DEB" ]]; then
    for dir in dist/deb src-tauri/target/release/bundle/deb; do
        candidate=$(ls "$dir"/dsone_*.deb 2>/dev/null | head -1 || true)
        if [[ -n "$candidate" ]]; then
            DEB="$candidate"
            break
        fi
    done
fi

if [[ -z "$DEB" || ! -f "$DEB" ]]; then
    echo "ERROR: No .deb found. Build first or pass path as argument."
    exit 1
fi

DEB=$(realpath "$DEB")

echo "=== SONE .deb Multi-Distro Test ==="
echo "Package: $DEB"
echo ""
echo "Running smoke tests across 3 distros..."
echo "─────────────────────────────────────────────────"

# ── Distro definitions ────────────────────────────────────────────────────────
# dpkg -i + apt-get install -f resolves declared dependencies — validates correctness
DISTROS=(
    "Ubuntu 22.04|ubuntu:22.04|apt-get update && dpkg -i /pkg/*.deb || true && apt-get install -f -y && apt-get install -y xvfb dbus-x11 xdotool"
    "Ubuntu 24.04|ubuntu:24.04|apt-get update && dpkg -i /pkg/*.deb || true && apt-get install -f -y && apt-get install -y xvfb dbus-x11 xdotool"
    "Debian 12|debian:12|apt-get update && dpkg -i /pkg/*.deb || true && apt-get install -f -y && apt-get install -y xvfb dbus-x11 xdotool"
)

# Write native-package test script (runs /usr/bin/sone, adds package registry check)
TEST_SCRIPT=$(mktemp /tmp/sone-test-XXXXXX.sh)
trap 'rm -f "$TEST_SCRIPT"' EXIT
write_test_script "$TEST_SCRIPT" "/usr/bin/dsone"

# Prepend package registry check
ORIG=$(cat "$TEST_SCRIPT")
cat > "$TEST_SCRIPT" << 'PREPEND'
#!/usr/bin/env bash
set -e

# Check: package installed in registry
if dpkg -s dsone > /dev/null 2>&1; then
    echo "CHECK:pkg_installed:PASS:in_dpkg_registry"
else
    echo "CHECK:pkg_installed:FAIL:not_in_registry"
fi

PREPEND
# Append original script (skip its shebang and set -e)
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
        -v "$DEB:/pkg/$(basename "$DEB"):ro"
done

print_summary
