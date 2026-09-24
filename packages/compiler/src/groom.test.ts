import { describe, expect, test } from "bun:test";
import { creatureSchema, type Vec3 } from "@wrela/model";

import { compileGroom, type GroomChartSample, type GroomLayerSource } from "./groom";
import { compileCreatureAppearance } from "./groom-appearance";
import { compileCreatureGroom } from "./groom-creature";

const layer = (overrides: Partial<GroomLayerSource> = {}): GroomLayerSource => ({
  id: "mane",
  region: "neck",
  chart: "neck-chart",
  material: "fur",
  seed: 73,
  guideCount: 100,
  density: 1,
  length: 0.2,
  width: 0.01,
  taper: 0.9,
  lift: 0.7,
  flow: [1, 0, 0],
  clump: 0.3,
  curl: 0.4,
  frizz: 0.1,
  rootColor: [0.1, 0.1, 0.1],
  tipColor: [0.5, 0.4, 0.2],
  stiffness: 35,
  damping: 6,
  ...overrides,
});
const chart =
  (width = 1, revision = 1) =>
  (id: string, coordinates: Vec3): GroomChartSample => ({
    position: [coordinates[0] * width, 0, coordinates[1]],
    normal: [0, 1, 0],
    tangent: [1, 0, 0],
    region: "neck",
    chart: id,
    chartRevision: revision,
  });

describe("compiled anchored groom", () => {
  test("proportion edits transport roots and retain guide identity at every detail", () => {
    const a = compileGroom([layer()], chart(1)),
      b = compileGroom([layer()], chart(2));
    expect(a.guides.length).toBe(100);
    expect(b.guides.map((g) => g.root)).toEqual(a.guides.map((g) => g.root));
    for (let i = 0; i < a.guides.length; i++) {
      expect(b.guides[i].points[0][0]).toBe(a.guides[i].points[0][0] * 2);
      expect(b.guides[i].points[0][2]).toBe(a.guides[i].points[0][2]);
    }
    for (const detail of a.details.slice(1)) {
      expect(detail.guideIds.every((id) => a.details[0].guideIds.includes(id))).toBe(true);
      for (const index of detail.vertexGuideIndices) expect(a.guides[index]).toBeDefined();
    }
    expect(a.details[2].guideIds.every((id) => a.details[1].guideIds.includes(id))).toBe(true);
    expect(b.key).not.toBe(a.key);
    expect(compileGroom([layer()], chart(1)).key).toBe(a.key);
  });
  test("tufts have bounded closed geometry, colors, provenance and consistent material groups", () => {
    const result = compileGroom([layer(), layer({ id: "brow", material: "brow-fur", seed: 14 })], chart(), {
      maxGuides: 25,
      maxVertices: 130,
    });
    expect(result.guides.length).toBeLessThanOrEqual(25);
    expect(result.guides.some((guide) => guide.root.layer === "mane")).toBe(true);
    expect(result.guides.some((guide) => guide.root.layer === "brow")).toBe(true);
    for (const detail of result.details) {
      expect(detail.cost.vertices).toBeLessThanOrEqual(130);
      expect(detail.mesh.positions.every(Number.isFinite)).toBe(true);
      expect(detail.mesh.normals.every(Number.isFinite)).toBe(true);
      expect(detail.mesh.indices.every((index) => index < detail.cost.vertices)).toBe(true);
      expect(detail.mesh.sourceIds?.length).toBe(detail.cost.vertices);
      expect(detail.mesh.colors?.length).toBe(detail.mesh.positions.length);
      expect(detail.mesh.materialGroups?.reduce((count, group) => count + group.count, 0)).toBe(
        detail.mesh.indices.length,
      );
    }
    expect(result.diagnostics.some((d) => d.code === "groom-guide-budget")).toBe(true);
    expect(result.diagnostics.some((d) => d.code === "groom-vertex-budget")).toBe(true);
  });
  test("invalid topology and mismatched region fail explicitly without nearest-surface reassignment", () => {
    const topology = compileGroom([layer({ chartRevision: 1 })], chart(1, 2));
    expect(topology.guides).toHaveLength(0);
    expect(topology.diagnostics.some((d) => d.code === "groom-invalid-anchor")).toBe(true);
    const missing = compileGroom([layer()], () => {
      throw new Error("missing chart");
    });
    expect(missing.guides).toHaveLength(0);
    expect(missing.details[0].mesh.bounds).toEqual({ min: [0, 0, 0], max: [0, 0, 0] });
  });
  test("growth masks suppress roots without resampling retained identities", () => {
    const original = compileGroom([layer()], chart());
    const masked = compileGroom([layer({ coverage: ([u]) => (u < 0.5 ? 0 : 1) })], chart());
    expect(masked.guides.length).toBeLessThan(original.guides.length);
    expect(masked.guides.length).toBeGreaterThan(0);
    for (const guide of masked.guides) {
      expect(guide.root.coordinates[0]).toBeGreaterThanOrEqual(0.5);
      const retained = original.guides.find((g) => g.root.id === guide.root.id);
      if (!retained) throw new Error("Missing retained root");
      expect(guide).toEqual(retained);
    }
  });
  test("color and canonical guide tips survive detail reduction", () => {
    const result = compileGroom([layer()], chart());
    for (const detail of result.details)
      for (const id of detail.guideIds) {
        const guide = result.guides.find((g) => g.root.id === id);
        if (!guide || !detail.mesh.sourceIds || !detail.mesh.colors) throw new Error("Missing groom data");
        const vertices = detail.mesh.sourceIds
          .map((source, index) => (source === id ? index : -1))
          .filter((i) => i >= 0);
        const tip = vertices[vertices.length - 2];
        for (let axis = 0; axis < 3; axis++) {
          expect(detail.mesh.positions[tip * 3 + axis]).toBeCloseTo(guide.points[8][axis], 6);
          expect(detail.mesh.colors[tip * 3 + axis]).toBeCloseTo(guide.tipColor[axis], 6);
        }
      }
  });
  test("rejects nonfinite budgets and source values", () => {
    expect(() => compileGroom([layer()], chart(), { maxVertices: NaN })).toThrow();
    expect(() => compileGroom([layer({ length: Infinity })], chart())).toThrow();
    expect(() => compileGroom([layer({ coverage: () => NaN })], chart())).toThrow();
  });
});

