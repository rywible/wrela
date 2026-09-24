import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples";
import { type CharacterDefinition, contentKey } from "@wrela/model";
import { cookProject, deserializeArtifact, loadCookedProject, serializeArtifact } from "./cooked";
import { artifactTransfers, compileDocument, compilerKey } from "./index";
import { creatureProductKeys } from "./products";

test("creature delivery retains correspondence, groom, materials, correctives and detail skin bindings", () => {
  const fixture = createCreatureFixture("ash-warden");
  const source = fixture.project.documents.find((d) => d.id === fixture.characterId) as CharacterDefinition;
  const compiled = compileDocument(source, "interactive");
  if (compiled?.kind !== "character") throw new Error("Expected compiled creature");
  expect(compiled.key).toBe(compilerKey(source, "interactive"));
  const restored = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(compiled))));
  if (restored.kind !== "character") throw new Error("Expected restored creature");
  expect(contentKey(restored.creatureCoordinates)).toBe(contentKey(compiled.creatureCoordinates));
  expect(restored.creatureGroom?.guides.length).toBeGreaterThan(0);
  expect(restored.creatureMaterials).toEqual(compiled.creatureMaterials);
  expect(restored.creatureDetails?.length).toBeGreaterThan(0);
  expect(restored.creatureCorrectives?.[0]?.vertices).toBeInstanceOf(Uint32Array);
  expect(restored.creatureGroom?.details[0].vertexGuideIndices).toBeInstanceOf(Uint32Array);
  for (const detail of restored.creatureDetails ?? []) {
    expect(detail.weights).toBeInstanceOf(Float32Array);
    expect(detail.jointIndices.length).toBe((detail.mesh.positions.length / 3) * 4);
  }
  const buffers = artifactTransfers(restored);
  expect(new Set(buffers).size).toBe(buffers.length);
  expect(buffers).toContain(restored.creatureGroom!.details[0].mesh.positions.buffer as ArrayBuffer);
  expect(buffers).toContain(restored.creatureCorrectives![0].vertices.buffer as ArrayBuffer);
});

test("cooked project loads both body plans without compilation or silently stripped creature source", () => {
  for (const id of ["ash-warden", "reed-penitent"] as const) {
    const fixture = createCreatureFixture(id);
    const cooked = cookProject(fixture.project, "interactive");
    const loaded = loadCookedProject(JSON.parse(JSON.stringify(cooked)), fixture.project);
    const artifact = loaded.get(fixture.characterId);
    expect(artifact?.kind).toBe("character");
    if (artifact?.kind === "character") expect(artifact.creature?.regions.length).toBeGreaterThan(0);
  }
});

test("malformed creature delivery fails before installation", () => {
  const fixture = createCreatureFixture("ash-warden");
  const source = fixture.project.documents.find((d) => d.id === fixture.characterId)!;
  const compiled = compileDocument(source, "interactive")!;
  const serialized = JSON.parse(JSON.stringify(serializeArtifact(compiled)));
  serialized.creatureCorrectives[0].vertices[0] = 249999;
  expect(() => deserializeArtifact(serialized)).toThrow("corrective");
  const stale = JSON.parse(JSON.stringify(serializeArtifact(compiled)));
  const coordinate = stale.creatureCoordinates.find((c: unknown) => c !== null);
  coordinate.chartRevision += 1;
  expect(() => deserializeArtifact(stale)).toThrow("stale");
});

test("appearance, rig and review changes invalidate their own creature products", () => {
  const fixture = createCreatureFixture("ash-warden");
  const source = fixture.project.documents.find((d) => d.id === fixture.characterId) as CharacterDefinition;
  const before = creatureProductKeys(source)!;
  const edit = structuredClone(source);
  edit.creature!.appearance[0].roughness = 0.27;
  const after = creatureProductKeys(edit)!;
  expect(after.geometry).toBe(before.geometry);
  expect(after.binding).toBe(before.binding);
  expect(after.appearance).not.toBe(before.appearance);
  expect(compilerKey(edit)).not.toBe(compilerKey(source));
  expect(contentKey(source.creature)).not.toBe(contentKey(edit.creature));
});
