import {
  clamp,
  coordinateHash,
  type GeologicalProfile,
  type Landform,
  type TerrainGeology,
} from "@wrela/model";

/** Distance to a polyline, retaining orientation for an authored cliff face. */
function nearestPath(points: [number, number][], x: number, z: number) {
  let distanceSquared = Infinity,
    side = 0;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1],
      b = points[i];
    const dx = b[0] - a[0],
      dz = b[1] - a[1];
    const length2 = dx * dx + dz * dz;
    const t = length2 ? clamp(((x - a[0]) * dx + (z - a[1]) * dz) / length2, 0, 1) : 0;
    const offsetX = x - a[0] - t * dx,
      offsetZ = z - a[1] - t * dz;
    const d = offsetX * offsetX + offsetZ * offsetZ;
    if (d < distanceSquared) {
      distanceSquared = d;
      side = dx * (z - a[1]) - dz * (x - a[0]);
    }
  }
  return { distance: Math.sqrt(distanceSquared), side };
}

/** Render-only bank darkening from an authored drainage centerline. The same
 * world-space query at patch boundaries keeps vertex colors seam-free. */
export function geologicalBankTint(geology: TerrainGeology | undefined, x: number, z: number): number {
  const wetness = geology?.bankWetness;
  if (!wetness) return 1;
  const drainage = geology.landforms.find(
    (form) => form.id === wetness.drainageId && form.kind === "drainage",
  );
  if (!drainage) return 1;
  const distance = nearestPath(drainage.points, x, z).distance;
  const t = clamp((distance - wetness.waterHalfWidth) / wetness.fadeWidth, 0, 1);
  const falloff = t * t * (3 - 2 * t);
  return 1 - wetness.darkening * (1 - falloff);
}

const smooth = (value: number) => {
  const t = clamp(value, 0, 1);
  return t * t * (3 - 2 * t);
};
/** Continuous banks, talus and crests share the exact world-space source query.
 * These profiles sculpt landforms; they do not simulate sediment transport. */
function profiledDelta(
  profile: GeologicalProfile,
  distance: number,
  side: number,
  width: number,
  x: number,
  z: number,
): number {
  const variation = profile.variation;
  const phase = variation ? (coordinateHash(0, 0, variation.seed) / 4_294_967_295) * Math.PI * 2 : 0;
  const support = variation
    ? 1 -
      variation.amplitude *
        (0.5 +
          0.25 * Math.sin((x / variation.wavelength) * Math.PI * 2 + phase) +
          0.25 * Math.sin((z / variation.wavelength) * 4.1 - phase))
    : 1;
  const d = distance / (width * support);
  if (d >= 1) return 0;
  if (profile.kind === "river") {
    const bed = clamp(profile.bedFraction * (1 + Math.sign(side) * profile.asymmetry), 0.02, 0.8);
    const bank = Math.min(0.96 - bed, profile.bankFraction * (1 - Math.sign(side) * profile.asymmetry * 0.5));
    if (d <= bed) return -1;
    if (d < bed + bank) return -(1 - (1 - profile.shoulderDepth) * smooth((d - bed) / bank));
    return -profile.shoulderDepth * (1 - smooth((d - bed - bank) / (1 - bed - bank)));
  }
  const signed = Math.sign(side) * d;
  const face = profile.faceFraction;
  if (signed < -face) return profile.toeHeight * smooth((signed + 1) / (1 - face));
  if (signed < face)
    return profile.toeHeight + (1 - profile.toeHeight) * smooth((signed + face) / (2 * face));
  if (signed < profile.crestFraction) return 1;
  return 1 - smooth((signed - profile.crestFraction) / (1 - profile.crestFraction));
}
function landformDelta(
  feature: Pick<Landform, "kind" | "width" | "height" | "falloff" | "profile">,
  distance: number,
  side: number,
  x: number,
  z: number,
): number {
  if (feature.profile)
    return feature.height * profiledDelta(feature.profile, distance, side, feature.width, x, z);
  const d = distance / feature.width;
  if (d >= 1) return 0;
  const influence = (1 - d * d * (3 - 2 * d)) ** feature.falloff;
  if (feature.kind === "ridge") return feature.height * influence;
  if (feature.kind === "drainage") return -feature.height * influence;
  // The oriented face is continuous and sufficiently wide for normal sampling.
  const face = clamp(0.5 + (Math.sign(side) * distance) / Math.max(0.25, feature.width * 0.08), 0, 1);
  return feature.height * influence * face;
}
export function geologicalLandformHeight(
  geology: TerrainGeology | undefined,
  height: number,
  x: number,
  z: number,
): number {
  if (!geology) return height;
  let result = height;
  for (const feature of geology.landforms) {
    const { distance, side } = nearestPath(feature.points, x, z);
    result += landformDelta(feature, distance, side, x, z);
  }
  return result;
}
/** Bounded local talus relaxation, deliberately independent of patch generation order.
 * This is a sculpting filter, not a water-flow or sediment simulation. */
