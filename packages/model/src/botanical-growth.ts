import { z } from "zod";
import type { Vec3 } from "./math";

export const BOTANICAL_GROWTH_VERSION = "development-1" as const;
const unit = z.number().min(0).max(1);
const vector = z.tuple([z.number().finite(), z.number().finite(), z.number().finite()]);
export const growthEnvironmentSchema = z.object({
  light: unit.default(1),
  water: unit.default(1),
  fertility: unit.default(1),
  neighbors: z
    .array(
      z.object({
        id: z.string().min(1).max(100),
        center: vector,
        radius: z.number().min(0.05).max(100),
        height: z.number().min(0.1).max(100),
        opacity: unit.default(0.85),
      }),
    )
    .max(32)
    .default([]),
});
export const growthEventSchema = z.discriminatedUnion("kind", [
  z.object({
    id: z.string().min(1).max(100),
    step: z.number().int().min(1).max(64),
    kind: z.literal("prune"),
    organ: z.string().min(1).max(256),
  }),
  z.object({
    id: z.string().min(1).max(100),
    step: z.number().int().min(1).max(64),
    kind: z.literal("damage"),
    organ: z.string().min(1).max(256),
  }),
  z.object({
    id: z.string().min(1).max(100),
    step: z.number().int().min(1).max(64),
    kind: z.literal("environment"),
    environment: growthEnvironmentSchema,
  }),
]);
export const botanicalDevelopmentSchema = z
  .object({
    version: z.literal(BOTANICAL_GROWTH_VERSION).default(BOTANICAL_GROWTH_VERSION),
    species: z.enum(["lodgepole-pine", "paper-birch"]),
    /** An uncalibrated developmental interval, deliberately not a calendar year. */
    steps: z.number().int().min(0).max(64).default(16),
    environment: growthEnvironmentSchema.prefault({}),
    events: z.array(growthEventSchema).max(256).default([]),
  })
  .superRefine((value, ctx) => {
    if (new Set(value.events.map((event) => event.id)).size !== value.events.length)
      ctx.addIssue({ code: "custom", message: "Growth event IDs must be unique", path: ["events"] });
    if (
      new Set(value.environment.neighbors.map((neighbor) => neighbor.id)).size !==
      value.environment.neighbors.length
    )
      ctx.addIssue({
        code: "custom",
        message: "Neighbor IDs must be unique",
        path: ["environment", "neighbors"],
      });
  });
export type BotanicalDevelopment = z.infer<typeof botanicalDevelopmentSchema>;
export type GrowthEnvironment = z.infer<typeof growthEnvironmentSchema>;
export type GrowthEvent = z.infer<typeof growthEventSchema>;
export type GrowthSpecies = BotanicalDevelopment["species"];
export type OrganState = "living" | "dead" | "pruned";
export type GrowthShoot = {
  id: string;
  parent: string | null;
  attachment?: number;
  habit?: "long" | "short";
  axis: string;
  birth: number;
  order: number;
  start: Vec3;
  end: Vec3;
  direction: Vec3;
  radius: number;
  tipRadius: number;
  state: OrganState;
  vigor: number;
  shadeSteps: number;
  foliage: { birth: number; count: number; retained: number; tint: number };
};
export type GrowthBud = {
  id: string;
  parent: string | null;
  attachment?: number;
  habit?: "long" | "short";
  birth: number;
  order: number;
  direction: Vec3;
  terminal: boolean;
  state: "active" | "dormant" | "dead";
  lastExtension: number;
  reason:
    | "seed"
    | "extended"
    | "apical-control"
    | "shade"
    | "resources"
    | "removed"
    | "capacity"
    | "short-shoot";
};
export type BotanicalGrowthState = {
  version: typeof BOTANICAL_GROWTH_VERSION;
  sourceKey: string;
  seed: number;
  species: GrowthSpecies;
  step: number;
  /** Model resource units, not measured carbon or biomass. */
  reserve: number;
  individual: { vigor: number; branching: number; angle: number; foliage: number };
  environment: GrowthEnvironment;
  shoots: GrowthShoot[];
  buds: GrowthBud[];
  appliedEvents: string[];
  ledger: {
    step: number;
    start: number;
    assimilated: number;
    maintenance: number;
    growth: number;
    end: number;
    lost: number;
  }[];
  limited: boolean;
};

const organId = z.string().min(1).max(256);
const organState = z.enum(["living", "dead", "pruned"]);
const step = z.number().int().min(0).max(64);
const resource = z.number().finite().min(0).max(1e6);
/** Portable checkpoint decoding validates bounds before the compiler validates graph semantics. */
export const botanicalGrowthStateSchema: z.ZodType<BotanicalGrowthState> = z
  .object({
    version: z.literal(BOTANICAL_GROWTH_VERSION),
    sourceKey: z.string().max(1_000_000),
    seed: z.number().int(),
    species: z.enum(["lodgepole-pine", "paper-birch"]),
    step,
    reserve: resource,
    individual: z
      .object({ vigor: resource, branching: resource, angle: z.number().finite(), foliage: resource })
      .strict(),
    environment: growthEnvironmentSchema,
    shoots: z
      .array(
        z
          .object({
            id: organId,
            parent: organId.nullable(),
            attachment: unit.optional(),
            habit: z.enum(["long", "short"]).optional(),
            axis: organId,
            birth: step,
            order: z.number().int().min(0).max(4),
            start: vector,
            end: vector,
            direction: vector,
            radius: resource,
            tipRadius: resource,
            state: organState,
            vigor: unit,
            shadeSteps: step,
            foliage: z
              .object({ birth: step, count: z.number().int().min(0).max(1000), retained: unit, tint: unit })
              .strict(),
          })
          .strict(),
      )
      .max(8192),
    buds: z
      .array(
        z
          .object({
            id: organId,
            parent: organId.nullable(),
            attachment: unit.optional(),
            habit: z.enum(["long", "short"]).optional(),
            birth: step,
            order: z.number().int().min(0).max(4),
            direction: vector,
            terminal: z.boolean(),
            state: z.enum(["active", "dormant", "dead"]),
            lastExtension: step,
            reason: z.enum([
              "seed",
              "extended",
              "apical-control",
              "shade",
              "resources",
              "removed",
              "capacity",
              "short-shoot",
            ]),
          })
          .strict(),
      )
      .max(24576),
    appliedEvents: z.array(organId).max(256),
    ledger: z
      .array(
        z
          .object({
            step,
            start: resource,
            assimilated: resource,
            maintenance: resource,
            growth: resource,
            end: resource,
            lost: resource,
          })
          .strict(),
      )
      .max(64),
    limited: z.boolean(),
  })
  .strict();
export const persistentVegetationGrowthSchema = z
  .object({
    version: z.literal(1),
    definition: z.string().min(1).max(100),
    definitionKey: z.string().min(1).max(128),
    seed: z.number().int(),
    development: botanicalDevelopmentSchema,
    checkpoint: botanicalGrowthStateSchema,
  })
  .strict();
export type PersistentVegetationGrowth = z.infer<typeof persistentVegetationGrowthSchema>;
