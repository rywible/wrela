import { expect, test } from "bun:test";
import { cloudFormationSchema, cloudscapeSchema } from "@wrela/model";

import {
  CLOUD_FORMATION_FRAME_OFFSET,
  CLOUD_FORMATION_HEADER_FLOATS,
  CLOUD_FORMATION_STRIDE,
  CLOUD_FRAME_FLOATS,
  CLOUD_LOBE_OFFSET,
  compileCloudGrowth,
  packCloudFormations,
} from "./cloud-formations";
import { canReuseCloudHistory } from "./cloud-history";

const tower = cloudFormationSchema.parse({
  id: "tower",
  kind: "tower",
  center: [1200, -4500],
  base: 1200,
  size: [1900, 4500, 2100],
  yaw: Math.PI / 2,
  density: 1,
  erosion: 0.4,
  seed: 7,
});
test("formation bounds, identities and work are validated at authoring", () => {
  expect(() => cloudFormationSchema.parse({ ...tower, size: [1900, 6000, 2100] })).toThrow();
  expect(() => cloudFormationSchema.parse({ ...tower, size: [0, 4000, 2100] })).toThrow();
  expect(() => cloudscapeSchema.parse({ formations: [tower, tower] })).toThrow();
  expect(() =>
    cloudscapeSchema.parse({
      formations: Array.from({ length: 5 }, (_, i) => ({ ...tower, id: String(i) })),
    }),
  ).toThrow();
});
test("one upload preserves world coordinates, support and independent envelope orientation", () => {
  const data = packCloudFormations({
    development: 0.5,
    storminess: 0,
    highCloudCover: 0,
    background: 0,
    highCloudYaw: 0,
    formations: [tower],
  });
  expect(Array.from(data.slice(0, 4))).toEqual([1, 0, 5700, 0]);
  expect(Array.from(data.slice(8, 12))).toEqual([1200, 1200, -4500, 0]);
  expect(data[16]).toBeCloseTo(0);
  expect(data[17]).toBeCloseTo(1);
  expect(
    data
      .slice(CLOUD_FORMATION_HEADER_FLOATS + CLOUD_FORMATION_STRIDE, CLOUD_LOBE_OFFSET)
      .every((v) => v === 0),
  ).toBe(true);
  expect(packCloudFormations(undefined)[1]).toBe(1);
});
test("shape edits invalidate history even when wind, camera and weather are unchanged", () => {
  const before = new Float32Array(CLOUD_FRAME_FLOATS);
  before[16] = before[24] = 1;
  before[36] = 0.5;
  before.set(
    packCloudFormations({ development: 0.5, storminess: 0, highCloudCover: 0, formations: [tower] }),
    CLOUD_FORMATION_FRAME_OFFSET,
  );
  const after = before.slice();
  expect(canReuseCloudHistory(before, after)).toBe(true);
  after[CLOUD_FORMATION_FRAME_OFFSET + CLOUD_FORMATION_HEADER_FLOATS] += 100;
  expect(canReuseCloudHistory(before, after)).toBe(false);
});

test("growth is repeatable, changes silhouette across seeds, and stays inside support under shear", () => {
  expect(compileCloudGrowth(tower)).toEqual(compileCloudGrowth(tower));
  const silhouettes = new Set<string>();
  for (const kind of ["tower", "bank", "wisp"] as const) {
    for (let seed = 0; seed < 100; seed++) {
      const lobes = compileCloudGrowth({
        ...tower,
        kind,
        seed,
        shear: seed % 2 ? -1 : 1,
        maturity: seed / 100,
      });
      expect(lobes).toHaveLength(6);
      for (const lobe of lobes) {
        expect(lobe.radius.every((r) => r > 0)).toBe(true);
        expect(Math.abs(lobe.center[0]) + lobe.radius[0]).toBeLessThanOrEqual(0.971);
        expect(Math.abs(lobe.center[2]) + lobe.radius[2]).toBeLessThanOrEqual(0.971);
        expect(lobe.center[1] + lobe.radius[1]).toBeLessThanOrEqual(0.981);
      }
      silhouettes.add(JSON.stringify(lobes));
    }
  }
  expect(silhouettes.size).toBeGreaterThanOrEqual(200);
});

test("middle and high decks validate separation and upload independently of low weather", () => {
  const cloud = cloudscapeSchema.parse({
    midCloudCover: 0.4,
    midCloudHeight: 7200,
    midCloudYaw: 1.2,
    highCloudHeight: 11000,
  });
  const data = packCloudFormations(cloud);
  expect(data[4]).toBeCloseTo(0.4);
  expect(data[5]).toBe(7200);
  expect(data[6]).toBeCloseTo(1.2);
  expect(data[7]).toBe(11000);
  expect(() => cloudscapeSchema.parse({ midCloudHeight: 6400 })).toThrow();
  expect(() => cloudFormationSchema.parse({ ...tower, maturity: 1.1 })).toThrow();
  expect(() => cloudFormationSchema.parse({ ...tower, shear: -1.1 })).toThrow();
});
