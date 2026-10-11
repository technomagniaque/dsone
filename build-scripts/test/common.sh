#!/usr/bin/env bash
# Shared test infrastructure for SONE package testing.
# Source this file from individual test scripts.
#
# Required by caller before sourcing:
#   APP_CMD  — command to launch SONE inside the container
#              AppImage: "cd /tmp/squashfs-root && ./AppRun"
#              Native:   "/usr/bin/dsone"
#
# Provides:
#   write_test_script()  — writes the inner test script to $TEST_SCRIPT
#   run_test()           — runs a single distro test container
#   print_summary()      — prints pass/fail/skip summary, exits 1 on failures
#
# Caller manages: DISTROS array, PASSED/FAILED/SKIPPED/FAIL_DETAILS globals

PASSED=0
FAILED=0
SKIPPED=0
FAIL_DETAILS=""

# Write the inner test script that runs inside each container.
# Args: $1 = path to write, $2 = app launch command
write_test_script() {
    local script_path="$1"
    local app_cmd="$2"

    cat > "$script_path" << TESTEOF
#!/usr/bin/env bash
set -e

# ldd check
LDD_OUTPUT=\$(ldd /usr/bin/dsone 2>&1 || true)
if echo "\$LDD_OUTPUT" | grep -q "not found"; then
    echo "LDD_MISSING:"
    echo "\$LDD_OUTPUT" | grep "not found"
    echo "LDD_END"
fi

# Start dbus session
eval \$(dbus-launch --sh-syntax)
export DBUS_SESSION_BUS_ADDRESS

# Start Xvfb
Xvfb :99 -screen 0 1024x768x24 &
XVFB_PID=\$!
export DISPLAY=:99
sleep 1

# Start app in background with debug logging
RUST_LOG=debug $app_cmd 2>/tmp/stderr.log &
APP_PID=\$!

# ── Poll for window (up to 15s) ──────────────────────────────────────────────
WINDOW_FOUND=0
for i in \$(seq 1 30); do
    if ! kill -0 \$APP_PID 2>/dev/null; then
        break
    fi
    if xdotool search --name "dsone" > /dev/null 2>&1; then
        WINDOW_FOUND=1
        break
    fi
    sleep 0.5
done

# Settle time for subsystems to initialize
if [[ \$WINDOW_FOUND -eq 1 ]]; then
    sleep 2
fi

# ── Run checks ───────────────────────────────────────────────────────────────
STDERR_CONTENT=""
if [[ -f /tmp/stderr.log ]]; then
    STDERR_CONTENT=\$(cat /tmp/stderr.log 2>/dev/null || true)
fi

# Check 1: window_created
if [[ \$WINDOW_FOUND -eq 1 ]]; then
    echo "CHECK:window_created:PASS:window_found"
elif ! kill -0 \$APP_PID 2>/dev/null; then
    echo "CHECK:window_created:FAIL:app_crashed_before_window"
else
    echo "CHECK:window_created:FAIL:no_window_within_15s"
fi

# Check 2: mpris_registered
APP_ALIVE=0
kill -0 \$APP_PID 2>/dev/null && APP_ALIVE=1
if [[ \$APP_ALIVE -eq 1 ]]; then
    DBUS_NAMES=\$(dbus-send --session --dest=org.freedesktop.DBus --type=method_call \
        --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>/dev/null || true)
    if echo "\$DBUS_NAMES" | grep -q "org.mpris.MediaPlayer2.io.github.technomagniaque.dsone"; then
        echo "CHECK:mpris_registered:PASS:on_bus"
    elif echo "\$STDERR_CONTENT" | grep -q "MPRIS D-Bus server started"; then
        echo "CHECK:mpris_registered:WARN:started_different_bus"
    else
        echo "CHECK:mpris_registered:FAIL:not_on_bus"
    fi
else
    if echo "\$STDERR_CONTENT" | grep -q "MPRIS D-Bus server started"; then
        echo "CHECK:mpris_registered:WARN:started_then_crashed"
    else
        echo "CHECK:mpris_registered:FAIL:no_mpris_evidence"
    fi
fi

# Check 3: gstreamer_init
if echo "\$STDERR_CONTENT" | grep -q "DeviceMonitor found"; then
    echo "CHECK:gstreamer_init:PASS:device_monitor_ok"
elif echo "\$STDERR_CONTENT" | grep -qi "gstreamer.*error\|gst_init.*failed"; then
    echo "CHECK:gstreamer_init:FAIL:init_error"
elif echo "\$STDERR_CONTENT" | grep -qi "GStreamer"; then
    echo "CHECK:gstreamer_init:PASS:gst_loaded"
else
    echo "CHECK:gstreamer_init:WARN:no_gst_log_found"
fi

# Check 4: config_dir
if [[ -d "\$HOME/.config/sone" ]]; then
    echo "CHECK:config_dir:PASS:dir_exists"
else
    echo "CHECK:config_dir:FAIL:dir_missing"
fi

# Check 5: crypto_init
if [[ -f "\$HOME/.config/sone/sone.key" ]]; then
    echo "CHECK:crypto_init:PASS:key_file_exists"
elif echo "\$STDERR_CONTENT" | grep -qi "crypto.*error\|key.*error\|encryption.*fail"; then
    echo "CHECK:crypto_init:FAIL:crypto_error_in_log"
elif echo "\$STDERR_CONTENT" | grep -qi "crypto\|encrypt\|master.key\|sone\.key"; then
    echo "CHECK:crypto_init:PASS:crypto_log_found"
else
    echo "CHECK:crypto_init:WARN:no_crypto_evidence"
fi

# Check 6: no_crash
if kill -0 \$APP_PID 2>/dev/null; then
    echo "CHECK:no_crash:PASS:app_running"
else
    echo "CHECK:no_crash:FAIL:app_died"
fi

# Kill app
kill \$APP_PID 2>/dev/null || true
wait \$APP_PID 2>/dev/null || true
kill \$XVFB_PID 2>/dev/null || true

# Output stderr tail for diagnostics
if [[ -f /tmp/stderr.log ]]; then
    echo "STDERR_START"
    tail -30 /tmp/stderr.log
    echo "STDERR_END"
fi
TESTEOF
    chmod +x "$script_path"
}

