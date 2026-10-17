"""Tests for tools/bvh.py on small written BVH files (no capture data).

    python3 -m unittest discover -s tools/tests -p test_bvh.py
"""

import math
import os
import sys
import tempfile
import unittest

TESTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(TESTS))

import bvh  # noqa: E402


def write(text):
    f = tempfile.NamedTemporaryFile("w", suffix=".bvh", delete=False)
    f.write(text)
    f.close()
    return f.name


ARM = """HIERARCHY
ROOT Hips
{
  OFFSET 0 0 0
  CHANNELS 6 Xposition Yposition Zposition Yrotation Xrotation Zrotation
  JOINT Arm
  {
    OFFSET 0 0 100
    CHANNELS 3 Yrotation Xrotation Zrotation
    End Site
    {
      OFFSET 0 0 10
    }
  }
}
MOTION
Frames: 1
Frame Time: 0.0166667
"""


class BvhTest(unittest.TestCase):
    def test_channels_turn_in_the_files_order_and_offsets_are_scaled(self):
        # The hips stand 100 cm up, turned 90° about y, then 90° about x (the first channel
        # outermost); the arm 100 cm along the hips' +z.
        path = write(ARM + "0 100 0 90 90 0 0 0 0\n")
        joints, frames, dt = bvh.parse(open(path).read())
        os.unlink(path)
        self.assertEqual([j.name for j in joints], ["Hips", "Arm"])
        self.assertEqual(joints[1].parent, 0)
        self.assertEqual(joints[1].end, (0.0, 0.0, 10.0))
        self.assertAlmostEqual(dt, 1 / 60, places=5)
        local, root = bvh.pose(joints, frames[0], 0.01)
        self.assertEqual(root, (0.0, 1.0, 0.0))
        rot, at = bvh.world(joints, local, root, 0.01)
        # Ry(90) Rx(90): Rx takes the arm's +z to -y, and Ry leaves -y, so the arm reaches
        # 1 m down from the hips, to the ground.
        for got, want in zip(at[1], (0.0, 0.0, 0.0)):
            self.assertAlmostEqual(got, want, places=6)
        # In the other order, Ry takes +z to +x first, and Rx leaves +x.
        q = bvh.q_mul(bvh.q_axis((1, 0, 0), 90), bvh.q_axis((0, 1, 0), 90))
        for got, want in zip(bvh.q_rotate(q, (0, 0, 1)), (1.0, 0.0, 0.0)):
            self.assertAlmostEqual(got, want, places=6)

    def test_contacts_are_the_first_frames_of_each_stance(self):
        # A toe that rests 20 frames, then swings 20 frames through the air, three times over.
        lines = []
        stances = []
        z = 0.0
        for c in range(3):
            stances.append(len(lines))
            for _ in range(20):
                lines.append(f"0 0 {z * 100:.3f} 0 0 0")
            for k in range(20):
                z += 0.05
                y = 0.2 * math.sin(math.pi * (k + 1) / 21)
                lines.append(f"0 {y * 100:.3f} {z * 100:.3f} 0 0 0")
        for _ in range(20):
            lines.append(f"0 0 {z * 100:.3f} 0 0 0")
        stances.append(len(lines) - 20)
        text = f"""HIERARCHY
ROOT LeftToe
{{
  OFFSET 0 0 0
  CHANNELS 6 Xposition Yposition Zposition Yrotation Xrotation Zrotation
  End Site
  {{
    OFFSET 0 0 5
  }}
}}
MOTION
Frames: {len(lines)}
Frame Time: 0.0166667
""" + "\n".join(lines) + "\n"
        path = write(text)
        clip = bvh.Clip(path)
        os.unlink(path)
        # The first stance starts the file, before 8 frames of swing: it isn't a contact.
        self.assertEqual(clip.contacts("LeftToe"), stances[1:])


if __name__ == "__main__":
    unittest.main()
