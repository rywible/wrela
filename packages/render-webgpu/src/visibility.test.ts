import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type RenderSurface, transformMatrix } from "@wrela/model";
import { directionalShadow, shadowRadiusForScene } from "./shadows";
import { intersectsFrustum, selectVisibility, surfaceBounds } from "./visibility";

const surface: RenderSurface = {
  id: "surface",
  source: "surface",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array(),
    normals: new Float32Array(),
    indices: new Uint32Array(),
    bounds: { min: [-0.1, -0.1, 0.1], max: [0.1, 0.1, 0.3] },
  },
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
const scene: EvaluatedScene = {
  surfaces: [surface],
  camera: { position: [1, 2, 5], target: [0, 0, 0], fov: 50 },
  environment: {
    sunDirection: [0.4, 0.8, 0.2],
    sunColor: [1, 1, 1],
    sunIntensity: 1,
    ambient: 1,
    skyColor: [1, 1, 1],
    horizonColor: [1, 1, 1],
    groundColor: [1, 1, 1],
    fogDensity: 0,
    wind: [10, 0, 0],
    exposure: 1,
  },
  time: 0,
  mode: "beauty",
  grid: false,
};
test("WebGPU frustum keeps intersecting bounds and excludes behind-near/far/offscreen bounds", () => {
  const matrix = identityMatrix();
  expect(intersectsFrustum(surface.mesh.bounds, matrix)).toBe(true);
  expect(intersectsFrustum({ min: [0.9, -0.1, 0.1], max: [2, 0.1, 0.5] }, matrix)).toBe(true);
  for (const bounds of [
    { min: [2, 0, 0.1], max: [3, 1, 0.5] },
    { min: [0, 0, -1], max: [0.5, 0.5, -0.1] },
    { min: [0, 0, 1.1], max: [0.5, 0.5, 2] },
  ])
    expect(intersectsFrustum(bounds as typeof surface.mesh.bounds, matrix)).toBe(false);
  expect(intersectsFrustum({ min: [NaN, 0, 0], max: [1, 1, 1] }, matrix)).toBe(true);
});
test("conservative bounds retain wind and skin deformations entering the view", () => {
  const outside = { ...surface, matrix: transformMatrix([1.6, 0, 0]) };
  expect(intersectsFrustum(surfaceBounds(outside, scene.environment), identityMatrix())).toBe(false);
  expect(intersectsFrustum(surfaceBounds({ ...outside, wind: 2 }, scene.environment), identityMatrix())).toBe(
    true,
  );
  const skinned = {
    ...outside,
    skin: {
      matrices: transformMatrix([-1, 0, 0]),
      jointIndices: new Uint16Array([0]),
      weights: new Float32Array([1]),
    },
  };
  expect(intersectsFrustum(surfaceBounds(skinned, scene.environment), identityMatrix())).toBe(true);
});
test("water uses its analytic displaced level and never casts a directional shadow", () => {
  const water: RenderSurface = {
    ...surface,
    matrix: transformMatrix([0, 100, 0]),
    water: {
      id: "water",
      name: "Water",
      kind: "water",
      schemaVersion: 1,
      dependencies: [],
      level: 0.5,
      color: [0, 0, 1],
      roughness: 0.2,
      waves: [{ amplitude: 1, wavelength: 10, direction: 0, phase: 0, speed: 1 }],
    },
  };
  const bounds = surfaceBounds(water, scene.environment);
  expect(bounds.min[1]).toBe(-0.5);
  expect(bounds.max[1]).toBe(1.5);
  expect(
    selectVisibility({ ...scene, surfaces: [water] }, identityMatrix(), identityMatrix()).get(water),
  ).toEqual({ camera: true, shadow: false });
});
test("shadow coordinates survive a render-origin change", () => {
  const absolute = {
    ...scene,
    camera: {
      ...scene.camera,
      position: [257, 2, 5] as [number, number, number],
      target: [256, 0, 0] as [number, number, number],
    },
  };
  const rebased = { ...scene, origin: [256, 0, 0] as [number, number, number] };
  const before = directionalShadow(absolute, 120, 2048).matrix;
  const after = directionalShadow(rebased, 120, 2048).matrix;
  const clip = (matrix: Float32Array, point: number[]) =>
    [0, 1, 2].map((row) => point.reduce((sum, value, column) => sum + matrix[column * 4 + row] * value, 0));
  const a = clip(before, [256, 0, 0, 1]),
    b = clip(after, [0, 0, 0, 1]);
  for (let axis = 0; axis < 3; axis++) expect(a[axis]).toBeCloseTo(b[axis], 5);
});

test("subject coverage retains contact detail while world coverage is explicit and depth bias retains world units", () => {
  expect(shadowRadiusForScene(scene, 120)).toBe(14);
  expect(shadowRadiusForScene({ ...scene, shadowRadius: 240 }, 120)).toBe(120);
  const close = directionalShadow(scene, 14, 2048);
  const world = directionalShadow(scene, 120, 2048);
  expect(close.worldTexel).toBeCloseTo(28 / 2048, 8);
  for (const [radius, light] of [
    [14, close],
    [120, world],
  ] as const) {
    const worldBias = Math.max(0.00025, light.worldTexel * 0.03);
    expect(worldBias * light.inverseDepthRange * (radius * 5 - 0.1)).toBeCloseTo(worldBias, 12);
    expect(worldBias).toBeLessThan(0.004);
  }
});
