#!/usr/bin/env bash
# Build once, then start a host on the left half of the secondary monitor and a joiner on the
# right. Set SMOCKA_MONITOR=primary to use the primary monitor instead.
# Closing the joiner (or Ctrl+C) also closes the host. Extra arguments go to `cargo build`,
# e.g. `tools/dev.sh --release`. Shows the game's debug logs (set RUST_LOG to override), each line
# marked [host] or [join], and keeps each player's log in logs/host.log and logs/join.log.
# Only the host plays sound: the joiner is muted, so nothing is heard twice.
# Set SMOCKA_LAG=good, average or poor to play as if over the internet: both get `--lag` (see the
# README), e.g. `SMOCKA_LAG=average tools/dev.sh`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

monitor=()
[[ "${SMOCKA_MONITOR:-secondary}" == secondary ]] && monitor=(--secondary)
lag=()
[[ -n "${SMOCKA_LAG:-}" ]] && lag=(--lag="$SMOCKA_LAG")

cargo build "$@"
profile=debug
[[ " $* " == *" --release "* ]] && profile=release
game="$root/target/$profile/smocka"

# The game's `debug!` logs too; the graphics libraries only when something's wrong (see README).
export RUST_LOG="${RUST_LOG:-info,smocka=debug,wgpu=error,naga=warn}"
# Started directly, not through cargo, the game would look for `assets/` next to the binary.
export BEVY_ASSET_ROOT="$root"
# Wayland doesn't let apps place their own windows, so run on X11 (XWayland) where it's available.
if [[ -n "${WAYLAND_DISPLAY:-}" && -n "${DISPLAY:-}" ]]; then
    unset WAYLAND_DISPLAY
fi

logs="$root/logs"
mkdir -p "$logs"

# Mark each line with the peer it came from on the terminal, and save a copy without colours to
# logs/<peer>.log, replacing the last run's.
log_as() {
    local peer=$1
    tee >(sed -u 's/\x1b\[[0-9;]*m//g' >"$logs/$peer.log") | sed -u "s/^/[$peer] /"
}

"$game" host --left --name=Host "${monitor[@]}" "${lag[@]}" > >(log_as host) 2>&1 &
host=$!
trap 'kill "$host" 2>/dev/null || true' EXIT

# Give the host a moment to start listening before the joiner knocks.
sleep 1
"$game" join --right --name=Joiner --mute "${monitor[@]}" "${lag[@]}" > >(log_as join) 2>&1
