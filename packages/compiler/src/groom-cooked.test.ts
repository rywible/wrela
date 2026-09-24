import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples";
import { deserializeArtifact, serializeArtifact } from "./cooked";
import { compileCharacter } from "./surface";

test("generated fiber optical aliases and scoped fidelity survive cooked roundtrip", () => {
  const fixture = createCreatureFixture("ash-warden");
  const source = fixture.project.documents.find((document) => document.kind === "character");
  if (source?.kind !== "character") throw new Error("Missing fixture creature");
  if (source.creature?.grooms[0]) source.creature.grooms[0].representation = "ribbons";
  const artifact = compileCharacter(source, "interactive");
  const restored = deserializeArtifact(JSON.parse(JSON.stringify(serializeArtifact(artifact))));
  if (restored.kind !== "character") throw new Error("Missing restored character");
  expect(restored.creatureGroomMaterialSources).toEqual(artifact.creatureGroomMaterialSources);
  expect(restored.creatureGroom?.representation).toBe("mixed-opaque");
  expect(restored.creatureGroom?.guides[0].representation).toBe("ribbons");
  expect(restored.creatureGroom?.details.map((detail) => detail.fidelity)).toEqual(
    artifact.creatureGroom?.details.map((detail) => detail.fidelity),
  );
  for (const [generated, original] of Object.entries(artifact.creatureGroomMaterialSources ?? {})) {
    expect(source.creature?.grooms.some((groom) => groom.material === original)).toBe(true);
    const material = artifact.creatureMaterials?.find((candidate) => candidate.id === generated);
    expect(material?.color).toEqual([1, 1, 1]);
    expect(material?.creature?.family).toBe("fiber");
    expect(artifact.creatureGroom?.guides.some((guide) => guide.material === generated)).toBe(true);
  }
});
