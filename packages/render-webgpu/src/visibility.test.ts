import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type RenderSurface, transformMatrix } from "@wrela/model";

import { directionalShadow, shadowRadiusForScene } from "./shadows";
import {
  intersectsFrustum,
  MIN_SEMANTIC_OCCLUDER_COVERAGE,
  projectedOccluderCoverage,
  selectVisibility,
  surfaceBounds,
} from "./visibility";

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
  expect(shadowRadiusForScene(scene, 120)).toBe(7);
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

test("invalid bounds and malformed skin influences cannot remove visible work", () => {
  expect(intersectsFrustum({ min: [3, 0, 0], max: [2, 1, 1] }, identityMatrix())).toBe(true);
  expect(intersectsFrustum(surface.mesh.bounds, new Float32Array(4))).toBe(true);
  for (const weights of [new Float32Array([-1, 2]), new Float32Array([2]), new Float32Array([NaN])]) {
    const malformed = {
      ...surface,
      matrix: transformMatrix([10, 0, 0]),
      skin: { matrices: identityMatrix(), jointIndices: new Uint16Array([0]), weights },
    };
    expect(intersectsFrustum(surfaceBounds(malformed, scene.environment), identityMatrix())).toBe(true);
  }
});

test("opt-in survivor visibility preserves shadow-only contributors and uses the active analytic domain", () => {
  const candidate: RenderSurface = {
    ...surface,
    mesh: { ...surface.mesh, bounds: { min: [-0.05, -0.05, 0.75], max: [0.05, 0.05, 0.85] } },
  };
  const blocker: RenderSurface = {
    ...surface,
    id: "blocker",
    matrix: transformMatrix([0, 0, 0.3]),
    selectedRenderProduct: {
      kind: "analytic-quadric",
      primitive: { center: [0, 0, 0], radii: [0.1, 0.1, 0.1], rotation: [0, 0, 0], nodeId: "sphere" },
    } as NonNullable<RenderSurface["selectedRenderProduct"]>,
  };
  const query = {
    ...scene,
    surfaces: [candidate, blocker],
    camera: {
      position: [0, 0, 0] as [number, number, number],
      target: [0, 0, 1] as [number, number, number],
      fov: 60,
    },
    environment: { ...scene.environment, sunDirection: [1, 0, 0] as [number, number, number] },
  };
  // Positive-Z perspective, WebGPU near depth zero and far depth one.
  const camera = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1.01, 1, 0, 0, -0.0101, 0]);
  expect(selectVisibility(query, camera, identityMatrix()).get(candidate)).toEqual({
    camera: true,
    shadow: true,
  });
  expect(selectVisibility(query, camera, identityMatrix(), { enabled: true }).get(candidate)).toEqual({
    camera: false,
    shadow: true,
  });
  const analyticBounds = surfaceBounds(blocker, scene.environment);
  expect(analyticBounds.min[2]).toBeCloseTo(0.2, 6);
  expect(analyticBounds.max[2]).toBeCloseTo(0.4, 6);
  const deformed = { ...query, surfaces: [candidate, { ...blocker, wind: 1 }] };
  expect(selectVisibility(deformed, camera, identityMatrix(), { enabled: true }).get(candidate)?.camera).toBe(
    true,
  );
});

test("semantic proxy coverage drops tiny or clipped occluders without authorizing removal", () => {
  const projection = identityMatrix();
  projection[10] = 0.01;
  expect(projectedOccluderCoverage({ center: [0, 0, 5], radius: 0.001 }, projection)).toBeLessThan(
    MIN_SEMANTIC_OCCLUDER_COVERAGE,
  );
  expect(projectedOccluderCoverage({ center: [0, 0, 5], radius: 0.1 }, projection)).toBeGreaterThan(
    MIN_SEMANTIC_OCCLUDER_COVERAGE,
  );
  expect(projectedOccluderCoverage({ center: [0, 0, 0], radius: 1 }, projection)).toBe(0);
});

test("affine rigid bounds match transformed corners under shear, reflection and rebasing", () => {
  for (let seed = 1; seed <= 64; seed++) {
    const m = identityMatrix();
    for (let column = 0; column < 3; column++)
      for (let row = 0; row < 3; row++)
        m[column * 4 + row] = Math.sin(seed * 1.71 + column * 0.74 + row * 2.11) * 3;
    m[12] = seed % 2 ? 1e6 : -40;
    m[13] = seed * 0.5;
    m[14] = -seed;
    const b = { min: [-2, -0.4, 0.2], max: [1, 3, 4] } as const;
    const subject = {
      ...surface,
      matrix: m,
      mesh: { ...surface.mesh, bounds: { min: [...b.min], max: [...b.max] } },
    } as RenderSurface;
    const actual = surfaceBounds(subject, scene.environment);
    const corners = Array.from({ length: 8 }, (_, i) => [
      b[i & 1 ? "max" : "min"][0],
      b[i & 2 ? "max" : "min"][1],
      b[i & 4 ? "max" : "min"][2],
    ]);
    for (let axis = 0; axis < 3; axis++) {
      const values = corners.map(
        (p) => m[axis] * p[0] + m[axis + 4] * p[1] + m[axis + 8] * p[2] + m[axis + 12],
      );
      expect(actual.min[axis]).toBeCloseTo(Math.min(...values), 7);
      expect(actual.max[axis]).toBeCloseTo(Math.max(...values), 7);
    }
  }
});

test("small-scene shadow fit contains every caster and declines uncertain animated bounds", () => {
  const small = {
    ...scene,
    camera: {
      position: [0, 0.5, 0.8] as [number, number, number],
      target: [0, 0.5, 0] as [number, number, number],
      fov: 70,
    },
  };
  const radius = shadowRadiusForScene(small, 120);
  expect(radius).toBeLessThan(14);
  const light = directionalShadow(small, radius, 2048).matrix;
  for (const s of small.surfaces)
    for (let corner = 0; corner < 8; corner++) {
      const local = [0, 1, 2].map((a) => (corner & (1 << a) ? s.mesh.bounds.max[a] : s.mesh.bounds.min[a]));
      const point = [0, 1, 2].map(
        (a) =>
          s.matrix[a] * local[0] + s.matrix[4 + a] * local[1] + s.matrix[8 + a] * local[2] + s.matrix[12 + a],
      );
      const clip = [0, 1, 2].map(
        (a) => light[a] * point[0] + light[4 + a] * point[1] + light[8 + a] * point[2] + light[12 + a],
      );
      expect(Math.abs(clip[0])).toBeLessThan(1);
      expect(Math.abs(clip[1])).toBeLessThan(1);
      expect(clip[2]).toBeGreaterThan(0);
      expect(clip[2]).toBeLessThan(1);
    }
  const uncertain = { ...small, surfaces: small.surfaces.map((s) => ({ ...s, wind: 1 })) };
  expect(shadowRadiusForScene(uncertain, 120)).toBe(14);
});
