import {
  AuthoringSession,
  authorCreatureExpression,
  authorCreatureReview,
  type Operation,
} from "@wrela/authoring";
import { authorCreatureMantle, type CreatureMantleRecipe } from "@wrela/authoring/creature-mantle";
import { createCreatureFixture } from "@wrela/examples";
import { applyBipedAnatomy, type BipedAnatomyRecipe } from "@wrela/examples/creature-biped-construction";
import { applyBipedWalk, type BipedWalkRecipe } from "@wrela/examples/creature-biped-motion";
import {
  type CharacterDefinition,
  contentKey,
  createSurfaceAppearance,
  parseProject,
  type Vec3,
} from "@wrela/model";

export const CREATURE_FITTING_VARIANTS = ["pilgrim", "broad-keeper", "long-mantle"] as const;
export type CreatureFittingVariant = (typeof CREATURE_FITTING_VARIANTS)[number];
const recipes: Record<
  CreatureFittingVariant,
  {
    anatomy: BipedAnatomyRecipe;
    mantle: Pick<CreatureMantleRecipe, "shoulderWidth" | "backDepth" | "length" | "flare">;
    walk: BipedWalkRecipe;
    color: Vec3;
  }
> = {
  pilgrim: {
    anatomy: { build: 0.88, headScale: 0.84, crownReach: 0.85 },
    mantle: { shoulderWidth: 0.67, backDepth: 0.235, length: 1.11, flare: 0.06 },
    walk: { stride: 0.42, duration: 2.4, weightShift: 0.055 },
    color: [0.23, 0.13, 0.068],
  },
  "broad-keeper": {
    anatomy: { build: 1.2, shoulderBreadth: 1.16, pelvisBreadth: 1.1, headScale: 0.91, crownReach: 0.65 },
    mantle: { shoulderWidth: 0.76, backDepth: 0.28, length: 0.86, flare: 0.08 },
    walk: { stride: 0.33, duration: 2.8, weightShift: 0.045 },
    color: [0.14, 0.18, 0.15],
  },
  "long-mantle": {
    anatomy: { build: 0.75, shoulderBreadth: 0.92, pelvisBreadth: 0.9, headScale: 0.8, crownReach: 1.08 },
    mantle: { shoulderWidth: 0.64, backDepth: 0.225, length: 1.38, flare: 0.11 },
    walk: { stride: 0.36, duration: 2.65, weightShift: 0.05 },
    color: [0.18, 0.125, 0.16],
  },
};

/** Reusable anatomy and garment recipes produce ordinary source; the final
 * corner edit goes through the same live fitting transaction as Studio. */