const creature = () =>
  creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      { id: "neck", name: "Neck", frame: { position: [0, 0, 0], rotation: [0, 0, 0] }, extent: [1, 1, 1] },
    ],
    charts: [
      {
        id: "neck-chart",
        region: "neck",
        revision: 1,
        kind: "patch",
        points: [
          [0, 0, 0],
          [1, 0, 0],
          [0, 0, 1],
          [1, 0, 1],
        ],
        thickness: 0.01,
      },
    ],
    grooms: [
      {
        id: "coat",
        region: "neck",
        chart: "neck-chart",
        chartRevision: 1,
        seed: 5,
        density: 100,
        length: 0.2,
        width: 0.01,
        direction: [1, 0, 0],
        taper: 0.7,
        clump: 0.2,
        curl: 0,
        rootColor: [0.1, 0.1, 0.1],
        tipColor: [0.5, 0.5, 0.5],
        maxCards: 200,
      },
    ],
  });
const context = {
  evaluate: chart(),
  toLocal: (_region: string, point: Vec3) => point,
  toWorld: (_region: string, point: Vec3) => point,
};
test("physical density is bounded and shared scar recipe suppresses coat in its visible region", () => {
  const source = creature(),
    a = compileCreatureGroom(source, context);
  expect(a.guides.length).toBe(100);
  source.appearance = [
    {
      id: "scar",
      region: "neck",
      family: "skin",
      color: [0.5, 0.2, 0.2],
      roughness: 0.4,
      metallic: 0,
      subsurface: 0.1,
      transmission: 0,
      anisotropy: 0,
      direction: [1, 0, 0],
      scale: 1,
      variation: 0,
      displacement: 0.01,
      growthSuppression: 1,
      mask: { center: [0.5, 0, 0.5], radius: 3, falloff: 1 },
    },
  ];
  const b = compileCreatureGroom(source, context);
  expect(b.guides.length).toBeLessThan(a.guides.length / 5);
  const appearance = compileCreatureAppearance(source, context.toWorld);
  expect(appearance.sample("neck", [0.5, 0, 0.5]).growthCoverage).toBe(0);
  expect(appearance.sample("neck", [0.5, 0, 0.5]).color).toEqual([0.5, 0.2, 0.2]);
  expect(appearance.materials[0].creature?.family).toBe("skin");
  expect(appearance.sample("other", [0.5, 0, 0.5]).growthCoverage).toBe(1);
});
test("authored guide curve is transported onto each root and resampled by arc length", () => {
  const source = creature();
  source.grooms[0].guides = [
    {
      id: "hero-flow",
      points: [
        [0, 0, 0],
        [0, 0.1, 0],
        [0.1, 0.1, 0],
      ],
    },
  ];
  const groom = compileCreatureGroom(source, context);
  for (const guide of groom.guides) {
    expect(guide.points[8][0] - guide.points[0][0]).toBeCloseTo(0.1);
    expect(guide.points[8][1] - guide.points[0][1]).toBeCloseTo(0.1);
  }
});
test("ambiguous implicit chart selection is rejected", () => {
  const source = creature();
  source.grooms[0].chart = undefined;
  source.charts.push({ ...source.charts[0], id: "extra-chart" });
  const groom = compileCreatureGroom(source, context);
  expect(groom.guides).toHaveLength(0);
  expect(groom.diagnostics[0].code).toBe("groom-chart-ambiguous");
});

