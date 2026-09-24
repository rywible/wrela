import { expect, test } from "bun:test";
import { createSurfaceAppearance, identityMatrix, type RenderSurface } from "@wrela/model";

import {
  cachedSurfaceRadiance,
  materialSpecialization,
  specializeMaterialShader,
} from "./material-specialization";
import { shader } from "./shader";

const surface = (): RenderSurface => ({
  id: "test",
  source: "test",
  matrix: identityMatrix(),
  mesh: {
    positions: new Float32Array(),
    normals: new Float32Array(),
    indices: new Uint32Array(),
    bounds: { min: [0, 0, 0], max: [0, 0, 0] },
  },
  material: {
    color: [0.2, 0.3, 0.1],
    secondary: [0.2, 0.3, 0.1],
    roughness: 0.8,
    metallic: 0,
    pattern: 0,
    scale: 1,
    normalStrength: 0,
  },
});
test("specialization requires source proof and falls back after feature edits", () => {
  const s = surface();
  expect(materialSpecialization(s)).toBe("plain");
  s.material.pattern = 1;
  expect(materialSpecialization(s)).toBeUndefined();
  s.material.appearance = createSurfaceAppearance("foliage");
  s.material.creature = { family: "skin" };
  expect(materialSpecialization(s)).toBe("foliage");
  for (const key of ["wetness", "weathering", "dirt", "damage"] as const) {
    s.material.appearance[key] = 0.1;
    expect(materialSpecialization(s)).toBeUndefined();
    s.material.appearance[key] = 0;
  }
  s.material.creature.clearcoat = 0.2;
  expect(materialSpecialization(s)).toBeUndefined();
  s.material.creature = { family: "hard" };
  s.material.appearance = createSurfaceAppearance();
  s.material.appearance.detail.kind = "bark";
  expect(materialSpecialization(s)).toBe("bark");
});
test("partial evaluation removes only exact component references", () => {
  const code = specializeMaterialShader(shader, "foliage");
  expect(code).not.toContain("obj.surface.x");
  expect(code).toContain("obj.surfaceHistory.y");
  expect(code).toContain("obj.creatureScatter.xyz");
  expect(code).toContain("obj.creature.z");
  expect(code).toContain("fn shadow");
  expect(code).toContain("irradianceBasis(normal,8u)");
  expect(code).not.toContain("irradianceBasis(normal,i)");
  expect(code).toContain("physicalAerialPerspective");
});

test("cached rough reflection specialization withdraws during blends and edits", () => {
  const s = surface();
  s.mesh.radianceFlatNormals = true;
  s.mesh.radianceProbes = new Float32Array();
  s.radianceWeight = 1;
  expect(cachedSurfaceRadiance(s)).toBe(true);
  s.radianceWeight = 0.5;
  expect(cachedSurfaceRadiance(s)).toBe(false);
  s.radianceWeight = 1;
  s.material.roughness = 0.2;
  expect(cachedSurfaceRadiance(s)).toBe(true);
  s.material.roughness = 0.8;
  s.material.creature = { family: "hard" };
  expect(cachedSurfaceRadiance(s)).toBe(false);
  s.material.creature = undefined;
  s.mesh.radianceFlatNormals = false;
  expect(cachedSurfaceRadiance(s)).toBe(false);
});
