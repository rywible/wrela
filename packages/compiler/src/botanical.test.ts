import { describe, expect, test } from "bun:test";
import {
  type BotanicalAuthoring,
  type BotanicalSpecies,
  botanicalPreset,
  botanicalSchema,
  contentKey,
  type VegetationDefinition,
  vegetationSchema,
  vegetationWindResponse,
} from "@wrela/model";

import { botanicalMeshes } from "./botanical-mesh";
import { botanicalBranchPoint, botanicalStructure } from "./botanical-structure";
import { geometryKey } from "./products";
import { compileVegetation } from "./vegetation";

function plant(species: BotanicalSpecies = "pine"): VegetationDefinition & { botanical: BotanicalAuthoring } {
  return {
    id: "plant",
    name: "Plant",
    schemaVersion: 1,
    dependencies: [],
    kind: "vegetation",
    height: 5,
    radius: 2,
    branches: 7,
    seed: 58,
    material: "leaf",
    trunkMaterial: "wood",
    windResponse: 0.7,
    variation: 0.5,
    botanical: botanicalPreset(species),
  };
}

describe("botanical authoring realization", () => {
  test("schema preserves botanical authoring and bounds work amplification", () => {
    const doc = plant();
    expect(vegetationSchema.parse(doc)).toEqual(doc);
    expect(botanicalSchema.safeParse({ growth: { levels: 20 } }).success).toBe(false);
    expect(botanicalSchema.safeParse({ canopy: { leavesPerCluster: 9000 } }).success).toBe(false);
    expect(botanicalSchema.safeParse({ review: { standCount: 1000 } }).success).toBe(false);
  });
  test("all species generate distinct finite, indexed bark and foliage geometry", () => {
    const signatures = new Set<string>();
    for (const species of ["pine", "oak", "birch", "shrub", "fern", "grass"] as const) {
      const doc = plant(species);
      const result = botanicalMeshes(doc, "review");
      expect(result.leafCount).toBeGreaterThan(0);
      signatures.add(contentKey(Array.from(result.foliage.positions)));
      for (const mesh of [result.trunk, result.foliage]) {
        expect(mesh.positions.every(Number.isFinite)).toBe(true);
        expect(mesh.normals.every(Number.isFinite)).toBe(true);
        expect(mesh.colors?.length).toBe(mesh.positions.length);
        expect(mesh.sourceIds?.length).toBe(mesh.positions.length / 3);
        expect(Math.max(...mesh.indices)).toBeLessThan(mesh.positions.length / 3);
        for (let i = 0; i < mesh.normals.length; i += 3)
          expect(Math.hypot(mesh.normals[i], mesh.normals[i + 1], mesh.normals[i + 2])).toBeCloseTo(1, 4);
      }
    }
    expect(signatures.size).toBe(6);
  });
  test("stable subtree pruning preserves siblings and their source identities", () => {
    const doc = plant();
    const original = botanicalStructure(doc);
    const changed = structuredClone(doc);
    changed.botanical.pruning.removedBranches = ["b2"];
    const result = botanicalStructure(changed);
    expect(result.branches.some((branch) => branch.id === "b2" || branch.id.startsWith("b2/"))).toBe(false);
    expect(result.branches.filter((branch) => branch.id.startsWith("b3"))).toEqual(
      original.branches.filter((branch) => branch.id.startsWith("b3")),
    );
  });
  test("child branches attach to bent rendered stems and edits move descendants coherently", () => {
    const doc = plant();
    doc.botanical.branchEdits = [{ branch: "b0", lengthScale: 1.6, bend: [0, 0.8, 0], bare: false }];
    const structure = botanicalStructure(doc);
    const parent = structure.branches.find((branch) => branch.id === "b0");
    const child = structure.branches.find((branch) => branch.id === "b0/0");
    if (!parent || !child) throw new Error("Expected botanical hierarchy");
    const t = 0.35 + (0.5 / doc.botanical.growth.children) * 0.58;
    expect(child.start).toEqual(botanicalBranchPoint(parent, t));
    expect(child.start).not.toEqual(
      botanicalStructure(plant()).branches.find((branch) => branch.id === "b0/0")?.start,
    );
  });
  test("bare branch edits remove descendant foliage without removing the branch skeleton", () => {
    const doc = plant();
    const original = botanicalStructure(doc);
    doc.botanical.branchEdits = [{ branch: "b0", lengthScale: 1, bend: [0, 0, 0], bare: true }];
    const edited = botanicalStructure(doc);
    expect(edited.branches.length).toBe(original.branches.length);
    expect(
      edited.branches
        .filter((branch) => branch.id === "b0" || branch.id.startsWith("b0/"))
        .every((branch) => branch.bare),
    ).toBe(true);
    expect(botanicalMeshes(doc, "review").foliage.sourceIds?.some((id) => id.startsWith("plant/b0/"))).toBe(
      false,
    );
  });
  test("complete leaf loss produces finite empty foliage; branch damage prunes recursion", () => {
    const doc = plant();
    doc.botanical.damage.leafLoss = 1;
    const lost = compileVegetation(doc);
    expect(lost.surfaces[1].mesh.indices.length).toBe(0);
    expect(lost.bounds.min.every(Number.isFinite)).toBe(true);
    expect(lost.surfaces[1].mesh.bounds.max.every(Number.isFinite)).toBe(true);
    doc.botanical.damage.brokenBranches = 1;
    const damaged = botanicalStructure(doc);
    expect(damaged.branches.filter((branch) => branch.level > 1)).toHaveLength(0);
  });
  test("maturity, canopy, and silhouette edits invalidate geometry; inspection and motion do not", () => {
    const doc = plant();
    const geometry = geometryKey(doc);
    doc.botanical.review.standCount = 16;
    doc.botanical.motion.stiffness = 1;
    expect(geometryKey(doc)).toBe(geometry);
    expect(compileVegetation(doc).windResponse).toBeCloseTo(vegetationWindResponse(doc));
    doc.botanical.age = 0.2;
    expect(geometryKey(doc)).not.toBe(geometry);
    expect(botanicalStructure(doc).height).toBeCloseTo(1.8);
  });
  test("cached clones remap hierarchical ids and distant details reduce geometry", () => {
    const doc = plant();
    const original = compileVegetation(doc);
    const clone = compileVegetation({ ...doc, id: "clone" });
    expect(clone.surfaces[1].mesh.positions).toBe(original.surfaces[1].mesh.positions);
    expect(clone.surfaces[1].mesh.sourceIds?.every((id) => id.startsWith("clone/"))).toBe(true);
    expect(clone.surfaces[1].details?.[0].mesh.indices.length).toBeLessThan(
      clone.surfaces[1].mesh.indices.length,
    );
    expect(clone.surfaces[1].details?.[0].mesh.sourceIds?.every((id) => id.startsWith("clone/"))).toBe(true);
  });
  test("wind displacement envelope includes taller distant blades", () => {
    const doc = plant("grass");
    doc.height = 1;
    const artifact = compileVegetation(doc);
    for (const surface of artifact.surfaces)
      for (const mesh of [surface.mesh, ...(surface.details ?? []).map((detail) => detail.mesh)]) {
        for (let vertex = 1; vertex < mesh.positions.length; vertex += 3) {
          const displacement = Math.min(Math.max(mesh.positions[vertex], 0) ** 2 * 0.012, 0.6) * 2;
          expect(displacement).toBeLessThanOrEqual(artifact.maxDisplacement + 1e-6);
        }
      }
  });
  test("maximum authoring inputs stay bounded and disclose leaf budget truncation", () => {
    const doc = plant();
    doc.branches = 32;
    doc.botanical.growth.levels = 3;
    doc.botanical.growth.children = 4;
    doc.botanical.canopy.density = 1;
    doc.botanical.canopy.clusters = 8;
    doc.botanical.canopy.leavesPerCluster = 12;
    const result = botanicalMeshes(doc, "export");
    expect(result.structure.branches.length).toBeLessThanOrEqual(768);
    expect(result.leafCount).toBeLessThanOrEqual(16000);
    expect(result.truncated).toBe(true);
  });
});
