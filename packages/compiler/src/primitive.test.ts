import { expect, test } from "bun:test";
import {
  type FieldDefinition,
  identityMatrix,
  intersectQuadric,
  inverseMatrix,
  multiplyMatrices,
  type ObjectDefinition,
  quadricUnitToLocal,
  type Vec3,
} from "@wrela/model";

import { compilePrimitiveProducts, parametricPrimitive, staticPrimitive } from "./primitive";
import { clearCompilerCaches, compilerCacheMetrics, compileSurface, extractSurface } from "./surface";

const field: FieldDefinition = {
  root: "shape",
  resolution: 12,
  bounds: { min: [-4, -4, -4], max: [4, 4, 4] },
  nodes: [
    {
      id: "shape",
      name: "Shape",
      kind: "ellipsoid",
      position: [0, 0, 0],
      rotation: [0.1, 0.3, 0.2],
      size: [2, 1, 0.5],
      radius: 1,
      blend: 0,
      children: [],
    },
  ],
};
const document: ObjectDefinition = {
  kind: "object",
  id: "object",
  name: "Object",
  schemaVersion: 1,
  dependencies: [],
  material: "stone",
  collision: "none",
  field,
};

test("primitive compilation replaces extraction with quality-sized parametric primary and smaller bounded controls", () => {
  clearCompilerCaches();
  const surface = compileSurface(document, "interactive");
  expect(compilerCacheMetrics().geometry.misses).toBe(0);
  expect(surface.mesh.positions.length / 3).toBe(266);
  expect(compileSurface(document, "interactive").mesh).toBe(surface.mesh);
  expect(compilerCacheMetrics().primitive.hits).toBeGreaterThan(0);
  expect(surface.mesh.indices.length).toBeGreaterThan(0);
  expect(surface.renderProducts?.map((p) => p.kind)).toEqual([
    "direct-mesh",
    "analytic-quadric",
    "parametric-mesh",
  ]);
  const products = compilePrimitiveProducts(document, "interactive");
  expect(products.every((p) => p.kind === "direct-mesh" || p.fallbackKey === products[0].key)).toBe(true);
  const changed = compilePrimitiveProducts({ ...document, material: "clay" }, "interactive");
  expect(changed[0].key).toBe(products[0].key);
  expect(changed[1].key).not.toBe(products[1].key);
});

test("parametric control has closed topology, outward normals and bounded zero-set deviation", () => {
  const primitive = staticPrimitive(field);
  if (!primitive) throw new Error("Missing primitive");
  const { mesh, surfaceError } = parametricPrimitive(primitive, 8);
  const edges = new Map<string, number>();
  const inverse = inverseMatrix(quadricUnitToLocal(primitive));
  if (!inverse) throw new Error("Missing inverse");
  for (let i = 0; i < mesh.indices.length; i += 3) {
    const ids = Array.from(mesh.indices.slice(i, i + 3));
    for (let c = 0; c < 3; c++) {
      const a = ids[c],
        b = ids[(c + 1) % 3];
      const key = `${Math.min(a, b)}:${Math.max(a, b)}`;
      edges.set(key, (edges.get(key) ?? 0) + 1);
    }
    const center = [0, 1, 2].map((axis) =>
      ids.reduce((sum, id) => sum + mesh.positions[id * 3 + axis] / 3, 0),
    ) as Vec3;
    const length = Math.hypot(...center);
    const hit = intersectQuadric(inverse, [0, 0, 0], center.map((v) => v / length) as Vec3);
    if (!hit) throw new Error("Missing ray hit");
    expect(hit.distance - length).toBeLessThanOrEqual(surfaceError + 1e-6);
  }
  expect([...edges.values()].every((count) => count === 2)).toBe(true);
  expect(mesh.fidelity?.maxError).toBeNull();
  expect(mesh.sourceIds?.every((id) => id === "shape")).toBe(true);
});

test("quadric intersections handle tangency, near-plane origins, inside, misses, and transformed rebased coordinates", () => {
  const sphere = {
    center: [0, 0, 0] as Vec3,
    radii: [1, 1, 1] as Vec3,
    rotation: [0, 0, 0] as Vec3,
    nodeId: "sphere",
  };
  const inverse = inverseMatrix(quadricUnitToLocal(sphere));
  if (!inverse) throw new Error("Missing inverse");
  expect(intersectQuadric(inverse, [0, 0, 4], [0, 0, -1])?.distance).toBeCloseTo(3);
  expect(intersectQuadric(inverse, [0, 0, 0], [0, 0, -1])?.distance).toBeCloseTo(1);
  expect(intersectQuadric(inverse, [1, 0, 4], [0, 0, -1])?.distance).toBeCloseTo(4);
  expect(intersectQuadric(inverse, [1.00001, 0, 4], [0, 0, -1])).toBeNull();
  expect(intersectQuadric(inverse, [0, 0, 0.99], [0, 0, -1])?.distance).toBeCloseTo(1.99);
  expect(intersectQuadric(inverse, [0, 0, 4], [0, 0, -1], 3.5)?.distance).toBeCloseTo(5);
  expect(intersectQuadric(inverse, [0, 0, 4], [0, 0, 0])).toBeNull();
  const model = identityMatrix();
  model[0] = 2;
  model[5] = 3;
  model[10] = 4;
  model[12] = 20;
  const transformed = inverseMatrix(multiplyMatrices(model, quadricUnitToLocal(sphere)));
  if (!transformed) throw new Error("Missing inverse");
  expect(intersectQuadric(transformed, [20, 0, 9], [0, 0, -1])?.distance).toBeCloseTo(5);
  expect(intersectQuadric(transformed, [20, 0, 9], [0, 0, -1])?.normal).toEqual([0, 0, 1]);
});

test("composition and clipped primitives retain the direct fallback", () => {
  expect(staticPrimitive({ ...field, bounds: { min: [-1, -1, -1], max: [1, 1, 1] } })).toBeNull();
  expect(
    staticPrimitive({
      ...field,
      root: "group",
      nodes: [...field.nodes, { ...field.nodes[0], id: "group", kind: "smoothUnion", children: ["shape"] }],
    }),
  ).toBeNull();
});

test("primitive primary avoids extraction cost and publishes tighter geometric evidence at every quality", () => {
  const extracted = extractSurface(document.field, "review");
  const compiled = compileSurface(document, "review");
  expect(compiled.mesh.positions.length).not.toBe(extracted.mesh.positions.length);
  const primary = compiled.renderProducts?.[0];
  const evidence = primary?.errors[0];
  expect(evidence?.kind).toBe("real-bound");
  if (evidence?.kind !== "real-bound") throw new Error("Missing evidence");
  expect(evidence.maximum).toBeLessThan(0.02);
  expect(
    compiled.renderProducts
      ?.filter((p) => p.kind === "parametric-mesh")
      .every((p) => p.kind === "parametric-mesh" && p.mesh.indices.length < compiled.mesh.indices.length),
  ).toBe(true);
  const exportSurface = compileSurface(document, "export");
  expect(exportSurface.mesh.positions.length).toBeGreaterThan(compiled.mesh.positions.length);
});
