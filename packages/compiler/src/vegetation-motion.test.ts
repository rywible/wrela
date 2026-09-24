import { expect, test } from "bun:test";
import { alpinePineLookdevDefinition } from "@wrela/examples";
import {
  botanicalPreset,
  contentKey,
  createVegetationStand,
  type Vec3,
  type VegetationDefinition,
  vegetationMotionEnvelope,
  vegetationMotionOffset,
  vegetationWindResponse,
} from "@wrela/model";
import { botanicalMeshes } from "./botanical-mesh";
import { MAX_BOTANICAL_VERTICES } from "./botanical-primitives";
import { botanicalBranchPoint, botanicalStructure } from "./botanical-structure";
import { coniferStructure, MAX_CONIFER_BRANCHES } from "./conifer-growth";
import { coniferMeshes } from "./conifer-mesh";
import { deserializeArtifact, serializeArtifact } from "./cooked";
import { geometryKey } from "./products";
import { compileVegetation } from "./vegetation";

function coniferPlant() {
  const plant = alpinePineLookdevDefinition();
  if (!plant.botanical?.conifer) throw new Error("Expected authored conifer");
  return { ...plant, botanical: { ...plant.botanical, conifer: plant.botanical.conifer } };
}

test("edited trunks move primary branch origins onto their rendered centerline", () => {
  for (const conifer of [false, true]) {
    const plant: VegetationDefinition = coniferPlant();
    if (!conifer) plant.botanical = botanicalPreset("oak");
    if (!plant.botanical) throw new Error("Expected botanical source");
    const before = botanicalStructure(plant);
    const oldTrunk = before.branches[0];
    plant.botanical.branchEdits = [{ branch: "trunk", lengthScale: 1.5, bend: [0.5, 0, 0], bare: true }];
    const changed = botanicalStructure(plant);
    for (const branch of changed.branches.filter((branch) => branch.level === 1)) {
      const original = before.branches.find((original) => original.id === branch.id);
      if (!original) throw new Error("Missing branch before edit");
      // Conifers have a linear vertical centerline; generic trunk's tropism affects Y.
      const t = conifer
        ? original.start[1] / oldTrunk.end[1]
        : 0.18 + ((Number(branch.id.slice(1)) + 0.5) / plant.branches) * 0.72;
      const attachment = botanicalBranchPoint(changed.branches[0], t);
      for (let axis = 0; axis < 3; axis++) expect(branch.start[axis]).toBeCloseTo(attachment[axis], 10);
      expect(branch.bare).toBe(true);
    }
    expect(changed.branches.filter((branch) => branch.level > 0).every((branch) => branch.bare)).toBe(true);
  }
});

test("conifer hierarchy, canopy density and authored needle count affect realization", () => {
  const plant = coniferPlant();
  plant.botanical.growth.levels = 1;
  expect(coniferStructure(plant).branches.every((branch) => branch.level <= 1)).toBe(true);
  plant.botanical.canopy.density = 0;
  expect(botanicalMeshes(plant, "review").foliage.indices.length).toBe(0);
  plant.botanical.canopy.density = 1;
  plant.botanical.damage.leafLoss = 0;
  plant.botanical.conifer.needlesPerShoot = 16;
  const sparse = botanicalMeshes(plant, "review").leafCount;
  plant.botanical.conifer.needlesPerShoot = 64;
  expect(botanicalMeshes(plant, "review").leafCount).toBe(sparse * 4);
});

test("conifer triangle winding and shading normals agree on bark, sprays and needles", () => {
  const plant = coniferPlant();
  plant.branches = 3;
  const result = botanicalMeshes(plant, "review");
  const distant = botanicalMeshes(plant, "review", true);
  for (const mesh of [result.trunk, result.foliage, distant.trunk, distant.foliage]) {
    for (let triangle = 0; triangle < mesh.indices.length; triangle += 3) {
      const ids = Array.from(mesh.indices.subarray(triangle, triangle + 3));
      const [a, b, c] = ids.map((id) => Array.from(mesh.positions.subarray(id * 3, id * 3 + 3)));
      const u = b.map((value, axis) => value - a[axis]),
        v = c.map((value, axis) => value - a[axis]);
      const geometric = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
      const agreement = geometric.reduce(
        (sum, value, axis) => sum + value * ids.reduce((total, id) => total + mesh.normals[id * 3 + axis], 0),
        0,
      );
      expect(agreement).toBeGreaterThanOrEqual(-1e-9);
    }
  }
});

