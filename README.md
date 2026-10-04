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

Move the drone stl into bv_core/meshes/ via
https://buckeyemailosu-my.sharepoint.com/my?id=%2Fpersonal%2Fclute%5F25%5Fosu%5Fedu%2FDocuments%2FRender%5FCAD%2ESTL&parent=%2Fpersonal%2Fclute%5F25%5Fosu%5Fedu%2FDocuments&ga=1
make sure file is named Render_CAD.STL

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
# or alternatively skip long docker build step 
# by pulling prebuilt image (see below)
cargo build
```

## Run the simulation

See [docs/Run.md](docs/Run.md).
