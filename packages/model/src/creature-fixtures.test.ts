import { describe, expect, test } from "bun:test";
import { createCreatureFixture, creatureFixtureCatalog } from "@wrela/examples/creature-fixtures";
import type { CharacterDefinition } from "./documents";
import { contentKey } from "./math";
import { instantiateRecipe } from "./recipes";
import { validateProject } from "./validation";

describe("original authored creature studies", () => {
  for (const entry of creatureFixtureCatalog)
    test(`${entry.id} is portable, independently editable source`, () => {
      const fixture = createCreatureFixture(entry.id);
      expect(validateProject(JSON.parse(JSON.stringify(fixture.project))).diagnostics).toEqual([]);
      const character = fixture.project.documents.find(
        (document) => document.id === entry.id,
      ) as CharacterDefinition;
      expect(character.field.nodes.length).toBeGreaterThan(40);
      expect(character.creature!.anchors.length).toBeGreaterThanOrEqual(3);
      expect(character.creature!.contacts.length).toBeGreaterThan(4);
      expect(character.creature!.reviewScenarios.length).toBe(6);
      const untouched = contentKey(fixture.project);
      const second = createCreatureFixture(entry.id);
      second.project.documents[0].name = "Independent revision";
      expect(contentKey(fixture.project)).toBe(untouched);
      const recipe = fixture.project.recipes![0];
      const generated = instantiateRecipe(recipe, {
        id: "variant",
        name: "Broader study",
        recipe: recipe.id,
        version: recipe.version,
        parameters: { chestWidth: 0.6 },
      });
      expect(generated.kind).toBe("character");
      if (generated.kind === "character")
        expect(generated.field.nodes.find((node) => node.id === "ribcage")!.size[0]).toBe(0.6);
    });
  test("second creature challenges body plan, symmetry, and surface realization", () => {
    const first = createCreatureFixture("ash-warden").project.documents.find(
      (document) => document.id === "ash-warden",
    ) as CharacterDefinition;
    const second = createCreatureFixture("reed-penitent").project.documents.find(
      (document) => document.id === "reed-penitent",
    ) as CharacterDefinition;
    expect(first.creature!.grooms).toHaveLength(2);
    expect(first.creature!.ikChains).toHaveLength(4);
    expect(second.creature!.ikChains).toHaveLength(2);
    expect(
      second.creature!.charts.filter((chart) => chart.kind === "patch" && chart.id.startsWith("cloth")),
    ).toHaveLength(2);
    expect(second.joints.find((joint) => joint.id === "arm-left-upper")!.position[1]).not.toBe(
      second.joints.find((joint) => joint.id === "arm-right-upper")!.position[1],
    );
  });
});
