# Run a World

Bevy draws the world and the drone's camera. Pick a world and run it from
`~/Code/bv_bevy`. To fly a mission in it, see [Run_ROS.md](Run_ROS.md).

Every world also takes `--screenshot <png>`: save one frame once it has
loaded, then exit.

## minimal

Flat grass with a grid, a landing pad and three obstacles.

```bash
cargo run -- minimal
```

## gazebo_boxes

Coloured boxes where the objects of the PX4 Gazebo world stand, for checking
that Bevy and Gazebo line up.

```bash
cargo run -- gazebo_boxes
```

## grass_targets

Grass with a mannequin and a tent along a scan line: a quick vision test.

```bash
cargo run -- grass_targets
```

## suas_2026

The SUAS 2026 competition field: boundaries, lap route, targets and trees.

```bash
cargo run -- suas_2026
```

## forest

A forest clearing with a launch pad, road, shed and power line.

```bash
cargo run -- forest
```

- `--quality low|medium|high` trades detail for frame rate (default `medium`).
- `--no-shadows` turns sun shadows off.
- `--tour <dir>` screenshots fixed viewpoints with their frame times, then exits.

## satellite_map

Real satellite imagery and terrain, streamed from the internet and cached in
`.cache/`.

```bash
cargo run -- satellite_map
```

- `--lat <deg> --lon <deg>` centres the map (default Tuttle Park, Columbus).
