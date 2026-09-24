import { z } from "zod";

export { creaturePatchHasConsistentOrientation } from "./creature-fitting";
export { creaturePatchPoint } from "./creature-patch";

// Kept independent of documents.ts so creature source remains a reusable schema.
const id = z
  .string()
  .min(1)
  .max(100)
  .regex(/^[a-zA-Z0-9_-]+$/)
  .refine(
    (value) => !["__proto__", "prototype", "constructor"].includes(value),
    "Reserved creature identity",
  );
const finite = z.number().finite();
const vec3 = z.tuple([finite, finite, finite]);
const positive = finite.positive().max(200);
const unit = finite.min(0).max(1);
const color = z.tuple([unit, unit, unit]);
const patchOffsets = z
  .array(z.array(vec3).min(2).max(33))
  .min(2)
  .max(17)
  .superRefine((grid, context) => {
    if (grid.length < 2 || grid.some((row) => row.length < 2)) return;
    const columns = grid[0].length;
    if (grid.some((row) => row.length !== columns))
      context.addIssue({ code: "custom", message: "Patch control rows must have equal lengths" });
    for (const row of [grid[0], grid[grid.length - 1]])
      for (const corner of [row[0], row[row.length - 1]])
        if (corner.some((value) => Math.abs(value) > 1e-9))
          context.addIssue({
            code: "custom",
            message: "Patch corner offsets must remain zero for live fitting",
          });
  });
const ids = z.array(id).max(128).default([]);
export const creatureFrameSchema = z.strictObject({ position: vec3, rotation: vec3 });
export const creatureRegionSchema = z.strictObject({
  id,
  name: z.string().min(1).max(120),
  parent: id.optional(),
  mirror: id.optional(),
  nodeIds: ids,
  jointIds: ids,
  frame: creatureFrameSchema,
  extent: z.tuple([positive, positive, positive]),
  material: id.optional(),
});
export const creatureLandmarkSchema = z.strictObject({ id, region: id, position: vec3 });
const chartBase = {
  id,
  region: id,
  revision: z.number().int().min(0),
  material: id.optional(),
  realization: z.enum(["surface", "correspondence-only"]).optional(),
};
export const creatureChartSchema = z.discriminatedUnion("kind", [
  z.strictObject({
    ...chartBase,
    kind: z.literal("sweep"),
    points: z.array(vec3).min(2).max(128),
    radii: z.array(positive).min(2).max(128),
    crossSections: z
      .array(z.tuple([positive, positive]))
      .min(2)
      .max(128)
      .optional(),
    twists: z.array(finite).min(2).max(128).optional(),
    caps: z.boolean().default(true),
  }),
  z.strictObject({
    ...chartBase,
    kind: z.literal("patch"),
    points: z.tuple([vec3, vec3, vec3, vec3]),
    thickness: positive,
    /** Smooth [v][u] offsets from the fitted corner surface; corners remain authoritative. */
    controlOffsets: patchOffsets.optional(),
  }),
]);
/** Region-local without a chart. Sweep: (path fraction, turn fraction, radial scale).
 * Patch: (u, v, thickness-normalized displacement). Revisions detect topology ambiguity. */
