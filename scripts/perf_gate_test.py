#!/usr/bin/env python3
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from scripts import perf_gate


class PerfGateTest(unittest.TestCase):
    def test_parse_perf_line(self):
        out = "\n".join(
            [
                "ok foo",
                "perf: p50_ns=10 p99_ns=99 allocs/request=1.25",
                "tests: 1 passed, 0 failed",
            ]
        )
        parsed = perf_gate.parse_perf_line(out)
        self.assertEqual(parsed["p50_ns"], 10)
        self.assertEqual(parsed["p99_ns"], 99)
        self.assertAlmostEqual(parsed["allocs_per_request"], 1.25)

    def test_parse_perf_line_missing(self):
        self.assertIsNone(perf_gate.parse_perf_line("no perf here"))

    def test_calc_limit(self):
        self.assertEqual(perf_gate.calc_limit(100, 0.05), 105)
        self.assertEqual(perf_gate.calc_limit(1000, 0.10), 1100)


if __name__ == "__main__":
    unittest.main()
