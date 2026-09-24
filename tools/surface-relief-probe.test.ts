import { expect, test } from "bun:test";
import {
  packReliefSourceVertices,
  packVertices,
  RELIEF_VERTEX_FLOATS,
  VERTEX_FLOATS,
} from "@wrela/render-webgpu/packing";
import { surfaceReliefProbeCases, surfaceReliefProbeReference } from "./fixtures/surface-relief-probe";

const cases = surfaceReliefProbeCases();
test("relief parity cases preserve all signed seed bits and packed bark phase", () => {
  expect(cases.length).toBe(1728);
  for (const sample of cases) {
    expect(sample.packed[3] | (sample.packed[7] << 16) | 0).toBe(sample.seed);
    expect(Math.sin(sample.packed[11])).toBeCloseTo(Math.sin(sample.seed), 6);
    expect(sample.packed.slice(8, 11)).toEqual(new Float32Array(sample.appearance.geometryWeights));
    expect(sample.packed.slice(12, 15)).toEqual(new Float32Array(sample.appearance.residualWeights));
  }
});

test("large pixel footprints transfer every unresolved residual band to finite nominal slope variance", () => {
  for (const sample of cases) {
    const expected = surfaceReliefProbeReference(sample);
    expect([...expected.bands, ...expected.resolved].every(Number.isFinite)).toBe(true);
    expect(expected.resolved[1]).toBeGreaterThanOrEqual(0);
    if (sample.footprint >= sample.scale || sample.allocation === "complete")
      expect(Math.abs(expected.resolved[0])).toBe(0);
    if (sample.allocation === "complete" || sample.footprint === 0) expect(expected.resolved[1]).toBe(0);
  }
});

test("vertex packing retains exact relief reference coordinates and frames independently of deformed geometry", () => {
  const mesh = {
    positions: new Float32Array([0, 0, -0.01]),
    normals: new Float32Array([0.1, 0, 0.995]),
    indices: new Uint32Array(),
    bounds: { min: [0, 0, -0.01] as [number, number, number], max: [0, 0, 0] as [number, number, number] },
    reliefCoordinates: new Float32Array([0, 0, 0]),
    reliefNormals: new Float32Array([0, 0, 1]),
  };
  const packed = packVertices(mesh);
  expect(packed.length).toBe(VERTEX_FLOATS);
  expect(packed.slice(0, 3)).toEqual(mesh.positions);
  expect(packed.slice(3, 6)).toEqual(mesh.normals);
  const reference = packReliefSourceVertices(mesh);
  expect(reference?.length).toBe(RELIEF_VERTEX_FLOATS);
  expect(reference?.slice(0, 3)).toEqual(mesh.reliefCoordinates);
  expect(reference?.slice(3, 6)).toEqual(mesh.reliefNormals);
  expect(
    packReliefSourceVertices({ ...mesh, reliefCoordinates: undefined, reliefNormals: undefined }),
  ).toBeUndefined();
  expect(() => packReliefSourceVertices({ ...mesh, reliefNormals: undefined })).toThrow("Malformed relief");
});
