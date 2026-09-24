import { expect, test } from "bun:test";
import {
  createSurfaceAppearance,
  createSurfaceLayer,
  materialSchema,
  type RenderMaterial,
  surfaceAppearanceSchema,
} from "@wrela/model";

import { applySurfaceAppearance, evaluateSurfaceMask, surfaceReviewWarnings } from "./surface-appearance";

const material: RenderMaterial = {
  color: [0.2, 0.4, 0.1],
  secondary: [1, 1, 1],
  roughness: 0.7,
  metallic: 0,
  pattern: 0,
  scale: 1,
  normalStrength: 0,
};
test("legacy materials remain unchanged and each substance resolves a real runtime response", () => {
  expect(applySurfaceAppearance(material)).toBe(material);
  expect(applySurfaceAppearance({ ...material, appearance: createSurfaceAppearance("metal") }).metallic).toBe(
    1,
  );
  expect(
    applySurfaceAppearance({ ...material, appearance: createSurfaceAppearance("foliage") }).creature
      ?.thickness,
  ).toBe(0.0005);
  expect(
    applySurfaceAppearance({ ...material, appearance: createSurfaceAppearance("skin") }).creature?.family,
  ).toBe("skin");
  expect(
    applySurfaceAppearance({ ...material, appearance: createSurfaceAppearance("fabric") }).creature?.sheen,
  ).toBe(0.4);
  const explicit = { family: "fiber" as const, anisotropy: 0.7 };
  expect(
    applySurfaceAppearance({ ...material, appearance: createSurfaceAppearance("skin"), creature: explicit })
      .creature,
  ).toBe(explicit);
});
test("height, slope, noise and inverse masks preserve authored spatial intent", () => {
  const mask = createSurfaceLayer("dust").mask;
  expect(evaluateSurfaceMask({ ...mask, kind: "slope" }, [0, 0, 0], [0, 1, 0])).toBe(1);
  expect(evaluateSurfaceMask({ ...mask, kind: "slope" }, [0, 0, 0], [1, 0, 0])).toBe(0);
  const band = { ...mask, kind: "height" as const, minimumHeight: 2, maximumHeight: 4 };
  expect(evaluateSurfaceMask(band, [0, 3, 0], [0, 1, 0])).toBe(1);
  expect(evaluateSurfaceMask(band, [0, 6, 0], [0, 1, 0])).toBe(0);
  const normal = evaluateSurfaceMask(mask, [0, 0, 0], [0, 1, 0], 0.55);
  expect(normal + evaluateSurfaceMask({ ...mask, invert: true }, [0, 0, 0], [0, 1, 0], 0.55)).toBeCloseTo(1);
});
test("schema rejects ambiguous layers and nonfinite values and material roundtrips appearance", () => {
  const layer = createSurfaceLayer("dust");
  expect(surfaceAppearanceSchema.safeParse({ wetness: Number.NaN }).success).toBe(false);
  expect(surfaceAppearanceSchema.safeParse({ layers: [layer, layer] }).success).toBe(false);
  expect(
    surfaceAppearanceSchema.safeParse({
      layers: [{ ...layer, mask: { ...layer.mask, minimumHeight: 5, maximumHeight: 2 } }],
    }).success,
  ).toBe(false);
  const appearance = createSurfaceAppearance("glass");
  const authored = materialSchema.parse({
    ...material,
    id: "glass",
    name: "Glass",
    kind: "material",
    schemaVersion: 1,
    dependencies: [],
    pattern: "solid",
    appearance,
  });
  expect(authored.appearance).toEqual(appearance);
  expect(surfaceReviewWarnings(appearance)[0]).toContain("refraction");
});

test("combined masks restrict deposits to noisy upper surfaces inside a height band", () => {
  const mask = {
    ...createSurfaceLayer("deposit").mask,
    kind: "combined" as const,
    minimumHeight: 2,
    maximumHeight: 4,
  };
  expect(evaluateSurfaceMask(mask, [0, 3, 0], [0, 1, 0], 0.9)).toBe(1);
  expect(evaluateSurfaceMask(mask, [0, 3, 0], [1, 0, 0], 0.9)).toBe(0);
  expect(evaluateSurfaceMask(mask, [0, 6, 0], [0, 1, 0], 0.9)).toBe(0);
  expect(evaluateSurfaceMask(mask, [0, 3, 0], [0, 1, 0], 0.1)).toBe(0);
  expect(
    evaluateSurfaceMask({ ...mask, slopeInfluence: 0.4, heightInfluence: 0.75 }, [0, 6, 0], [1, 0, 0], 0.9),
  ).toBeCloseTo(0.15);
  const sample = evaluateSurfaceMask(mask, [0, 3, 0], [0, 0.5, 0], 0.55);
  expect(sample + evaluateSurfaceMask({ ...mask, invert: true }, [0, 3, 0], [0, 0.5, 0], 0.55)).toBeCloseTo(
    1,
  );
});

test("subpixel noise coatings retain occupancy instead of vanishing at a threshold", () => {
  const mask = { ...createSurfaceLayer("distant-rust").mask, threshold: 0.75, softness: 0.1, scale: 10 };
  expect(evaluateSurfaceMask(mask, [0, 0, 0], [0, 1, 0], 0.5, 0)).toBe(0);
  expect(evaluateSurfaceMask(mask, [0, 0, 0], [0, 1, 0], 0.5, 1)).toBeCloseTo(0.25);
  expect(evaluateSurfaceMask({ ...mask, invert: true }, [0, 0, 0], [0, 1, 0], 0.5, 1)).toBeCloseTo(0.75);
});

test("authored thickness, scattering, sheen and coat reach runtime without overriding creature optics", () => {
  const skin = createSurfaceAppearance("skin");
  skin.response = { thickness: 0.012, subsurface: 0.6, scatterColor: [0.8, 0.3, 0.2], clearcoat: 0.25 };
  const rendered = applySurfaceAppearance({ ...material, appearance: skin });
  expect(rendered.creature?.thickness).toBe(0.012);
  expect(rendered.creature?.subsurface).toBe(0.6);
  expect(rendered.creature?.scatterColor).toEqual([0.8, 0.3, 0.2]);
  expect(rendered.creature?.clearcoat).toBe(0.25);
  const fabric = createSurfaceAppearance("fabric");
  fabric.response = { anisotropy: -0.6, sheen: 0.8 };
  expect(applySurfaceAppearance({ ...material, appearance: fabric }).creature).toMatchObject({
    family: "fiber",
    anisotropy: -0.6,
    sheen: 0.8,
  });
  const explicit = { family: "skin" as const, thickness: 0.001 };
  expect(applySurfaceAppearance({ ...material, appearance: skin, creature: explicit }).creature).toBe(
    explicit,
  );
  expect(surfaceAppearanceSchema.safeParse({ response: { thickness: -0.1 } }).success).toBe(false);
  expect(surfaceAppearanceSchema.safeParse({ response: { anisotropy: 1 } }).success).toBe(false);
});
