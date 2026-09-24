import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type RenderSurface, surfaceReliefSchema } from "@wrela/model";

import { DetailSelector, projectedDiameter } from "./detail";

const base = {
  positions: new Float32Array(12),
  normals: new Float32Array(12),
  indices: new Uint32Array([0, 1, 2, 0, 2, 3]),
  bounds: { min: [-1, -1, 0] as [number, number, number], max: [1, 1, 0] as [number, number, number] },
};
const coarse = { ...base, indices: new Uint32Array([0, 1, 2]) };
const surface: RenderSurface = {
  id: "tree",
  source: "tree",
  mesh: base,
  details: [{ label: "distant", mesh: coarse, maxProjectedDiameter: 64, maxError: null }],
  matrix: identityMatrix(),
  material: {
    color: [1, 1, 1],
    secondary: [1, 1, 1],
    roughness: 1,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  },
};
function sceneAtDiameter(pixels: number): EvaluatedScene {
  const radius = Math.SQRT2;
  return {
    surfaces: [surface],
    camera: {
      position: [0, 0, radius + (radius * 1080) / (pixels * Math.tan((25 * Math.PI) / 180))],
      target: [0, 0, 0],
      fov: 50,
    },
    environment: {
      sunDirection: [0, 1, 0],
      sunColor: [1, 1, 1],
      sunIntensity: 1,
      ambient: 1,
      skyColor: [1, 1, 1],
      horizonColor: [1, 1, 1],
      groundColor: [1, 1, 1],
      fogDensity: 0,
      wind: [0, 0, 0],
      exposure: 1,
    },
    time: 0,
    mode: "beauty",
    grid: false,
  };
}
test("projected detail selection reduces geometry only at small size and does not chatter near a threshold", () => {
  const selector = new DetailSelector();
  expect(projectedDiameter(sceneAtDiameter(50), surface, 1080)).toBeCloseTo(50, 8);
  const selected = selector.select(sceneAtDiameter(50), 1080);
  expect(selected.scene.surfaces[0].mesh).toBe(coarse);
  expect(selected.details[0]).toEqual({
    id: "tree",
    label: "distant",
    selection: "projected-size",
    maxError: null,
  });
  expect(selector.select(sceneAtDiameter(70), 1080).scene.surfaces[0].mesh).toBe(coarse);
  expect(selector.select(sceneAtDiameter(75), 1080).scene.surfaces[0].mesh).toBe(base);
  expect(selector.select(sceneAtDiameter(60), 1080).scene.surfaces[0].mesh).toBe(base);
  expect(selector.select(sceneAtDiameter(50), 1080).scene.surfaces[0].mesh).toBe(coarse);
});
test("characters and camera-intersecting bounds retain the authored geometry", () => {
  const selector = new DetailSelector();
  const skin = {
    jointIndices: new Uint16Array(16),
    weights: new Float32Array(16),
    matrices: identityMatrix(),
  };
  const scene = sceneAtDiameter(1);
  expect(selector.select({ ...scene, surfaces: [{ ...surface, skin }] }, 1080).scene.surfaces[0].mesh).toBe(
    base,
  );
  expect(
    selector.select({ ...scene, camera: { ...scene.camera, position: [0, 0, 0.1] } }, 1080).scene.surfaces[0]
      .mesh,
  ).toBe(base);
});
test("detail hysteresis resets when the source realization changes", () => {
  const selector = new DetailSelector();
  selector.select(sceneAtDiameter(50), 1080);
  const scene = sceneAtDiameter(70);
  const changed = { ...base };
  expect(
    selector.select({ ...scene, surfaces: [{ ...surface, mesh: changed }] }, 1080).scene.surfaces[0].mesh,
  ).toBe(changed);
});

test("canonical capture selection is independent of the preceding camera path", () => {
  const selector = new DetailSelector();
  selector.select(sceneAtDiameter(50), 1080);
  expect(selector.select(sceneAtDiameter(70), 1080).scene.surfaces[0].mesh).toBe(coarse);
  selector.clear();
  expect(selector.select(sceneAtDiameter(70), 1080).scene.surfaces[0].mesh).toBe(base);
  expect(new DetailSelector().select(sceneAtDiameter(70), 1080).scene.surfaces[0].mesh).toBe(base);
});

test("coarse wind and generator details restore all relief bands to appearance", () => {
  const near: RenderSurface = {
    ...surface,
    wind: 0.5,
    mesh: { ...base, reliefCoordinates: base.positions.slice(), reliefNormals: base.normals.slice() },
    reliefAppearance: {
      recipe: surfaceReliefSchema.parse({
        kind: "bark",
        amplitude: 0.02,
        scale: 0.07,
        seed: 37,
        targetEdgeLength: 0.02,
      }),
      geometryWeights: [1, 0.5, 0],
      residualWeights: [0, 0.5, 1],
      slopeVariance: [0.1, 0.2, 0.3],
    },
  };
  const selector = new DetailSelector();
  const far = selector.select({ ...sceneAtDiameter(1), surfaces: [near] }, 1080).scene.surfaces[0];
  expect(far.mesh).toBe(coarse);
  expect(far.reliefAppearance?.geometryWeights).toEqual([0, 0, 0]);
  expect(far.reliefAppearance?.residualWeights).toEqual([1, 1, 1]);
  expect(far.reliefAppearance?.recipe).toBe(near.reliefAppearance?.recipe);
  const close = selector.select({ ...sceneAtDiameter(10000), surfaces: [near] }, 1080).scene.surfaces[0];
  expect(close.mesh).toBe(near.mesh);
  expect(close.reliefAppearance?.geometryWeights).toEqual([1, 0.5, 0]);
  expect(near.reliefAppearance?.geometryWeights).toEqual([1, 0.5, 0]);
});
