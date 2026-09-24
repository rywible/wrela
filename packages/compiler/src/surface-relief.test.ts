import { expect, test } from "bun:test";
import {
  type AssemblyDefinition,
  assemblySchema,
  type MeshData,
  type SurfaceRelief,
  surfaceReliefSchema,
} from "@wrela/model";

import { compileAssemblyMesh } from "./assembly";
import { compileSurfaceRelief } from "./surface-relief";
import { surfaceReliefDepth } from "./surface-relief-pattern";

function cube(): MeshData {
  const assembly: AssemblyDefinition = assemblySchema.parse({
    grid: 0.1,
    clearances: [],
    parts: [
      {
        id: "stone",
        name: "stone",
        profile: { kind: "rectangle", width: 0.4, height: 0.4 },
        path: [
          [0, 0, 0],
          [0, 0, 0.4],
        ],
        bevel: 0,
        position: [0, 0, 0],
        rotation: [0, 0, 0],
        repeat: { count: 1, offset: [0, 0, 0] },
        sockets: [],
        wear: { amount: 0, scale: 1, seed: 1 },
      },
    ],
  });
  return compileAssemblyMesh(assembly, "stone").mesh;
}
const relief = (kind: SurfaceRelief["kind"] = "stone"): SurfaceRelief => ({
  kind,
  amplitude: 0.015,
  scale: 0.1,
  seed: 7,
  targetEdgeLength: 0.035,
  direction: [0, 1, 0],
});
function weldedEdges(mesh: MeshData): Map<string, number> {
  const vertex = (index: number) => [...mesh.positions.subarray(index * 3, index * 3 + 3)].join(",");
  const edges = new Map<string, number>();
  for (let i = 0; i < mesh.indices.length; i += 3)
    for (let n = 0; n < 3; n++) {
      const a = vertex(mesh.indices[i + n]),
        b = vertex(mesh.indices[i + ((n + 1) % 3)]);
      const key = [a, b].sort().join("/");
      edges.set(key, (edges.get(key) ?? 0) + 1);
    }
  return edges;
}
test("stone relief is deterministic real geometry, bounded inward with watertight hard-edge seams", () => {
  const source = cube(),
    original = structuredClone(source);
  const first = compileSurfaceRelief(source, relief(), { maxTriangles: 5000 });
  const second = compileSurfaceRelief(source, relief(), { maxTriangles: 5000 });
  expect(source).toEqual(original);
  expect(first.mesh.positions).toEqual(second.mesh.positions);
  expect(first.mesh.normals).toEqual(second.mesh.normals);
  expect(first.review.applied).toBe(true);
  expect(first.review.maxDisplacement).toBeGreaterThan(0.004);
  expect(first.review.maxDisplacement).toBeLessThanOrEqual(relief().amplitude + 1e-7);
  expect(first.review.coarseDistanceBound).toBe(first.review.maxDisplacement);
  expect(first.mesh.indices.length).toBeGreaterThan(source.indices.length);
  expect([...weldedEdges(first.mesh).values()].every((count) => count === 2)).toBe(true);
  for (let i = 0; i < first.mesh.positions.length; i++) {
    expect(first.mesh.positions[i]).toBeGreaterThanOrEqual(source.bounds.min[i % 3]);
    expect(first.mesh.positions[i]).toBeLessThanOrEqual(source.bounds.max[i % 3]);
  }
  // Flat stone fronts acquire actual sloped normals from the displaced positions.
  let tilted = 0;
  for (let i = 0; i < first.mesh.normals.length; i += 3) {
    const length = Math.hypot(...first.mesh.normals.subarray(i, i + 3));
    expect(length).toBeCloseTo(1, 5);
    if (Math.abs(first.mesh.normals[i + 2]) > 0.3 && Math.abs(first.mesh.normals[i]) > 0.1) tilted++;
  }
  expect(tilted).toBeGreaterThan(100);
  const variant = compileSurfaceRelief(source, { ...relief(), seed: 99 }, { maxTriangles: 5000 });
  expect(variant.mesh.positions).not.toEqual(first.mesh.positions);
});

test("conforming adaptive subdivision respects hard budgets and reports unresolved spacing", () => {
  const source = cube();
  const bounded = compileSurfaceRelief(
    source,
    { ...relief(), targetEdgeLength: 0.002 },
    { maxTriangles: 101 },
  );
  expect(bounded.mesh.indices.length / 3).toBeLessThanOrEqual(101);
  expect(bounded.review.budgetLimited).toBe(true);
  expect(bounded.review.achievedMaxEdgeLength).toBeGreaterThan(0.002);
  expect(bounded.diagnostics.some((entry) => entry.code === "surface-relief.detail-budget")).toBe(true);
  expect([...weldedEdges(bounded.mesh).values()].every((count) => count === 2)).toBe(true);
  const skipped = compileSurfaceRelief(source, relief(), { maxTriangles: 1 });
  expect(skipped.mesh).toBe(source);
  expect(skipped.review.applied).toBe(false);
  expect(skipped.diagnostics[0].code).toBe("surface-relief.source-budget");
  expect(compileSurfaceRelief(source, { ...relief(), amplitude: 0 }).mesh).toBe(source);
});

