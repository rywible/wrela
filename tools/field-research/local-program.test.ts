import { describe, expect, test } from "bun:test";
import { compileField } from "@wrela/compiler";
import {
  type Bounds,
  type FieldDefinition,
  type FieldNode,
  referenceProject,
  shape,
  type Vec3,
} from "@wrela/model";
import { compileGauge, gaugeValue } from "./gauge";
import { buildAtlas, jet, lower, quadricHit, sample, smoothMin, specialize, value } from "./local-program";

let seed = 812731;
function random() {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return seed / 4294967296;
}
function point(bounds: Bounds): Vec3 {
  return bounds.min.map((v, i) => v + random() * (bounds.max[i] - v)) as Vec3;
}
function field(nodes: FieldNode[], root = nodes[0].id): FieldDefinition {
  return { nodes, root, bounds: { min: [-3, -3, -3], max: [3, 3, 3] }, resolution: 24 };
}

describe("regional field research", () => {
  test("range bounds enclose values, including thin ellipsoids and their singular centers", () => {
    for (const kind of ["sphere", "ellipsoid", "box", "capsule", "torus"] as const) {
      for (let trial = 0; trial < 100; trial++) {
        const node = {
          ...shape(
            "p",
            "p",
            [0, 0, 0],
            [0.0001 + random() * 2, 0.0001 + random() * 2, 0.0001 + random() * 2],
            kind,
          ),
          rotation: [random(), random(), random()] as Vec3,
        };
        const f = field([node]),
          expression = lower(f),
          original = compileField(f);
        const c: Vec3 =
          trial % 5 === 0 ? [0, 0, 0] : [random() * 3 - 1.5, random() * 3 - 1.5, random() * 3 - 1.5];
        const radius = 0.001 + random();
        const bounds: Bounds = {
          min: c.map((v) => v - radius) as Vec3,
          max: c.map((v) => v + radius) as Vec3,
        };
        const reduced = specialize(expression, bounds);
        for (let i = 0; i < 60; i++) {
          const p = i === 0 ? c : point(bounds),
            d = original.distance(p);
          expect(d).toBeGreaterThanOrEqual(reduced.range.lo);
          expect(d).toBeLessThanOrEqual(reduced.range.hi);
          expect(Math.abs(value(expression, p) - d)).toBeLessThan(1e-8 * (1 + Math.abs(d)));
        }
      }
    }
  });
  test("preserves ordered blends, subtraction, transforms, material overrides, and ties", () => {
    const nodes: FieldNode[] = [
      {
        ...shape("root", "root", [0.1, 0.2, -0.1], [1, 1, 1], "union"),
        children: ["blend", "cut"],
        rotation: [0.1, -0.3, 0.4],
      },
      {
        ...shape("blend", "blend", [0, 0, 0], [1, 1, 1], "smoothUnion"),
        children: ["a", "b", "c"],
        blend: 0.25,
        material: "override",
      },
      { ...shape("cut", "cut", [0, 0.2, 0], [1, 1, 1], "subtract"), children: ["d", "e"] },
      { ...shape("a", "a", [-0.6, 0, 0], [0.7, 0.3, 0.6]), material: "a-material" },
      shape("b", "b", [0, 0.1, 0], [0.5, 0.6, 0.4]),
      shape("c", "c", [0.6, 0, 0], [0.7, 0.3, 0.5]),
      shape("d", "d", [0, -0.7, 0], [1, 0.8, 1], "box"),
      shape("e", "e", [0, -0.5, 0.1], [0.3, 0.6, 0.5], "capsule"),
    ];
    const f = field(nodes),
      original = compileField(f),
      atlas = buildAtlas(f, 4);
    for (let i = 0; i < 12000; i++) {
      const p = point(f.bounds),
        actual = sample(atlas.at(p).expression, p),
        expected = original.sample(p);
      expect(Math.abs(actual.distance - expected.distance)).toBeLessThan(1e-10);
      expect(actual.source).toBe(expected.source);
      expect(actual.material).toBe(expected.material);
    }
    const tied = field([
      { ...shape("root", "root", [0, 0, 0], [1, 1, 1], "union"), children: ["a", "b"] },
      shape("a", "a", [0, 0, 0], [1, 1, 1]),
      shape("b", "b", [0, 0, 0], [1, 1, 1]),
    ]);
    expect(sample(specialize(lower(tied), tied.bounds).expression, [1, 0, 0]).source).toBe("a");
  });
  test("automatic gradients and Hessians agree with independent finite differences", () => {
    const f = field([
      {
        ...shape("root", "root", [0.2, 0, 0], [1, 1, 1], "smoothUnion"),
        children: ["a", "b"],
        blend: 0.5,
        rotation: [0.2, 0.3, 0.4],
      },
      shape("a", "a", [-0.3, 0, 0], [0.8, 0.5, 0.7]),
      shape("b", "b", [0.3, 0, 0], [0.8, 0.5, 0.7]),
    ]);
    const expression = lower(f),
      program = compileField(f),
      p: Vec3 = [0.23, 0.47, 0.11],
      j = jet(expression, p),
      e = 1e-4;
    for (let i = 0; i < 3; i++) {
      const a = [...p] as Vec3,
        b = [...p] as Vec3;
      a[i] += e;
      b[i] -= e;
      expect(Math.abs(j.g[i] - (program.distance(a) - program.distance(b)) / (2 * e))).toBeLessThan(1e-6);
      const ga = jet(expression, a).g,
        gb = jet(expression, b).g;
      for (let k = 0; k < 3; k++)
        expect(Math.abs(j.h[k * 3 + i] - (ga[k] - gb[k]) / (2 * e))).toBeLessThan(1e-5);
    }
  });
  test("canonical ellipsoid roots are exact and gauge approximations keep their zero sets", () => {
    const f = field([shape("a", "a", [0, 0, 0], [0.4, 1.2, 0.7])]),
      expr = lower(f),
      original = compileField(f);
    const bounds: Bounds = { min: [0.25, -0.1, -0.1], max: [0.55, 0.1, 0.1] };
    const local = specialize(expr, bounds),
      hits = quadricHit(local, [2, 0.02, 0.03], [-1, 0, 0]);
    expect(hits.length).toBe(2);
    for (const t of hits) expect(Math.abs(original.distance([2 - t, 0.02, 0.03]))).toBeLessThan(1e-12);
    for (const order of [0, 1, 2] as const) {
      const g = compileGauge(expr, bounds, order);
      expect(Math.abs(gaugeValue(g, [0.4, 0, 0]))).toBeLessThan(1e-14);
      expect(gaugeValue(g, [0.35, 0, 0])).toBeLessThan(0);
      expect(gaugeValue(g, [0.45, 0, 0])).toBeGreaterThan(0);
    }
  });
  test("smooth union is nonexpansive in the maximum leaf error", () => {
    for (let i = 0; i < 10000; i++) {
      const a = random() * 4 - 2,
        b = random() * 4 - 2,
        da = random() * 0.2 - 0.1,
        db = random() * 0.2 - 0.1,
        k = random() + 0.01;
      expect(Math.abs(smoothMin(a + da, b + db, k) - smoothMin(a, b, k))).toBeLessThanOrEqual(
        Math.max(Math.abs(da), Math.abs(db)) + 1e-14,
      );
    }
  });
  test("reference bunny atlas preserves field values and source identity", () => {
    const doc = referenceProject().documents.find((d) => d.kind === "character");
    if (doc?.kind !== "character") throw new Error("Missing bunny");
    const original = compileField(doc.field),
      atlas = buildAtlas(doc.field);
    for (let i = 0; i < 8000; i++) {
      const p = point(doc.field.bounds),
        actual = sample(atlas.at(p).expression, p),
        expected = original.sample(p);
      expect(Math.abs(actual.distance - expected.distance)).toBeLessThan(1e-9);
      expect(actual.source).toBe(expected.source);
    }
    expect(() => atlas.at([100, 0, 0])).toThrow();
  });
});
