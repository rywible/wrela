import { expect, test } from "bun:test";
import type { IndirectLightingField, Vec3 } from "@wrela/model";

import { indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectProbe, relocateIndirectProbe, sampleIndirectField } from "./indirect-probes";
import { compileIndirectGeometry, indirectGeometrySteps, traceIndirectRay } from "./indirect-query";

test("query BVH preserves affine-transformed nearest hits, ranges and absolute rebasing", () => {
  const floor = indirectBoxFixture().surfaces[0];
  floor.matrix[0] = 2;
  floor.matrix[5] = 3;
  floor.matrix[13] = 5;
  const geometry = compileIndirectGeometry([floor], { origin: [1e7, 0, 0] });
  const hit = traceIndirectRay(geometry, [1e7 + 0.123, 10, 0], [0, -1, 0]);
  expect(hit?.distance).toBeCloseTo(5, 10);
  expect(hit?.position[0]).toBe(1e7 + 0.123);
  expect(hit?.normal).toEqual([0, 1, 0]);
  expect(traceIndirectRay(geometry, [1e7 + 3, 10, 0], [0, -1, 0])).toBeUndefined();
  expect(() => compileIndirectGeometry([floor], { maxTriangles: 1 })).toThrow("whole build refused");
  const steps = indirectGeometrySteps([floor]);
  let done = steps.next();
  while (!done.done) done = steps.next();
  expect(done.value.report.triangles).toBe(2);
});

test("bounded probe placement moves away from nearby back faces without moving open-space or distant probes", () => {
  const geometry = compileIndirectGeometry([indirectBoxFixture().surfaces[0]]);
  const offset = relocateIndirectProbe(geometry, [0, -0.1, 0], [1, 1, 1]);
  expect(offset[1]).toBeCloseTo(0.125, 8);
  expect(Math.hypot(...offset)).toBeLessThan(0.48);
  expect(relocateIndirectProbe(geometry, [0, 0.1, 0], [1, 1, 1])).toEqual([0, 0, 0]);
  expect(relocateIndirectProbe(geometry, [0, -0.6, 0], [1, 1, 1])).toEqual([0, 0, 0]);
  expect(relocateIndirectProbe(compileIndirectGeometry([]), [0, 0, 0], [1, 1, 1])).toEqual([0, 0, 0]);
});

test("uniform incoming sky integrates to identical diffuse irradiance/pi for all orientations", () => {
  const geometry = compileIndirectGeometry([]),
    sky: Vec3 = [0.4, 0.7, 1.2];
  const data = compileIndirectProbe(
    geometry,
    [0, 0, 0],
    { sunDirection: [0, 1, 0], sunRadiance: [0, 0, 0], skyRadiance: sky },
    { samples: 1024 },
  );
  const field: IndirectLightingField = {
    key: "uniform",
    revision: 1,
    origin: [-1, -1, -1],
    spacing: [2, 2, 2],
    dimensions: [2, 2, 2],
    data: new Float32Array(8 * 60),
    completedProbes: 8,
    totalProbes: 8,
    report: {
      status: "ready",
      bounces: 1,
      source: "constant-sky-and-directional-sun",
      triangles: 0,
      rays: 8192,
      excluded: [],
      buildMs: 0,
      maxSliceMs: 0,
    },
  };
  for (let i = 0; i < 8; i++) field.data.set(data, i * 60);
  for (const normal of [
    [0, 1, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 0, 1],
  ] as Vec3[]) {
    const value = sampleIndirectField(field, [0, 0, 0], normal);
    for (let c = 0; c < 3; c++) expect(value[c]).toBeCloseTo(sky[c], 3);
    expect(value[3]).toBe(1);
  }
  expect(sampleIndirectField(field, [2, 0, 0], [0, 1, 0])).toEqual([0, 0, 0, 0]);
});

test("fully closed diffuse enclosure blocks external sky and sun at an interior probe", () => {
  const fixture = indirectBoxFixture({ ceiling: true, front: true, occluder: false });
  const geometry = compileIndirectGeometry(fixture.surfaces);
  const data = compileIndirectProbe(geometry, [0, 0.7, 0], fixture.lighting, {
    samples: 256,
    skySamples: 16,
  });
  for (let band = 0; band < 9; band++)
    for (let channel = 0; channel < 3; channel++) expect(data[band * 4 + channel]).toBe(0);
});