test("every authored detail fraction compiles with nested persistent identities and distinct keys", () => {
  const fractions = [1, 0.8, 0.6, 0.4, 0.2, 0];
  const result = compileGroom([layer({ lodFractions: fractions })], chart());
  expect(result.details.map((detail) => detail.label)).toEqual([
    "hero",
    "gameplay",
    "distant",
    "detail-3",
    "detail-4",
    "detail-5",
  ]);
  expect(result.details[5].guideIds).toHaveLength(0);
  for (let i = 1; i < result.details.length; i++)
    expect(result.details[i].guideIds.every((id) => result.details[i - 1].guideIds.includes(id))).toBe(true);
  const changed = compileGroom([layer({ lodFractions: [1, 0.7, 0.5, 0.3, 0.1, 0] })], chart());
  expect(result.key).not.toBe(changed.key);
  expect(compileGroom([layer({ lodFractions: [1] })], chart()).details).toHaveLength(1);
});

test("nonempty groom details retain layer support tips with explicitly scoped fidelity", () => {
  const result = compileGroom([layer({ lodFractions: [1, 0.15, 0.01], guideCount: 120 })], chart());
  for (const detail of result.details) {
    expect(detail.fidelity?.scope).toBe("rest-guide-envelope");
    expect(detail.fidelity?.tipBoundsError).toEqual([0, 0, 0]);
    expect(detail.fidelity?.coverageError).toBeNull();
    expect(detail.fidelity?.drawGroups).toBe(detail.mesh.materialGroups?.length);
  }
  expect(result.details[2].cost.triangles).toBeLessThan(result.details[0].cost.triangles / 5);
});

test("closed opaque ribbons share tuft roots and canonical guides while exposing their approximation", () => {
  const tuft = compileGroom([layer()], chart());
  const ribbon = compileGroom([layer({ representation: "ribbons", ribbonThickness: 0.08 })], chart());
  expect(ribbon.representation).toBe("opaque-ribbons");
  expect(ribbon.guides.map((guide) => guide.root)).toEqual(tuft.guides.map((guide) => guide.root));
  expect(ribbon.guides.map((guide) => guide.points)).toEqual(tuft.guides.map((guide) => guide.points));
  for (let index = 0; index < ribbon.details.length; index++) {
    const detail = ribbon.details[index];
    expect(detail.guideIds).toEqual(tuft.details[index].guideIds);
    expect(detail.cost.triangles).toBeLessThan(tuft.details[index].cost.triangles);
    expect(detail.cost.vertices).toBeLessThan(tuft.details[index].cost.vertices);
    expect(detail.fidelity?.realizations).toEqual(["ribbons"]);
    expect(detail.fidelity?.maxGuideInterpolationError).toBeGreaterThan(0);
    expect(detail.fidelity?.coverageError).toBeNull();
    expect(detail.mesh.positions.every(Number.isFinite)).toBe(true);
    expect(detail.mesh.normals.every(Number.isFinite)).toBe(true);
    expect(detail.vertexGuideIndices.length).toBe(detail.cost.vertices);
    const edges = new Map<string, number>();
    for (let triangle = 0; triangle < detail.mesh.indices.length; triangle += 3)
      for (let side = 0; side < 3; side++) {
        const a = detail.mesh.indices[triangle + side],
          b = detail.mesh.indices[triangle + ((side + 1) % 3)];
        const key = a < b ? `${a}-${b}` : `${b}-${a}`;
        edges.set(key, (edges.get(key) ?? 0) + 1);
      }
    expect([...edges.values()].every((count) => count === 2)).toBe(true);
  }
});

