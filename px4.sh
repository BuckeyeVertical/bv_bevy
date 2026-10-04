#!/usr/bin/env bash
# Start PX4 + Gazebo with the home position of a Bevy world.
#
#   ./px4.sh <world> [world options]             e.g. ./px4.sh suas_2026
#   ./px4.sh --prebuilt <world> [world options]  use the published image
set -euo pipefail
cd "$(dirname "$0")"

compose=gazebo/compose.px4.yaml
if [[ "${1:-}" == "--prebuilt" ]]; then
    compose=gazebo/compose.px4.prebuilt.yaml
    shift
fi

# The world knows its home; ask it rather than keeping a second copy here.
home="$(cargo run --quiet -- "$@" --px4-home)"
eval "$(sed 's/^/export /' <<<"$home")"
echo "PX4 home: $PX4_HOME_LAT, $PX4_HOME_LON (alt $PX4_HOME_ALT)" >&2

exec docker compose -f "$compose" up
