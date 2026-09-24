import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import type { CharacterDefinition, FieldDefinition, FieldNode } from "@wrela/model";
import {
  artifactTransfers,
  clearCompilerCaches,
  compileCharacter,
  compileField,
  compilerCacheMetrics,
  compileVegetation,
  extractSurface,
  productKeys,
} from "./index";

function sphere(id: string, x: number, radius = 0.4): FieldNode {
  return {
    id,
    name: id,
    kind: "sphere",
    position: [x, 0, 0],
    rotation: [0, 0, 0],
    size: [1, 1, 1],
    radius,
    blend: 0,
    children: [],
  };
}
test("a separated sub-grid feature is diagnosed even when the rest of the mesh is nonempty", () => {
  const small = {
    ...sphere("small", 0.025, 0.01),
    position: [0.025, 0.025, 0.025] as [number, number, number],
  };
  const field: FieldDefinition = {
    root: "root",
    nodes: [
      sphere("large", -0.8),
      small,
      { ...sphere("root", 0), kind: "union", children: ["large", "small"] },
    ],
    bounds: { min: [-2, -2, -2], max: [2, 2, 2] },
    resolution: 80,
  };
  expect(compileField(field).sample(small.position)).toEqual({
    distance: -0.01,
    source: "small",
    material: undefined,
  });
  const result = extractSurface(field, "export");
  expect(result.mesh.indices.length).toBeGreaterThan(0);
  expect(
    result.diagnostics.some(
      (diagnostic) => diagnostic.node === "small" && diagnostic.code === "field.feature-undersampled",
    ),
  ).toBe(true);
  expect(result.mesh.fidelity?.maxError).toBeNull();
  expect(result.mesh.fidelity?.unresolvedFeatures).toContain("small");
  expect(() =>
    extractSurface({ ...field, fidelity: { minimumFeatureSize: 0.02, strict: true } }, "export"),
  ).toThrow("Feature small");
});
test("explicit geometric error requests are not falsely certified for generic implicit extraction", () => {
  const field: FieldDefinition = {
    root: "ball",
    nodes: [sphere("ball", 0)],
    bounds: { min: [-1, -1, -1], max: [1, 1, 1] },
    resolution: 12,
    fidelity: { maxError: 0.001 },
  };
  const result = extractSurface(field);
  expect(result.diagnostics.some((diagnostic) => diagnostic.code === "field.error-unproven")).toBe(true);
  expect(result.mesh.fidelity?.requestedMaxError).toBe(0.001);
});
test("IR pruning preserves exact samples and provenance while skipping distant union branches", () => {
  const nodes = Array.from({ length: 60 }, (_, index) => sphere(`sphere-${index}`, index * 2));
  nodes.push({
    ...sphere("root", 0),
    kind: "smoothUnion",
    blend: 0.2,
    children: nodes.map((node) => node.id),
  });
  const field: FieldDefinition = {
    root: "root",
    nodes,
    bounds: { min: [-1, -1, -1], max: [120, 1, 1] },
    resolution: 24,
  };
  const optimized = compileField(field),
    baseline = compileField(field, { prune: false });
  for (let index = 0; index < 200; index++) {
    const point: [number, number, number] = [
      Math.sin(index) * 0.8,
      Math.cos(index * 0.7),
      Math.sin(index * 0.3),
    ];
    expect(optimized.sample(point)).toEqual(baseline.sample(point));
  }
  expect(optimized.ir.evaluationCost).toBe(61);
  expect(optimized.metrics.evaluatedNodes).toBeLessThan(baseline.metrics.evaluatedNodes / 10);
  expect(optimized.metrics.prunedNodes).toBeGreaterThan(10000);
});
test("IR never applies Euclidean distance pruning to ellipsoid estimates or unknown smooth support", () => {
  const field: FieldDefinition = {
    root: "root",
    nodes: [
      sphere("ball", 0),
      { ...sphere("ellipse", 1), kind: "ellipsoid", size: [0.03, 0.6, 2], rotation: [0.3, 0.5, 1] },
      { ...sphere("root", 0), kind: "smoothUnion", blend: 0.3, children: ["ball", "ellipse"] },
    ],
    bounds: { min: [-3, -3, -3], max: [3, 3, 3] },
    resolution: 24,
  };
  const optimized = compileField(field),
    baseline = compileField(field, { prune: false });
  expect(optimized.ir.instructions.find((instruction) => instruction.id === "ellipse")?.boundDistance).toBe(
    false,
  );
  expect(optimized.ir.instructions[optimized.ir.root].bounds).toBeNull();
  for (let index = 0; index < 300; index++) {
    const point: [number, number, number] = [
      Math.sin(index) * 3,
      Math.cos(index * 0.7) * 3,
      Math.sin(index * 0.3) * 3,
    ];
    expect(optimized.sample(point)).toEqual(baseline.sample(point));
  }
});
test("animation and material edits reuse extraction and binding products; shape edits invalidate both", () => {
  clearCompilerCaches();
  const source = referenceProject().documents.find(
    (document) => document.kind === "character",
  ) as CharacterDefinition;
  const first = compileCharacter(source, "interactive");
  const animation = structuredClone(source);
  animation.motions[0].keys[0].rotation[0] += 0.1;
  const animated = compileCharacter(animation, "interactive");
  expect(animated.mesh.positions).toBe(first.mesh.positions);
  expect(animated.weights).toBe(first.weights);
  expect(animated.motions).not.toEqual(first.motions);
  expect(animated.key).not.toBe(first.key);
  const material = structuredClone(animation);
  material.material = "changed";
  material.field.nodes[0].material = "changed";
  const remapped = compileCharacter(material, "interactive");
  expect(remapped.mesh.positions).toBe(first.mesh.positions);
  expect(remapped.weights).toBe(first.weights);
  expect(productKeys(material, "interactive").geometry).toBe(productKeys(source, "interactive").geometry);
  const shape = structuredClone(material);
  shape.field.nodes[0].size[0] += 0.1;
  const reshaped = compileCharacter(shape, "interactive");
  expect(reshaped.mesh.positions).not.toBe(first.mesh.positions);
  expect(reshaped.weights).not.toBe(first.weights);
  expect(compilerCacheMetrics().geometry.misses).toBe(2);
  expect(compilerCacheMetrics().binding.misses).toBe(2);
  expect(compilerCacheMetrics().geometry.bytes).toBeLessThanOrEqual(16 * 1024 * 1024);
});
test("distant vegetation preserves semantic parts while actually reducing geometry without claiming an error bound", () => {
  const document = referenceProject().documents.find((document) => document.kind === "vegetation");
  if (!document || document.kind !== "vegetation") throw new Error("Missing vegetation fixture");
  const product = compileVegetation(document, "interactive");
  expect(product.surfaces.some((surface) => surface.details?.length)).toBe(true);
  for (const surface of product.surfaces) {
    const detail = surface.details?.[0];
    if (!detail) continue;
    expect(detail.maxProjectedDiameter).toBe(96);
    expect(detail.maxError).toBeNull();
    expect(detail.mesh.indices.length).toBeLessThan(surface.mesh.indices.length);
    expect(detail.mesh.indices.length).toBeGreaterThan(0);
    expect(detail.mesh.positions.every(Number.isFinite)).toBe(true);
    expect(artifactTransfers(product)).toContain(detail.mesh.positions.buffer as ArrayBuffer);
    for (let index = 0; index < detail.mesh.positions.length; index++) {
      expect(detail.mesh.positions[index]).toBeGreaterThanOrEqual(surface.mesh.bounds.min[index % 3] - 1e-5);
      expect(detail.mesh.positions[index]).toBeLessThanOrEqual(surface.mesh.bounds.max[index % 3] + 1e-5);
    }
  }
});
