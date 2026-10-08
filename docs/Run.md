# Run a World

Each world takes two terminals from the `bv_bevy` directory: PX4 + Gazebo (the drone)
and Bevy (the world and the drone's camera). Start PX4 in terminal 1 first, then Bevy in terminal 2.

In the Bevy window the camera follows the drone. Press `F` for a free camera;
left click enables mouse look, and `W/A/S/D`, `E/Q` and Shift move it.

## suas_2026

The SUAS 2026 competition field: boundaries, lap route, targets and trees.

Terminal 1:

```bash
./px4.sh suas_2026
```

Terminal 2:

```bash
cargo run -- suas_2026
```

## forest

A forest clearing with a launch pad, road, shed and power line.

Terminal 1:

```bash
./px4.sh forest
```

Terminal 2:

```bash
cargo run -- forest
```

- `--quality low|medium|high` trades detail for frame rate (default `medium`).
- `--no-shadows` turns sun shadows off.

## satellite_map

Real satellite imagery and terrain, streamed from the internet and cached in
`.cache/`.

Terminal 1:

```bash
./px4.sh satellite_map
```

Terminal 2:

```bash
cargo run -- satellite_map
```

- `--lat <deg> --lon <deg>` centres the map (default Tuttle Park, Columbus).
  Pass the same values to `./px4.sh` so PX4's home matches.

## Other options

`./px4.sh --prebuilt <world>` uses the published simulator image instead of a
local build. `cargo run -- <world> --screenshot <png>` (and `forest --tour
<dir>`) render without a drone and exit, for checking the world itself.
