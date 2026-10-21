# Run a World

Each world takes two terminals from the `bv_bevy` directory: PX4 + Gazebo (the drone)
and Bevy (the world and the drone's camera). Start PX4 in terminal 1 first, then Bevy in terminal 2.

`--prebuilt` runs the team's published simulator image, downloading it the first time and
whenever it changes. To run an image built from your own `gazebo/` changes instead, build it
with `docker compose -f gazebo/compose.px4.yaml build` and leave out `--prebuilt`.

In the Bevy window the camera follows the drone. Press `F` for a free camera;
left click enables mouse look, and `W/A/S/D`, `E/Q` and Shift move it.

## suas_2026

The SUAS 2026 competition field: boundaries, lap route, targets and trees.

Terminal 1:

```bash
./px4.sh --prebuilt suas_2026
```

Terminal 2:

```bash
cargo run -- suas_2026
```

## forest

A forest clearing with a launch pad, road, shed and power line.

Terminal 1:

```bash
./px4.sh --prebuilt forest
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
./px4.sh --prebuilt satellite_map
```

Terminal 2:

```bash
cargo run -- satellite_map
```

- `--lat <deg> --lon <deg>` centres the map (default Tuttle Park, Columbus).
  Pass the same values to `./px4.sh` so PX4's home matches.

## Connecting

PX4 sends MAVLink to this machine on UDP 14540, so MAVSDK connects with
`udpin://0.0.0.0:14540`, and can reconnect on every run without restarting the simulator.
QGroundControl uses PX4's ground-station link on UDP 18570. Bevy serves the drone camera
on TCP 7002.

## Other options

`cargo run -- <world> --screenshot <png>` (and `forest --tour <dir>`) render without a
drone and exit, for checking the world itself. `cargo run -- --help` lists every world and
its options.

Stop with Ctrl+C in both terminals.