export const creatureAnchorSchema = z.strictObject({
  id,
  region: id,
  /** Optional live binding to a region-local anatomical landmark. */
  landmark: id.optional(),
  chart: id.optional(),
  chartRevision: z.number().int().min(0).optional(),
  coordinates: vec3,
  offset: finite.min(-100).max(100),
  purpose: z.enum(["scar", "groom", "attachment", "landmark"]),
  tolerance: positive,
});
export const creatureSculptSchema = z.strictObject({
  id,
  region: id,
  center: vec3,
  radius: positive,
  displacement: vec3,
  strength: finite.min(-10).max(10),
  falloff: positive,
  mirror: z.boolean().default(false),
  support: z.strictObject({ radii: z.tuple([positive, positive, positive]), rotation: vec3 }).optional(),
  path: z.array(vec3).min(2).max(32).optional(),
  mode: z.enum(["push", "flatten", "inflate"]).optional(),
  detail: z
    .strictObject({ maxEdgeLength: finite.min(0.002).max(2), passes: z.number().int().min(1).max(6) })
    .optional(),
  nodeIds: z.array(id).min(1).max(128).optional(),
});
export const creatureAppearanceSchema = z.strictObject({
  anchor: id.optional(),
  id,
  region: id,
  material: id.optional(),
  family: z.enum(["skin", "eye", "wet", "cloth", "hair", "hard"]),
  growthSuppression: unit.default(0),
  color,
  roughness: finite.min(0.02).max(1),
  metallic: unit.default(0),
  subsurface: unit.default(0),
  transmission: unit.default(0),
  anisotropy: finite.min(-1).max(1).default(0),
  direction: vec3.default([0, 1, 0]),
  scale: positive.default(1),
  variation: unit.default(0),
  displacement: finite.min(-1).max(1).default(0),
  mask: z.strictObject({ center: vec3, radius: positive, falloff: positive }).optional(),
});
export const creatureGroomSchema = z.strictObject({
  rootProjection: z
    .strictObject({
      maxDistance: positive,
      direction: z.enum(["outward", "both"]).default("outward"),
      nodeIds: z.array(id).min(1).max(128).optional(),
    })
    .optional(),
  representation: z.enum(["tufts", "ribbons"]).default("tufts"),
  ribbonThickness: finite.min(0.001).max(1).default(0.08),
  chartRevision: z.number().int().min(0).optional(),
  lift: unit.default(0.65),
  frizz: unit.default(0),
  stiffness: finite.min(0).max(10000).default(35),
  damping: finite.min(0).max(1000).default(6),
  id,
  region: id,
  chart: id.optional(),
  material: id.optional(),
  seed: z.number().int(),
  density: finite.min(0).max(100000),
  length: positive,
  width: positive,
  direction: vec3,
  guides: z
    .array(z.strictObject({ id, points: z.array(vec3).min(2).max(64) }))
    .max(128)
    .default([]),
  taper: unit,
  clump: unit,
  curl: finite.min(0).max(20),
  rootColor: color,
  tipColor: color,
  masks: z
    .array(z.strictObject({ center: vec3, radius: positive, strength: unit }))
    .max(64)
    .default([]),
  maxCards: z.number().int().min(0).max(20000),
  lodFractions: z.array(unit).min(1).max(6).default([1, 0.5, 0.2]),
});
export const creatureInfluenceRuleSchema = z.strictObject({
  id,
  region: id,
  allowedJoints: ids,
  excludedJoints: ids,
  rigidJoint: id.optional(),
});
export const creatureCorrectiveSchema = z.strictObject({
  id,
  region: id,
  joint: id,
  axis: z.enum(["x", "y", "z"]),
  angle: finite.min(-Math.PI).max(Math.PI),
  radius: positive,
  center: vec3,
  displacement: vec3,
});
export const creatureContactSchema = z.strictObject({
  id,
  motion: id,
  joint: id,
  start: finite.min(0),
  end: finite.min(0),
  target: vec3,
  space: z.enum(["character", "world"]),
  weight: unit,
  tolerance: positive,
  blendIn: finite.min(0).max(10).default(0.08),
  blendOut: finite.min(0).max(10).default(0.08),
  ground: z.boolean().default(true),
  offset: finite.min(-100).max(100).default(0),
});
export const creatureIKChainSchema = z.strictObject({
  id,
  joints: z.array(id).min(2).max(32),
  target: vec3,
  pole: vec3,
  weight: unit,
  iterations: z.number().int().min(1).max(64),
  tolerance: positive,
});
export const creatureSecondaryChainSchema = z.strictObject({
  id,
  joints: z.array(id).min(2).max(32),
  stiffness: finite.min(0).max(10000),
  damping: finite.min(0).max(1000),
  gravity: vec3,
  wind: vec3,
  maxAngle: finite.min(0).max(Math.PI),
  collisionRadius: finite.min(0).max(10),
  weight: unit.default(1),
});
export const creatureExpressionSchema = z.strictObject({
  id,
  weights: z.array(z.strictObject({ joint: id, rotation: vec3, translation: vec3 })).max(64),
  weight: unit,
});
export const creatureReviewScenarioSchema = z.strictObject({
  id,
  name: z.string().min(1).max(120),
  motion: id.optional(),
  duration: finite.positive().max(120),
  sampleRate: finite.min(1).max(120),
  cameras: z
    .array(z.strictObject({ id, position: vec3, target: vec3 }))
    .min(1)
    .max(16),
  thresholds: z.strictObject({
    contactSlip: positive,
    penetration: positive,
    anchorError: positive,
    stretch: positive,
  }),
});
export const creatureAttachmentSchema = z.strictObject({
  id,
  anchor: id,
  nodeIds: z.array(id).min(1).max(128),
  offset: vec3.default([0, 0, 0]),
  rigidJoint: id.optional(),
  /** Seat primitive support above the anchor plane before applying its offset. */
  placement: z.literal("surface").optional(),
  minimumClearance: finite.min(0).max(100).default(0),
});
export const creatureClothSchema = z.strictObject({
  simulationResolution: z.number().int().min(2).max(16).optional(),
  id,
  region: id,
  chart: id,
  chartRevision: z.number().int().min(0),
  /** Patch corners in u0v0, u1v0, u0v1, u1v1 order, kept live by landmark edits. */
  fittingLandmarks: z.tuple([id, id, id, id]).optional(),
  material: id.optional(),
  pinEdges: z
    .array(z.enum(["u0", "u1", "v0", "v1"]))
    .max(4)
    .default([]),
  pins: z
    .array(z.strictObject({ coordinates: z.tuple([unit, unit]), anchor: id, weight: unit.optional() }))
    .max(128)
    .default([]),
  stiffness: unit.default(1),
  bendStiffness: unit.default(0.2),
  damping: unit.default(0.04),
  gravity: vec3.default([0, -9.81, 0]),
  wind: vec3.default([0, 0, 0]),
  iterations: z.number().int().min(1).max(32).default(8),
  collisionRadius: finite.min(0).max(1).default(0.01),
  maxStretch: finite.min(1).max(2).default(1.1),
});
export const creatureArticulationSchema = z.strictObject({
  bodies: z
    .array(
      z.strictObject({
        joint: id,
        mass: finite.positive().max(1000),
        radius: finite.positive().max(10),
        halfHeight: finite.min(0).max(100).optional(),
        offset: vec3.optional(),
      }),
    )
    .min(1)
    .max(64),
  joints: z
    .array(
      z.strictObject({
        id,
        parent: id,
        child: id,
        kind: z.enum(["spherical", "revolute", "fixed"]),
        axis: vec3.optional(),
        minimum: finite.optional(),
        maximum: finite.optional(),
      }),
    )
    .max(63),
  recoveryStiffness: finite.min(0).max(500).optional(),
  recoveryDamping: finite.min(0).max(100).optional(),
  friction: finite.min(0).max(2).optional(),
  restitution: unit.optional(),
});
export const creaturePelvisSchema = z.strictObject({
  joint: id,
  maxOffset: z.tuple([finite.min(0).max(10), finite.min(0).max(10), finite.min(0).max(10)]),
  weight: unit.default(1),
  iterations: z.number().int().min(1).max(8).default(3),
});
export const creatureSchema = z.strictObject({
  schemaVersion: z.literal(1),
  articulation: creatureArticulationSchema.optional(),
  pelvis: creaturePelvisSchema.optional(),
  attachments: z.array(creatureAttachmentSchema).max(128).default([]),
  cloth: z.array(creatureClothSchema).max(32).default([]),
  regions: z.array(creatureRegionSchema).max(128).default([]),
  landmarks: z.array(creatureLandmarkSchema).max(256).default([]),
  charts: z.array(creatureChartSchema).max(128).default([]),
  anchors: z.array(creatureAnchorSchema).max(1024).default([]),
  sculpts: z.array(creatureSculptSchema).max(256).default([]),
  appearance: z.array(creatureAppearanceSchema).max(128).default([]),
  grooms: z.array(creatureGroomSchema).max(64).default([]),
  influenceRules: z.array(creatureInfluenceRuleSchema).max(128).default([]),
  correctives: z.array(creatureCorrectiveSchema).max(256).default([]),
  contacts: z.array(creatureContactSchema).max(256).default([]),
  ikChains: z.array(creatureIKChainSchema).max(32).default([]),
  secondaryChains: z.array(creatureSecondaryChainSchema).max(32).default([]),
  expressions: z.array(creatureExpressionSchema).max(64).default([]),
  reviewScenarios: z.array(creatureReviewScenarioSchema).max(32).default([]),
});
export type CreatureDefinition = z.infer<typeof creatureSchema>;
export type CreatureRegion = z.infer<typeof creatureRegionSchema>;
export type CreatureLandmark = z.infer<typeof creatureLandmarkSchema>;
export type CreatureChart = z.infer<typeof creatureChartSchema>;
export type CreatureAnchor = z.infer<typeof creatureAnchorSchema>;
export type CreatureSculpt = z.infer<typeof creatureSculptSchema>;
export type CreatureAppearance = z.infer<typeof creatureAppearanceSchema>;
export type CreatureGroom = z.infer<typeof creatureGroomSchema>;
export type CreatureInfluenceRule = z.infer<typeof creatureInfluenceRuleSchema>;
export type CreatureCorrective = z.infer<typeof creatureCorrectiveSchema>;
export type CreatureContact = z.infer<typeof creatureContactSchema>;
export type CreatureIKChain = z.infer<typeof creatureIKChainSchema>;
export type CreatureSecondaryChain = z.infer<typeof creatureSecondaryChainSchema>;
export type CreatureExpression = z.infer<typeof creatureExpressionSchema>;
export type CreatureReviewScenario = z.infer<typeof creatureReviewScenarioSchema>;

/** Optional material-family response; all lengths are metres and directions are rest-local. */
export const creatureMaterialSchema = z.strictObject({
  family: z.enum(["skin", "fiber", "cloth", "eye", "wet", "hard"]),
  subsurface: unit.optional(),
  transmission: unit.optional(),
  thickness: finite.min(0).max(100).optional(),
  scatterColor: color.optional(),
  fiberDirection: vec3.optional(),
  anisotropy: finite.min(-0.95).max(0.95).optional(),
  sheen: unit.optional(),
  clearcoat: unit.optional(),
  clearcoatRoughness: unit.optional(),
  frame: z.strictObject({ origin: vec3, tangent: vec3, normal: vec3 }).optional(),
});
export type CreatureMaterial = z.infer<typeof creatureMaterialSchema>;

export type CreatureAttachment = z.infer<typeof creatureAttachmentSchema>;
export type CreatureCloth = z.infer<typeof creatureClothSchema>;

export type CreatureArticulation = z.infer<typeof creatureArticulationSchema>;

export type CreaturePelvis = z.infer<typeof creaturePelvisSchema>;
