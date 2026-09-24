import { expect, test } from "bun:test";
import { createLookdevMaterials, createSubstanceLookdevMaterials } from "@wrela/examples/material-lookdev";
import { materialSchema } from "./documents";

test("material lookdev covers every family with independent valid clean and coated source documents", () => {
  const clean = createSubstanceLookdevMaterials();
  const coated = createSubstanceLookdevMaterials(true);
  expect(clean.map((source) => source.appearance?.family)).toEqual([
    "generic",
    "skin",
    "foliage",
    "fabric",
    "metal",
    "glass",
  ]);
  for (const source of [...clean, ...coated, ...createLookdevMaterials()])
    expect(materialSchema.safeParse(source).success).toBe(true);
  expect(clean.every((source) => !source.appearance?.layers.length)).toBe(true);
  expect(coated.every((source) => source.appearance?.layers[0].mask.kind === "combined")).toBe(true);
  expect(new Set(clean.map((source) => source.id)).size).toBe(6);
});
