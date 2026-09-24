import { describe, expect, test } from "bun:test";
import { type CreatureDefinition, creatureSchema, type Joint } from "@wrela/model";

import {
  applyCreatureCorrectives,
  bindCreatureGeometry,
  clearCreatureCompilerCache,
  compileCreatureCorrectives,
  compileCreatureGeometry,
  creatureCompilerCacheMetrics,
  creatureGeometryProductKeys,
  creatureRegionLocalPoint,
  creatureRegionPoint,
  evaluateCreatureChart,
  projectCreatureAnchor,
  resolveCreatureAnchor,
} from "./creature";

function fixture(): CreatureDefinition {
  return creatureSchema.parse({
    schemaVersion: 1,
    regions: [
      {
        id: "arm",
        name: "Arm",
        frame: { position: [0, 0, 0], rotation: [0, 0, 0] },
        extent: [1, 2, 1],
        jointIds: ["armJoint"],
      },
    ],
    charts: [
      {
        id: "limb",
        region: "arm",
        revision: 1,
        kind: "sweep",
        points: [
          [0, 0, 0],
          [0, 2, 0],
        ],
        radii: [0.2, 0.1],
        crossSections: [
          [0.2, 0.3],
          [0.1, 0.15],
        ],
      },
    ],
    anchors: [
      {
        id: "scar",
        region: "arm",
        chart: "limb",
        chartRevision: 1,
        coordinates: [0.5, 0, 1],
        offset: 0.01,
        purpose: "scar",
        tolerance: 0.01,
      },
    ],
  });
}
function joint(id: string, position: [number, number, number]): Joint {
  return {
    id,
    name: id,
    parent: null,
    position,
    rotation: [0, 0, 0],
    radius: 2,
    minimum: -Math.PI,
    maximum: Math.PI,
  };
}

