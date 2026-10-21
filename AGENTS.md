Keep this file general and useful: only what agents need to write, test and debug code in this repo, not one-off details, history or anything already clear from the code.

# Layout

- `src/worlds/`: one module per world; shared pieces in `worlds/shared.rs`.
- `src/camera/`: the drone camera and its frame server.
- `src/sim/`: receives Gazebo's state.
- `gazebo/`: the PX4 + Gazebo image.
- `tools/`: Python stream tools and asset converters.

Gazebo and PX4 use x east, y north, z up; Bevy uses x = -north, y up, z = -east. Convert
only through `src/sim/coordinates.rs`.

# Running the simulator

Run from the repo root, each as a background process, PX4 first:

1. PX4 + Gazebo: `./px4.sh --prebuilt <world>`. Without `--prebuilt` it uses an image
   built locally with `docker compose -f gazebo/compose.px4.yaml build`, which is only
   needed to test changes under `gazebo/`.
2. Bevy: `cargo run -- <world>`, with the same world.

`cargo run -- --help` lists the worlds and their options. `cargo run -- <world>
--screenshot <png>` renders a world without PX4 and exits.

Check the streams with `python3 tools/verify_sim_stream.py` and
`python3 tools/verify_camera_stream.py --output frame.jpg`. Inspect PX4 with
`docker logs -f bv-simulator` or `docker exec bv-simulator bash -c "<command>"`. Stop
with `docker compose -f gazebo/compose.px4.prebuilt.yaml down` (or `compose.px4.yaml`
for a local image).

Test: `cargo test && python3 -m unittest discover -s tools`.

# Ports

- `7001` TCP: simulation state, from the Gazebo bridge to Bevy (`docs/sim_state_v1.md`).
- `7002` TCP: camera frames from Bevy (`docs/camera_frame_v1.md`). Bevy sends none until
  PX4 and Gazebo are running.
- `14540` UDP: PX4 sends MAVLink to the host here (`gazebo/px4/run.sh`); MAVSDK listens
  with `udpin://0.0.0.0:14540`.
- `18570` UDP: PX4's ground-station link, for QGroundControl. PX4 replies to the first
  address that talks to it, so a client must always use the same local port.
- `14580` UDP: PX4's onboard link, for one long-lived client such as MAVROS.
