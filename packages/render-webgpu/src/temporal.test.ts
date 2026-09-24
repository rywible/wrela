import { expect, test } from "bun:test";
import { perspective } from "./math";
import { jitterProjection, temporalJitter } from "./temporal";

test("temporal jitter shifts by subpixels at every depth while preserving depth and the source projection", () => {
  const projection = perspective(50, 16 / 9),
    original = projection.slice();
  const project = (matrix: Float32Array, z: number) =>
    [0, 1, 2, 3].map((r) => matrix[8 + r] * z + matrix[12 + r]);
  for (let frame = 0; frame < 16; frame++) {
    const jitter = temporalJitter(frame),
      matrix = jitterProjection(projection, frame, 1920, 1080);
    expect(jitter).toEqual(temporalJitter(frame % 8));
    expect(Math.max(...jitter.map(Math.abs))).toBeLessThan(0.5);
    for (const z of [-0.1, -1, -100, -2000]) {
      const a = project(projection, z),
        b = project(matrix, z);
      expect((b[0] / b[3] - a[0] / a[3]) * 960).toBeCloseTo(jitter[0], 6);
      expect((b[1] / b[3] - a[1] / a[3]) * -540).toBeCloseTo(jitter[1], 6);
      expect(b[2]).toBe(a[2]);
      expect(b[3]).toBe(a[3]);
    }
  }
  expect(projection).toEqual(original);
});
