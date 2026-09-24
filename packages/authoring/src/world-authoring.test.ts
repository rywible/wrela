import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { type WorldDefinition, worldCompositionSchema } from "@wrela/model";
import { addWorldLayout, duplicateAssemblyPlacement } from "./world-authoring";

test("layout commands create schema-valid immutable source and reusable placements", () => {
  let world = referenceProject().documents.find((item) => item.kind === "world") as WorldDefinition;
  for (const kind of ["assembly", "path", "room", "biome", "landmark", "encounter", "streaming"] as const) {
    const before = JSON.stringify(world);
    const composition = addWorldLayout(world, kind, "prop");
    expect(worldCompositionSchema.safeParse(composition).success).toBe(true);
    expect(JSON.stringify(world)).toBe(before);
    world = { ...world, composition };
  }
  if (!world.composition) throw new Error("Missing composition");
  const composition = duplicateAssemblyPlacement(world.composition, "assembly-1");
  expect(composition.placements[1].assembly).toBe(composition.placements[0].assembly);
  expect(composition.placements[1].id).not.toBe(composition.placements[0].id);
  expect(world.composition.placements).toHaveLength(1);
});

test("route extension continues the last direction without duplicate control points", async () => {
  const { appendWorldPathPoint } = await import("./world-authoring");
  const world = referenceProject().documents.find((item) => item.kind === "world") as WorldDefinition;
  const composition = addWorldLayout(world, "path");
  const result = appendWorldPathPoint(composition, "path-1");
  expect(result.paths[0].points.at(-1)).toEqual([15, 0, 0]);
  expect(composition.paths[0].points).toHaveLength(2);
  expect(worldCompositionSchema.safeParse(result).success).toBe(true);
});

test("deleting an assembly or entry route leaves valid relationships in the same transaction", async () => {
  const { removeWorldLayoutElement } = await import("./world-authoring");
  const world = referenceProject().documents.find((item) => item.kind === "world") as WorldDefinition;
  const assembly = addWorldLayout(world, "assembly", "prop");
  expect(removeWorldLayoutElement(assembly, "assemblies", "assembly-1").placements).toHaveLength(0);
  expect(assembly.placements).toHaveLength(1);
  const composition = addWorldLayout(world, "path");
  composition.review = {
    actorRadius: 0.35,
    actorHeight: 1.8,
    maxStepHeight: 0.3,
    entryPath: "path-1",
    sightline: { from: [0, 1, 0], to: [2, 1, 0] },
  };
  expect(removeWorldLayoutElement(composition, "paths", "path-1").review?.entryPath).toBeUndefined();
});
