import { expect, test } from "bun:test";
import { coniferFamilyFixture, referenceProject } from "./fixtures";
import { migrateProjectSource } from "./migrations";
import { instantiateRecipe, realizeRecipeProject } from "./recipes";
import { parseProject, validateProject } from "./validation";

test("parameterized families produce deterministic editable source and survive project round trips", () => {
  const family = coniferFamilyFixture(),
    project = referenceProject();
  project.documents.push(...family.documents);
  project.recipes = family.recipes;
  project.recipeInstances = family.recipeInstances;
  const reopened = parseProject(JSON.parse(JSON.stringify(project)));
  expect(reopened).toEqual(project);
  expect(coniferFamilyFixture()).toEqual(family);
  expect(family.documents.map((document) => (document.kind === "vegetation" ? document.height : 0))).toEqual([
    7, 3, 5,
  ]);
  expect(family.documents[0].generated?.policy).toBe("detached");
  expect(family.documents[2].generated?.recipe?.overrides).toEqual([{ path: ["windResponse"], value: 0.8 }]);
  const edited = reopened.documents.find((document) => document.id === "family-pine-young");
  if (!edited) throw new Error("Missing family member");
  edited.name = "Hand edited";
  expect(parseProject(reopened).documents.find((document) => document.id === "family-pine-young")?.name).toBe(
    "Hand edited",
  );
  expect(() => realizeRecipeProject(reopened)).toThrow("explicit replacement");
  expect(
    realizeRecipeProject(reopened, { replace: true }).documents.find(
      (document) => document.id === "family-pine-young",
    )?.name,
  ).toBe("Young alpine pine");
});
test("recipes reject unsafe paths, invalid parameters, unavailable versions and source identity writes", () => {
  const {
    recipes: [recipe],
    recipeInstances: [instance],
  } = coniferFamilyFixture();
  expect(() => instantiateRecipe(recipe, { ...instance, parameters: { stature: 100 } })).toThrow("range");
  expect(() => instantiateRecipe(recipe, { ...instance, parameters: { unknown: 1 } })).toThrow("Unknown");
  expect(() => instantiateRecipe(recipe, { ...instance, version: "future" })).toThrow("unavailable");
  expect(() =>
    instantiateRecipe(recipe, { ...instance, overrides: [{ path: ["__proto__", "x"], value: 1 }] }),
  ).toThrow("Unsafe");
  expect(() =>
    instantiateRecipe(recipe, { ...instance, overrides: [{ path: ["id"], value: "replacement" }] }),
  ).toThrow("identities");
});
test("explicit legacy migration is lossless, reports its step, and never coerces future source", () => {
  const original = referenceProject(),
    legacy = JSON.parse(JSON.stringify(original));
  legacy.schemaVersion = 0;
  for (const document of legacy.documents) {
    delete document.schemaVersion;
    delete document.dependencies;
  }
  const before = JSON.stringify(legacy),
    migrated = migrateProjectSource(legacy);
  expect(migrated.from).toBe(0);
  expect(migrated.steps).toHaveLength(1);
  expect(JSON.stringify(legacy)).toBe(before);
  expect(parseProject(legacy)).toEqual(original);
  expect(validateProject(legacy).diagnostics.some((diagnostic) => diagnostic.code === "migration")).toBe(
    true,
  );
  expect(() => parseProject({ ...original, schemaVersion: 99 })).toThrow("Unsupported project schema");
});