test("relief preserves material ranges, source provenance, colors and interpolated wind", () => {
  const source = cube();
  source.materialGroups = [
    { material: "stone", start: 0, count: 24 },
    { material: "bark", start: 24, count: source.indices.length - 24 },
  ];
  source.wind = new Float32Array((source.positions.length / 3) * 4);
  for (let i = 0; i < source.wind.length; i += 4) source.wind.set([0.7, 0.02, 1.2, 0.005], i);
  const uv = new Float32Array((source.positions.length / 3) * 2);
  for (let i = 0; i < source.positions.length / 3; i++)
    uv.set([source.positions[i * 3] + 0.5, source.positions[i * 3 + 1] + 0.5], i * 2);
  source.thinCoverage = {
    version: 1,
    key: "coverage",
    width: 2,
    height: 2,
    uv,
    levels: [new Uint8Array([0, 255, 255, 0]), new Uint8Array([128])],
  };
  const result = compileSurfaceRelief(source, relief("bark"), { maxTriangles: 800 });
  const groups = result.mesh.materialGroups;
  expect(groups?.map((group) => group.material)).toEqual(["stone", "bark"]);
  expect(groups?.[1].start).toBe(groups?.[0].count);
  expect(groups?.reduce((sum, group) => sum + group.count, 0)).toBe(result.mesh.indices.length);
  expect(new Set(result.mesh.sourceIds)).toEqual(new Set(["stone"]));
  expect(result.mesh.colors?.every((value) => value === 1)).toBe(true);
  expect(result.mesh.wind?.length).toBe(result.mesh.indices.length * 4);
  expect(result.mesh.thinCoverage?.levels).toBe(source.thinCoverage.levels);
  expect(result.mesh.thinCoverage?.uv.length).toBe(result.mesh.indices.length * 2);
  for (let i = 0; i < result.mesh.indices.length; i++)
    expect(result.mesh.thinCoverage?.uv[i * 2]).toBeCloseTo(
      (result.mesh.reliefCoordinates?.[i * 3] ?? 0) + 0.5,
      6,
    );
  for (let i = 0; i < (result.mesh.wind?.length ?? 0); i++)
    expect(result.mesh.wind?.[i]).toBeCloseTo(source.wind[i % 4], 6);
  expect(result.review.byteLength).toBe(
    result.mesh.positions.byteLength +
      result.mesh.normals.byteLength +
      result.mesh.indices.byteLength +
      (result.mesh.reliefCoordinates?.byteLength ?? 0) +
      (result.mesh.reliefNormals?.byteLength ?? 0) +
      (result.mesh.thinCoverage?.uv.byteLength ?? 0) +
      5 +
      (result.mesh.colors?.byteLength ?? 0) +
      (result.mesh.wind?.byteLength ?? 0),
  );
});

test("open mesh boundaries remain pinned while interior relief produces actual geometric depth", () => {
  const source: MeshData = {
    positions: new Float32Array([-0.2, -0.2, 0, 0.2, -0.2, 0, 0.2, 0.2, 0, -0.2, 0.2, 0, 0, 0, 0]),
    normals: new Float32Array(Array.from({ length: 5 }, () => [0, 0, 1]).flat()),
    indices: new Uint32Array([0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4]),
    bounds: { min: [-0.2, -0.2, -0.02], max: [0.2, 0.2, 0] },
  };
  const result = compileSurfaceRelief(source, relief(), { maxTriangles: 1000 });
  expect(result.review.sourceClosed).toBe(false);
  expect(result.review.boundaryVertices).toBeGreaterThan(4);
  expect(result.mesh.bounds.min[2]).toBeLessThan(-0.001);
  for (let i = 0; i < result.mesh.positions.length; i += 3) {
    const x = result.mesh.positions[i],
      y = result.mesh.positions[i + 1];
    if (Math.abs(x) >= 0.199999 || Math.abs(y) >= 0.199999) expect(result.mesh.positions[i + 2]).toBe(0);
  }
});

