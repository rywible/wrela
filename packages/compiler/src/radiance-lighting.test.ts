import { expect, test } from "bun:test";
import type { RadianceLightingField, Vec3 } from "@wrela/model";
import { indirectBoxFixture, quad } from "../../../tools/fixtures/indirect-scenes";
import { indirectSH } from "./indirect-probes";
import { compileIndirectGeometry, traceIndirectRay } from "./indirect-query";
import {
  compileRadianceLightingSteps,
  radianceReceiverMixture,
  radianceReceiverSamples,
} from "./radiance-lighting";

function compile(
  options: Parameters<typeof compileRadianceLightingSteps>[1],
  closed = true,
  emission = false,
  scale = 1,
) {
  const f = indirectBoxFixture({ ceiling: closed, front: closed, occluder: false });
  if (emission)
    f.surfaces.find((s) => s.id === "ceiling")!.material.emission = { color: [1, 0.1, 0], intensity: 3 };
  for (const s of f.surfaces) for (const a of [0, 5, 10]) s.matrix[a] *= scale;
  const steps = compileRadianceLightingSteps(f.surfaces, { ...options, maxSamples: 128, raysPerSample: 64 });
  let result = steps.next();
  while (!result.done) result = steps.next();
  return result.value;
}
function diffuse(
  field: RadianceLightingField,
  id: number,
  normal: Vec3,
  input: "sky" | "point" | "emission",
) {
  const result = [0, 0, 0];
  indirectSH(normal).forEach((v, k) => {
    const band = k === 0 ? 1 : k < 4 ? 2 / 3 : 1 / 4;
    const offset = ((id - 1) * 9 + k) * 27 * 4;
    for (let c = 0; c < 3; c++) {
      const value =
        input === "sky"
          ? (field.transfer[offset + c] + field.transfer[offset + 9 * 4 + c]) / 0.2820947918
          : field.transfer[offset + (input === "point" ? 18 : 26) * 4 + c];
      result[c] += value * v * band;
    }
  });
  return result.map((v) => Math.max(0, v));
}

test("sealed rooms reject sky and exterior probes, including caves larger than the sky horizon", () => {
  for (const scale of [1, 15]) {
    const p = compile({ key: "sealed", center: [0, scale, 0] }, true, false, scale);
    const floor = p.meshes.get("floor")!.radianceProbes!;
    const ids = [
      ...new Set(
        Array.from(floor)
          .filter((v, i) => i % 4 === 0 && v >= 1024)
          .flatMap((v) => {
            const start = (Math.floor(v) - 1024) * 8;
            return [0, 1, 2, 3]
              .filter((j) => p.field.receivers![start + 4 + j] > 0)
              .map((j) => p.field.receivers![start + j]);
          }),
      ),
    ];
    expect(ids.length).toBeGreaterThan(0);
    for (const id of ids)
      expect(Math.max(...diffuse(p.field, Math.floor(id), [0, 1, 0], "sky"))).toBeLessThan(1e-6);
    const pair = radianceReceiverSamples(p.geometry, [[2 * scale, scale, 0]], [0, scale, 0]);
    expect(pair).toEqual([0, 0]);
  }
});

test("local lights and emissive surfaces provide colored indirect light in a sealed room", () => {
  const p = compile(
    { key: "local", center: [0, 1, 0], lights: [{ position: [0, 1.6, 0], range: 8 }] },
    true,
    true,
  );
  const ids = radianceReceiverSamples(p.geometry, p.field.positions, [0, 0.01, 0]);
  expect(ids[0]).toBeGreaterThan(0);
  expect(diffuse(p.field, ids[0], [0, 1, 0], "point").reduce((a, b) => a + b)).toBeGreaterThan(0.01);
  const emitted = diffuse(p.field, ids[0], [0, 1, 0], "emission");
  expect(emitted[0]).toBeGreaterThan(0.1);
  expect(emitted[0]).toBeGreaterThan(emitted[1] * 3);
  expect(emitted[2]).toBe(0);
  expect(Math.max(...diffuse(p.field, ids[0], [0, 1, 0], "sky"))).toBeLessThan(1e-6);
});

test("open rooms receive sky bounce; static vertex records retain mesh geometry and stay bounded", () => {
  const p = compile({ key: "open", center: [0, 1, 0] }, false);
  const ids = radianceReceiverSamples(p.geometry, p.field.positions, [0, 0.01, 0]);
  expect(diffuse(p.field, ids[0], [0, 1, 0], "sky").reduce((a, b) => a + b)).toBeGreaterThan(0.2);
  expect(p.field.positions.length).toBeLessThanOrEqual(128);
  expect(p.field.transfer.every(Number.isFinite)).toBe(true);
  expect(p.field.report.unmappedVertices).toBe(0);
  for (const mesh of p.meshes.values())
    if (mesh.radianceProbes?.some((v, i) => i % 2 === 0 && v >= 1024))
      expect(mesh.radianceMixtures).toBe(true);
});

