import { expect, test } from "bun:test";
import { creatureSchema, type Vec3 } from "@wrela/model";

import type { CreatureGeometry } from "./creature";
import { compileCreatureAppearance, creatureAppearanceCoverage } from "./groom-appearance";
import { bindCreatureAppearanceFields } from "./groom-appearance-binding";

const transform = (_region: string, point: Vec3) => point;
const source = () =>
  creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      { id: "skin", name: "Skin", extent: [1, 1, 1], frame: { position: [0, 0, 0], rotation: [0, 0, 0] } },
    ],
    appearance: [
      {
        id: "base",
        region: "skin",
        family: "skin",
        color: [0.2, 0.3, 0.4],
        roughness: 0.9,
        subsurface: 0.05,
      },
      {
        id: "scar",
        region: "skin",
        family: "skin",
        color: [0.8, 0.2, 0.1],
        roughness: 0.3,
        subsurface: 0.3,
        growthSuppression: 1,
        mask: { center: [0, 0, 0], radius: 1, falloff: 1 },
      },
    ],
  });
function grid(): CreatureGeometry {
  const positions: number[] = [],
    normals: number[] = [],
    indices: number[] = [];
  for (let x = 0; x <= 20; x++)
    for (const z of [-0.01, 0.01]) {
      positions.push(x / 20, 0, z);
      normals.push(0, 1, 0);
    }
  for (let x = 0; x < 20; x++) {
    const a = x * 2;
    indices.push(a, a + 2, a + 1, a + 1, a + 2, a + 3);
  }
  return {
    key: "grid",
    regions: Array(42).fill("skin"),
    coordinates: Array(42).fill(null),
    diagnostics: [],
    mesh: {
      positions: new Float32Array(positions),
      normals: new Float32Array(normals),
      indices: new Uint32Array(indices),
      colors: new Float32Array(positions.length).fill(0.5),
      sourceIds: Array(42).fill("skin"),
      bounds: { min: [0, 0, -0.01], max: [1, 0, 0.01] },
    },
  };
}

test("scar albedo is continuous and composed once, while roughness retains masked variation", () => {
  const creature = source(),
    geometry = grid(),
    appearance = compileCreatureAppearance(creature, transform);
  const compiled = bindCreatureAppearanceFields(creature, geometry, "external-base", transform, transform);
  const materials = new Map(compiled.materials.map((material) => [material.id, material]));
  expect(compiled.geometry.mesh.materialGroups?.length).toBeGreaterThan(5);
  for (let vertex = 0; vertex < 42; vertex++) {
    const local: Vec3 = [geometry.mesh.positions[vertex * 3], 0, geometry.mesh.positions[vertex * 3 + 2]];
    const sample = appearance.sample("skin", local);
    for (let axis = 0; axis < 3; axis++)
      expect(compiled.geometry.mesh.colors?.[vertex * 3 + axis]).toBeCloseTo(sample.color[axis], 6);
    expect(sample.growthCoverage).toBeCloseTo(
      1 - creatureAppearanceCoverage(creature.appearance[1], local),
      10,
    );
    if (vertex >= 2)
      expect(
        Math.abs(
          (compiled.geometry.mesh.colors?.[vertex * 3] ?? 0) -
            (compiled.geometry.mesh.colors?.[(vertex - 2) * 3] ?? 0),
        ),
      ).toBeLessThan(0.05);
  }
  for (const group of compiled.geometry.mesh.materialGroups ?? []) {
    const material = materials.get(group.material);
    if (!material) throw new Error("Missing generated material");
    expect(material.color).toEqual([1, 1, 1]);
    for (let t = group.start; t < group.start + group.count; t += 3) {
      const centroid: Vec3 = [0, 0, 0];
      for (let corner = 0; corner < 3; corner++)
        for (let axis = 0; axis < 3; axis++)
          centroid[axis] +=
            compiled.geometry.mesh.positions[compiled.geometry.mesh.indices[t + corner] * 3 + axis] / 3;
      expect(
        Math.abs(material.roughness - appearance.sample("skin", centroid).roughness),
      ).toBeLessThanOrEqual(1 / 64 + 1e-8);
    }
  }
});

test("seam duplication preserves untouched source material colors", () => {
  const geometry = grid();
  geometry.regions[2] = null;
  const original = geometry.mesh.colors?.slice();
  const compiled = bindCreatureAppearanceFields(source(), geometry, "original", transform, transform);
  expect(compiled.geometry.mesh.positions.length).toBeGreaterThan(geometry.mesh.positions.length);
  const group = compiled.geometry.mesh.materialGroups?.find((entry) => entry.material === "original");
  if (!group) throw new Error("Missing original material group");
  for (let index = group.start; index < group.start + group.count; index++) {
    const vertex = compiled.geometry.mesh.indices[index];
    expect(compiled.geometry.mesh.colors?.[vertex * 3]).toBe(original?.[vertex * 3]);
  }
});