export function createCreatureFittingLookdev(variant: CreatureFittingVariant = "pilgrim") {
  const recipe = recipes[variant];
  if (!recipe) throw Error("Unknown creature fitting variant");
  const fixture = createCreatureFixture("reed-penitent");
  const index = fixture.project.documents.findIndex((document) => document.id === fixture.characterId);
  const original = fixture.project.documents[index];
  if (original.kind !== "character") throw Error("Missing fitting character");
  fixture.project.documents[index] = applyBipedWalk(applyBipedAnatomy(original, recipe.anatomy), recipe.walk);
  const session = new AuthoringSession(fixture.project);
  const character = () => session.inspect(fixture.characterId) as CharacterDefinition;
  const linen = fixture.project.documents.find((document) => document.id === "marsh-linen");
  if (!linen || linen.kind !== "material") throw Error("Missing wardrobe material");
  const appearance = createSurfaceAppearance("fabric");
  appearance.response = { sheen: 0.32, anisotropy: 0.23 };
  const wardrobeSetup: Operation[] = [
    {
      kind: "document.create",
      document: {
        ...structuredClone(linen),
        id: "pilgrim-ochre-wool",
        name: "Pilgrim weathered wool",
        color: recipe.color,
        secondary: recipe.color.map((value) => value * 1.08) as Vec3,
        roughness: 0.9,
        pattern: "weave",
        scale: 90,
        normalStrength: 0.012,
        appearance,
      },
    },
    { kind: "document.set", target: "sallow-skin", path: ["color"], value: [0.29, 0.255, 0.205] },
    { kind: "document.set", target: "sallow-skin", path: ["secondary"], value: [0.32, 0.281, 0.226] },
    { kind: "document.set", target: "sallow-skin", path: ["scale"], value: 5 },
    { kind: "document.set", target: "sallow-skin", path: ["normalStrength"], value: 0.006 },
    { kind: "document.set", target: "peat-antler", path: ["pattern"], value: "noise" },
    { kind: "document.set", target: "peat-antler", path: ["color"], value: [0.19, 0.16, 0.12] },
    { kind: "document.set", target: "peat-antler", path: ["secondary"], value: [0.24, 0.205, 0.16] },
    { kind: "document.set", target: "marsh-linen", path: ["color"], value: [0.125, 0.11, 0.083] },
    { kind: "document.set", target: "marsh-linen", path: ["secondary"], value: [0.14, 0.124, 0.097] },
    { kind: "document.set", target: "marsh-linen", path: ["pattern"], value: "noise" },
    { kind: "document.set", target: "creature-review-sky", path: ["sunAzimuth"], value: -2.45 },
    { kind: "document.set", target: "creature-review-sky", path: ["sunElevation"], value: 0.9 },
    { kind: "document.set", target: "creature-review-light", path: ["ambient"], value: 0.65 },
    {
      kind: "document.set",
      target: "creature-review-light",
      path: ["lights"],
      value: [
        { id: "key", type: "directional", position: [-3, 5, -4], color: [1, 0.95, 0.86], intensity: 3.2 },
        { id: "rear-fill", type: "point", position: [2, 2.8, -3], color: [0.8, 0.88, 1], intensity: 25 },
        { id: "face-fill", type: "point", position: [1.5, 3.3, 2.8], color: [1, 0.94, 0.87], intensity: 30 },
      ],
    },
    { kind: "field.remove", target: fixture.characterId, node: "shoulder-wrap" },
  ];
  for (const side of ["left", "right"]) {
    const region = character().creature?.regions.find((item) => item.id === `cloth-${side}`);
    const chart = character().creature?.charts.find((item) => item.id === `cloth-${side}-surface`);
    if (!region || chart?.kind !== "patch") throw Error("Missing original cloth region");
    wardrobeSetup.push(
      { kind: "creature.region", target: fixture.characterId, value: { ...region, nodeIds: [] } },
      { kind: "field.remove", target: fixture.characterId, node: `cloth-panel-${side}` },
      {
        kind: "creature.chart",
        target: fixture.characterId,
        correspondence: "preserve",
        value: {
          ...chart,
          points: [
            [-0.13, 0, 0],
            [0.13, 0, 0],
            [-0.085, -0.66, 0.08],
            [0.115, -0.71, 0.07],
          ],
        },
      },
    );
  }
  for (const layer of character().creature?.appearance ?? []) {
    if (layer.id.startsWith("cloth-"))
      wardrobeSetup.push({
        kind: "creature.appearance",
        target: fixture.characterId,
        value: { ...layer, color: [0.22, 0.19, 0.145], variation: 0.045, scale: 80, displacement: 0.0004 },
      });
  }
  session.apply({ expectedRevision: 0, operations: wardrobeSetup });
  session.apply({
    expectedRevision: 1,
    operations: authorCreatureMantle(character(), {
      id: "pilgrim-mantle",
      region: "shoulder",
      material: "pilgrim-ochre-wool",
      collarOffset: [0.055, 0, 0.035],
      ...recipe.mantle,
    }),
  });
  const garment = "pilgrim-mantle";
  const cloth = character().creature?.cloth.find((item) => item.id === garment);
  if (!cloth?.fittingLandmarks) throw Error("Missing fitted mantle panel");
  const landmarks = cloth.fittingLandmarks;
  const corner = character().creature?.landmarks.find((entry) => entry.id === landmarks[2]);
  if (!corner) throw Error("Missing hem fitting landmark");
  const beforeFit = contentKey(character());
  const refitCorner = corner.position.map((value, axis) => value + (axis === 1 ? -0.025 : 0)) as Vec3;
  session.apply({
    expectedRevision: 2,
    operations: [
      { kind: "creature.landmark", target: fixture.characterId, value: { ...corner, position: refitCorner } },
      authorCreatureExpression(character(), {
        id: "pilgrim-resolve",
        joint: "jaw",
        rotation: [0.025, 0, 0],
        translation: [0, 0, 0],
        weight: 0.6,
      }),
      authorCreatureReview(character(), { id: "cloak-fitting-review", region: "shoulder", motion: "walk" }),
    ],
  });
  fixture.project = parseProject(JSON.parse(session.export()));
  return {
    ...fixture,
    fitting: {
      variant,
      recipe,
      garment,
      landmarks,
      refitCorner,
      beforeFit,
      afterFit: contentKey(character()),
      visualApproval: "pending",
      scope:
        "Parameterized anatomy, shoulder yoke and live fitted cloth gores; silhouette, closeup and supported walking require visual review",
    },
  };
}
