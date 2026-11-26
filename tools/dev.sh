#!/usr/bin/env bash
# Build once, then start a host on the left half of the secondary monitor and a joiner on the
# right. Set SMOCKA_MONITOR=primary to use the primary monitor instead.
# Closing the joiner (or Ctrl+C) also closes the host. Extra arguments go to `cargo build`,
# e.g. `tools/dev.sh --release`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

monitor=()
[[ "${SMOCKA_MONITOR:-secondary}" == secondary ]] && monitor=(--secondary)

cargo build "$@"
profile=debug
[[ " $* " == *" --release "* ]] && profile=release
game="$root/target/$profile/smocka"

# Started directly, not through cargo, the game would look for `assets/` next to the binary.
export BEVY_ASSET_ROOT="$root"
# Wayland doesn't let apps place their own windows, so run on X11 (XWayland) where it's available.
if [[ -n "${WAYLAND_DISPLAY:-}" && -n "${DISPLAY:-}" ]]; then
    unset WAYLAND_DISPLAY
fi

"$game" host --left --name=Host "${monitor[@]}" &
host=$!
trap 'kill "$host" 2>/dev/null || true' EXIT

# Give the host a moment to start listening before the joiner knocks.
sleep 1
"$game" join --right --name=Joiner "${monitor[@]}"
