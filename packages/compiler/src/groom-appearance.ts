import {
  add,
  type CreatureAppearance,
  type CreatureDefinition,
  type MaterialDefinition,
  normalize,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

/** One shared mask drives visible appearance and coat suppression. Coordinates are region-local metres. */
export function creatureAppearanceCoverage(
  appearance: CreatureAppearance,
  local: Vec3,
  anchorOrigin?: Vec3,
): number {
  if (appearance.anchor && !anchorOrigin) return 0;
  if (!appearance.mask) return 1;
  const distance =
    Math.hypot(...sub(local, add(appearance.mask.center, anchorOrigin ?? [0, 0, 0]))) /
    appearance.mask.radius;
  if (distance >= 1) return 0;
  const t = 1 - distance;
  return (t * t * (3 - 2 * t)) ** appearance.mask.falloff;
}
export type CompiledCreatureAppearance = {
  materials: MaterialDefinition[];
  /** Source-order composition gives local exceptions deterministic, inspectable priority. */
  sample: (
    region: string,
    local: Vec3,
  ) => {
    material?: string;
    color: Vec3;
    roughness: number;
    coverage: number;
    growthCoverage: number;
    displacement: number;
    sources: string[];
  };
};
export function compileCreatureAppearance(
  creature: CreatureDefinition,
  toWorld: (region: string, point: Vec3) => Vec3,
  resolveAnchor?: (region: string, id: string) => Vec3 | undefined,
): CompiledCreatureAppearance {
  const anchorOrigins = new Map(
    creature.appearance
      .filter((source) => source.anchor)
      .map((source) => [source.id, resolveAnchor?.(source.region, source.anchor ?? "")]),
  );
  const materialId = (source: CreatureAppearance) =>
    source.material ?? `${source.id.slice(0, 80)}-appearance`;
  const materials: MaterialDefinition[] = creature.appearance.map((appearance) => {
    const origin = toWorld(appearance.region, [0, 0, 0]),
      direction = normalize(sub(toWorld(appearance.region, appearance.direction), origin));
    const normal = normalize(sub(toWorld(appearance.region, [0, 0, 1]), origin));
    const tangentCandidate =
      Math.abs(direction[0] * normal[0] + direction[1] * normal[1] + direction[2] * normal[2]) > 0.99
        ? normalize(sub(toWorld(appearance.region, [1, 0, 0]), origin))
        : direction;
    return {
      id: materialId(appearance),
      name: appearance.id,
      schemaVersion: 1,
      dependencies: [],
      kind: "material",
      color: appearance.color,
      secondary: appearance.color.map((c) => c * (1 - appearance.variation * 0.25)) as Vec3,
      roughness: Math.max(0.04, appearance.roughness),
      metallic: appearance.metallic,
      pattern: appearance.variation > 0 ? "stripes" : "solid",
      scale: Math.max(0.01, Math.min(100, appearance.scale)),
      normalStrength: Math.min(1, Math.abs(appearance.displacement)),
      domain: "local",
      creature: {
        family: appearance.family === "hair" ? "fiber" : appearance.family,
        subsurface: appearance.subsurface,
        transmission: appearance.transmission,
        fiberDirection: direction,
        anisotropy: Math.min(0.95, Math.max(-0.95, appearance.anisotropy)),
        sheen: appearance.family === "hair" || appearance.family === "cloth" ? 0.5 : 0,
        clearcoat: appearance.family === "wet" || appearance.family === "eye" ? 1 : 0,
        clearcoatRoughness: 0.08,
        frame: { origin, tangent: tangentCandidate, normal },
      },
    };
  });
  for (const groom of creature.grooms) {
    const origin = toWorld(groom.region, [0, 0, 0]);
    materials.push({
      id: `${groom.id.slice(0, 80)}-fiber`,
      name: `${groom.id} fibers`,
      schemaVersion: 1,
      dependencies: [],
      kind: "material",
      color: [1, 1, 1],
      secondary: [1, 1, 1],
      roughness: 0.65,
      metallic: 0,
      pattern: "solid",
      scale: 1,
      normalStrength: 0,
      domain: "local",
      creature: {
        family: "fiber",
        fiberDirection: normalize(sub(toWorld(groom.region, groom.direction), origin)),
        anisotropy: 0.65,
        sheen: 0.6,
      },
    });
  }
  return {
    materials,
    sample: (region, local) => {
      let color: Vec3 = [1, 1, 1],
        roughness = 0.6,
        coverage = 0,
        growthCoverage = 1,
        displacement = 0;
      let material: string | undefined;
      const sources: string[] = [];
      for (const appearance of creature.appearance) {
        if (appearance.region !== region) continue;
        const weight = creatureAppearanceCoverage(appearance, local, anchorOrigins.get(appearance.id));
        if (weight <= 0) continue;
        sources.push(appearance.id);
        color = add(scale(color, 1 - weight), scale(appearance.color, weight));
        roughness = roughness * (1 - weight) + appearance.roughness * weight;
        displacement += appearance.displacement * weight;
        growthCoverage *= 1 - appearance.growthSuppression * weight;
        coverage = 1 - (1 - coverage) * (1 - weight);
        if (weight >= 0.5) material = materialId(appearance);
      }
      return { material, color, roughness, coverage, growthCoverage, displacement, sources };
    },
  };
}
