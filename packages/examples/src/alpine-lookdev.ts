import type { VegetationDefinition } from "@wrela/model";
import { alpineConiferSchema, botanicalPreset } from "@wrela/model";

/** Look development subject: young alpine lodgepole with live foliage down the lower crown. */
export function alpinePineLookdevDefinition(): VegetationDefinition {
  const botanical = botanicalPreset("pine");
  botanical.conifer = alpineConiferSchema.parse({
    whorlSize: 4,
    whorlJitter: 0.9,
    crownBias: 0.82,
    shootsPerLimb: 10,
    needlesPerShoot: 144,
    needleLength: 0.085,
    needleWidth: 0.0032,
    shootSpread: 0.68,
  });
  botanical.age = 0.85;
  botanical.canopy.density = 0.94;
  botanical.damage.brokenBranches = 0.035;
  botanical.damage.leafLoss = 0.025;
  botanical.growth.asymmetry = 0.5;
  botanical.motion.stiffness = 0.6;
  botanical.motion.branchSway = 0.55;
  botanical.motion.leafFlutter = 0.45;
  botanical.review.distances = [3, 14, 45];
  return {
    id: "alpine-pine",
    name: "Alpine lodgepole pine",
    kind: "vegetation",
    schemaVersion: 1,
    dependencies: [],
    seed: 73,
    height: 8,
    radius: 2.25,
    branches: 60,
    material: "alpine-lookdev-needles",
    trunkMaterial: "alpine-lookdev-bark",
    windResponse: 0.35,
    variation: 0.7,
    botanical,
  };
}

/** Shape-first specimen. The original lookdev definition remains the comparison control. */
export function shapedPineLookdevDefinition(): VegetationDefinition {
  const plant = alpinePineLookdevDefinition();
  const botanical = plant.botanical;
  if (!botanical) throw Error("Missing pine authoring");
  plant.id = "shaped-pine";
  plant.name = "Shaped lodgepole pine";
  plant.branches = 56;
  botanical.conifer = alpineConiferSchema.parse({
    architecture: { twigLength: 0.14, foliageStart: 0.38 },
    whorlSize: 4,
    whorlJitter: 0.85,
    crownStart: 0.1,
    crownBias: 0.9,
    crownPower: 0.7,
    limbSag: 0.14,
    trunkRadius: 0.12,
    shootsPerLimb: 6,
    shootSpread: 0.48,
    needlesPerShoot: 144,
    needleLength: 0.065,
    needleWidth: 0.0022,
  });
  botanical.canopy.density = 0.98;
  botanical.damage.brokenBranches = 0.015;
  botanical.damage.leafLoss = 0.025;
  return plant;
}
