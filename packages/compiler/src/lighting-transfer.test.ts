import { expect, test } from "bun:test";
import type { IndirectLightingField, Vec3 } from "@wrela/model";

import { indirectAlpineFixture, indirectBoxFixture } from "../../../tools/fixtures/indirect-scenes";
import { compileIndirectProbe, indirectSH } from "./indirect-probes";
import { compileIndirectGeometry, indirectNormalize } from "./indirect-query";
import {
  indirectProbeVisible,
  indirectVisibilityCellSteps,
  indirectVisibilitySteps,
} from "./indirect-visibility";

function finish<T>(steps: Generator<void, T>): T {
  let r = steps.next();
  while (!r.done) r = steps.next();
  return r.value;
}
test("compiled sky/sun transfer reproduces constant-source transport without retracing", () => {
  const f = indirectBoxFixture(),
    geometry = compileIndirectGeometry(f.surfaces);
  for (const position of [
    [0, 0.5, 0],
    [0.9, 1.7, -0.8],
    [0, 3, 0],
  ] as Vec3[]) {
    const transfer = new Float32Array(360);
    const probe = compileIndirectProbe(geometry, position, f.lighting, {
      samples: 256,
      skySamples: 16,
      transfer,
    });
    for (let band = 0; band < 9; band++)
      for (let c = 0; c < 3; c++) {
        const value =
          transfer[band * 40 + c] * (f.lighting.skyRadiance[c] / 0.2820947918) +
          transfer[band * 40 + 36 + c] * f.lighting.sunRadiance[c];
        expect(Math.abs(value - probe[band * 4 + c])).toBeLessThan(2e-7);
      }
  }
});

test("local reflections separate visible sky from colored geometry and preserve constant relighting at every bounce budget", () => {
  const fixture = indirectBoxFixture(),
    geometry = compileIndirectGeometry(fixture.surfaces);
  const average: number[] = [];
  for (const bounces of [1, 2, 3] as const) {
    const transfer = new Float32Array(360);
    const reflections = { data: new Float32Array(36), transfer: new Float32Array(360) };
    const probe = compileIndirectProbe(geometry, [0, 0.8, 0.3], fixture.lighting, {
      samples: 512,
      skySamples: 16,
      transfer,
      reflections,
      bounces,
    });
    average.push(probe[0] + probe[1] + probe[2]);
    expect(reflections.data[0]).toBeGreaterThan(0);
    expect(reflections.data[1]).toBeGreaterThan(0);
    expect(reflections.data[3]).toBeGreaterThan(0);
    for (const [coefficients, transport] of [
      [probe, transfer],
      [reflections.data, reflections.transfer],
    ])
      for (let band = 0; band < 9; band++)
        for (let c = 0; c < 3; c++) {
          const relit =
            (transport[band * 40 + c] * fixture.lighting.skyRadiance[c]) / 0.2820947918 +
            transport[band * 40 + 36 + c] * fixture.lighting.sunRadiance[c];
          expect(Math.abs(relit - coefficients[band * 4 + c])).toBeLessThan(3e-7);
        }
  }
  expect(average[1]).toBeGreaterThan(average[0]);
  expect(average[2]).toBeGreaterThan(average[1]);
  const closed = compileIndirectGeometry(indirectBoxFixture({ ceiling: true, front: true }).surfaces);
  const reflections = { data: new Float32Array(36), transfer: new Float32Array(360) };
  compileIndirectProbe(closed, [0, 1.2, 0.5], fixture.lighting, {
    samples: 256,
    skySamples: 16,
    reflections,
    bounces: 3,
  });
  expect([...reflections.data]).toEqual(new Array(36).fill(0));
  expect([...reflections.transfer]).toEqual(new Array(360).fill(0));
});

