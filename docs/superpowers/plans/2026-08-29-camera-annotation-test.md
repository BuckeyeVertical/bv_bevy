# Camera Annotation Test Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone preflight utility that saves 30 LT-DETR-annotated real-camera frames.

**Architecture:** A single Python script owns camera capture, Lightly model inference, annotation, and output. Pure conversion and annotation helpers are tested independently of Jetson hardware.

**Tech Stack:** Python 3, OpenCV with GStreamer, Pillow, NumPy, Lightly Train

---

### Task 1: Test annotation behavior

**Files:**
- Create: `tools/test_capture_annotated_frames.py`
- Create: `tools/capture_annotated_frames.py`

- [ ] **Step 1: Write a failing test**

Create a synthetic image and model result containing person and tent boxes.
Assert that conversion returns both detections and annotation changes pixels.

- [ ] **Step 2: Verify the test fails**

Run: `python3 -m unittest tools/test_capture_annotated_frames.py -v`

Expected: import failure because `capture_annotated_frames.py` does not exist.

- [ ] **Step 3: Implement the helpers and command-line program**

Implement tensor-to-NumPy conversion, result parsing, box annotation, the exact
GStreamer camera pipeline, 30-frame capture loop, timestamped output folder,
regular Lightly inference, progress output, and cleanup.

- [ ] **Step 4: Verify the test and syntax**

Run: `python3 -m unittest tools/test_capture_annotated_frames.py -v`

Expected: two tests pass.

Run: `python3 -m py_compile tools/capture_annotated_frames.py`

Expected: exit status 0.

