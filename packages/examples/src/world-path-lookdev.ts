import type { Document, MaterialDefinition, ObjectDefinition } from "@wrela/model";
import {
  alpineSurfaceHistory,
  assemblySchema,
  constructionRandom,
  createSurfaceAppearance,
  createSurfaceHistoryMaterial,
  createSurfaceLayer,
  cutStoneProfile,
  type SurfaceHistory,
  sampleSurfaceHistory,
} from "@wrela/model";

/** Editable reclaimed flagstones make the authored route readable without a
 * renderer-only decal or a second hidden definition of the path geometry. */
export function createWorldPathLookdev(options: { seed?: number; history?: SurfaceHistory } = {}): {
  documents: Document[];
  object: string;
} {
  const seed = options.seed ?? 113;
  const history = options.history ?? alpineSurfaceHistory();
  const appearance = createSurfaceAppearance();
  appearance.detail = { kind: "mineral", scale: 2.5, strength: 0.65 };
  const soil = createSurfaceLayer("flagstone-earth");
  soil.color = [0.06, 0.046, 0.024];
  soil.roughness = 0.98;
  soil.coverage = 0.42;
  soil.mask = { ...soil.mask, kind: "noise", scale: 3, threshold: 0.5, softness: 0.2 };
  appearance.layers = [soil];
  const baseMaterial: MaterialDefinition = {
    id: "alpine-lookdev-trail-stone",
    name: "Worn trail flagstones",
    kind: "material",
    schemaVersion: 1,
    dependencies: [],
    color: [0.145, 0.124, 0.083],
    secondary: [0.19, 0.166, 0.114],
    roughness: 0.96,
    metallic: 0,
    pattern: "noise",
    scale: 3,
    normalStrength: 0,
    domain: "world",
    appearance,
  };
  const material = createSurfaceHistoryMaterial(
    baseMaterial,
    baseMaterial.id,
    sampleSurfaceHistory(history, { position: [0, 0.04, 0], normal: [0, 1, 0], contact: 1, shelter: 0.1 }),
  );
  const assembly = assemblySchema.parse({
    grid: 0.01,
    clearances: [],
    parts: [-1, 0, 1].map((column, index) => ({
      id: `flagstone-${index}`,
      name: `Trail flagstone ${index + 1}`,
      profile: cutStoneProfile([0.51, 0.58, 0.5][index], 0.07, seed, `trail-${index}`, history.damage),
      path: [
        [0, 0, -0.45 - constructionRandom(seed, `trail-${index}`, 1) * 0.04],
        [0, 0, 0.43 + constructionRandom(seed, `trail-${index}`, 2) * 0.06],
      ],
      position: [column * 0.59, 0.015 + index * 0.003, column * 0.024],
      rotation: [0, column * 0.027, column * 0.003],
      bevel: 0.004,
      endBevel: 0.004,
      material: material.id,
      collision: false,
      repeat: { count: 1, offset: [0, 0, 0] },
      sockets: [],
      wear: { amount: 0.34, scale: 6, seed: 17 + index * 29 },
    })),
  });
  const object: ObjectDefinition = {
    id: "alpine-lookdev-trail-module",
    name: "Reclaimed flagstone trail course",
    kind: "object",
    schemaVersion: 1,
    dependencies: [material.id],
    material: material.id,
    collision: "none",
    assembly,
    field: {
      root: "trail-placeholder",
      resolution: 24,
      bounds: { min: [-0.95, -0.1, -0.6], max: [0.95, 0.1, 0.6] },
      nodes: [
        {
          id: "trail-placeholder",
          name: "Trail assembly fallback",
          kind: "box",
          position: [0, 0, 0],
          rotation: [0, 0, 0],
          size: [0.9, 0.025, 0.5],
          radius: 0.01,
          blend: 0,
          children: [],
        },
      ],
    },
  };
  return { documents: [material, object], object: object.id };
}
