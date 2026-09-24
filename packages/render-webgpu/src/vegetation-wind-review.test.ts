import { expect, test } from "bun:test";
import {
  identityMatrix,
  type RenderEnvironment,
  type RenderSurface,
  type Vec3,
  vegetationMotionOffset,
} from "@wrela/model";

import { surfaceBounds } from "./visibility";

const environment: RenderEnvironment = {
  sunDirection: [0, 1, 0],
  sunColor: [1, 1, 1],
  sunIntensity: 1,
  ambient: 1,
  skyColor: [1, 1, 1],
  horizonColor: [1, 1, 1],
  groundColor: [1, 1, 1],
  fogDensity: 0,
  wind: [10, 0, 0],
  exposure: 1,
};

test("visibility envelope contains vertical flutter above the rest canopy", () => {
  const weights = [0, 0.25, 0, 0.035] as const;
  const surface: RenderSurface = {
    id: "leaf",
    source: "leaf",
    matrix: identityMatrix(),
    wind: 2,
    material: {
      color: [1, 1, 1],
      secondary: [1, 1, 1],
      roughness: 1,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
    mesh: {
      positions: new Float32Array([0, 2, 0]),
      normals: new Float32Array([0, 1, 0]),
      indices: new Uint32Array(),
      wind: new Float32Array(weights),
      bounds: { min: [0, 2, 0], max: [0, 2, 0] },
    },
  };
  const bounds = surfaceBounds(surface, environment);
  for (let sample = 0; sample < 120; sample++) {
    const offset = vegetationMotionOffset(weights, [0, 2, 0], sample / 30, environment.wind, 2);
    expect(2 + offset[1]).toBeGreaterThanOrEqual(bounds.min[1]);
    expect(2 + offset[1]).toBeLessThanOrEqual(bounds.max[1]);
  }
});

test("branch and leaf motion stay fixed in world space across render-origin rebasing", () => {
  const absolute: Vec3 = [900, 10, 700],
    origin: Vec3 = [896, 0, 692];
  const relative = absolute.map((value, axis) => value - origin[axis]) as Vec3;
  const weights = [0.7, 0.2, 1.2, 0.035] as const;
  const before = vegetationMotionOffset(weights, absolute, 2.3, environment.wind, 1);
  const after = vegetationMotionOffset(
    weights,
    relative,
    2.3,
    environment.wind,
    1,
    (origin[0] * 0.17 + origin[2] * 0.23) % (2 * Math.PI),
  );
  before.forEach((value, axis) => {
    expect(after[axis]).toBeCloseTo(value, 10);
  });
});
