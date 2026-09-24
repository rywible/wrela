import { expect, test } from "bun:test";
import { alpinePineLookdevDefinition } from "@wrela/examples";
import { botanicalDevelopmentSchema, botanicalPreset } from "@wrela/model";
import { developmentalStructure } from "./botanical-growth-structure";
import { deserializeArtifact, serializeArtifact } from "./cooked";
import { compileVegetation } from "./vegetation";

for (const species of ["lodgepole-pine", "paper-birch"] as const)
  test(`${species} development compiles, cooks and preserves surface frames`, () => {
    const doc = alpinePineLookdevDefinition();
    if (species === "paper-birch") doc.botanical = botanicalPreset("birch");
    if (!doc.botanical) throw Error("Missing fixture");
    doc.botanical.development = botanicalDevelopmentSchema.parse({ species, steps: 12 });
    const structure = developmentalStructure(doc);
    expect(structure.branches.length).toBeGreaterThan(20);
    const product = compileVegetation(doc, "review");
    expect(product.diagnostics).toEqual([]);
    for (const s of product.surfaces) {
      expect(s.mesh.indices.length).toBeGreaterThan(0);
      expect(s.mesh.positions.every(Number.isFinite)).toBe(true);
      expect(s.mesh.wind?.every(Number.isFinite)).toBe(true);
      if (s.mesh.materialCoordinates) expect(s.mesh.materialCoordinates.length).toBe(s.mesh.positions.length);
    }
    const restored = deserializeArtifact(serializeArtifact(product));
    expect(restored).toEqual(product);
  });

test("mature held-out specimens retain complete cookable geometry", () => {
  for (const species of ["lodgepole-pine", "paper-birch"] as const)
    for (const seed of [1009, 8928, 16847]) {
      const doc = alpinePineLookdevDefinition();
      doc.seed = seed;
      if (species === "paper-birch") doc.botanical = botanicalPreset("birch");
      if (!doc.botanical) throw Error("Missing source");
      doc.botanical.development = botanicalDevelopmentSchema.parse({ species, steps: 24 });
      const artifact = compileVegetation(doc, "review");
      expect(artifact.diagnostics).toEqual([]);
      for (const surface of artifact.surfaces) {
        expect(surface.mesh.positions.length / 3).toBeLessThanOrEqual(240000);
        expect(surface.mesh.materialCoordinates?.length ?? surface.mesh.positions.length).toBe(
          surface.mesh.positions.length,
        );
      }
      expect(deserializeArtifact(serializeArtifact(artifact)).kind).toBe("vegetation");
    }
}, 30000);
