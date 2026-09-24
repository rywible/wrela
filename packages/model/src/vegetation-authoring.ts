import { z } from "zod";
import { botanicalDevelopmentSchema } from "./botanical-growth";
import type { VegetationDefinition } from "./documents";

const unit = z.number().min(0).max(1);
export const botanicalSpeciesSchema = z.enum(["pine", "oak", "birch", "shrub", "fern", "grass"]);
/** Optional constrained architecture; absent sources retain the original conifer generator. */
export const pineArchitectureSchema = z.object({
  version: z.literal("pine-architecture-1").default("pine-architecture-1"),
  sharedShoots: z.boolean().default(true),
  canopyVisibility: z.boolean().default(true),
  twigPairs: z.number().int().min(1).max(4).default(3),
  twigLength: z.number().min(0.08).max(0.6).default(0.26),
  limbThickness: z.number().min(0.15).max(0.5).default(0.32),
  crownAsymmetry: unit.optional(),
  clusterVariation: unit.optional(),
  cohortContrast: unit.optional(),
  foliageStart: z.number().min(0.1).max(0.75).default(0.3),
});
export const alpineConiferSchema = z.object({
  architecture: pineArchitectureSchema.optional(),
  whorlSize: z.number().int().min(2).max(6).default(4),
  whorlJitter: unit.default(0.45),
  crownStart: z.number().min(0.04).max(0.5).default(0.11),
  crownBias: z.number().min(0.6).max(1.5).default(1),
  crownPower: z.number().min(0.3).max(1.8).default(0.72),
  limbSag: unit.default(0.28),
  trunkRadius: z.number().min(0.025).max(0.6).default(0.1),
  trunkFlare: unit.default(0.6),
  shootsPerLimb: z.number().int().min(4).max(20).default(14),
  shootSpread: z.number().min(0.2).max(1).default(0.62),
  needlesPerShoot: z.number().int().min(16).max(160).default(64),
  needleLength: z.number().min(0.02).max(0.12).default(0.055),
  needleWidth: z.number().min(0.0008).max(0.006).default(0.0018),
});
export type AlpineConiferAuthoring = z.infer<typeof alpineConiferSchema>;
export const botanicalSchema = z.object({
  species: botanicalSpeciesSchema.default("pine"),
  /** 0 is a sapling, 1 is the authored mature size. */
  age: unit.default(1),
  conifer: alpineConiferSchema.optional(),
  development: botanicalDevelopmentSchema.optional(),
  growth: z
    .object({
      levels: z.number().int().min(1).max(3).default(2),
      children: z.number().int().min(1).max(4).default(3),
      branchAngle: z.number().min(0.1).max(1.5).default(1.05),
      lengthRatio: z.number().min(0.15).max(0.8).default(0.48),
      taper: z.number().min(0.15).max(0.8).default(0.45),
      tropism: z.number().min(-1).max(1).default(0.15),
      asymmetry: unit.default(0.3),
    })
    .prefault({}),
  canopy: z
    .object({
      density: unit.default(0.8),
      clusters: z.number().int().min(1).max(8).default(4),
      leavesPerCluster: z.number().int().min(2).max(12).default(7),
      leafLength: z.number().min(0.01).max(1.5).default(0.2),
      leafWidth: z.number().min(0.003).max(0.8).default(0.065),
      droop: unit.default(0.2),
    })
    .prefault({}),
  damage: z
    .object({
      brokenBranches: unit.default(0),
      leafLoss: unit.default(0),
    })
    .prefault({}),
  pruning: z
    .object({
      clearTrunk: unit.default(0),
      radiusLimit: z.number().min(0.1).max(20).default(20),
      removedBranches: z.array(z.string().min(1).max(100)).max(256).default([]),
    })
    .prefault({}),
  branchEdits: z
    .array(
      z.object({
        branch: z.string().min(1).max(100),
        lengthScale: z.number().min(0.05).max(2).default(1),
        bend: z
          .tuple([z.number().min(-1).max(1), z.number().min(-1).max(1), z.number().min(-1).max(1)])
          .default([0, 0, 0]),
        bare: z.boolean().default(false),
      }),
    )
    .max(256)
    .default([]),
  motion: z
    .object({
      stiffness: unit.default(0.35),
      branchSway: unit.default(0.4),
      leafFlutter: unit.default(0.3),
    })
    .prefault({}),
  review: z
    .object({
      distances: z.array(z.number().min(0.5).max(500)).min(1).max(4).default([3, 15, 60]),
      standCount: z.number().int().min(1).max(25).default(9),
      spacing: z.number().min(0.1).max(30).default(3),
      seedVariation: unit.default(0.6),
      ageVariation: unit.default(0.25),
    })
    .prefault({}),
});
export type BotanicalAuthoring = z.infer<typeof botanicalSchema>;
export type BotanicalSpecies = z.infer<typeof botanicalSpeciesSchema>;

/** Presets are editable semantic starting points, never hidden generator modes. */
export function botanicalPreset(species: BotanicalSpecies = "pine"): BotanicalAuthoring {
  const preset = botanicalSchema.parse({ species });
  if (species === "pine") {
    preset.growth.levels = 3;
    preset.canopy.clusters = 5;
    preset.canopy.leavesPerCluster = 10;
    preset.canopy.density = 0.95;
    preset.canopy.leafLength = 0.22;
    preset.canopy.leafWidth = 0.018;
  } else if (species === "oak" || species === "shrub") {
    preset.growth.levels = 3;
    preset.canopy.density = 0.95;
    preset.growth.branchAngle = 0.8;
    preset.growth.tropism = 0.4;
    preset.canopy.leafWidth = 0.12;
  } else if (species === "birch") {
    preset.growth.levels = 3;
    preset.growth.branchAngle = 0.65;
    preset.canopy.droop = 0.6;
    preset.canopy.leafWidth = 0.075;
  } else if (species === "fern") {
    preset.growth.levels = 1;
    preset.canopy.clusters = 6;
    preset.canopy.leavesPerCluster = 4;
    preset.canopy.density = 1;
    preset.growth.children = 4;
    preset.growth.branchAngle = 1.2;
    preset.canopy.leafLength = 0.24;
    preset.canopy.leafWidth = 0.07;
    preset.canopy.droop = 0.5;
  } else {
    preset.growth.levels = 1;
    preset.canopy.clusters = 1;
    preset.canopy.leavesPerCluster = 3;
    preset.canopy.leafLength = 0.65;
    preset.canopy.leafWidth = 0.028;
    preset.canopy.droop = 0.65;
  }
  return preset;
}

/** Shared by compilation and live instance updates so motion edits do not need a mesh rebuild. */
export function vegetationWindResponse(doc: VegetationDefinition): number {
  return Math.max(
    0,
    Math.min(2, doc.windResponse * (doc.botanical ? 1 - doc.botanical.motion.stiffness * 0.85 : 1)),
  );
}

/** Inspection and whole-plant stiffness are live; branch/leaf weights belong to geometry. */
export function botanicalGeometrySource(botanical: BotanicalAuthoring | undefined): unknown {
  if (!botanical) return undefined;
  const { review: _review, motion: _motion, ...geometry } = botanical;
  return {
    ...geometry,
    motion: { branchSway: botanical.motion.branchSway, leafFlutter: botanical.motion.leafFlutter },
  };
}
