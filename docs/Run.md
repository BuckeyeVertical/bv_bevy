# Run the Mission

Run these commands in four terminals, in order.

## 1. Gazebo and PX4

```bash
cd ~/Code/bv_bevy
docker compose -f gazebo/compose.px4.yaml up
```

## 2. Bevy

```bash
cd ~/Code/bv_bevy
./run_suas.sh
```

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

Open <http://localhost:8765> in a browser on the host machine.
