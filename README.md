# Buckeye Vertical Simulation

Gazebo and PX4 fly the drone. Bevy draws the world and renders the drone's
camera, streaming it to whatever consumes it (a vision pipeline, a recorder).

```text
PX4 <-> Gazebo --state :7001--> Bevy --camera :7002--> your consumer
  ^
  MAVLink (UDP 14580, 18570) from your flight stack / ground station
```

Gazebo and PX4 run headless in the `bv-simulator` Docker container; Bevy runs
natively.

## Setup

Install Docker Desktop or OrbStack, and Rust.

Build the Gazebo and PX4 image (the first build takes several minutes), then
Bevy:

```bash
cd ~/Code/bv_bevy
docker compose -f gazebo/compose.px4.yaml build
cargo build
```

To skip the Docker build, use the published image instead: run with
`./px4.sh --prebuilt <world>` (see [docs/Run.md](docs/Run.md)).

## Run the simulation

See [docs/Run.md](docs/Run.md).
