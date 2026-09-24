import { expect, test } from "bun:test";
import { createCreatureFixture } from "@wrela/examples/creature-fixtures";
import { contentKey } from "@wrela/model/math";
import { validateProject } from "@wrela/model/validation";
import { createCreatureCaptureRecipe } from "./creature-capture-recipes";

test("clay and skeleton recipes preserve exactly the accepted source on both body plans", () => {
  for (const id of ["ash-warden", "reed-penitent"] as const) {
    const before = contentKey(createCreatureFixture(id).project);
    for (const mode of ["clay", "skeleton"] as const) {
      const recipe = createCreatureCaptureRecipe(id, mode);
      expect(
        validateProject(recipe.project).diagnostics.filter((diagnostic) => diagnostic.severity === "error"),
      ).toEqual([]);
      expect(contentKey(recipe.project)).toBe(before);
      expect(recipe.sourceRevision).toBe(before);
      expect(recipe.sourceOperations).toHaveLength(0);
      expect(recipe.inspection.channel).toBe("clay");
      expect(recipe.inspection.hideGroom).toBe(true);
      expect(recipe.inspection.overlays).toEqual(mode === "skeleton" ? ["rig"] : []);
      expect(recipe.captures.every((capture) => capture.status === "pending")).toBe(true);
    }
    expect(contentKey(createCreatureFixture(id).project)).toBe(before);
  }
});
