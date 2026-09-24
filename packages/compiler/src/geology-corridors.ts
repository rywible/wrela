import {
  clamp,
  type GeologyCorridor,
  type TerrainDefinition,
  type TerrainGeology,
  type Vec3,
} from "@wrela/model";

/** Corridors are evaluated in source order so a later authored crossing wins.
 * The flat-width core is exact; the shoulder's zero-gradient endpoints avoid seams. */
export function geologicalCorridorHeight(
  geology: TerrainGeology | undefined,
  initial: number,
  x: number,
  z: number,
): number {
  let height = initial;
  for (const corridor of geology?.corridors ?? []) height = sampleCorridor(corridor, height, x, z);
  return height;
}

function sampleCorridor(corridor: GeologyCorridor, initial: number, x: number, z: number): number {
  let bestDistance = Infinity,
    elevation = initial;
  const candidates: { distance: number; elevation: number }[] = [];
  for (let i = 1; i < corridor.points.length; i++) {
    const a = corridor.points[i - 1],
      b = corridor.points[i];
    const dx = b[0] - a[0],
      dz = b[2] - a[2],
      length2 = dx * dx + dz * dz;
    const t = length2 ? clamp(((x - a[0]) * dx + (z - a[2]) * dz) / length2, 0, 1) : 0;
    const distance = Math.hypot(x - a[0] - t * dx, z - a[2] - t * dz);
    if (distance < corridor.halfWidth + corridor.shoulder)
      candidates.push({ distance, elevation: a[1] + (b[1] - a[1]) * t });
    if (distance < bestDistance) {
      bestDistance = distance;
      elevation = a[1] + (b[1] - a[1]) * t;
    }
  }
  if (bestDistance >= corridor.halfWidth + corridor.shoulder) return initial;
  // Blend equally close profiles through bends instead of jumping elevations at
  // a nearest-segment boundary. Exact centerline samples retain their profile.
  if (bestDistance > 1e-8 && candidates.length > 1) {
    let weighted = 0,
      weights = 0;
    for (const candidate of candidates) {
      const edge = clamp((candidate.distance - corridor.halfWidth) / corridor.shoulder, 0, 1);
      const weight = (bestDistance / candidate.distance) ** 4 * (1 - edge * edge * (3 - 2 * edge));
      weighted += candidate.elevation * weight;
      weights += weight;
    }
    elevation = weighted / weights;
  }
  const t = clamp((bestDistance - corridor.halfWidth) / corridor.shoulder, 0, 1);
  return initial + (elevation - initial) * (1 - t * t * (3 - 2 * t));
}

/** Precompute corridor influence bounds for terrain patch and route batches. */
export function compileGeologicalCorridors(geology: TerrainGeology | undefined) {
  const corridors = (geology?.corridors ?? []).map((source) => {
    const corridor = { ...source, points: source.points.map((point): Vec3 => [...point]) };
    const reach = corridor.halfWidth + corridor.shoulder;
    return {
      corridor,
      minX: Math.min(...corridor.points.map((point) => point[0])) - reach,
      maxX: Math.max(...corridor.points.map((point) => point[0])) + reach,
      minZ: Math.min(...corridor.points.map((point) => point[2])) - reach,
      maxZ: Math.max(...corridor.points.map((point) => point[2])) + reach,
    };
  });
  return (initial: number, x: number, z: number) => {
    let height = initial;
    for (const item of corridors) {
      if (x < item.minX || x > item.maxX || z < item.minZ || z > item.maxZ) continue;
      height = sampleCorridor(item.corridor, height, x, z);
    }
    return height;
  };
}

/** Snap a review route to the current surface, limiting each segment's grade.
 * Start elevation remains authored; this is an explicit source edit, never an automatic correction. */
export function createGeologyCorridor(
  terrain: TerrainDefinition,
  sampleHeight: (x: number, z: number) => number,
  id: string,
): GeologyCorridor {
  const review = terrain.geology?.review;
  if (!review) throw new Error("Enable geology before creating a traversal corridor.");
  const grade = Math.tan((review.maxSlope * Math.PI) / 180);
  const points: Vec3[] = [];
  for (const [x, z] of review.route) {
    let height = clamp(sampleHeight(x, z), -5_000, 5_000);
    const previous = points.at(-1);
    if (previous) {
      const allowed = Math.hypot(x - previous[0], z - previous[2]) * grade;
      height = clamp(height, previous[1] - allowed, previous[1] + allowed);
    }
    points.push([x, height, z]);
  }
  return { id, points, halfWidth: Math.max(1, (review.bodyRadius ?? 0.35) + 0.5), shoulder: 2 };
}
