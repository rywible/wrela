import { expect, test } from "bun:test";
import { shapedPineLookdevDefinition } from "@wrela/examples";
import { coniferMeshes } from "./conifer-mesh";
import { deserializeArtifact, serializeArtifact } from "./cooked";
import { artifactTransfers } from "./index";
import { compileShootInstances } from "./shoot-instances";
import { compileVegetation } from "./vegetation";

test("shared geometry realizes the identical authored shoot and retains every source occurrence", () => {
  const doc = shapedPineLookdevDefinition(),
    surfaces = compileShootInstances(doc)!;
  const id = `${doc.id}/b16/s2-0/t1-0`;
  const surface = surfaces.find((s) => s.mesh.shoots?.sourceIds.includes(id))!;
  const shoots = surface.mesh.shoots!,
    occurrence = shoots.sourceIds.indexOf(id),
    m = shoots.transforms.subarray(occurrence * 16, occurrence * 16 + 16);
  const reference = coniferMeshes(doc, "review", false, "triangles", id.slice(doc.id.length + 1)).foliage;
  expect(reference.positions.length).toBe(surface.mesh.positions.length);
  for (let i = 0; i < reference.positions.length; i += 3)
    for (let a = 0; a < 3; a++) {
      const p = surface.mesh.positions;
      const value = m[a] * p[i] + m[a + 4] * p[i + 1] + m[a + 8] * p[i + 2] + m[a + 12];
      expect(value).toBeCloseTo(reference.positions[i + a], 5);
    }
  const ids = surfaces.flatMap((s) => s.mesh.shoots!.sourceIds);
  expect(new Set(ids).size).toBe(ids.length);
  expect(ids.length).toBeGreaterThan(2000);
  expect(surfaces.length).toBe(8);
  expect(surfaces.reduce((n, s) => n + s.mesh.positions.length / 3, 0)).toBeLessThan(12000);
  doc.botanical!.branchEdits = [{ branch: "b16/s2-0/t1-0", bend: [0, 0.6, 0], lengthScale: 1, bare: false }];
  expect(compileShootInstances(doc)).toBeUndefined();
});

test("shared shoot cooking, transfer and cached document remapping retain occurrence identity", () => {
  const doc = shapedPineLookdevDefinition(),
    artifact = compileVegetation(doc);
  expect(deserializeArtifact(serializeArtifact(artifact))).toEqual(artifact);
  const transfers = artifactTransfers(artifact);
  expect(new Set(transfers).size).toBe(transfers.length);
  for (const surface of artifact.surfaces)
    if (surface.mesh.shoots) {
      expect(transfers).toContain(surface.mesh.shoots.transforms.buffer as ArrayBuffer);
      expect(transfers).toContain(surface.mesh.shoots.motion.buffer as ArrayBuffer);
    }
  const renamed = compileVegetation({ ...doc, id: "renamed-pine" });
  expect(new Set(renamed.surfaces.map((s) => s.id)).size).toBe(renamed.surfaces.length);
  expect(
    renamed.surfaces
      .flatMap((s) => s.mesh.shoots?.sourceIds ?? [])
      .every((id) => id.startsWith("renamed-pine/")),
  ).toBe(true);
  const malformed = serializeArtifact(artifact) as {
    surfaces: { mesh: { shoots?: { transforms: number[] } } }[];
  };
  malformed.surfaces.find((s) => s.mesh.shoots)!.mesh.shoots!.transforms[15] = 0;
  expect(() => deserializeArtifact(malformed)).toThrow();
});
