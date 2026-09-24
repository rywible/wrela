import { expect, test } from "bun:test";
import type { Vec3 } from "@wrela/model";
import { quad } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectGeometry } from "./indirect-query";
import {
  compileSurfaceLatticeSteps,
  evaluateSurfaceLattice,
  surfaceBarycentric,
  surfaceLatticeBinding,
  surfaceLatticeIndex,
} from "./radiance-surface-lattice";

test("surface lattice interpolation preserves affine coordinates at interiors and all edges", () => {
  for (const n of [1, 2, 3, 8, 16]) {
    const nodes: Vec3[] = [];
    for (let y = 0; y <= n; y++)
      for (let x = 0; x <= n - y; x++) nodes[surfaceLatticeIndex(n, x, y)] = [1 - (x + y) / n, x / n, y / n];
    for (const barycentric of [
      ...nodes,
      ...Array.from({ length: 80 }, (_, i) => {
        const u = (i * 0.61803398875) % 1,
          v = ((i * 0.41421356237) % 1) * (1 - u);
        return [1 - u - v, u, v] as Vec3;
      }),
    ]) {
      const binding = surfaceLatticeBinding(n, barycentric);
      expect(binding.cell).toBeGreaterThanOrEqual(0);
      expect(binding.cell).toBeLessThan(n * n);
      expect(binding.weights.reduce((sum, w) => sum + w)).toBeCloseTo(1, 12);
      const interpolated = [0, 0, 0];
      binding.ids.forEach((id, i) => {
        expect(id).toBeGreaterThanOrEqual(0);
        expect(id).toBeLessThan(nodes.length);
        expect(binding.weights[i]).toBeGreaterThanOrEqual(-1e-12);
        for (let a = 0; a < 3; a++) interpolated[a] += nodes[id][a] * binding.weights[i];
      });
      for (let a = 0; a < 3; a++) expect(interpolated[a]).toBeCloseTo(barycentric[a], 12);
    }
  }
  expect(() => surfaceLatticeBinding(0, [1, 0, 0])).toThrow();
  expect(() => surfaceLatticeBinding(4, [-0.2, 0.6, 0.6])).toThrow();
});

test("surface lattices separate direct emission and retain the correct source plane", () => {
  const floor = quad(
    "floor",
    [
      [-2, 0, -2],
      [2, 0, -2],
      [2, 0, 2],
      [-2, 0, 2],
    ],
    [0, 0, 0],
  );
  const emitter = quad(
    "emitter",
    [
      [-1, 1, -1],
      [1, 1, -1],
      [1, 1, 1],
      [-1, 1, 1],
    ],
    [0, 0, 0],
  );
  emitter.material.emission = { color: [1, 0.4, 0.1], intensity: 1 };
  const geometry = compileIndirectGeometry([floor, emitter]);
  const steps = compileSurfaceLatticeSteps(geometry, 0, [0, 1, 0], [], { spacing: 3, samples: 256 });
  let item = steps.next();
  while (!item.done) item = steps.next();
  const patch = item.value;
  const barycentric = surfaceBarycentric(geometry, 0, [0, 0, 0]);
  expect(barycentric).toEqual([0.5, 0, 0.5]);
  const value = evaluateSurfaceLattice(patch, barycentric);
  if (!value) throw Error("Unobstructed surface cell was rejected");
  const side = 1 / Math.sqrt(2);
  expect(value.directEmission[0]).toBeCloseTo((4 / Math.PI) * side * Math.atan(side), 2);
  for (let c = 0; c < 3; c++) expect(value.transfer[104 + c]).toBeCloseTo(value.directEmission[c], 7);
});

test("surface lattice rejects a partition intersecting its source triangle", () => {
  const floor = quad(
    "floor",
    [
      [-2, 0, -2],
      [2, 0, -2],
      [2, 0, 2],
      [-2, 0, 2],
    ],
    [0, 0, 0],
  );
  const wall = quad(
    "wall",
    [
      [0, -1, -3],
      [0, 3, -3],
      [0, 3, 3],
      [0, -1, 3],
    ],
    [0, 0, 0],
  );
  const geometry = compileIndirectGeometry([floor, wall]);
  const steps = compileSurfaceLatticeSteps(geometry, 0, [0, 1, 0], [], { spacing: 10, samples: 16 });
  let item = steps.next();
  while (!item.done) item = steps.next();
  expect(Array.from(item.value.validCells)).toEqual([0]);
  expect(item.value.rays).toBe(0);
  expect(evaluateSurfaceLattice(item.value, [0.5, 0.25, 0.25])).toBeUndefined();
});
