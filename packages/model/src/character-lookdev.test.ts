import { expect, test } from "bun:test";
import { createCharacterLookdev } from "@wrela/examples/character-lookdev";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import { canonical } from "./math";
import { parseProject } from "./validation";

test("alpine sentinel remains fresh editable source and leaves the original Warden intact", () => {
  const original = canonical(createCreatureFixture("ash-warden"));
  const lookdev = createCharacterLookdev();
  const project = parseProject({
    schemaVersion: 1,
    id: "sentinel-study",
    name: "Sentinel study",
    documents: lookdev.documents,
    entry: "ash-warden-stage",
    recipes: [],
  });
  const character = project.documents.find((document) => document.id === lookdev.character);
  if (!character || character.kind !== "character") throw Error("Missing sentinel");
  expect(character.field.nodes.some((node) => node.id === "broken-pauldron")).toBe(false);
  expect(character.creature?.grooms.length).toBeGreaterThanOrEqual(4);
  expect(character.creature?.grooms.every((groom) => groom.maxCards <= 500)).toBe(true);
  expect(character.motions.some((motion) => motion.id === "turn")).toBe(true);
  expect(canonical(createCreatureFixture("ash-warden"))).toBe(original);
  expect(canonical(createCharacterLookdev())).toBe(canonical(lookdev));
});

test("Warden anatomy recipes vary mass without moving rig pivots or losing correspondence", () => {
  const studies = [
    createCharacterLookdev({ bodyMass: 0.82, chestDepth: 0.9, legStrength: 0.9 }),
    createCharacterLookdev({ bodyMass: 1.15, chestDepth: 1.1, legStrength: 1.18 }),
  ];
  const characters = studies.map((study) => {
    const project = parseProject({
      schemaVersion: 1,
      id: "warden-variant",
      name: "Warden variant",
      documents: study.documents,
      entry: "ash-warden-stage",
      recipes: [],
    });
    const character = project.documents.find((document) => document.id === study.character);
    if (character?.kind !== "character" || !character.creature) throw Error("Missing Warden variant");
    expect(character.creature.charts.find((chart) => chart.id === "shoulder-surface")?.realization).toBe(
      "correspondence-only",
    );
    expect(character.field.nodes.filter((node) => node.id.endsWith("-girdle"))).toHaveLength(4);
    return character;
  });
  expect(characters[0].joints).toEqual(characters[1].joints);
  expect(characters[0].creature?.anchors).toEqual(characters[1].creature?.anchors);
  const width = (index: number) =>
    characters[index].field.nodes.find((node) => node.id === "ribcage")?.size[0] ?? 0;
  expect(width(1)).toBeGreaterThan(width(0));
  expect(() => createCharacterLookdev({ legStrength: 5 })).toThrow();
});
