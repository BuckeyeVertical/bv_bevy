#!/usr/bin/env bash
set -euo pipefail

world_name="${BV_PX4_WORLD:-bv_mission}"
world="/opt/px4/worlds/${world_name}.sdf"
config="${BV_BRIDGE_CONFIG:-/opt/bv/config/px4_x500_gimbal.json}"

export GZ_IP="${GZ_IP:-127.0.0.1}"
export GZ_SIM_RESOURCE_PATH="/opt/px4/models:/opt/px4/worlds"
export GZ_SIM_SYSTEM_PLUGIN_PATH="/opt/px4/plugins"
export GZ_SIM_SERVER_CONFIG_PATH="/opt/px4/server.config"
export PX4_GZ_MODELS=/opt/px4/models
export PX4_GZ_WORLDS=/opt/px4/worlds
export PX4_GZ_PLUGINS=/opt/px4/plugins
export PX4_GZ_WORLD="$world_name"
export PX4_GZ_STANDALONE=1
export PX4_GZ_NO_FOLLOW=1
export PX4_SIM_MODEL=gz_x500_gimbal
: "${PX4_HOME_LAT:?PX4 home not set: start the simulator with ./px4.sh <world>}"
: "${PX4_HOME_LON:?PX4 home not set: start the simulator with ./px4.sh <world>}"
export PX4_HOME_LAT PX4_HOME_LON
export PX4_HOME_ALT="${PX4_HOME_ALT:-0}"

configure_px4() {
    local nav_dll_action="${BV_PX4_NAV_DLL_ACT:-0}"

    for _ in {1..300}; do
        if (
            cd /opt/px4/rootfs
            /opt/px4/bin/px4-commander status >/dev/null 2>&1 || exit 1
            /opt/px4/bin/px4-param set NAV_DLL_ACT "$nav_dll_action" >/dev/null 2>&1 || exit 1
            /opt/px4/bin/px4-param compare NAV_DLL_ACT "$nav_dll_action" >/dev/null 2>&1 || exit 1
            /opt/px4/bin/px4-param save >/dev/null 2>&1
        ); then
            return 0
        fi
        sleep 0.1
    done

    echo "Failed to configure PX4 simulation parameters" >&2
    return 1
}

# PX4's own links only reach programs inside this container. This one sends
# to the machine running Docker, on the API port MAVSDK listens on by default
# (udpin://0.0.0.0:14540), so a script there connects without speaking first
# and reconnects on every run.
start_host_link() {
    local host="${BV_PX4_HOST:-host.docker.internal}"
    local host_ip
    host_ip="$(getent hosts "$host" | awk '{ print $1; exit }')"
    if [ -z "$host_ip" ]; then
        echo "Cannot resolve $host; no MAVLink link to the host" >&2
        return 0
    fi
    (
        cd /opt/px4/rootfs
        /opt/px4/bin/px4-mavlink start -x -u 14590 -r 4000000 -f -m onboard \
            -o "${BV_PX4_HOST_PORT:-14540}" -t "$host_ip"
    )
}

gz sim --force-version 8 -s -r "$world" &
gazebo_pid=$!

bv_sim_bridge "$config" &
bridge_pid=$!

(
    cd /opt/px4/rootfs
    exec /opt/px4/bin/px4 -d
) &
px4_pid=$!

(configure_px4 && start_host_link) &

terminate() {
    kill "$px4_pid" "$gazebo_pid" "$bridge_pid" 2>/dev/null || true
    wait "$px4_pid" "$gazebo_pid" "$bridge_pid" 2>/dev/null || true
}

trap terminate EXIT INT TERM

set +e
wait -n "$px4_pid" "$gazebo_pid" "$bridge_pid"
status=$?
set -e

exit "$status"
