import { expect, test } from "bun:test";
import { referenceProject } from "@wrela/examples";
import { identityMatrix, type RenderSurface } from "@wrela/model";
import { packSurface } from "./packing";

test("water packing carries automatic quality, reference controls and finite shutter independently", () => {
  const water = referenceProject().documents.find((document) => document.kind === "water");
  if (!water || water.kind !== "water") throw new Error("Missing water fixture");
  const surface: RenderSurface = {
    id: water.id,
    source: water.id,
    matrix: identityMatrix(),
    material: {
      color: [0.1, 0.2, 0.3],
      secondary: [0.1, 0.2, 0.3],
      roughness: 0.18,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
    mesh: {
      positions: new Float32Array(),
      normals: new Float32Array(),
      indices: new Uint32Array(),
      bounds: { min: [0, 0, 0], max: [0, 0, 0] },
    },
    water,
  };
  for (const [quality, encoded] of [
    ["low", 1],
    ["balanced", 2],
    ["high", 3],
  ] as const) {
    for (const [mode, control] of [
      ["auto", 0],
      ["reference", 1],
      ["direct", 2],
      ["regular", 3],
    ] as const) {
      const data = packSurface({ ...surface, waterAppearance: { quality, mode, shutterSeconds: 2.25 } });
      expect(data[38]).toBe(encoded);
      expect(data[46]).toBe(control);
      expect(data[45]).toBe(2.25);
      expect(data[44]).toBeCloseTo(water.waves[1].phase, 6);
    }
  }
  expect(packSurface(surface)[38]).toBe(2);
  expect(packSurface(surface)[46]).toBe(0);
  expect(packSurface(surface)[45]).toBeCloseTo(1 / 60, 8);
  for (const shutterSeconds of [-1, Number.NaN, Number.POSITIVE_INFINITY, 4.01]) {
    expect(packSurface({ ...surface, waterAppearance: { mode: "auto", shutterSeconds } })[45]).toBe(0);
  }
});
