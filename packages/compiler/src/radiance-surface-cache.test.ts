import { expect, test } from "bun:test";
import { quad } from "../../../tools/fixtures/indirect-scenes";
import { compileRadianceLightingSteps } from "./radiance-lighting";

test("automatic surface transport admits a flat receiver while rejecting cells cut by a wall", () => {
  const floor = quad(
    "floor",
    [
      [-2, 0, -2],
      [-2, 0, 2],
      [2, 0, 2],
      [2, 0, -2],
    ],
    [0.6, 0.6, 0.6],
  );
  const partition = quad(
    "partition",
    [
      [0, -1, -3],
      [0, 3, -3],
      [0, 3, 3],
      [0, -1, 3],
    ],
    [0.4, 0.4, 0.4],
  );
  const steps = compileRadianceLightingSteps([floor, partition], {
    center: [0, 1, 0],
    key: "partition",
    maxSamples: 16,
    raysPerSample: 16,
  });
  let item = steps.next();
  while (!item.done) item = steps.next();
  const { field, meshes } = item.value,
    mesh = meshes.get("floor");
  expect(field.surfaceDiffuse?.positions.length).toBeGreaterThan(0);
  expect(field.surfaceDiffuse?.positions.length).toBeLessThanOrEqual(2048);
  if (!mesh?.radianceProbes) throw Error("Missing receiver");
  const ids = mesh.radianceProbes;
  expect(Array.from(ids).some((v, i) => i % 4 === 1 && v > 0)).toBe(true);
  let boundaryTriangles = 0;
  for (let t = 0; t < mesh.indices.length; t += 3) {
    const vertices = Array.from(mesh.indices.subarray(t, t + 3));
    const x = vertices.map((v) => mesh.positions[v * 3]);
    if (Math.min(...x) <= 0 && Math.max(...x) >= 0) {
      boundaryTriangles++;
      for (const v of vertices) expect(ids[v * 4 + 1]).toBe(0);
    }
  }
  expect(boundaryTriangles).toBeGreaterThan(0);
});