test("ribbon thickness follows its persistent chart flow frame and mixed layers retain their realization", () => {
  const result = compileGroom(
    [
      layer({
        id: "thin",
        guideCount: 1,
        representation: "ribbons",
        ribbonThickness: 0.08,
        width: 0.1,
        clump: 0,
        curl: 0,
        frizz: 0,
        lift: 0,
        flow: [1, 0, 0],
      }),
      layer({ id: "mane", guideCount: 1 }),
    ],
    chart(),
  );
  expect(result.representation).toBe("mixed-opaque");
  const detail = result.details[0],
    root = result.guides[0].points[0];
  for (let vertex = 0; vertex < 4; vertex++) {
    expect(Math.abs(detail.mesh.positions[vertex * 3 + 1] - root[1])).toBeCloseTo(0.1 * 0.5 * 0.08, 6);
    expect(Math.abs(detail.mesh.positions[vertex * 3 + 2] - root[2])).toBeCloseTo(0.1 * 0.5, 6);
  }
  expect(detail.fidelity?.realizations).toEqual(["ribbons", "tufts"]);
  expect(() => compileGroom([layer({ ribbonThickness: 0 })], chart())).toThrow();
});

test("explicit root projection moves the entire guide coherently while chart identity remains stable", () => {
  const source = creature(),
    baseline = compileCreatureGroom(source, context);
  source.grooms[0].rootProjection = { maxDistance: 0.3, direction: "outward", nodeIds: ["skin-surface"] };
  let calls = 0;
  const projected = compileCreatureGroom(source, {
    ...context,
    projectRoot: (_groom, sample) => {
      calls++;
      return {
        status: "resolved",
        position: [sample.position[0], 0.2, sample.position[2]],
        normal: [0, 1, 0],
        distance: 0.2,
        sourceNode: "skin-surface",
      };
    },
  });
  expect(calls).toBe(baseline.guides.length);
  expect(projected.guides.map((guide) => guide.root)).toEqual(baseline.guides.map((guide) => guide.root));
  for (let guide = 0; guide < projected.guides.length; guide++) {
    expect(projected.guides[guide].rootProjection?.distance).toBe(0.2);
    expect(projected.guides[guide].rootProjection?.domain).toBe("compiled-body");
    for (let point = 0; point < 9; point++) {
      expect(projected.guides[guide].points[point][0]).toBeCloseTo(
        baseline.guides[guide].points[point][0],
        7,
      );
      expect(projected.guides[guide].points[point][1] - baseline.guides[guide].points[point][1]).toBeCloseTo(
        0.2,
        7,
      );
      expect(projected.guides[guide].points[point][2]).toBeCloseTo(
        baseline.guides[guide].points[point][2],
        7,
      );
    }
  }
});

test("projection failures do not attach roots to nearby unapproved anatomy or bypass physical bounds", () => {
  const source = creature();
  source.grooms[0].rootProjection = { maxDistance: 0.3, direction: "outward" };
  const missing = compileCreatureGroom(source, context);
  expect(missing.guides).toHaveLength(0);
  expect(missing.diagnostics.some((d) => d.code === "groom-projection-no-hit")).toBe(true);
  const wrong = compileCreatureGroom(source, {
    ...context,
    projectRoot: () => ({ status: "wrong-region", reason: "A different limb blocks this normal" }),
  });
  expect(wrong.guides).toHaveLength(0);
  expect(wrong.diagnostics.some((d) => d.code === "groom-projection-wrong-region")).toBe(true);
  expect(wrong.diagnostics.some((d) => d.code === "groom-invalid-anchor")).toBe(false);
  expect(wrong.diagnostics.some((d) => d.severity === "error")).toBe(false);
  const excessive = compileCreatureGroom(source, {
    ...context,
    projectRoot: (_groom, sample) => ({
      status: "resolved",
      position: [sample.position[0], 3, sample.position[2]],
      normal: [0, 1, 0],
      distance: 0.1,
    }),
  });
  expect(excessive.guides).toHaveLength(0);
  expect(excessive.diagnostics.some((d) => d.code === "groom-projection-invalid-result")).toBe(true);
});

test("rest-space comb direction does not rotate around a curved growth chart", () => {
  const result = compileGroom(
    [layer({ guideCount: 2, direction: [0, 0, 1], lift: 0, clump: 0, curl: 0, frizz: 0 })],
    (id, coordinates) => ({
      ...chart()(id, coordinates),
      tangent: coordinates[0] > 0.5 ? [1, 0, 0] : [0, 0, -1],
    }),
  );
  expect(result.guides).toHaveLength(2);
  for (const guide of result.guides) {
    expect(guide.points[8][0]).toBeCloseTo(guide.points[0][0]);
    expect(guide.points[8][1]).toBeCloseTo(guide.points[0][1]);
    expect(guide.points[8][2]).toBeGreaterThan(guide.points[0][2]);
  }
});
