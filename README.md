# Buckeye Vertical Simulation

Gazebo and PX4 provide physics and vehicle state. Bevy renders the world and
camera. The ROS mission stack consumes the Bevy camera stream.

```text
PX4 <-> Gazebo --state :7001--> Bevy --camera :7002--> Vision
                                                        |
                                                     GCS :8765
```

The two Docker containers are `bv-simulator` for Gazebo/PX4 and `bv-mission`
for ROS, MAVROS, vision, and the GCS.

## Setup

Install Docker Desktop or OrbStack, Rust, and Node.js 20 or newer.

Keep the repositories in this layout:

```text
Code/
├── bv_bevy/
└── bv_ws/
    ├── ltdetr.pt
    └── src/
        ├── bv_core/
        ├── bv_msgs/
        └── bv_gcs/
```

Build the GCS webpage once:

```bash
cd ~/Code/bv_ws/src/bv_gcs/web
npm ci
npm run build
```

Build the ROS, ML, and GCS image:

```bash
cd ~/Code/bv_ws/src
docker build \
  -f bv_core/container/Dockerfile.arm_no_PX4 \
  -t bv-mission:latest \
  bv_core
```

Create the persistent mission container from the `bv_ws` directory:

```bash
cd ~/Code/bv_ws
docker run -d \
  --name bv-mission \
  --privileged \
  -v "$PWD:/bv_ws" \
  -p 8765:8765 \
  bv-mission:latest \
  sleep infinity
```

Build the ROS workspace inside it:

```bash
docker exec -it bv-mission bash
cd /bv_ws
colcon build
exit
```

Build the headless Gazebo and PX4 image. This first build takes several minutes.

```bash
cd ~/Code/bv_bevy
docker compose -f gazebo/compose.px4.yaml build
cargo build
```

## Run the simulation

Use four terminals. Start Gazebo and PX4 first:

```bash
cd ~/Code/bv_bevy
docker compose -f gazebo/compose.px4.yaml up
```

Start native Bevy:

```bash
cd ~/Code/bv_bevy
./run_suas.sh
```

This selects the 600 m × 500 m `SUAS` field. Use `./run_mission_test.sh` for
the earlier minimal mission-test world.

The window follows the drone by default. Press `F` for the free camera; left
click enables mouse look and `W/A/S/D`, `E/Q`, and Shift move it.

Start the mission container and MAVROS:

```bash
docker start bv-mission
docker exec -it bv-mission bash
ros2 launch mavros px4.launch \
  fcu_url:=udp://:14540@host.docker.internal:14580
```

Launch the mission in the fourth terminal:

```bash
docker exec -it bv-mission bash
ros2 launch bv_core mission.launch.py
```

Open the ground station at [http://localhost:8765](http://localhost:8765).

If you need to restart the sim

```bash
docker restart bv-simulator
docker logs -f bv-simulator
```

Stop the stack with `Ctrl-C` in the four running terminals, then stop the
mission container:

```bash
docker stop bv-mission
```

Camera and state protocol details are in
[`docs/camera_frame_v1.md`](docs/camera_frame_v1.md) and
[`docs/sim_state_v1.md`](docs/sim_state_v1.md).
