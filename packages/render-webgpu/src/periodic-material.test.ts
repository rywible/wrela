import { expect, test } from "bun:test";
import { identityMatrix, type RenderSurface, type Vec3 } from "@wrela/model";

import { packSurface } from "./packing";

test("woven phase preserves fractional positive and negative origins before float upload", () => {
  const surface: RenderSurface = {
    id: "cloth",
    source: "cloth",
    matrix: identityMatrix(),
    mesh: {
      positions: new Float32Array(),
      normals: new Float32Array(),
      indices: new Uint32Array(),
      bounds: { min: [0, 0, 0], max: [1, 1, 1] },
    },
    material: {
      color: [1, 1, 1],
      secondary: [0, 0, 0],
      roughness: 0.8,
      metallic: 0,
      pattern: 4,
      scale: 3.5,
      normalStrength: 0,
      domain: "world",
    },
  };
  const origin: Vec3 = [1000000000.125, 0, -1000000000.375];
  const data = packSurface(surface, origin);
  expect(data[118]).toBeCloseTo(0.4375 * 2 * Math.PI, 6);
  expect(data[119]).toBeCloseTo(-0.3125 * 2 * Math.PI, 6);
  surface.material.domain = "local";
  expect(Array.from(packSurface(surface, origin).slice(118, 120))).toEqual([0, 0]);
});
