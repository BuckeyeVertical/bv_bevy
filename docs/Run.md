# Run a World

Each world takes two terminals from `~/Code/bv_bevy`: PX4 + Gazebo (the drone)
and Bevy (the world and the drone's camera). Start PX4 first. When the drone
appears in the Bevy window it is ready; start MAVROS and the mission with
[Run_ROS.md](Run_ROS.md) steps 3 and 4, using the mission config listed.

## suas_2026

The SUAS 2026 competition field: boundaries, lap route, targets and trees.
Mission config: `sim_params.yaml`.

```bash
./px4.sh suas_2026
cargo run -- suas_2026
```

## forest

A forest clearing with a launch pad, road, shed and power line.
Mission config: `proving_ground_params.yaml`.

```bash
./px4.sh forest
cargo run -- forest
```

- `--quality low|medium|high` trades detail for frame rate (default `medium`).
- `--no-shadows` turns sun shadows off.

## satellite_map

Real satellite imagery and terrain, streamed from the internet and cached in
`.cache/`. There is no mission config for it yet.

```bash
./px4.sh satellite_map
cargo run -- satellite_map
```

- `--lat <deg> --lon <deg>` centres the map (default Tuttle Park, Columbus).
  Pass the same values to `./px4.sh` so PX4's home matches.

## Other options

`./px4.sh --prebuilt <world>` uses the published simulator image instead of a
local build. `cargo run -- <world> --screenshot <png>` (and `forest --tour
<dir>`) render without a drone and exit, for checking the world itself.
