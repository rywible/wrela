"""Independent invariants for the research kernels; run directly with Python."""
import math
import unittest
import numpy as np
from appearance import (unit, normal_aggregate, nearest_plate, trig_roots,
                        compile_branch_integrals, integrated_branch, weather_probe)


class AppearanceTests(unittest.TestCase):
    def test_weather_conserves_mass_and_responds_to_temperature(self):
        cold = weather_probe(-5, False)
        warm = weather_probe(10, False)
        for result in [cold, warm]:
            self.assertLess(result["conservationAbsolute"], 1e-10)
            self.assertGreaterEqual(result["minimumState"], 0)
            self.assertEqual(result["refreezeError"], 0)
        self.assertGreater(cold["finalSnow"], warm["finalSnow"])
        self.assertEqual(cold["finalIce"], 0)
        self.assertGreater(warm["finalIce"], 0)

    def test_aggregate_preserves_area_and_material_mass(self):
        normals = unit(np.random.default_rng(2).normal(size=(512, 3)))
        normals[:, 1] = np.abs(normals[:, 1])
        material = (normals[:, 1] > .7).astype(int)
        _, groups, weights = normal_aggregate(normals, material, 4)
        self.assertAlmostEqual(weights.sum(), 1)
        self.assertAlmostEqual(weights[groups == 1].sum(), material.mean())

    def test_ray_rectangle_hits_depth_and_misses_edges(self):
        geometry = (np.array([[0., 0, 0]]), np.array([[1., 0, 0]]),
                    np.array([[0., 0, 1]]), np.array([[0., 1, 0]]),
                    np.array([2.]), np.array([2.]), np.array([0.]))
        ids = nearest_plate(np.array([[0., 1, 0], [2., 1, 0], [0., -1, 0]]), np.array([0., -1, 0]), geometry)
        np.testing.assert_array_equal(ids, [0, -1, -1])

    def test_event_roots_satisfy_source_equation(self):
        for a, b, c in np.random.default_rng(45).normal(size=(100, 3)):
            for root in trig_roots((a, b, c)):
                self.assertLess(abs(a*math.cos(root)+b*math.sin(root)+c), 1e-12)

    def test_piecewise_integral_preserves_full_period_and_wrapping(self):
        geometry = (np.array([[0., 0, 0]]), np.array([[0., 1, 0]]),
                    np.array([[0., 0, 1]]), np.array([[1., 0, 0]]),
                    np.array([2.]), np.array([2.]), np.array([0.]))
        tables = compile_branch_integrals(geometry, [np.empty((0, 2))])
        expected = .08/math.pi/math.sqrt(1+.55**2)*2.3/(2*math.pi)
        actual = integrated_branch(tables, np.array([0., .7, 6., -5., 14.]), 2*math.pi)
        np.testing.assert_allclose(actual, expected, rtol=1e-12)
        a = integrated_branch(tables, np.array([.4, .4+2*math.pi]), .2)
        np.testing.assert_allclose(a[0], a[1], rtol=1e-12)


if __name__ == "__main__":
    unittest.main()
