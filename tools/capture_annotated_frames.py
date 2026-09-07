#!/usr/bin/env python3
"""Run LT-DETR on 30 real-camera frames and save annotated images."""

from datetime import datetime
from pathlib import Path
import time

import cv2
import numpy as np
from PIL import Image


MODEL_PATH = Path("/home/bvorinnano/bv_ws/src/bv_core/ltdetr.pt")
OUTPUT_ROOT = Path.home() / "camera_ws" / "annotated_frames"
FRAME_COUNT = 30
THRESHOLD = 0.5
CLASS_NAMES = ("person", "tent")
GST_PIPELINE = (
    "v4l2src device=/dev/video0 ! "
    "image/jpeg,width=4640,height=3480,framerate=8/1 ! "
    "jpegdec ! videoconvert ! "
    "appsink drop=true sync=false"
)


def to_numpy(value, dtype) -> np.ndarray:
    if value is None:
        return np.array([], dtype=dtype)
    if hasattr(value, "detach"):
        value = value.detach()
    if hasattr(value, "cpu"):
        value = value.cpu()
    if hasattr(value, "numpy"):
        value = value.numpy()
    return np.asarray(value, dtype=dtype)


def parse_detections(results: dict) -> list[tuple[str, float, tuple[int, int, int, int]]]:
    labels = to_numpy(results.get("labels"), np.int32).reshape(-1)
    boxes = to_numpy(results.get("bboxes"), np.float32).reshape((-1, 4))
    scores = to_numpy(results.get("scores"), np.float32).reshape(-1)

    detections = []
    for label, score, box in zip(labels, scores, boxes):
        class_id = int(label)
        class_name = (
            CLASS_NAMES[class_id]
            if 0 <= class_id < len(CLASS_NAMES)
            else f"class_{class_id}"
        )
        detections.append(
            (class_name, float(score), tuple(int(round(value)) for value in box))
        )
    return detections


def annotate_frame(
    frame: np.ndarray,
    detections: list[tuple[str, float, tuple[int, int, int, int]]],
) -> np.ndarray:
    annotated = frame.copy()
    height, width = annotated.shape[:2]
    thickness = max(2, round(width / 1600))
    font_scale = max(0.7, width / 3200)

    for class_name, confidence, (x1, y1, x2, y2) in detections:
        x1 = min(max(x1, 0), width - 1)
        x2 = min(max(x2, 0), width - 1)
        y1 = min(max(y1, 0), height - 1)
        y2 = min(max(y2, 0), height - 1)
        color = (40, 220, 40) if class_name == "person" else (0, 180, 255)
        label = f"{class_name} {confidence:.2f}"

        cv2.rectangle(annotated, (x1, y1), (x2, y2), color, thickness)
        text_size, baseline = cv2.getTextSize(
            label, cv2.FONT_HERSHEY_SIMPLEX, font_scale, thickness
        )
        text_y = max(y1, text_size[1] + baseline + 4)
        cv2.rectangle(
            annotated,
            (x1, text_y - text_size[1] - baseline - 4),
            (x1 + text_size[0] + 6, text_y + 2),
            color,
            cv2.FILLED,
        )
        cv2.putText(
            annotated,
            label,
            (x1 + 3, text_y - baseline),
            cv2.FONT_HERSHEY_SIMPLEX,
            font_scale,
            (0, 0, 0),
            thickness,
            cv2.LINE_AA,
        )

    return annotated


def main() -> None:
    if not MODEL_PATH.is_file():
        raise SystemExit(f"Model not found: {MODEL_PATH}")

    import lightly_train

    run_directory = OUTPUT_ROOT / datetime.now().strftime("%Y%m%d-%H%M%S")
    run_directory.mkdir(parents=True, exist_ok=False)

    print(f"Loading model: {MODEL_PATH}")
    model = lightly_train.load_model(str(MODEL_PATH))

    camera = cv2.VideoCapture(GST_PIPELINE, cv2.CAP_GSTREAMER)
    if not camera.isOpened():
        raise SystemExit("Could not open /dev/video0 with the GStreamer pipeline")

    print(f"Saving {FRAME_COUNT} frames to: {run_directory}")
    try:
        for frame_number in range(1, FRAME_COUNT + 1):
            captured, frame = camera.read()
            if not captured or frame is None:
                raise RuntimeError(f"Camera read failed at frame {frame_number}")

            started = time.perf_counter()
            image = Image.fromarray(cv2.cvtColor(frame, cv2.COLOR_BGR2RGB))
            results = model.predict(image, threshold=THRESHOLD)
            inference_seconds = time.perf_counter() - started

            detections = parse_detections(results)
            annotated = annotate_frame(frame, detections)
            output_path = run_directory / f"frame_{frame_number:03d}.jpg"
            if not cv2.imwrite(str(output_path), annotated):
                raise RuntimeError(f"Could not write {output_path}")

            print(
                f"[{frame_number:02d}/{FRAME_COUNT}] "
                f"{len(detections)} detection(s), {inference_seconds:.2f}s"
            )
    finally:
        camera.release()

    print(f"Finished. Annotated frames: {run_directory}")


if __name__ == "__main__":
    main()