test("cloth and mounted components retain independent materials inside skin anatomy", () => {
  const creature = creatureSchema.parse({
    ...source(),
    cloth: [
      {
        id: "wool-panel",
        region: "skin",
        chart: "wool-chart",
        chartRevision: 1,
        material: "ochre-wool",
        pinEdges: ["v0"],
      },
    ],
    attachments: [{ id: "brooch", anchor: "mount", nodeIds: ["bronze-brooch"] }],
  });
  for (const kind of ["cloth", "attachment"] as const) {
    const geometry = grid();
    geometry.mesh.materialGroups = [
      { material: kind === "cloth" ? "ochre-wool" : "bronze", start: 0, count: geometry.mesh.indices.length },
    ];
    if (kind === "cloth")
      geometry.coordinates = geometry.coordinates.map(() => ({
        region: "skin",
        chart: "wool-chart",
        chartRevision: 1,
        coordinates: [0.5, 0.5, 0],
      }));
    else geometry.mesh.sourceIds = geometry.mesh.sourceIds?.map(() => "bronze-brooch");
    const compiled = bindCreatureAppearanceFields(creature, geometry, "skin", transform, transform);
    expect(compiled.geometry.mesh.materialGroups).toEqual(geometry.mesh.materialGroups);
    expect(compiled.geometry.mesh.colors).toEqual(geometry.mesh.colors);
  }
});

test("anchored appearance and growth masks transport together and reject unresolved anchors", () => {
  const creature = source();
  creature.appearance[1].anchor = "moving-scar";
  const a = compileCreatureAppearance(creature, transform, () => [0, 0, 0]),
    b = compileCreatureAppearance(creature, transform, () => [2, 0, 0]);
  expect(a.sample("skin", [0, 0, 0]).color).toEqual(b.sample("skin", [2, 0, 0]).color);
  expect(a.sample("skin", [0, 0, 0]).growthCoverage).toBe(0);
  expect(b.sample("skin", [0, 0, 0]).growthCoverage).toBe(1);
  expect(compileCreatureAppearance(creature, transform).sample("skin", [0, 0, 0]).growthCoverage).toBe(1);
});

test("missing underlying material and cross-family interpolation are reported honestly", () => {
  const creature = source();
  creature.appearance.splice(0, 1);
  const result = bindCreatureAppearanceFields(creature, grid(), "unknown", transform, transform);
  expect(result.geometry.diagnostics.some((d) => d.code === "appearance-base-required")).toBe(true);
  expect(result.geometry.mesh.materialGroups?.[0].material).toBe("unknown");
  const mixed = source();
  mixed.appearance[0].family = "hair";
  expect(
    bindCreatureAppearanceFields(mixed, grid(), "unknown", transform, transform).geometry.diagnostics.some(
      (d) => d.code === "appearance-family-mixture",
    ),
  ).toBe(true);
});

test("correlated source fields share a small response palette rather than a Cartesian product of bins", () => {
  const creature = source();
  creature.appearance[0].roughness = 0.78;
  creature.appearance[0].anisotropy = 0.65;
  creature.appearance[0].scale = 24;
  creature.appearance[1].roughness = 0.58;
  creature.appearance[1].anisotropy = 0;
  creature.appearance[1].subsurface = 0.15;
  creature.appearance[1].scale = 12;
  const result = bindCreatureAppearanceFields(creature, grid(), "base", transform, transform);
  const bins = result.materials.filter((material) => material.id.startsWith("creature-blend-"));
  expect(bins.length).toBeLessThanOrEqual(8);
  expect(result.geometry.mesh.materialGroups?.length).toBe(bins.length);
});

test("large material buckets assemble without an unbounded function argument list", () => {
  const creature = source();
  creature.appearance = creature.appearance.slice(0, 1);
  const geometry = grid();
  geometry.mesh.indices = new Uint32Array(150_000);
  for (let i = 0; i < geometry.mesh.indices.length; i++) geometry.mesh.indices[i] = i % 3;
  const result = bindCreatureAppearanceFields(creature, geometry, "base", transform, transform);
  expect(result.geometry.mesh.indices.length).toBe(150_000);
  expect(result.geometry.mesh.materialGroups).toHaveLength(1);
  expect(result.geometry.mesh.materialGroups?.[0].count).toBe(150_000);
  expect(result.geometry.mesh.indices.slice(-3)).toEqual(new Uint32Array([0, 1, 2]));
});
