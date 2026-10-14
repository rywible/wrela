"""Small semantic checks for the compiler range-analysis experiment."""

from fractions import Fraction
import itertools
import os
import random
import sys
import unittest

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)

import compiler_ranges as ranges  # noqa: E402


class CompilerRangesTest(unittest.TestCase):
    def test_enclosures_hold_rounded_values_at_corners_and_seeded_points(self):
        rng = random.Random(90210)
        lo = (Fraction(-2), Fraction(-1), Fraction(0))
        hi = (Fraction(1), Fraction(2), Fraction(3))
        points = list(itertools.product(*zip(lo, hi)))
        points.extend(
            tuple(l + (h - l) * Fraction(rng.randrange(257), 256)
                  for l, h in zip(lo, hi))
            for _ in range(16)
        )
        saw_rounding = False
        for _ in range(16):
            branch = (
                tuple(Fraction(rng.randrange(-1000000, 1000001), 1024) for _ in range(3)),
                Fraction(rng.randrange(-1000000, 1000001), 1024),
            )
            lower, upper = ranges.interval(*branch, lo, hi)
            error = ranges.rounding_error(*branch, lo, hi)
            for point in points:
                actual = Fraction(ranges.value(branch, point))
                exact = sum(a * x for a, x in zip(branch[0], point)) + branch[1]
                saw_rounding |= actual != exact
                self.assertLessEqual(abs(actual - exact), error)
                self.assertLessEqual(lower - error, actual)
                self.assertLessEqual(actual, upper + error)
        self.assertTrue(saw_rounding, "fixture must exercise actual float32 rounding")

    def test_correlation_proves_dominance_despite_overlapping_ranges(self):
        lo, hi = (Fraction(-1),) * 3, (Fraction(1),) * 3
        branches = [
            ((Fraction(16), Fraction(16), Fraction(16)), Fraction(0)),
            ((Fraction(16) + Fraction(1, 64), Fraction(16), Fraction(16)), Fraction(1)),
        ]
        independent, live, _ = ranges.live_branches(branches, lo, hi)
        self.assertEqual(independent, [0, 1])
        self.assertEqual(live, [0])
        report = ranges.probe("small", branches, [(lo, hi)], random_samples=8)
        self.assertEqual(report["point_checks"], 17)
        self.assertEqual(report["bitwise_mismatches"], 0)

    def test_equal_and_touching_branches_are_retained(self):
        lo, hi = (Fraction(0),) * 3, (Fraction(1),) * 3
        zero = ((Fraction(0),) * 3, Fraction(0))
        touching = ((Fraction(1), Fraction(0), Fraction(0)), Fraction(0))
        self.assertEqual(ranges.live_branches([zero, zero], lo, hi)[1], [0, 1])
        self.assertEqual(ranges.live_branches([zero, touching], lo, hi)[1], [0, 1])

    def test_opposing_planes_cannot_be_pruned_over_the_crossing(self):
        lo, hi = (Fraction(-1),) * 3, (Fraction(1),) * 3
        branches = []
        for axis in range(3):
            for sign in (-1, 1):
                coefficients = tuple(Fraction(sign if i == axis else 0) for i in range(3))
                branches.append((coefficients, Fraction(0)))
        independent, live, _ = ranges.live_branches(branches, lo, hi)
        self.assertEqual(independent, list(range(6)))
        self.assertEqual(live, independent)
        # Every branch is uniquely smallest at some point: keeping each is
        # required by the value, not just a limitation of this proof method.
        for i, (coefficients, _) in enumerate(branches):
            point = tuple(-x for x in coefficients)
            values = [ranges.value(branch, point) for branch in branches]
            self.assertLess(values[i], min(v for j, v in enumerate(values) if j != i))

    def test_rejects_constants_outside_the_float32_model(self):
        lo, hi = (Fraction(0),) * 3, (Fraction(1),) * 3
        for bad in (Fraction(1, 3), Fraction(2**17)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                ranges.live_branches([((bad, Fraction(0), Fraction(0)), Fraction(0))], lo, hi)