describe("creature anatomical charts", () => {
  test("correspondence-only support charts retain anchors without adding overlapping visible geometry", () => {
    const source = fixture();
    const surface = compileCreatureGeometry(source, "interactive");
    const anchor = resolveCreatureAnchor(source, source.anchors[0]);
    source.charts[0].realization = "correspondence-only";
    const support = compileCreatureGeometry(source, "interactive");
    expect(surface.mesh.positions.length).toBeGreaterThan(0);
    expect(support.mesh.positions.length).toBe(0);
    expect(support.mesh.indices.length).toBe(0);
    expect(support.coordinates).toEqual([]);
    expect(support.key).not.toBe(surface.key);
    expect(resolveCreatureAnchor(source, source.anchors[0])).toEqual(anchor);
    expect(evaluateCreatureChart(source, "limb", [0.5, 0, 1]).position).toBeDefined();
  });

  test("proportion edits transport anchored details independently of tessellation", () => {
    const source = fixture(),
      before = resolveCreatureAnchor(source, source.anchors[0]);
    const chart = source.charts[0];
    if (chart.kind !== "sweep") throw new Error("fixture");
    chart.radii = chart.radii.map((v) => v * 2);
    chart.crossSections = chart.crossSections?.map(([x, y]) => [x * 2, y * 2]);
    const after = resolveCreatureAnchor(source, source.anchors[0]);
    expect(before.status).toBe("resolved");
    expect(after.status).toBe("resolved");
    if (!before.position || !after.position) throw new Error("Unresolved fixture anchor");
    expect(after.position[1]).toBeCloseTo(before.position[1], 3);
    expect(Math.abs(after.position[0])).toBeGreaterThan(Math.abs(before.position[0]));
    const low = compileCreatureGeometry(source, "interactive"),
      high = compileCreatureGeometry(source, "export");
    expect(high.mesh.positions.length).toBeGreaterThan(low.mesh.positions.length);
    expect(resolveCreatureAnchor(source, source.anchors[0])).toEqual(after);
    expect(high.coordinates.every((c) => c?.chartRevision === 1)).toBe(true);
  });
  test("topology changes and deleted regions require repair instead of nearest-surface reassignment", () => {
    const source = fixture();
    source.charts[0].revision++;
    expect(resolveCreatureAnchor(source, source.anchors[0]).diagnostics[0].code).toBe(
      "creature.anchor.topology",
    );
    source.regions = [];
    expect(resolveCreatureAnchor(source, source.anchors[0]).status).toBe("invalid");
  });
  test("bounded projection refuses competing anatomical sheets", () => {
    const source = fixture();
    source.charts.push({ ...structuredClone(source.charts[0]), id: "duplicate" });
    const p = evaluateCreatureChart(source, "limb", [0.5, 0, 1]).position;
    expect(projectCreatureAnchor(source, "arm", p, 0.01, 8).status).toBe("ambiguous");
    expect(projectCreatureAnchor(source, "leg", p, 0.01, 8).status).toBe("invalid");
  });
  test("projection distinguishes repeated sheets inside one chart without confusing periodic seams", () => {
    const source = fixture();
    const seam = evaluateCreatureChart(source, "limb", [0.5, 0, 1]).position;
    expect(projectCreatureAnchor(source, "arm", seam, 0.01, 16).status).toBe("resolved");
    const chart = source.charts[0];
    if (chart.kind !== "sweep") throw new Error("fixture");
    chart.points = [
      [0, 0, 0],
      [0, 1, 0],
      [1, 1, 0],
      [1, 0, 0],
      [0, 0, 0],
      [0, 1, 0],
      [1, 1, 0],
      [1, 0, 0],
    ];
    chart.radii = chart.points.map(() => 0.1);
    chart.crossSections = undefined;
    const repeated = evaluateCreatureChart(source, "limb", [1.5 / 7, 0, 1]).position;
    expect(projectCreatureAnchor(source, "arm", repeated, 0.01, 28).status).toBe("ambiguous");
  });
  test("thin patches retain thickness below any legacy extraction cell and have outward winding", () => {
    const source = fixture();
    source.charts = [
      {
        id: "ear",
        region: "arm",
        revision: 0,
        kind: "patch",
        points: [
          [0, 0, 0],
          [1, 0, 0],
          [0, 1, 0],
          [1, 1, 0],
        ],
        thickness: 0.0002,
      },
    ];
    const product = compileCreatureGeometry(source, "interactive");
    expect(product.mesh.bounds.max[2] - product.mesh.bounds.min[2]).toBeCloseTo(0.0002, 7);
    expect(product.mesh.indices.length).toBeGreaterThan(0);
    for (let i = 0; i < product.mesh.positions.length; i++)
      expect(Number.isFinite(product.mesh.positions[i])).toBe(true);
    let volume = 0;
    const p = product.mesh.positions;
    for (let i = 0; i < product.mesh.indices.length; i += 3) {
      const a = product.mesh.indices[i] * 3,
        b = product.mesh.indices[i + 1] * 3,
        c = product.mesh.indices[i + 2] * 3;
      volume +=
        (p[a] * (p[b + 1] * p[c + 2] - p[b + 2] * p[c + 1]) +
          p[a + 1] * (p[b + 2] * p[c] - p[b] * p[c + 2]) +
          p[a + 2] * (p[b] * p[c + 1] - p[b + 1] * p[c])) /
        6;
    }
    expect(volume).toBeCloseTo(0.0002, 7);
  });
  test("sculpt support is local and normals follow the authored displacement", () => {
    const source = fixture(),
      before = evaluateCreatureChart(source, "limb", [0.5, 0, 1]);
    source.sculpts.push({
      id: "scar",
      region: "arm",
      center: before.position,
      radius: 0.3,
      displacement: [-0.05, 0, 0],
      strength: 1,
      falloff: 2,
      mirror: false,
    });
    const after = evaluateCreatureChart(source, "limb", [0.5, 0, 1]);
    expect(after.position[0]).toBeCloseTo(before.position[0] - 0.05, 4);
    const other = fixture();
    expect(evaluateCreatureChart(source, "limb", [0, 0, 1]).position).toEqual(
      evaluateCreatureChart(other, "limb", [0, 0, 1]).position,
    );
  });
  test("rotated local metre coordinates roundtrip", () => {
    const source = fixture();
    source.regions[0].frame = { position: [4, 5, 6], rotation: [0.2, 0.7, -0.5] };
    const p: [number, number, number] = [0.4, -0.2, 0.8],
      result = creatureRegionLocalPoint(source, "arm", creatureRegionPoint(source, "arm", p));
    for (let i = 0; i < 3; i++) expect(result[i]).toBeCloseTo(p[i], 8);
  });
});

