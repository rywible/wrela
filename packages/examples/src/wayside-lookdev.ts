import { createLookdevMaterials } from "@wrela/examples/material-lookdev";
import type { ObjectDefinition } from "@wrela/model";
import {
  applyConstructionHistory,
  assemblySchema,
  type ConstructionRecipe,
  createConstructionBuilder,
  createSurfaceHistoryPalette,
} from "@wrela/model";

/** Held-out construction: roofless wayside room with a narrow arched window, return wall and stone bench. */
export function createWaysideArchitecture(recipe: ConstructionRecipe) {
  const builder = createConstructionBuilder(recipe);
  builder.wall({
    id: "shelter-west",
    position: [-1.35, 0, 0],
    length: 1.35,
    courses: 7,
    depth: 0.58,
    collapse: 0.18,
  });
  builder.wall({
    id: "shelter-east",
    position: [1.35, 0, 0],
    length: 1.35,
    courses: 5,
    depth: 0.58,
    collapse: 0.3,
  });
  builder.wall({ id: "window-sill", position: [0, 0, 0], length: 1.3, courses: 3, depth: 0.58 });
  builder.arch({
    id: "window-vault",
    position: [0, 1.35, 0],
    innerRadius: 0.64,
    thickness: 0.3,
    depth: 0.58,
    stones: 9,
  });
  builder.wall({
    id: "shelter-return",
    position: [-2.04, 0, 1.64],
    length: 2.7,
    courses: 5,
    depth: 0.58,
    collapse: 0.5,
    yaw: Math.PI / 2,
  });
  builder.stone("bench-seat", 1.6, 0.16, [-1.02, 0.5, 1.24], 0.55);
  builder.stone("bench-foot-left", 0.28, 0.48, [-1.45, 0.24, 1.26], 0.45);
  builder.stone("bench-foot-right", 0.28, 0.48, [-0.46, 0.24, 1.26], 0.45);
  builder.rubble({ id: "shelter-collapse", center: [1.5, 0, 1.7], radius: [0.7, 0.9], count: 12, size: 0.3 });
  const assembly = assemblySchema.parse({
    parts: builder.parts,
    grid: 0.025,
    clearances: [
      { id: "shelter-entry", name: "Wayside standing entry", min: [-0.12, 0.15, 1.0], max: [0.75, 2.0, 2.9] },
    ],
  });
  const palette = createSurfaceHistoryPalette(
    createLookdevMaterials().filter((material) => material.id === recipe.stoneMaterial),
    recipe.history,
    "wayside-history",
  );
  const history = applyConstructionHistory(assembly, recipe, palette.material);
  const object: ObjectDefinition = {
    id: "alpine-lookdev-wayside",
    name: "Roofless alpine wayside shelter",
    kind: "object",
    schemaVersion: 1,
    dependencies: [...new Set(assembly.parts.map((part) => part.material ?? recipe.stoneMaterial))],
    material: recipe.stoneMaterial,
    collision: "mesh",
    assembly,
    field: {
      root: "wayside-envelope",
      resolution: 24,
      bounds: { min: [-2.8, -0.1, -0.3], max: [2.4, 2.4, 3.2] },
      nodes: [
        {
          id: "wayside-envelope",
          name: "Wayside assembly envelope",
          kind: "box",
          position: [0, 1.1, 1.1],
          rotation: [0, 0, 0],
          size: [2.5, 1.2, 1.8],
          radius: 0.1,
          blend: 0,
          children: [],
        },
      ],
    },
  };
  return { documents: [...palette.documents, object], object: object.id, recipe, history };
}