export function geologicalSurfaceHeight(
  geology: TerrainGeology | undefined,
  sample: (x: number, z: number) => number,
  x: number,
  z: number,
): number {
  let height = sample(x, z);
  if (!geology) return height;
  const erosion = geology.erosion;
  if (erosion.strength > 0) {
    const r = erosion.radius,
      limit = Math.tan((erosion.talusAngle * Math.PI) / 180) * r;
    let correction = 0;
    for (const [dx, dz] of [
      [-r, 0],
      [r, 0],
      [0, -r],
      [0, r],
    ]) {
      const difference = sample(x + dx, z + dz) - height;
      correction += Math.sign(difference) * Math.max(0, Math.abs(difference) - limit);
    }
    height += (correction * erosion.strength) / 4;
  }
  const strata = geology.strata;
  if (strata.strength > 0) {
    const layer = Math.floor(height / strata.thickness),
      fraction = height / strata.thickness - layer;
    const terrace = layer + fraction * fraction * (3 - 2 * fraction);
    height += (terrace * strata.thickness - height) * strata.strength;
  }
  return height;
}

type Segment = {
  index: number;
  ax: number;
  az: number;
  dx: number;
  dz: number;
  length2: number;
  minX: number;
  minZ: number;
  maxX: number;
  maxZ: number;
};
type SegmentTree = {
  minX: number;
  minZ: number;
  maxX: number;
  maxZ: number;
  segments?: Segment[];
  left?: SegmentTree;
  right?: SegmentTree;
};
function buildSegmentTree(segments: Segment[]): SegmentTree {
  const bounds = {
    minX: Math.min(...segments.map((s) => s.minX)),
    minZ: Math.min(...segments.map((s) => s.minZ)),
    maxX: Math.max(...segments.map((s) => s.maxX)),
    maxZ: Math.max(...segments.map((s) => s.maxZ)),
  };
  if (segments.length <= 4) return { ...bounds, segments };
  const byX = bounds.maxX - bounds.minX >= bounds.maxZ - bounds.minZ;
  segments.sort((a, b) => (byX ? a.minX + a.maxX - (b.minX + b.maxX) : a.minZ + a.maxZ - (b.minZ + b.maxZ)));
  const middle = Math.floor(segments.length / 2);
  return {
    ...bounds,
    left: buildSegmentTree(segments.slice(0, middle)),
    right: buildSegmentTree(segments.slice(middle)),
  };
}
function boundsDistanceSquared(tree: SegmentTree, x: number, z: number): number {
  const dx = Math.max(tree.minX - x, 0, x - tree.maxX),
    dz = Math.max(tree.minZ - z, 0, z - tree.maxZ);
  return dx * dx + dz * dz;
}
/** Compile spatial bounds once for a batch of queries. The result is a snapshot:
 * callers rebuild it after edits, avoiding hidden mutable-source cache assumptions. */
export function compileGeologicalLandforms(
  geology: TerrainGeology | undefined,
): (height: number, x: number, z: number) => number {
  const features = (geology?.landforms ?? []).map((feature) => ({
    kind: feature.kind,
    width: feature.width,
    height: feature.height,
    falloff: feature.falloff,
    profile: feature.profile ? structuredClone(feature.profile) : undefined,
    tree: buildSegmentTree(
      feature.points.slice(1).map((b, index): Segment => {
        const a = feature.points[index],
          dx = b[0] - a[0],
          dz = b[1] - a[1];
        return {
          index,
          ax: a[0],
          az: a[1],
          dx,
          dz,
          length2: dx * dx + dz * dz,
          minX: Math.min(a[0], b[0]),
          maxX: Math.max(a[0], b[0]),
          minZ: Math.min(a[1], b[1]),
          maxZ: Math.max(a[1], b[1]),
        };
      }),
    ),
  }));
  return (height, x, z) => {
    let result = height;
    for (const feature of features) {
      const radiusSquared = feature.width * feature.width;
      if (boundsDistanceSquared(feature.tree, x, z) >= radiusSquared) continue;
      let best = radiusSquared,
        bestIndex = Infinity,
        side = 0;
      const visit = (tree: SegmentTree) => {
        if (boundsDistanceSquared(tree, x, z) > best) return;
        if (tree.segments)
          for (const segment of tree.segments) {
            const t = segment.length2
              ? clamp(((x - segment.ax) * segment.dx + (z - segment.az) * segment.dz) / segment.length2, 0, 1)
              : 0;
            const dx = x - segment.ax - t * segment.dx,
              dz = z - segment.az - t * segment.dz;
            const distance = dx * dx + dz * dz;
            if (distance < best || (distance === best && segment.index < bestIndex)) {
              best = distance;
              bestIndex = segment.index;
              side = segment.dx * (z - segment.az) - segment.dz * (x - segment.ax);
            }
          }
        else if (tree.left && tree.right) {
          const left = boundsDistanceSquared(tree.left, x, z),
            right = boundsDistanceSquared(tree.right, x, z);
          if (left <= right) {
            visit(tree.left);
            visit(tree.right);
          } else {
            visit(tree.right);
            visit(tree.left);
          }
        }
      };
      visit(feature.tree);
      if (best >= radiusSquared) continue;
      result += landformDelta(feature, Math.sqrt(best), side, x, z);
    }
    return result;
  };
}
