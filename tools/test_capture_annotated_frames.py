import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))

from capture_annotated_frames import annotate_frame, parse_detections


class CaptureAnnotatedFramesTest(unittest.TestCase):
    def test_parse_detections_keeps_every_result(self):
        results = {
            "labels": np.array([0, 1]),
            "bboxes": np.array([[10, 20, 50, 80], [60, 30, 100, 90]]),
            "scores": np.array([0.91, 0.74]),
        }

        detections = parse_detections(results)

        self.assertEqual(len(detections), 2)
        self.assertEqual(detections[0][0], "person")
        self.assertEqual(detections[1][0], "tent")

    def test_annotate_frame_draws_each_detection(self):
        frame = np.zeros((120, 140, 3), dtype=np.uint8)
        detections = [
            ("person", 0.91, (10, 20, 50, 80)),
            ("tent", 0.74, (60, 30, 100, 90)),
        ]

        annotated = annotate_frame(frame, detections)

        self.assertGreater(np.count_nonzero(annotated), 0)
        self.assertTrue(np.array_equal(frame, np.zeros_like(frame)))


if __name__ == "__main__":
    unittest.main()