test("selective material relief leaves other surfaces in place and seals their shared boundary", () => {
  const source = cube();
  // Last cap gets the stone treatment; all sidewalls and the opposite cap retain their shape.
  const capStart = source.indices.length - 12;
  source.materialGroups = [
    { material: "plain", start: 0, count: capStart },
    { material: "stone", start: capStart, count: 12 },
  ];
  const result = compileSurfaceRelief(source, relief(), { material: "stone", maxTriangles: 2000 });
  expect(result.review.applied).toBe(true);
  expect([...weldedEdges(result.mesh).values()].every((count) => count === 2)).toBe(true);
  const plain = result.mesh.materialGroups?.[0];
  if (!plain) throw new Error("Missing plain material range");
  for (let i = plain.start; i < plain.start + plain.count; i++) {
    const at = result.mesh.indices[i] * 3,
      point = result.mesh.positions.subarray(at, at + 3);
    // Every untreated point stays on one of the original five support planes.
    expect(
      Math.min(Math.abs(Math.abs(point[0]) - 0.2), Math.abs(Math.abs(point[1]) - 0.2), Math.abs(point[2])),
    ).toBeLessThan(1e-7);
  }
});

test("curvature limits relief on tiny features and schema rejects unbounded authoring", () => {
  const source = cube();
  source.positions = source.positions.map((value) => value * 0.02);
  source.bounds = {
    min: source.bounds.min.map((value) => value * 0.02) as [number, number, number],
    max: source.bounds.max.map((value) => value * 0.02) as [number, number, number],
  };
  const result = compileSurfaceRelief(
    source,
    { ...relief("bark"), amplitude: 0.08, targetEdgeLength: 0.002 },
    { maxTriangles: 1000 },
  );
  expect(result.review.curvatureLimitedVertices).toBeGreaterThan(0);
  expect(result.review.maxDisplacement).toBeLessThan(0.002);
  expect(result.mesh.positions.every(Number.isFinite)).toBe(true);
  expect(surfaceReliefSchema.safeParse({ ...relief(), amplitude: 2 }).success).toBe(false);
  expect(surfaceReliefSchema.safeParse({ ...relief(), direction: [0, 0, 0] }).success).toBe(false);
});

test("thickness guard preserves separate front/back faces of a thin closed slab", () => {
  const source = cube();
  for (let i = 2; i < source.positions.length; i += 3) source.positions[i] *= 0.005;
  source.bounds.max[2] *= 0.005;
  source.materialGroups = [
    { material: "sides", start: 0, count: 24 },
    { material: "front", start: 24, count: 12 },
    { material: "back", start: 36, count: 12 },
  ];
  const result = compileSurfaceRelief(
    source,
    { ...relief(), amplitude: 0.08, scale: 0.5, targetEdgeLength: 0.02 },
    { maxTriangles: 5000 },
  );
  expect(result.review.sourceClosed).toBe(true);
  expect(result.review.thicknessLimitedVertices).toBeGreaterThan(0);
  expect(result.review.maxDisplacement).toBeLessThan(0.08);
  for (const group of result.mesh.materialGroups ?? [])
    for (let i = group.start; i < group.start + group.count; i++) {
      const z = result.mesh.positions[result.mesh.indices[i] * 3 + 2];
      if (group.material === "back") expect(z).toBeGreaterThan(0.0015);
      if (group.material === "front") expect(z).toBeLessThan(0.0005);
    }
  expect([...weldedEdges(result.mesh).values()].every((count) => count === 2)).toBe(true);
});

test("relief retains undisplaced coordinates and authored frames for phase-coherent residual shading", () => {
  const source = cube(),
    recipe = relief();
  const result = compileSurfaceRelief(source, recipe, { maxTriangles: 5000 });
  const coordinates = result.mesh.reliefCoordinates,
    normals = result.mesh.reliefNormals;
  if (!coordinates || !normals) throw new Error("Missing relief reference attributes");
  expect(coordinates.length).toBe(result.mesh.positions.length);
  expect(normals.length).toBe(result.mesh.normals.length);
  expect(result.appearance.geometryWeights).toEqual(result.review.geometryBandWeights);
  expect(
    result.appearance.residualWeights.map((value, index) => value + result.appearance.geometryWeights[index]),
  ).toEqual([1, 1, 1]);
  let samples = 0;
  for (let i = 0; i < coordinates.length; i += 3) {
    const p = [...coordinates.subarray(i, i + 3)] as [number, number, number];
    if (normals[i + 2] < 0.999 || Math.abs(p[0]) > 0.12 || Math.abs(p[1]) > 0.12) continue;
    expect(p[2]).toBeCloseTo(0.4, 6);
    const depth =
      recipe.amplitude * surfaceReliefDepth(p, [0, 0, 1], recipe, result.appearance.geometryWeights);
    expect(result.mesh.positions[i + 2]).toBeCloseTo(p[2] - depth, 6);
    samples++;
  }
  expect(samples).toBeGreaterThan(20);
  expect(source.reliefCoordinates).toBeUndefined();
  expect(source.reliefNormals).toBeUndefined();
});