# Write the AppImage-specific test script (extracts + runs AppRun).
# Args: $1 = path to write
write_appimage_test_script() {
    local script_path="$1"

    cat > "$script_path" << 'TESTEOF'
#!/usr/bin/env bash
set -e

# Extract AppImage
cd /tmp
/appimage/*.AppImage --appimage-extract > /dev/null 2>&1

# ldd check
LDD_OUTPUT=$(ldd squashfs-root/usr/bin/dsone 2>&1 || true)
if echo "$LDD_OUTPUT" | grep -q "not found"; then
    echo "LDD_MISSING:"
    echo "$LDD_OUTPUT" | grep "not found"
    echo "LDD_END"
fi

# Start dbus session
eval $(dbus-launch --sh-syntax)
export DBUS_SESSION_BUS_ADDRESS

# Start Xvfb
Xvfb :99 -screen 0 1024x768x24 &
XVFB_PID=$!
export DISPLAY=:99
sleep 1

# Start app in background with debug logging
cd squashfs-root
RUST_LOG=debug ./AppRun 2>/tmp/stderr.log &
APP_PID=$!
cd /tmp

# ── Poll for window (up to 15s) ──────────────────────────────────────────────
WINDOW_FOUND=0
for i in $(seq 1 30); do
    if ! kill -0 $APP_PID 2>/dev/null; then
        break
    fi
    if xdotool search --name "dsone" > /dev/null 2>&1; then
        WINDOW_FOUND=1
        break
    fi
    sleep 0.5
done

# Settle time for subsystems to initialize
if [[ $WINDOW_FOUND -eq 1 ]]; then
    sleep 2
fi

# ── Run checks ───────────────────────────────────────────────────────────────
STDERR_CONTENT=""
if [[ -f /tmp/stderr.log ]]; then
    STDERR_CONTENT=$(cat /tmp/stderr.log 2>/dev/null || true)
fi

# Check 1: window_created
if [[ $WINDOW_FOUND -eq 1 ]]; then
    echo "CHECK:window_created:PASS:window_found"
elif ! kill -0 $APP_PID 2>/dev/null; then
    echo "CHECK:window_created:FAIL:app_crashed_before_window"
else
    echo "CHECK:window_created:FAIL:no_window_within_15s"
fi

# Check 2: mpris_registered
APP_ALIVE=0
kill -0 $APP_PID 2>/dev/null && APP_ALIVE=1
if [[ $APP_ALIVE -eq 1 ]]; then
    DBUS_NAMES=$(dbus-send --session --dest=org.freedesktop.DBus --type=method_call \
        --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>/dev/null || true)
    if echo "$DBUS_NAMES" | grep -q "org.mpris.MediaPlayer2.io.github.technomagniaque.dsone"; then
        echo "CHECK:mpris_registered:PASS:on_bus"
    elif echo "$STDERR_CONTENT" | grep -q "MPRIS D-Bus server started"; then
        echo "CHECK:mpris_registered:WARN:started_different_bus"
    else
        echo "CHECK:mpris_registered:FAIL:not_on_bus"
    fi
else
    if echo "$STDERR_CONTENT" | grep -q "MPRIS D-Bus server started"; then
        echo "CHECK:mpris_registered:WARN:started_then_crashed"
    else
        echo "CHECK:mpris_registered:FAIL:no_mpris_evidence"
    fi
fi

# Check 3: gstreamer_init
if echo "$STDERR_CONTENT" | grep -q "DeviceMonitor found"; then
    echo "CHECK:gstreamer_init:PASS:device_monitor_ok"
elif echo "$STDERR_CONTENT" | grep -qi "gstreamer.*error\|gst_init.*failed"; then
    echo "CHECK:gstreamer_init:FAIL:init_error"
elif echo "$STDERR_CONTENT" | grep -qi "GStreamer"; then
    echo "CHECK:gstreamer_init:PASS:gst_loaded"
else
    echo "CHECK:gstreamer_init:WARN:no_gst_log_found"
fi

# Check 4: config_dir
if [[ -d "$HOME/.config/sone" ]]; then
    echo "CHECK:config_dir:PASS:dir_exists"
else
    echo "CHECK:config_dir:FAIL:dir_missing"
fi

# Check 5: crypto_init
if [[ -f "$HOME/.config/sone/sone.key" ]]; then
    echo "CHECK:crypto_init:PASS:key_file_exists"
elif echo "$STDERR_CONTENT" | grep -qi "crypto.*error\|key.*error\|encryption.*fail"; then
    echo "CHECK:crypto_init:FAIL:crypto_error_in_log"
elif echo "$STDERR_CONTENT" | grep -qi "crypto\|encrypt\|master.key\|sone\.key"; then
    echo "CHECK:crypto_init:PASS:crypto_log_found"
else
    echo "CHECK:crypto_init:WARN:no_crypto_evidence"
fi

# Check 6: no_crash
if kill -0 $APP_PID 2>/dev/null; then
    echo "CHECK:no_crash:PASS:app_running"
else
    echo "CHECK:no_crash:FAIL:app_died"
fi

# Kill app
kill $APP_PID 2>/dev/null || true
wait $APP_PID 2>/dev/null || true
kill $XVFB_PID 2>/dev/null || true

# Output stderr tail for diagnostics
if [[ -f /tmp/stderr.log ]]; then
    echo "STDERR_START"
    tail -30 /tmp/stderr.log
    echo "STDERR_END"
fi
TESTEOF
    chmod +x "$script_path"
}

# Run a test in a Docker container.
# Args: $1=label, $2=image, $3=install_cmd, $4=test_script_path
#        $5=package_mount (e.g. "-v /path/to/pkg:/pkg:ro" or "-v /path:/appimage:ro")
run_test() {
    local label="$1"
    local image="$2"
    local install_cmd="$3"
    local test_script="$4"
    shift 4
    local extra_mounts=("$@")

    local output
    output=$(timeout 180 docker run --rm \
        --security-opt seccomp=unconfined \
        "${extra_mounts[@]}" \
        -v "$test_script:/test.sh:ro" \
        "$image" \
        bash -c "{ $install_cmd; } > /tmp/install.log 2>&1 || { echo INSTALL_FAILED; tail -20 /tmp/install.log; echo INSTALL_END; exit 1; }; bash /test.sh" 2>&1) || true

    # Parse install failure
    local install_failed
    install_failed=$(echo "$output" | sed -n '/^INSTALL_FAILED$/,/^INSTALL_END$/p' | grep -v 'INSTALL_FAILED\|INSTALL_END' || true)

    # Parse ldd missing
    local ldd_missing
    ldd_missing=$(echo "$output" | sed -n '/^LDD_MISSING:$/,/^LDD_END$/p' | grep -v 'LDD_MISSING:\|LDD_END' || true)

    # Parse stderr
    local stderr
    stderr=$(echo "$output" | sed -n '/^STDERR_START$/,/^STDERR_END$/p' | grep -v 'STDERR_START\|STDERR_END' || true)

    # Parse CHECK lines
    local check_lines
    check_lines=$(echo "$output" | grep '^CHECK:' || true)

    # Determine overall result
    local result="PASS"
    local reason=""
    local check_passed=0
    local check_failed=0
    local check_warned=0
    local check_total=0

    if [[ -n "$install_failed" ]]; then
        result="FAIL"
        reason="Package install failed"
        stderr="$install_failed"
    elif [[ -n "$ldd_missing" ]]; then
        result="FAIL"
        reason="Missing shared libraries"
    fi

    if [[ -z "$check_lines" && -z "$install_failed" && "$result" != "FAIL" ]]; then
        result="FAIL"
        reason="Container failed (no check output captured)"
        stderr="$output"
    fi

    # Count and evaluate checks
    local check_details=""
    while IFS= read -r line; do
        [[ -z "$line" ]] && continue
        local name status detail
        name=$(echo "$line" | cut -d: -f2)
        status=$(echo "$line" | cut -d: -f3)
        detail=$(echo "$line" | cut -d: -f4-)
        check_total=$((check_total + 1))

        case "$status" in
            PASS) check_passed=$((check_passed + 1)) ;;
            FAIL) check_failed=$((check_failed + 1)); result="FAIL" ;;
            WARN) check_warned=$((check_warned + 1)) ;;
        esac

        check_details+="$(printf "  %-26s %-5s %s\n" "$name" "$status" "$detail")"$'\n'
    done <<< "$check_lines"

    # Print result header
    local summary=""
    if [[ $check_total -gt 0 ]]; then
        summary="($check_passed/$check_total"
        if [[ $check_warned -gt 0 ]]; then
            summary+=", $check_warned warn"
        fi
        summary+=")"
    fi
    printf "%-26s %-4s %s\n" "$label" "$result" "$summary"

    # Print per-check breakdown
    if [[ -n "$check_details" ]]; then
        echo -n "$check_details"
    fi
    echo ""

    if [[ "$result" == "FAIL" ]]; then
        FAILED=$((FAILED + 1))
        FAIL_DETAILS+="
--- $label ---
$(if [[ -n "$reason" ]]; then echo "Reason: $reason"; fi)
$(echo "$stderr" | tail -20)
"
    else
        PASSED=$((PASSED + 1))
    fi
}

print_summary() {
    echo "─────────────────────────────────────────────────"
    echo "Results: $PASSED passed, $FAILED failed, $SKIPPED skipped"

    if [[ $FAILED -gt 0 ]]; then
        echo ""
        echo "=== Failure Details ==="
        echo "$FAIL_DETAILS"
        exit 1
    fi
}
