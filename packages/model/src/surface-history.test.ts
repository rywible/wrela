import { expect, test } from "bun:test";
import type { MaterialDefinition } from "./documents";
import { createSurfaceAppearance } from "./surface-appearance";
import { alpineSurfaceHistory, sampleSurfaceHistory, surfaceHistorySchema } from "./surface-history";
import { createSurfaceHistoryMaterial } from "./surface-history-material";

test("shared history separates contact moisture, rain exposure, shelter and age", () => {
  const history = alpineSurfaceHistory();
  const low = sampleSurfaceHistory(history, { position: [0.3, 0.1, 0], normal: [0, 0, -1] });
  const high = sampleSurfaceHistory(history, { position: [0.3, 3, 0], normal: [0, 0, -1] });
  const sheltered = sampleSurfaceHistory(history, { position: [0.3, 3, 0], normal: [0, 0, -1], shelter: 1 });
  const newStone = sampleSurfaceHistory(
    { ...history, ageYears: 0 },
    { position: [0.3, 0.1, 0], normal: [0, 0, -1] },
  );
  expect(low.contact).toBeGreaterThan(high.contact);
  expect(low.wetness).toBeGreaterThan(high.wetness);
  expect(low.moss).toBeGreaterThan(high.moss);
  expect(sheltered.exposure).toBe(0);
  expect(sheltered.runoff).toBe(0);
  expect(newStone.age).toBe(0);
  expect(newStone.moss).toBe(0);
  for (const sample of [low, high, sheltered, newStone])
    for (const value of Object.values(sample)) {
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThanOrEqual(1);
    }
});

test("history frame translates and rotates with authored construction", () => {
  const source = alpineSurfaceHistory({ rainDirection: [0, -1, 0] });
  const sample = {
    position: [0, 0.4, 0] as [number, number, number],
    normal: [0, 0, -1] as [number, number, number],
  };
  const first = sampleSurfaceHistory(source, sample);
  // Quarter-turn around Z maps local Y to world -X and Z to world Z.
  const transformed = sampleSurfaceHistory(
    { ...source, origin: [10, 20, 30], up: [-1, 0, 0], rainDirection: [1, 0, 0] },
    { position: [9.6, 20, 30], normal: [0, 0, -1] },
  );
  for (const key of Object.keys(first) as (keyof typeof first)[])
    expect(transformed[key]).toBeCloseTo(first[key], 10);
  expect(surfaceHistorySchema.safeParse({ ...source, up: [0, 0, 0] }).success).toBe(false);
  expect(() => sampleSurfaceHistory(source, { ...sample, normal: [0, 0, 0] })).toThrow("nonzero");
});

test("history material keeps source ownership and exposes physical stone relief and bounded layers", () => {
  const appearance = createSurfaceAppearance();
  appearance.detail.kind = "mineral";
  const material: MaterialDefinition = {
    id: "stone",
    name: "Stone",
    kind: "material",
    schemaVersion: 1,
    dependencies: [],
    color: [0.2, 0.2, 0.2],
    secondary: [0.2, 0.2, 0.2],
    roughness: 0.8,
    metallic: 0,
    normalStrength: 0,
    scale: 1,
    pattern: "solid",
    domain: "local",
    appearance,
  };
  const before = structuredClone(material);
  const signals = sampleSurfaceHistory(alpineSurfaceHistory(), {
    position: [0, 0.05, 0],
    normal: [0, 0, -1],
  });
  const result = createSurfaceHistoryMaterial(material, "stone-grounded", signals);
  expect(material).toEqual(before);
  expect(result.appearance?.relief?.amplitude).toBeGreaterThan(0.004);
  expect(result.appearance?.wetness).toBeGreaterThan(0.2);
  expect(result.appearance?.layers.length).toBeLessThanOrEqual(4);
});
