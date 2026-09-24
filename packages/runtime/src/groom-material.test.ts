import { expect, test } from "bun:test";
import type { RenderMaterial } from "@wrela/model";

import { resolveGroomMaterial } from "./groom-material";

const generated: RenderMaterial = {
  color: [1, 1, 1],
  secondary: [1, 1, 1],
  roughness: 0.65,
  metallic: 0,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
  creature: { family: "fiber", anisotropy: 0.65, fiberDirection: [0, 0, -1], sheen: 0.6 },
};

test("absolute groom colors are not multiplied by a dark material twice", () => {
  const source: RenderMaterial = {
    color: [0.05, 0.1, 0.2],
    secondary: [0.2, 0.3, 0.4],
    roughness: 0.8,
    metallic: 0.1,
    pattern: 2,
    scale: 24,
    normalStrength: 0.4,
  };
  const result = resolveGroomMaterial(generated, source);
  expect(result.color).toEqual([1, 1, 1]);
  expect(result.secondary[0]).toBeCloseTo(4);
  expect(result.secondary[1]).toBeCloseTo(3);
  expect(result.secondary[2]).toBeCloseTo(2);
  expect(result.roughness).toBe(0.8);
  expect(result.metallic).toBe(0.1);
  expect(result.pattern).toBe(2);
  expect(result.scale).toBe(24);
  expect(result.creature?.family).toBe("fiber");
  expect(result.creature?.fiberDirection).toEqual([0, 0, -1]);
  const root = [0.12, 0.15, 0.18];
  expect(root.map((c, axis) => c * result.color[axis])).toEqual(root);
  expect(source.color).toEqual([0.05, 0.1, 0.2]);
});
test("source optical edits remain live without changing compiled guide color or geometry", () => {
  const source = {
    ...generated,
    roughness: 0.2,
    creature: {
      family: "skin" as const,
      subsurface: 0.4,
      fiberDirection: [1, 0, 0] as [number, number, number],
    },
  };
  const result = resolveGroomMaterial(generated, source);
  expect(result.roughness).toBe(0.2);
  expect(result.creature?.subsurface).toBe(0.4);
  expect(result.creature?.family).toBe("fiber");
  expect(result.creature?.fiberDirection).toEqual([0, 0, -1]);
  expect(resolveGroomMaterial(generated)).toEqual(generated);
  expect(
    resolveGroomMaterial(generated, { ...source, color: [0, 0, 0], secondary: [1, 1, 1] }).secondary,
  ).toEqual([1, 1, 1]);
});
