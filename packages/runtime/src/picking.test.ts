import { expect, test } from "bun:test";
import {
  type EvaluatedScene,
  identityMatrix,
  type MeshData,
  type RenderSurface,
  transformMatrix,
} from "@wrela/model";

import { evaluateEnvironment } from "./environment";
import { cameraRay, pickScene, raycastMesh } from "./picking";

const mesh: MeshData = {
  positions: new Float32Array([-1, -1, 0, 1, -1, 0, 0, 1, 0]),
  normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1]),
  indices: new Uint32Array([0, 1, 2]),
  sourceIds: ["body", "body", "ear"],
  bounds: { min: [-1, -1, 0], max: [1, 1, 0] },
};
const material = {
  color: [1, 1, 1] as [number, number, number],
  secondary: [1, 1, 1] as [number, number, number],
  roughness: 1,
  metallic: 0,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
};
function scene(surfaces: RenderSurface[]): EvaluatedScene {
  return {
    surfaces,
    camera: { position: [0, 0, 5], target: [0, 0, 0], fov: 60 },
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
}
test("camera rays and triangle picking return nearest surface and source provenance", () => {
  const view = scene([]),
    ray = cameraRay(view.camera, 100, 100, 200, 200);
  expect(ray.direction).toEqual([0, 0, -1]);
  const hit = raycastMesh(mesh, ray);
  expect(hit?.distance).toBeCloseTo(5);
  expect(hit?.barycentric).toEqual([0.25, 0.25, 0.5]);
  expect(raycastMesh(mesh, { origin: [4, 0, 5], direction: [0, 0, -1] })).toBeNull();
  const near: RenderSurface = {
      id: "near",
      instanceId: "canonical-placement-17",
      source: "near-document",
      mesh,
      matrix: transformMatrix([0, 0, 2]),
      material,
    },
    far: RenderSurface = { ...near, id: "far", source: "far-document", matrix: identityMatrix() };
  const selected = pickScene(scene([far, near]), ray);
  expect(selected?.surfaceId).toBe("near");
  expect(selected?.instanceId).toBe("canonical-placement-17");
  expect(selected?.documentId).toBe("near-document");
  expect(selected?.position[2]).toBeCloseTo(2);
  const ear = raycastMesh(mesh, { origin: [0, 0.8, 5], direction: [0, 0, -1] });
  expect(ear?.nodeId).toBe("ear");
});
test("picking follows skin deformation instead of undeformed bounds", () => {
  const surface: RenderSurface = {
    id: "animated",
    source: "creature",
    mesh,
    matrix: identityMatrix(),
    material,
    skin: {
      jointIndices: new Uint16Array(12),
      weights: new Float32Array([1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]),
      matrices: transformMatrix([4, 0, 0]),
    },
  };
  expect(pickScene(scene([surface]), { origin: [4, 0, 5], direction: [0, 0, -1] })?.surfaceId).toBe(
    "animated",
  );
  expect(pickScene(scene([surface]), { origin: [0, 0, 5], direction: [0, 0, -1] })).toBeNull();
});
test("picking water uses the same displaced wave mesh as rendering", () => {
  const waterMesh: MeshData = {
    ...mesh,
    positions: new Float32Array([-1, 0, -1, 1, 0, -1, 0, 0, 1]),
    bounds: { min: [-1, 0, -1], max: [1, 0, 1] },
  };
  const surface: RenderSurface = {
    id: "water",
    source: "sea",
    mesh: waterMesh,
    matrix: identityMatrix(),
    material,
    water: {
      id: "sea",
      name: "Sea",
      schemaVersion: 1,
      dependencies: [],
      kind: "water",
      level: 3,
      color: [0, 0, 1],
      roughness: 0.2,
      waves: [],
    },
  };
  const hit = pickScene(scene([surface]), { origin: [0, 8, 0], direction: [0, -1, 0] });
  expect(hit?.distance).toBeCloseTo(5);
  expect(hit?.position[1]).toBeCloseTo(3);
});

test("analytic selection picks the exact quadric, including inside rays and overlaps, with source provenance", () => {
  const selectedRenderProduct = {
    kind: "analytic-quadric" as const,
    key: "analytic",
    sourceKey: "source",
    algorithmVersion: "test",
    formatVersion: 1 as const,
    domainKey: "domain",
    assumptions: [],
    errors: [],
    byteLength: 0,
    fallbackKey: "direct",
    dependencies: [],
    primitive: {
      center: [0, 0, 0] as [number, number, number],
      radii: [1, 1, 1] as [number, number, number],
      rotation: [0, 0, 0] as [number, number, number],
      nodeId: "analytic-node",
    },
  };
  const surface: RenderSurface = {
    id: "stone",
    source: "stone-doc",
    instanceId: "stone-placement",
    mesh,
    matrix: identityMatrix(),
    material,
    selectedRenderProduct,
  };
  const hit = pickScene(scene([surface]), { origin: [0, 0, 3], direction: [0, 0, -1] });
  expect(hit?.distance).toBeCloseTo(2);
  expect(hit?.nodeId).toBe("analytic-node");
  expect(hit?.triangle).toBe(-1);
  expect(hit?.instanceId).toBe("stone-placement");
  expect(pickScene(scene([surface]), { origin: [0, 0, 0], direction: [0, 0, 1] })?.distance).toBeCloseTo(1);
  expect(
    pickScene(scene([{ ...surface, id: "far", matrix: transformMatrix([0, 0, -2]) }, surface]), {
      origin: [0, 0, 3],
      direction: [0, 0, -1],
    })?.surfaceId,
  ).toBe("stone");
  // Dropping selection returns to the actual extracted triangle, not an inferred source quadric.
  expect(
    pickScene(scene([{ ...surface, selectedRenderProduct: undefined }]), {
      origin: [0, 0, 3],
      direction: [0, 0, -1],
    })?.distance,
  ).toBeCloseTo(3);
});

test("picking follows branch and leaf motion carried by cooked vegetation vertices", () => {
  const moving: MeshData = {
    ...mesh,
    positions: new Float32Array([-1, -2, 0, 1, -2, 0, 0, 0, 0]),
    bounds: { min: [-1, -2, 0], max: [1, 0, 0] },
    wind: new Float32Array([0, 0, Math.PI / 2, 1, 0, 0, Math.PI / 2, 1, 0, 0, Math.PI / 2, 1]),
  };
  const view = scene([
    { id: "leaf", source: "plant", mesh: moving, matrix: identityMatrix(), material, wind: 1 },
  ]);
  view.environment.wind = [10, 0, 0];
  const hit = pickScene(view, { origin: [0, -1, 5], direction: [0, 0, -1] });
  expect(hit?.surfaceId).toBe("leaf");
  expect(hit?.distance).toBeCloseTo(5 - 1 / Math.sqrt(1 + 0.35 ** 2), 6);
  view.environment.wind = [0, 0, 0];
  expect(pickScene(view, { origin: [0, -1, 5], direction: [0, 0, -1] })?.distance).toBeCloseTo(5, 6);
});

test("shared shoot picking preserves occurrence identity and skips shadow-only selections", () => {
  const transforms = new Float32Array(32);
  transforms.set(identityMatrix());
  transforms.set(transformMatrix([4, 0, 0]), 16);
  const surface: RenderSurface = {
    id: "shoots",
    source: "pine",
    material,
    matrix: identityMatrix(),
    mesh: {
      ...mesh,
      bounds: { min: [-1, -1, 0], max: [5, 1, 0] },
      shoots: {
        transforms,
        anchors: new Float32Array(8),
        motion: new Float32Array(8),
        sourceIds: ["pine/branch0", "pine/branch1"],
        templateBounds: mesh.bounds,
      },
    },
  };
  const view = scene([surface]);
  view.environment.wind = [0, 0, 0];
  const ray = {
    origin: [4, 0, 5] as [number, number, number],
    direction: [0, 0, -1] as [number, number, number],
  };
  const hit = pickScene(view, ray);
  expect(hit?.surfaceId).toBe("shoots");
  expect(hit?.nodeId).toBe("pine/branch1/body");
  surface.shootSelection = { indices: new Uint32Array([1]), masks: new Uint8Array([2]) };
  expect(pickScene(view, ray)).toBeNull();
});
