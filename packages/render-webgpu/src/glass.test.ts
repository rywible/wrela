import { expect, test } from "bun:test";
import { createSurfaceAppearance, type RenderSurface } from "@wrela/model";
import { batchSurfaces } from "./batching";
import { glassDrawDistance, isThinGlass } from "./glass";

test("glass sorts transformed material ranges independently and never merges instances", () => {
  const appearance = createSurfaceAppearance("glass");
  appearance.transmission = 0.9;
  const surface = {
    id: "glass",
    material: { appearance },
    matrix: new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 3, 1]),
    mesh: {
      positions: new Float32Array([0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 9, 1, 0, 9, 0, 1, 9]),
      indices: new Uint32Array([0, 1, 2, 3, 4, 5]),
    },
    drawRange: { start: 0, count: 3 },
  } as RenderSurface;
  const rear = { ...surface, id: "rear", drawRange: { start: 3, count: 3 } };
  expect(isThinGlass(surface)).toBe(true);
  expect(glassDrawDistance(rear, [0, 0, 0])).toBeGreaterThan(glassDrawDistance(surface, [0, 0, 0]));
  expect(glassDrawDistance(surface, [0, 0, 20])).toBeGreaterThan(glassDrawDistance(rear, [0, 0, 20]));
  expect(batchSurfaces([surface, { ...surface, id: "copy" }], () => 1).map((b) => b.surfaces.length)).toEqual(
    [1, 1],
  );
  appearance.transmission = 0;
  expect(isThinGlass(surface)).toBe(false);
});
