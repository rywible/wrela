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
