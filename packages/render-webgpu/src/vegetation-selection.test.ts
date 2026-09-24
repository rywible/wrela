import { expect, test } from "bun:test";
import { type EvaluatedScene, identityMatrix, type MeshDetail, type RenderSurface } from "@wrela/model";

import { crownAdmissible, crownViewRanges } from "./vegetation-selection";

const mesh = {
  positions: new Float32Array(3),
  normals: new Float32Array(3),
  indices: new Uint32Array(12),
  bounds: { min: [-1, -1, -1] as [number, number, number], max: [1, 1, 1] as [number, number, number] },
};
const surface = { id: "tree", mesh, matrix: identityMatrix() } as RenderSurface;
const scene = {
  camera: { position: [0, 0, 10] },
  environment: { sunDirection: [1, 0, 0], wind: [0, 0, 0] },
} as EvaluatedScene;
const detail: MeshDetail = {
  label: "crown",
  mesh,
  maxProjectedDiameter: 24,
  maxError: null,
  vegetation: {
    version: 1,
    kind: "multiview-crown",
    key: "crown",
    sourceKey: "source",
    algorithmVersion: "1",
    byteLength: 0,
    windEnvelope: 0,
    sourceOrgans: ["branch"],
    fallback: "source-mesh",
    qualification: { status: "candidate", maximumPixels: 24, maxWind: 0, evidence: "test" },
    views: [
      { direction: [0, 0, 1], firstIndex: 0, indexCount: 6 },
      { direction: [1, 0, 0], firstIndex: 6, indexCount: 6 },
    ],
  },
};
test("camera and light independently choose a single crown view", () => {
  expect(crownViewRanges(scene, surface, detail)).toEqual({
    drawRange: { start: 0, count: 6 },
    shadowDrawRange: { start: 6, count: 6 },
  });
});
test("candidate crowns retain fallback, and research still respects light footprint and transform validity", () => {
  expect(crownAdmissible(scene, surface, detail, 1, false)).toBe(false);
  expect(crownAdmissible(scene, surface, detail, 1, true)).toBe(true);
  expect(crownAdmissible(scene, surface, detail, 0.01, true)).toBe(false);
  const matrix = identityMatrix();
  matrix[0] = 2;
  expect(crownAdmissible(scene, { ...surface, matrix }, detail, 1, true)).toBe(false);
});
