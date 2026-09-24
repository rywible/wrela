import {
  creatureAnchorSchema,
  creatureAppearanceSchema,
  creatureArticulationSchema,
  creatureAttachmentSchema,
  creatureChartSchema,
  creatureClothSchema,
  creatureContactSchema,
  creatureCorrectiveSchema,
  creatureExpressionSchema,
  creatureGroomSchema,
  creatureIKChainSchema,
  creatureInfluenceRuleSchema,
  creatureLandmarkSchema,
  creaturePelvisSchema,
  creatureRegionSchema,
  creatureReviewScenarioSchema,
  creatureSchema,
  creatureSculptSchema,
  creatureSecondaryChainSchema,
  idSchema,
  vec3Schema,
} from "@wrela/model";

import { z } from "zod";

const scale = z.tuple([
  z.number().min(0.1).max(10),
  z.number().min(0.1).max(10),
  z.number().min(0.1).max(10),
]);
export const creatureDomains = [
  "attachments",
  "cloth",
  "regions",
  "landmarks",
  "charts",
  "anchors",
  "sculpts",
  "appearance",
  "grooms",
  "influenceRules",
  "correctives",
  "contacts",
  "ikChains",
  "secondaryChains",
  "expressions",
  "reviewScenarios",
] as const;
/** Commands reject unknown keys instead of silently losing an agent's authored source. */
export const creatureOperationSchemas = [
  z.strictObject({
    kind: z.literal("creature.pelvis"),
    target: idSchema,
    value: creaturePelvisSchema.nullable(),
  }),
  z.strictObject({
    kind: z.literal("creature.articulation"),
    target: idSchema,
    value: creatureArticulationSchema.nullable(),
  }),
  z.strictObject({
    kind: z.literal("creature.attachment"),
    target: idSchema,
    value: creatureAttachmentSchema,
  }),
  z.strictObject({ kind: z.literal("creature.cloth"), target: idSchema, value: creatureClothSchema }),
  z
    .object({ kind: z.literal("creature.initialize"), target: idSchema, source: creatureSchema.strict() })
    .strict(),
  z
    .object({ kind: z.literal("creature.region"), target: idSchema, value: creatureRegionSchema.strict() })
    .strict(),
  z
    .object({
      kind: z.literal("creature.landmark"),
      target: idSchema,
      value: creatureLandmarkSchema.strict(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.chart"),
      target: idSchema,
      value: creatureChartSchema,
      correspondence: z.enum(["preserve", "repair"]).optional(),
    })
    .strict(),
  z
    .object({ kind: z.literal("creature.anchor"), target: idSchema, value: creatureAnchorSchema.strict() })
    .strict(),
  z
    .object({ kind: z.literal("creature.sculpt"), target: idSchema, value: creatureSculptSchema.strict() })
    .strict(),
  z
    .object({
      kind: z.literal("creature.appearance"),
      target: idSchema,
      value: creatureAppearanceSchema.strict(),
    })
    .strict(),
  z
    .object({ kind: z.literal("creature.groom"), target: idSchema, value: creatureGroomSchema.strict() })
    .strict(),
  z
    .object({
      kind: z.literal("creature.influence"),
      target: idSchema,
      value: creatureInfluenceRuleSchema.strict(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.corrective"),
      target: idSchema,
      value: creatureCorrectiveSchema.strict(),
    })
    .strict(),
  z
    .object({ kind: z.literal("creature.contact"), target: idSchema, value: creatureContactSchema.strict() })
    .strict(),
  z
    .object({ kind: z.literal("creature.ik"), target: idSchema, value: creatureIKChainSchema.strict() })
    .strict(),
  z
    .object({
      kind: z.literal("creature.secondary"),
      target: idSchema,
      value: creatureSecondaryChainSchema.strict(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.expression"),
      target: idSchema,
      value: creatureExpressionSchema.strict(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.review"),
      target: idSchema,
      value: creatureReviewScenarioSchema.strict(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.remove"),
      target: idSchema,
      domain: z.enum(creatureDomains),
      id: idSchema,
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.proportion"),
      target: idSchema,
      region: idSchema,
      scale,
      preserve: z.array(idSchema).max(128).optional(),
      propagate: z.enum(["region", "descendants"]).optional(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("creature.move"),
      target: idSchema,
      region: idSchema,
      translation: vec3Schema,
      preserve: z.array(idSchema).max(128).optional(),
      propagate: z.enum(["region", "descendants"]).optional(),
    })
    .strict(),
] as const;
export const creatureOperationSchema = z.discriminatedUnion("kind", creatureOperationSchemas);
export type CreatureOperation = z.infer<typeof creatureOperationSchema>;
export const creatureCommandDescriptions: Record<CreatureOperation["kind"], string> = {
  "creature.pelvis": "Replace or clear bounded pelvis adaptation for anatomical contacts",
  "creature.articulation":
    "Replace or clear articulated skeletal physics with explicit bodies and constraints",
  "creature.attachment": "Mount source components on persistent anatomical anchors with required clearance",
  "creature.cloth": "Author pinned cloth or membranes with bounded dynamics and explicit source charts",
  "creature.initialize": "Attach a new, validated semantic creature description",
  "creature.region": "Create or replace an anatomical region",
  "creature.landmark":
    "Create or replace an anatomical landmark, refitting its bound cloth and mounted components atomically",
  "creature.chart": "Create or replace an editable sweep or patch with explicit correspondence revision",
  "creature.anchor": "Place or repair a persistent surface anchor",
  "creature.sculpt": "Create or replace a bounded local displacement layer",
  "creature.appearance": "Author correlated anatomical surface appearance",
  "creature.groom": "Author a deterministic coat with guides and local masks",
  "creature.influence": "Author anatomical skinning restrictions",
  "creature.corrective": "Author a pose-dependent local correction",
  "creature.contact": "Author a planted contact interval",
  "creature.ik": "Author a bounded limb constraint solver",
  "creature.secondary": "Author secondary chain dynamics",
  "creature.expression": "Author an editable facial or body expression",
  "creature.review": "Author reproducible creature review cameras and thresholds",
  "creature.remove": "Remove source only when its remaining dependencies stay valid",
  "creature.proportion": "Scale anatomical fields and dependent source while retaining chart coordinates",
  "creature.move": "Translate anatomical fields and dependent source coherently",
};