test("empty-space reflection visibility preserves a white environment in all directions", () => {
  const reflections = { data: new Float32Array(36) };
  compileIndirectProbe(
    compileIndirectGeometry([]),
    [0, 0, 0],
    { sunDirection: [0, 1, 0], skyRadiance: [1, 1, 1], sunRadiance: [0, 0, 0] },
    { samples: 1024, reflections },
  );
  for (let i = 0; i < 50; i++) {
    const y = 1 - (2 * (i + 0.5)) / 50,
      r = Math.sqrt(1 - y * y);
    const direction: Vec3 = [r * Math.cos(i * 2.4), y, r * Math.sin(i * 2.4)];
    const value = indirectSH(direction).reduce((sum, b, k) => sum + b * reflections.data[k * 4 + 3], 0);
    expect(Math.abs(value - 1)).toBeLessThan(0.0005);
  }
  for (let i = 0; i < 36; i++) if (i % 4 !== 3) expect(reflections.data[i]).toBe(0);
});
test("cell blocker lists match full BVH across receivers, normals, cell planes, and fallback budgets", () => {
  for (const fixture of [indirectBoxFixture({ ceiling: true, front: true }), indirectAlpineFixture()]) {
    const geometry = compileIndirectGeometry(fixture.surfaces);
    const origin = geometry.bounds.min.map((v) => v - 0.1) as Vec3;
    const spacing = geometry.bounds.max.map((v, a) => (v - origin[a] + 0.1) / 5) as Vec3;
    const field = {
      origin,
      spacing,
      dimensions: [6, 6, 6],
      visibility: finish(indirectVisibilitySteps(geometry, origin)),
    } as IndirectLightingField;
    const visibility = field.visibility;
    if (!visibility) throw Error("Missing compiled visibility");
    const baseline = { ...field, visibility: { ...visibility } };
    for (const limit of [1, 48]) {
      visibility.cells = finish(indirectVisibilityCellSteps(geometry, field, limit));
      for (let i = 0; i < 700; i++) {
        const q: Vec3 = [((i * 17) % 101) / 20, ((i * 37) % 101) / 20, ((i * 53) % 101) / 20];
        const n: Vec3 = [Math.sin(i * 1.7), Math.cos(i * 1.7), 0];
        const world = q.map((v, a) => origin[a] + Math.min(v, 5) * spacing[a]) as Vec3;
        const cell = q.map((v, a) =>
          Math.min(4, Math.max(0, Math.floor(v + (n[a] * Math.min(...spacing) * 0.02) / spacing[a]))),
        );
        for (let corner = 0; corner < 8; corner++) {
          const probe = cell.map((v, a) => origin[a] + (v + ((corner >> a) & 1)) * spacing[a]) as Vec3;
          expect(indirectProbeVisible(field, world, n, probe)).toBe(
            indirectProbeVisible(baseline, world, n, probe),
          );
        }
      }
    }
  }
});

test("surface-constrained regions agree with exact segments on transformed surfaces and perturbed normals", () => {
  const fixture = indirectBoxFixture({ ceiling: true, front: true });
  for (const s of fixture.surfaces) {
    s.matrix[0] = -1.7;
    s.matrix[5] = 0.8;
    s.matrix[8] = 0.2;
    s.matrix[12] = 1e7;
  }
  const geometry = compileIndirectGeometry(fixture.surfaces);
  const origin = geometry.bounds.min.map((v) => v - 0.15) as Vec3;
  const spacing = geometry.bounds.max.map((v, a) => (v - origin[a] + 0.15) / 4) as Vec3;
  const field = {
    origin,
    spacing,
    dimensions: [5, 5, 5],
    visibility: finish(indirectVisibilitySteps(geometry, origin)),
  } as IndirectLightingField;
  const visibility = field.visibility;
  if (!visibility) throw Error("Missing visibility");
  const exact = { ...field, visibility: { ...visibility } };
  visibility.cells = finish(indirectVisibilityCellSteps(geometry, field, 48, true, true));
  for (const t of geometry.triangles)
    for (let i = 0; i < 24; i++) {
      const u = i === 0 ? 0 : ((i * 17) % 97) / 100,
        v = ((1 - u) * ((i * 31) % 89)) / 100;
      const world = t.a.map((x, a) => x + t.ab[a] * u + t.ac[a] * v) as Vec3;
      const n = indirectNormalize(
        t.normal.map((x, a) => x * (i % 2 ? 1 : -1) + Math.sin(i + a) * 0.4) as Vec3,
      );
      const cell = world.map((x, a) =>
        Math.min(
          3,
          Math.max(0, Math.floor((x - origin[a] + n[a] * Math.min(...spacing) * 0.02) / spacing[a])),
        ),
      );
      for (let c = 0; c < 8; c++) {
        const probe = cell.map((x, a) => origin[a] + (x + ((c >> a) & 1)) * spacing[a]) as Vec3;
        expect(indirectProbeVisible(field, world, n, probe, true)).toBe(
          indirectProbeVisible(exact, world, n, probe),
        );
      }
    }
});
