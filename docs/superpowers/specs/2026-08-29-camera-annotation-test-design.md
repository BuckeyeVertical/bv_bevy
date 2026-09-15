# Camera Annotation Test Design

## Goal

Provide one standalone Jetson script that verifies the real camera and LT-DETR
model before flight without starting ROS, MAVROS, PX4 communication, or mission
logic.

## Behavior

- Open `/dev/video0` through the mission's 4640x3480 at 8 FPS MJPEG GStreamer
  pipeline.
- Load `/home/bvorinnano/bv_ws/src/bv_core/ltdetr.pt` with Lightly Train.
- Run regular inference with a 0.5 threshold on 30 successfully captured frames.
- Draw every returned person or tent detection with its class and confidence.
- Save every processed frame beneath a timestamped directory in
  `~/camera_ws/annotated_frames` and then exit.
- Print per-frame detection counts and inference times.
- Fail clearly if the model, camera, or image output is unavailable.

## Structure

The utility is a single Python file in `tools/` so it can be copied directly to
the Jetson. Model loading stays inside `main`, while result conversion and image
annotation remain small functions that can be tested without camera hardware.

