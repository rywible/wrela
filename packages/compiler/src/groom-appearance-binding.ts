import { type CreatureDefinition, contentKey, type MaterialDefinition, type Vec3 } from "@wrela/model";

import type { CreatureGeometry } from "./creature";
import { compileCreatureAppearance, creatureAppearanceCoverage } from "./groom-appearance";

const lerp = (a: number, b: number, t: number) => a * (1 - t) + b * t;
/** Quantize a shared blend coordinate, not each response channel independently.
 * This prevents the Cartesian product of tiny roughness/SSS/anisotropy changes
 * from producing a separate draw for each combination. */
function blendSteps(a: MaterialDefinition, b: MaterialDefinition): number {
  const contrast = [
    Math.abs(a.roughness - b.roughness) / (1 / 32),
    Math.abs(a.metallic - b.metallic) / 0.05,
    Math.abs((a.creature?.anisotropy ?? 0) - (b.creature?.anisotropy ?? 0)) / 0.1,
    Math.abs(a.scale - b.scale) / Math.max(0.01, Math.max(a.scale, b.scale) * 0.2),
  ];
  for (const property of ["subsurface", "transmission", "sheen", "clearcoat", "clearcoatRoughness"] as const)
    contrast.push(Math.abs((a.creature?.[property] ?? 0) - (b.creature?.[property] ?? 0)) / 0.0625);
  return Math.max(1, Math.ceil(Math.max(...contrast)));
}

/** Bounded centroid bins for scalar response plus continuous vertex albedo. This
 * retains spatial color instead of multiplying already-colored materials twice.
 * Scalar interpolation is an authored material approximation, not BRDF-mixture equivalence. */