describe("creature binding and dependency products", () => {
  test("excluded neighboring joints never influence a region; weights normalize", () => {
    const source = fixture(),
      geometry = compileCreatureGeometry(source, "interactive"),
      joints = [joint("armJoint", [0, 8, 0]), joint("torso", [0, 1, 0])];
    source.influenceRules.push({
      id: "arm-only",
      region: "arm",
      allowedJoints: ["armJoint"],
      excludedJoints: ["torso"],
    });
    const result = bindCreatureGeometry(joints, source, geometry);
    for (let i = 0; i < geometry.regions.length; i++) {
      expect(result.jointIndices[i * 4]).toBe(0);
      expect(Array.from(result.weights.slice(i * 4, i * 4 + 4)).reduce((a, b) => a + b, 0)).toBeCloseTo(1, 6);
    }
    source.influenceRules[0].excludedJoints.push("armJoint");
    expect(() => bindCreatureGeometry(joints, source, geometry)).toThrow("No permitted influence");
  });
  test("corrective support is anatomical, directional and rest-preserving", () => {
    const source = fixture();
    source.correctives.push({
      id: "bulge",
      region: "arm",
      joint: "armJoint",
      axis: "x",
      angle: 1,
      radius: 3,
      center: [0, 1, 0],
      displacement: [0.1, 0, 0],
    });
    const geometry = compileCreatureGeometry(source, "interactive"),
      products = compileCreatureCorrectives(source, geometry),
      rest = geometry.mesh.positions.slice();
    expect(products[0].vertices.length).toBeGreaterThan(0);
    const posed = applyCreatureCorrectives(geometry.mesh, products, { armJoint: [1, 0, 0] });
    expect(posed.positions[0]).toBeGreaterThan(rest[0]);
    expect(geometry.mesh.positions).toEqual(rest);
    expect(applyCreatureCorrectives(geometry.mesh, products, { armJoint: [-1, 0, 0] }).positions).toEqual(
      rest,
    );
  });
  test("material, pose, anchor edits do not invalidate shape; geometry edits do", () => {
    const source = fixture(),
      a = creatureGeometryProductKeys(source);
    source.charts[0].material = "skin";
    source.anchors[0].offset += 0.02;
    const b = creatureGeometryProductKeys(source);
    expect(b.geometry).toBe(a.geometry);
    expect(b.material).not.toBe(a.material);
    expect(b.correspondence).not.toBe(a.correspondence);
    const c = source.charts[0];
    if (c.kind !== "sweep") throw new Error("fixture");
    c.points[1][1] += 1;
    expect(creatureGeometryProductKeys(source).geometry).not.toBe(a.geometry);
  });
  test("cached products cannot be mutated through consumers", () => {
    clearCreatureCompilerCache();
    const source = fixture(),
      first = compileCreatureGeometry(source);
    const x = first.mesh.positions[0];
    first.mesh.positions[0] = 999;
    expect(compileCreatureGeometry(source).mesh.positions[0]).toBe(x);
    expect(creatureCompilerCacheMetrics().hits).toBe(1);
  });
});

