# Buckeye Vertical Simulation

PX4 and Gazebo fly the drone; Bevy renders the world and the drone's camera.

## Setup

Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) (or
[OrbStack](https://orbstack.dev) on a Mac) and [Rust](https://rustup.rs). On Windows, run
the commands below in WSL.

```bash
git clone https://github.com/BuckeyeVertical/bv_bevy.git
cd bv_bevy
cargo build --release
```

## Run

Two terminals in `bv_bevy`, PX4 first:

```bash
./px4.sh --prebuilt forest
```

```bash
cargo run --release -- forest
```


In the Bevy window, press `F` for a free camera, left-click for mouse look, and move with
`W/A/S/D`, `E/Q` and Shift. World options are in [docs/Run.md](docs/Run.md).

Stop with Ctrl+C in both terminals.

## Local Build

```bash
docker compose -f gazebo/compose.px4.yaml build
./px4.sh forest
```