export function bindCreatureAppearanceFields(
  creature: CreatureDefinition,
  geometry: CreatureGeometry,
  defaultMaterial: string,
  toWorld: (region: string, point: Vec3) => Vec3,
  toLocal: (region: string, point: Vec3) => Vec3,
  resolveAnchor?: (region: string, id: string) => Vec3 | undefined,
) {
  const appearance = compileCreatureAppearance(creature, toWorld, resolveAnchor);
  if (!creature.appearance.length) return { geometry, materials: appearance.materials };
  const sourceMaterials = new Map(appearance.materials.map((material) => [material.id, material]));
  const diagnostics = [...geometry.diagnostics];
  const warned = new Set<string>();
  const warn = (code: string, region: string, message: string) => {
    const id = `${code}-${region}`;
    if (warned.has(id)) return;
    warned.add(id);
    diagnostics.push({ severity: "warning", code, node: region, message });
  };
  const fieldsByRegion = new Map<string, typeof creature.appearance>();
  const anchorOrigins = new Map<string, Vec3 | undefined>();
  for (const field of creature.appearance) {
    const list = fieldsByRegion.get(field.region) ?? [];
    list.push(field);
    fieldsByRegion.set(field.region, list);
    if (field.anchor) anchorOrigins.set(field.id, resolveAnchor?.(field.region, field.anchor));
  }
  const fields = (region: string) => fieldsByRegion.get(region) ?? [];
  const bases = new Set(creature.appearance.filter((source) => !source.mask).map((source) => source.region));
  // Clothing and mounted components share anatomical frames for deformation,
  // but do not inherit the body's skin/fur appearance. Their explicit material
  // remains authoritative; otherwise a fitted wool panel becomes skin-colored.
  const materialCharts = new Set(
    creature.cloth
      .filter(
        (cloth) => cloth.material || creature.charts.find((chart) => chart.id === cloth.chart)?.material,
      )
      .map((cloth) => cloth.chart),
  );
  const mountedNodes = new Set(creature.attachments.flatMap((attachment) => attachment.nodeIds));
  const mesh = geometry.mesh,
    triangleRegions: (string | null)[] = [],
    vertexModes = new Uint8Array(mesh.positions.length / 3);
  for (let triangle = 0; triangle < mesh.indices.length; triangle += 3) {
    const vertices = [mesh.indices[triangle], mesh.indices[triangle + 1], mesh.indices[triangle + 2]];
    const region = geometry.regions[vertices[0]];
    const active =
      region &&
      bases.has(region) &&
      vertices.every(
        (vertex) =>
          geometry.regions[vertex] === region &&
          !materialCharts.has(geometry.coordinates[vertex]?.chart ?? "") &&
          !mountedNodes.has(mesh.sourceIds?.[vertex] ?? ""),
      );
    triangleRegions.push(active ? region : null);
    for (const vertex of vertices) vertexModes[vertex] |= active ? 2 : 1;
    if (region && !bases.has(region) && fields(region).length)
      warn(
        "appearance-base-required",
        region,
        "Masked appearance requires an unmasked regional base; original material retained because its response is unavailable to this compiler",
      );
  }
  const positions = [...mesh.positions],
    normals = [...mesh.normals],
    colors = mesh.colors ? [...mesh.colors] : Array(positions.length).fill(1),
    sourceIds = mesh.sourceIds ? [...mesh.sourceIds] : undefined,
    regions = [...geometry.regions],
    coordinates = [...geometry.coordinates],
    aliases = new Map<number, number>();
  for (let vertex = 0; vertex < vertexModes.length; vertex++) {
    if (!(vertexModes[vertex] & 2)) continue;
    const region = geometry.regions[vertex];
    if (!region) continue;
    let target = vertex;
    // Only duplicate the seam vertices also used by untouched material groups.
    if (vertexModes[vertex] === 3) {
      if (positions.length / 3 >= 250_000)
        throw new Error("Appearance seam exceeds the 250,000 vertex budget");
      target = positions.length / 3;
      positions.push(...mesh.positions.slice(vertex * 3, vertex * 3 + 3));
      normals.push(...mesh.normals.slice(vertex * 3, vertex * 3 + 3));
      colors.push(1, 1, 1);
      regions.push(region);
      coordinates.push(geometry.coordinates[vertex]);
      sourceIds?.push(mesh.sourceIds?.[vertex] ?? region);
    }
    aliases.set(vertex, target);
    const p: Vec3 = [positions[target * 3], positions[target * 3 + 1], positions[target * 3 + 2]];
    colors.splice(target * 3, 3, ...appearance.sample(region, toLocal(region, p)).color);
  }
  const bins = new Map<string, MaterialDefinition>(),
    regionBins = new Map<string, MaterialDefinition[]>(),
    binsPerRegion = Math.max(1, Math.floor(128 / Math.max(1, bases.size))),
    buckets = new Map<string, number[]>();
  const resolve = (region: string, local: Vec3): string => {
    let response: MaterialDefinition | undefined;
    for (const source of fields(region)) {
      let weight = creatureAppearanceCoverage(source, local, anchorOrigins.get(source.id));
      if (weight <= 0) continue;
      const sourceMaterial = sourceMaterials.get(source.material ?? `${source.id.slice(0, 80)}-appearance`);
      if (!sourceMaterial) continue;
      if (!response || weight === 1) {
        response = structuredClone(sourceMaterial);
        continue;
      }
      const steps = blendSteps(response, sourceMaterial);
      weight = Math.round(weight * steps) / steps;
      const previousFamily = response.creature?.family,
        nextFamily = sourceMaterial.creature?.family;
      if (previousFamily !== nextFamily && weight > 0 && weight < 1)
        warn(
          "appearance-family-mixture",
          region,
          "Color and scalar response interpolate across different material families; dominant-family shading is an approximation, not a layered scattering model",
        );
      response.roughness = lerp(response.roughness, sourceMaterial.roughness, weight);
      response.metallic = lerp(response.metallic, sourceMaterial.metallic, weight);
      response.normalStrength = lerp(response.normalStrength, sourceMaterial.normalStrength, weight);
      response.scale = lerp(response.scale, sourceMaterial.scale, weight);
      if (response.creature && sourceMaterial.creature) {
        for (const parameter of [
          "subsurface",
          "transmission",
          "anisotropy",
          "sheen",
          "clearcoat",
          "clearcoatRoughness",
        ] as const)
          response.creature[parameter] = lerp(
            response.creature[parameter] ?? 0,
            sourceMaterial.creature[parameter] ?? 0,
            weight,
          );
        if (weight >= 0.5) response.creature.family = sourceMaterial.creature.family;
      }
    }
    if (!response) return defaultMaterial;
    // Both primary and secondary colors are ratios: authored albedo lives solely on vertices.
    const base = fields(region).find((field) => !field.mask);
    const secondaryRatio = 1 - (base?.variation ?? 0) * 0.25;
    response.color = [1, 1, 1];
    response.secondary = [secondaryRatio, secondaryRatio, secondaryRatio];
    response.roughness = Math.max(0.04, response.roughness);
    const identity = { region, response: { ...response, id: "", name: "" } },
      key = contentKey(identity);
    const existing = bins.get(key);
    if (existing) return existing.id;
    const candidates = regionBins.get(region) ?? [];
    if (candidates.length >= binsPerRegion) {
      warn(
        "appearance-bin-budget",
        region,
        `Response bins capped at ${binsPerRegion} for this region (128 total); nearest scalar bin reused; vertex albedo remains continuous`,
      );
      let best: MaterialDefinition | undefined,
        error = Infinity;
      for (const candidate of candidates) {
        const distance =
          Math.abs(candidate.roughness - response.roughness) +
          Math.abs(candidate.metallic - response.metallic) +
          (candidate.creature?.family !== response.creature?.family ? 10 : 0);
        if (distance < error) {
          best = candidate;
          error = distance;
        }
      }
      return best?.id ?? defaultMaterial;
    }
    response.id = `creature-blend-${key}`;
    response.name = `${region} compiled appearance`;
    bins.set(key, response);
    candidates.push(response);
    regionBins.set(region, candidates);
    return response.id;
  };
  for (let triangle = 0; triangle < mesh.indices.length; triangle += 3) {
    const vertices = [mesh.indices[triangle], mesh.indices[triangle + 1], mesh.indices[triangle + 2]],
      region = triangleRegions[triangle / 3];
    let material =
      mesh.materialGroups?.find((group) => triangle >= group.start && triangle < group.start + group.count)
        ?.material ?? defaultMaterial;
    if (region) {
      const centroid: Vec3 = [0, 0, 0];
      for (const vertex of vertices)
        for (let axis = 0; axis < 3; axis++) centroid[axis] += mesh.positions[vertex * 3 + axis] / 3;
      material = resolve(region, toLocal(region, centroid));
      for (let i = 0; i < vertices.length; i++) vertices[i] = aliases.get(vertices[i]) ?? vertices[i];
    }
    const bucket = buckets.get(material) ?? [];
    bucket.push(...vertices);
    buckets.set(material, bucket);
  }
  const indices: number[] = [],
    materialGroups: NonNullable<typeof mesh.materialGroups> = [];
  for (const [material, bucket] of buckets) {
    materialGroups.push({ material, start: indices.length, count: bucket.length });
    // A review-quality body can put more than 80k indices in one material group.
    // Spreading them into a call exceeds browser-worker argument-stack limits.
    for (const index of bucket) indices.push(index);
  }
  return {
    geometry: {
      ...geometry,
      regions,
      coordinates,
      diagnostics,
      mesh: {
        ...mesh,
        positions: new Float32Array(positions),
        normals: new Float32Array(normals),
        colors: new Float32Array(colors),
        sourceIds,
        indices: new Uint32Array(indices),
        materialGroups,
      },
    },
    materials: [
      ...appearance.materials.filter(
        (material) =>
          buckets.has(material.id) ||
          creature.grooms.some((groom) => material.id === `${groom.id.slice(0, 80)}-fiber`),
      ),
      ...bins.values(),
    ],
  };
}
