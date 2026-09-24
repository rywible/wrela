import { expect, test } from "bun:test";
import { creatureChartSchema } from "./creature";
import { creaturePatchPoint } from "./creature-patch";
import type { Vec3 } from "./math";

const points: [Vec3, Vec3, Vec3, Vec3] = [
  [-1, 2, 0],
  [1, 2, 0],
  [-1.2, 0, 0.1],
  [1.2, 0, 0.1],
];
const zero: Vec3 = [0, 0, 0];
const controlOffsets: Vec3[][] = [
  [zero, [0, 0, -0.2], zero],
  [zero, [0, 0, -0.4], zero],
  [zero, [0, -0.1, -0.3], zero],
];
const source = {
  id: "curved-cloth",
  kind: "patch",
  region: "shoulder",
  revision: 1,
  thickness: 0.004,
  points,
  controlOffsets,
};

test("curved patches preserve fitted corners while their interior loft follows editable offsets", () => {
  expect(creatureChartSchema.safeParse(source).success).toBe(true);
  for (const [index, u, v] of [
    [0, 0, 0],
    [1, 1, 0],
    [2, 0, 1],
    [3, 1, 1],
  ])
    expect(creaturePatchPoint(source, u, v)).toEqual(points[index]);
  const bilinear = creaturePatchPoint({ points }, 0.5, 0.5),
    curved = creaturePatchPoint(source, 0.5, 0.5);
  expect(curved[2]).toBeCloseTo(bilinear[2] - 0.4, 10);
  const edited = {
    ...source,
    points: points.map((point, index) =>
      index === 0 ? ([point[0], point[1] + 0.1, point[2]] as Vec3) : point,
    ) as typeof points,
  };
  expect(creaturePatchPoint(edited, 0.5, 0.5)[1] - curved[1]).toBeCloseTo(0.025, 10);
});

test("offset grids reject ragged topology, invalid values and overrides of live corners", () => {
  expect(creatureChartSchema.safeParse({ ...source, controlOffsets: [] }).success).toBe(false);
  expect(
    creatureChartSchema.safeParse({
      ...source,
      controlOffsets: [
        [zero, zero],
        [zero, zero, zero],
      ],
    }).success,
  ).toBe(false);
  expect(
    creatureChartSchema.safeParse({
      ...source,
      controlOffsets: [
        [[1, 0, 0], zero],
        [zero, zero],
      ],
    }).success,
  ).toBe(false);
  expect(
    creatureChartSchema.safeParse({
      ...source,
      controlOffsets: [
        [zero, [0, NaN, 0], zero],
        [zero, zero, zero],
      ],
    }).success,
  ).toBe(false);
});