test("branch and leaf motion bindings stay finite, grounded, variant-stable and inside bounds", () => {
  const plant = coniferPlant();
  const artifact = compileVegetation(plant);
  let movingWood = false,
    movingLeaf = false;
  for (const [surfaceIndex, surface] of artifact.surfaces.entries())
    for (const mesh of [surface.mesh, ...(surface.details ?? []).map((detail) => detail.mesh)]) {
      if (!mesh.wind) throw new Error("Missing wind bindings");
      expect(mesh.wind?.length).toBe((mesh.positions.length / 3) * 4);
      expect(mesh.wind?.every(Number.isFinite)).toBe(true);
      expect(vegetationMotionEnvelope(mesh.wind)).toBeLessThanOrEqual(artifact.maxDisplacement);
      for (let vertex = 0; vertex < mesh.positions.length / 3; vertex += 17) {
        const slot = vertex * 4;
        const weights = Array.from(mesh.wind.subarray(slot, slot + 4)) as [number, number, number, number];
        const position = Array.from(mesh.positions.subarray(vertex * 3, vertex * 3 + 3)) as Vec3;
        movingWood ||= surfaceIndex === 0 && weights[1] > 0;
        movingLeaf ||= surfaceIndex === 1 && weights[3] > 0;
        if (position[1] <= 0) expect(weights[1] + weights[3]).toBe(0);
        for (const time of [0, 0.5, 2]) {
          const offset = vegetationMotionOffset(weights, position, time, [10, 0, 2], 2);
          expect(Math.hypot(...offset)).toBeLessThanOrEqual((weights[1] + weights[3]) * 2 + 1e-6);
        }
      }
    }
  expect(movingWood).toBe(true);
  expect(movingLeaf).toBe(true);
  const clone = compileVegetation({ ...plant, id: "shared-motion-clone" });
  expect(clone.surfaces[1].mesh.wind).toBe(artifact.surfaces[1].mesh.wind);
  const originalKey = geometryKey(plant);
  plant.botanical.motion.stiffness = 1;
  expect(geometryKey(plant)).toBe(originalKey);
  expect(compileVegetation(plant).windResponse).toBe(vegetationWindResponse(plant));
  plant.botanical.motion.leafFlutter = 0;
  expect(geometryKey(plant)).not.toBe(originalKey);
});

test("cooked botanical motion round-trips and rejects malformed or negative weights", () => {
  const plant = coniferPlant();
  plant.branches = 3;
  const artifact = compileVegetation(plant);
  const serialized = serializeArtifact(artifact);
  const roundTrip = deserializeArtifact(serialized);
  expect(serializeArtifact(roundTrip)).toEqual(serialized);
  const malformed = structuredClone(serialized) as { surfaces: { mesh: { wind: number[] } }[] };
  malformed.surfaces[0].mesh.wind.pop();
  expect(() => deserializeArtifact(malformed)).toThrow("Malformed cooked mesh layout");
  const negative = structuredClone(serialized) as typeof malformed;
  negative.surfaces[0].mesh.wind[1] = -1;
  expect(() => deserializeArtifact(negative)).toThrow("Malformed cooked mesh layout");
});

test("extreme conifer output stays within cookable geometry budgets and discloses loss", () => {
  const plant = coniferPlant();
  plant.branches = 96;
  plant.botanical.conifer.shootsPerLimb = 20;
  plant.botanical.conifer.needlesPerShoot = 160;
  plant.botanical.canopy.density = 1;
  plant.botanical.damage.brokenBranches = 0;
  const mesh = coniferMeshes(plant, "export", false, "triangles");
  expect(mesh.structure.branches.length).toBeLessThanOrEqual(MAX_CONIFER_BRANCHES);
  expect(mesh.foliage.positions.length / 3).toBeLessThanOrEqual(MAX_BOTANICAL_VERTICES);
  expect(mesh.trunk.positions.length / 3).toBeLessThanOrEqual(MAX_BOTANICAL_VERTICES);
  expect(mesh.truncated).toBe(true);
});

test("stand reviews use deterministic seed, maturity, orientation and position variation", () => {
  const plant = coniferPlant();
  const stand = createVegetationStand(plant);
  expect(createVegetationStand(plant)).toEqual(stand);
  expect(stand.instances).toHaveLength(plant.botanical.review.standCount);
  expect(new Set(stand.documents.map((document) => document.seed)).size).toBe(5);
  expect(new Set(stand.documents.map((document) => document.botanical?.age)).size).toBe(5);
  expect(new Set(stand.instances.map((instance) => instance.rotation?.[1])).size).toBe(
    stand.instances.length,
  );
  const source = contentKey(plant);
  const first = stand.documents[0].botanical;
  if (!first) throw new Error("Missing stand botanical source");
  first.age = 0;
  expect(contentKey(plant)).toBe(source);
});
