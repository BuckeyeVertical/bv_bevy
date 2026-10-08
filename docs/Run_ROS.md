# Run the Full Mission

Fly the mission stack (PX4, Gazebo, MAVROS, ROS) through a Bevy world. Use four
terminals, in order. The example flies the SUAS 2026 field; for another world,
use its name in steps 1 and 2 (see [Run.md](Run.md)).

Open each terminal in the directory that holds `bv_bevy` and `bv_ws` (see the
[README](../README.md#setup)); the `cd` paths below are relative to it.

## 1. Gazebo and PX4

```bash
cd bv_bevy
./px4.sh suas_2026
# or, with the prebuilt image instead of a local docker build:
./px4.sh --prebuilt suas_2026
```

`px4.sh` gives PX4 the home position of the world you name.

## 2. Bevy

```bash
cd bv_bevy
cargo run -- suas_2026
```

The window follows the drone. Press `F` for the free camera; left click enables
mouse look, and `W/A/S/D`, `E/Q` and Shift move it. Overlays such as the flight
boundary and lap route show here but not in the onboard camera.

## 3. MAVROS

```bash
docker start bv-mission
docker exec -it bv-mission bash
ros2 launch mavros px4.launch \
  fcu_url:=udp://:14540@host.docker.internal:14580
```

## 4. Mission

```bash
docker exec -it bv-mission bash
export BV_MISSION_CONFIG=sim_params.yaml
ros2 launch bv_core mission.launch.py
```

`sim_params.yaml` flies the SUAS 2026 field; `proving_ground_params.yaml` goes
with `forest`.

Open the ground station at <http://localhost:8765>.

## Restart and stop

```bash
docker restart bv-simulator
docker logs -f bv-simulator
```

Stop with `Ctrl-C` in the four terminals, then `docker stop bv-mission`.

Camera and state protocols: [camera_frame_v1.md](camera_frame_v1.md),
[sim_state_v1.md](sim_state_v1.md).