test("bounded receiver search matches a full sort with visibility rejection", () => {
  const p = compile({ key: "search", center: [0, 1, 0] }, true);
  for (let i = 0; i < 80; i++) {
    const position: Vec3 = [Math.sin(i * 1.73) * 2, 1 + Math.sin(i * 0.37), Math.cos(i * 0.53) * 2];
    const nearest = p.field.positions
      .map((point, index) => ({ index, distance: Math.hypot(...point.map((v, a) => v - position[a])) }))
      .filter((v) => v.distance <= 24)
      .sort((a, b) => a.distance - b.distance || a.index - b.index)
      .slice(0, 16);
    const found: number[] = [];
    for (const n of nearest) {
      const direction = p.field.positions[n.index].map(
        (v, a) => (v - position[a]) / Math.max(n.distance, 1e-9),
      ) as Vec3;
      if (n.distance < 0.004 || !traceIndirectRay(p.geometry, position, direction, n.distance - 0.002))
        found.push(n.index + 1);
      if (found.length === 2) break;
    }
    expect(radianceReceiverSamples(p.geometry, p.field.positions, position)).toEqual([
      found[0] ?? 0,
      found[1] ?? found[0] ?? 0,
    ]);
  }
});

test("a blocked near set does not hide a valid farther receiver sample", () => {
  const wall = quad(
    "partition",
    [
      [0, -2, -2],
      [0, 2, -2],
      [0, 2, 2],
      [0, -2, 2],
    ],
    [0.5, 0.5, 0.5],
  );
  const geometry = compileIndirectGeometry([wall]);
  const positions: Vec3[] = Array.from({ length: 32 }, (_, i) => [0.01 + i * 0.002, 0, 0]);
  positions.push([-2, 1, 0]);
  const receiver: Vec3 = [-0.03, 0, 0];
  expect(radianceReceiverSamples(geometry, positions, receiver)).toEqual([33, 33]);
  const mixture = radianceReceiverMixture(geometry, positions, receiver);
  expect(mixture).toEqual([33, 0, 0, 0, 1, 0, 0, 0]);
});

test("alternate receiver bindings come from exterior space rather than an object's underside", () => {
  const fixture = indirectBoxFixture();
  const steps = compileRadianceLightingSteps(fixture.surfaces, { key: "alternate", center: [0, 1, 0] });
  let result = steps.next();
  while (!result.done) result = steps.next();
  const ids = result.value.fallbacks.get("neutral-block");
  expect(ids?.[0]).toBeGreaterThan(0);
  for (const id of ids ?? []) expect(result.value.field.enclosed[Math.floor(id) - 1]).toBe(0);
});

test("four-sample interpolation stays continuous across a nearest-pair boundary and rejects exterior samples", () => {
  const p = compile({ key: "mixture", center: [0, 1, 0] });
  const positions: Vec3[] = [
    [-0.5, 1, -0.5],
    [0.5, 1, -0.5],
    [-0.5, 1, 0.5],
    [0.5, 1, 0.5],
    [0, 1.8, 0],
    [4, 1, 0],
  ];
  const evaluate = (x: number) => {
    const mixture = radianceReceiverMixture(p.geometry, positions, [x, 1, 0]);
    expect(mixture.slice(4).reduce((sum, w) => sum + w)).toBeCloseTo(1, 8);
    expect(mixture.slice(0, 4)).not.toContain(6);
    return mixture.slice(0, 4).reduce((sum, id, i) => sum + id * mixture[4 + i], 0);
  };
  expect(Math.abs(evaluate(-0.000001) - evaluate(0.000001))).toBeLessThan(0.0001);
  const blocked = radianceReceiverMixture(p.geometry, [[4, 1, 0]], [0, 1, 0]);
  expect(blocked).toEqual([0, 0, 0, 0, 0, 0, 0, 0]);
});

test("compatible transport reuse preserves every radiometric coefficient", () => {
  const first = compile({ key: "first", center: [0, 1, 0] });
  const next = compile({ key: "next", center: [16, 1, 0], reuse: first });
  const old = new Map(first.field.positions.map((p, i) => [p.join(","), i]));
  let compared = 0;
  next.field.positions.forEach((p, i) => {
    const j = old.get(p.join(","));
    if (j === undefined) return;
    expect(next.field.transfer.subarray(i * 972, (i + 1) * 972)).toEqual(
      first.field.transfer.subarray(j * 972, (j + 1) * 972),
    );
    compared++;
  });
  expect(compared).toBeGreaterThan(0);
  expect(next.field.report.reusedSamples).toBe(compared);
  expect(next.geometry).toBe(first.geometry);
});

test("unchanged geometry reuses position-correct emission when its probe region changes", () => {
  const first = compile({ key: "emission-first", center: [0, 1, 0] }, true, true);
  const next = compile({ key: "emission-next", center: [2, 1, 0], reuse: first }, true, true);
  const fresh = compile({ key: "emission-fresh", center: [2, 1, 0] }, true, true);
  expect(next.field.report.reusedEmissionReceivers).toBeGreaterThan(0);
  expect(next.field.report.rays).toBeLessThan(fresh.field.report.rays);
  expect(next.field.receiverEmission).toEqual(fresh.field.receiverEmission);
  expect(next.field.transfer).toEqual(fresh.field.transfer);
  expect(next.field.report.reusedSurfaceSamples).toBeGreaterThan(0);
  expect(next.field.report.surfaceRays).toBeLessThan(fresh.field.report.surfaceRays ?? 0);
  expect(next.field.surfaceDiffuse).toEqual(fresh.field.surfaceDiffuse);
  const refined = [...first.meshes.values()].filter((mesh) => mesh.indices.length > 1536);
  expect(refined.length).toBeGreaterThan(0);
  expect(refined.every((mesh) => mesh.radianceFlatNormals)).toBe(true);
});