describe("compiled creature integration", () => {
  test("both body plans compile with portable bindings, source identities and material references", async () => {
    const { createCreatureFixture } = await import("@wrela/examples/creature-fixtures");
    const { compileDocument, compilerKey } = await import("./index");
    for (const id of ["ash-warden", "reed-penitent"] as const) {
      const fixture = createCreatureFixture(id),
        doc = fixture.project.documents.find((d) => d.id === id);
      if (!doc || doc.kind !== "character") throw new Error("Missing character fixture");
      const artifact = compileDocument(doc, "interactive");
      if (!artifact || artifact.kind !== "character") throw new Error("Missing compiled character");
      expect(artifact.key).toBe(compilerKey(doc, "interactive"));
      expect(artifact.diagnostics.filter((d) => d.severity === "error")).toEqual([]);
      expect(artifact.creatureCoordinates?.length).toBe(artifact.mesh.positions.length / 3);
      expect(artifact.weights.length).toBe((artifact.mesh.positions.length / 3) * 4);
      const groups = artifact.mesh.materialGroups ?? [];
      expect(new Set(groups.map((group) => group.material)).size).toBe(groups.length);
      expect(artifact.mesh.sourceIds?.filter((source) => source === "eye-left").length).toBeGreaterThan(50);
      const fineFeature = id === "ash-warden" ? "fore-left-claw-0" : "finger-left-0";
      expect(artifact.mesh.sourceIds?.filter((source) => source === fineFeature).length).toBeGreaterThan(50);
      const materials = new Set([
        ...fixture.project.documents.filter((d) => d.kind === "material").map((d) => d.id),
        ...(artifact.creatureMaterials ?? []).map((m) => m.id),
      ]);
      for (const group of artifact.mesh.materialGroups ?? [])
        expect(materials.has(group.material)).toBe(true);
      for (const detail of artifact.creatureDetails ?? []) {
        expect(detail.weights.length).toBe((detail.mesh.positions.length / 3) * 4);
        const prefix = (artifact.creatureBodyVertexCount ?? 0) * 3;
        expect(detail.mesh.positions.slice(0, prefix)).toEqual(artifact.mesh.positions.slice(0, prefix));
        for (const product of detail.correctives ?? [])
          expect(Array.from(product.vertices).every((v) => v < detail.mesh.positions.length / 3)).toBe(true);
      }
      if (id === "ash-warden") {
        expect(artifact.creatureDetails?.[1].mesh.indices.length).toBeLessThan(artifact.mesh.indices.length);
        const bodyCount = artifact.creatureBodyVertexCount ?? 0;
        const hero = artifact.creatureGroom?.details.find(
          (detail) => detail.label === artifact.creatureGroomDetail,
        );
        if (!hero) throw new Error("Missing groom realization");
        const corrective = artifact.creatureCorrectives?.find((item) => item.id === "shoulder-compression");
        if (!corrective) throw new Error("Missing shoulder corrective");
        const displacements = new Map<number, number[]>();
        for (let i = 0; i < corrective.vertices.length; i++) {
          const vertex = corrective.vertices[i] - bodyCount;
          if (vertex < 0) continue;
          const guide = hero.vertexGuideIndices[vertex];
          const delta = Array.from(corrective.displacements.slice(i * 3, i * 3 + 3));
          const previous = displacements.get(guide);
          if (previous) expect(delta).toEqual(previous);
          else displacements.set(guide, delta);
        }
        expect(displacements.size).toBeGreaterThan(0);
      }
    }
  });
  test("legacy sculpt fields preserve source extraction and unrelated regions", async () => {
    const { sculptLegacyCreatureGeometry } = await import("./creature");
    const source = fixture();
    source.regions[0].nodeIds = ["old-arm"];
    source.sculpts = [
      {
        id: "raise",
        region: "arm",
        center: [0, 0, 0],
        radius: 1,
        displacement: [0, 0, 0.2],
        strength: 1,
        falloff: 2,
        mirror: false,
      },
    ];
    const mesh = {
      positions: new Float32Array([0, 0, 0, 0, 0, 0]),
      normals: new Float32Array([0, 0, 1, 0, 0, 1]),
      indices: new Uint32Array(),
      sourceIds: ["old-arm", "torso"],
      bounds: { min: [0, 0, 0] as [number, number, number], max: [0, 0, 0] as [number, number, number] },
    };
    const result = sculptLegacyCreatureGeometry(mesh, source);
    expect(result.positions[2]).toBeCloseTo(0.2, 6);
    expect(result.positions[5]).toBe(0);
    expect(mesh.positions[2]).toBe(0);
    expect(result.normals[2]).toBeCloseTo(1, 6);
  });
});
