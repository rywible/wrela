import type { Bounds, Camera, CompiledCharacter, CreatureDetail, RenderSurface, Vec3 } from "@wrela/model";

export const DEFAULT_CREATURE_VIEWPORT_HEIGHT = 720;
export type CreatureDetailChoice = {
  detail?: CreatureDetail;
  metadata: NonNullable<RenderSurface["creatureDetail"]>;
};
const empty = (): Bounds => ({ min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] });
const include = (bounds: Bounds, point: Vec3) => {
  for (let axis = 0; axis < 3; axis++) {
    bounds.min[axis] = Math.min(bounds.min[axis], point[axis]);
    bounds.max[axis] = Math.max(bounds.max[axis], point[axis]);
  }
};
const validBounds = (bounds: Bounds) =>
  bounds.min.every(
    (value, axis) => Number.isFinite(value) && Number.isFinite(bounds.max[axis]) && value <= bounds.max[axis],
  );
const corners = (bounds: Bounds): Vec3[] =>
  Array.from(
    { length: 8 },
    (_, bits) => [0, 1, 2].map((axis) => (bits & (1 << axis) ? bounds.max : bounds.min)[axis]) as Vec3,
  );

/** Contains every normalized nonnegative linear skin blend, for a known rest-space
 * displacement bound. This is a current-pose bound, not an approximation error. */
export function creatureDeformedBounds(bounds: Bounds, matrices?: Float32Array, maxDisplacement = 0): Bounds {
  if (!validBounds(bounds) || !Number.isFinite(maxDisplacement) || maxDisplacement < 0)
    throw Error("Creature bounds and displacement must be finite and ordered");
  const expanded: Bounds = {
    min: bounds.min.map((v) => v - maxDisplacement) as Vec3,
    max: bounds.max.map((v) => v + maxDisplacement) as Vec3,
  };
  if (!matrices) return expanded;
  if (!matrices.length || matrices.length % 16 || !matrices.every(Number.isFinite))
    throw Error("Creature skin matrices must contain finite affine transforms");
  const result = empty(),
    points = corners(expanded);
  for (let offset = 0; offset < matrices.length; offset += 16) {
    if (
      Math.abs(matrices[offset + 3]) + Math.abs(matrices[offset + 7]) + Math.abs(matrices[offset + 11]) >
        1e-6 ||
      Math.abs(matrices[offset + 15] - 1) > 1e-6
    )
      throw Error("Creature bounds require affine skin matrices");
    for (const point of points)
      include(result, [
        matrices[offset] * point[0] +
          matrices[offset + 4] * point[1] +
          matrices[offset + 8] * point[2] +
          matrices[offset + 12],
        matrices[offset + 1] * point[0] +
          matrices[offset + 5] * point[1] +
          matrices[offset + 9] * point[2] +
          matrices[offset + 13],
        matrices[offset + 2] * point[0] +
          matrices[offset + 6] * point[1] +
          matrices[offset + 10] * point[2] +
          matrices[offset + 14],
      ]);
  }
  // Buffer arithmetic and normalized skin weights are represented as Float32.
  for (let axis = 0; axis < 3; axis++) {
    const padding = 1e-6 * Math.max(1, Math.abs(result.min[axis]), Math.abs(result.max[axis]));
    result.min[axis] -= padding;
    result.max[axis] += padding;
  }
  return result;
}

type Envelope = { bounds: Bounds; margin: number };
const envelopeCache = new WeakMap<CompiledCharacter, Envelope>();
/** Source envelopes support a conservative heuristic for projected size. Dynamic
 * simulation is not a proven approximation domain; metadata explicitly retains
 * source-envelope-estimate and unknown maxError. Visibility uses actual pose and
 * deformation bounds independently after the chosen variant is evaluated. */
function sourceEnvelope(artifact: CompiledCharacter): Envelope {
  const cached = envelopeCache.get(artifact);
  if (cached) return cached;
  const bounds = empty();
  for (const mesh of [artifact.mesh, ...(artifact.creatureDetails ?? []).map((detail) => detail.mesh)]) {
    if (!validBounds(mesh.bounds)) throw Error("Creature detail has invalid bounds");
    include(bounds, mesh.bounds.min);
    include(bounds, mesh.bounds.max);
  }
  const correctiveMargin = (products: CompiledCharacter["creatureCorrectives"]) =>
    (products ?? []).reduce((sum, product) => {
      let maximum = 0;
      for (let i = 0; i < product.displacements.length; i += 3)
        maximum = Math.max(
          maximum,
          Math.hypot(product.displacements[i], product.displacements[i + 1], product.displacements[i + 2]),
        );
      return sum + maximum;
    }, 0);
  let margin = Math.max(
    correctiveMargin(artifact.creatureCorrectives),
    ...(artifact.creatureDetails ?? []).map((detail) => correctiveMargin(detail.correctives)),
    0,
  );
  let maxGuideLength = 0;
  for (const guide of artifact.creatureGroom?.guides ?? []) {
    let length = 0;
    for (let i = 1; i < guide.points.length; i++)
      length += Math.hypot(...guide.points[i].map((value, axis) => value - guide.points[i - 1][axis]));
    maxGuideLength = Math.max(maxGuideLength, length);
  }
  margin += 2 * maxGuideLength;
  // Panels may drape about their authored pins. This margin is an estimate, not a
  // solver convergence promise; final deformed visibility remains independent.
  if (artifact.creature?.cloth.length)
    margin += Math.hypot(...bounds.max.map((value, axis) => value - bounds.min[axis]));
  const result = { bounds, margin };
  envelopeCache.set(artifact, result);
  return result;
}

/** Diameter in framebuffer pixels. A sphere centered on the actor origin avoids
 * requiring its rigid orientation; it encloses every corner of the posed bounds. */
export function projectedCreatureDiameter(
  artifact: CompiledCharacter,
  position: Vec3,
  scale: number,
  camera: Camera,
  viewportHeight = DEFAULT_CREATURE_VIEWPORT_HEIGHT,
  skinMatrices?: Float32Array,
): number {
  if (!Number.isFinite(viewportHeight) || viewportHeight < 1 || viewportHeight > 32768)
    throw Error("Viewport height must be in [1,32768] framebuffer pixels");
  if (
    !Number.isFinite(scale) ||
    scale <= 0 ||
    !position.every(Number.isFinite) ||
    !camera.position.every(Number.isFinite) ||
    !camera.target.every(Number.isFinite) ||
    !Number.isFinite(camera.fov) ||
    camera.fov <= 0 ||
    camera.fov >= 180
  )
    throw Error(
      "Creature projection requires finite position, positive scale and vertical field of view in (0,180) degrees",
    );
  const envelope = sourceEnvelope(artifact),
    bounds = creatureDeformedBounds(envelope.bounds, skinMatrices, envelope.margin);
  const radius =
    scale *
    Math.hypot(...bounds.max.map((value, axis) => Math.max(Math.abs(value), Math.abs(bounds.min[axis]))));
  const forward = camera.target.map((value, axis) => value - camera.position[axis]),
    length = Math.hypot(...forward);
  if (length < 1e-8) throw Error("Camera target must differ from its position");
  const depth =
    forward.reduce(
      (sum, value, axis) => sum + (value * (position[axis] - camera.position[axis])) / length,
      0,
    ) - radius;
  if (depth <= 0) return Number.MAX_SAFE_INTEGER;
  return Math.min(
    Number.MAX_SAFE_INTEGER,
    (radius * viewportHeight) / (depth * Math.tan((camera.fov * Math.PI) / 360)),
  );
}

/** Stable per-instance hysteresis; source artifacts and all binding arrays remain immutable. */
export class CreatureDetailSelector {
  private choices = new Map<
    string,
    { artifact: CompiledCharacter; label: string | undefined; choice: CreatureDetailChoice }
  >();
  constructor(
    readonly hysteresis = 0.15,
    readonly maxActors = 256,
  ) {
    if (!Number.isFinite(hysteresis) || hysteresis < 0 || hysteresis >= 0.5)
      throw Error("Creature detail hysteresis must be in [0,0.5)");
    if (!Number.isSafeInteger(maxActors) || maxActors < 1 || maxActors > 4096)
      throw Error("Creature detail actor budget must be in [1,4096]");
  }
  select(
    instanceId: string,
    artifact: CompiledCharacter,
    position: Vec3,
    scale: number,
    camera: Camera,
    viewportHeight = DEFAULT_CREATURE_VIEWPORT_HEIGHT,
    skinMatrices?: Float32Array,
  ): CreatureDetailChoice {
    const diameter = projectedCreatureDiameter(
      artifact,
      position,
      scale,
      camera,
      viewportHeight,
      skinMatrices,
    );
    const details = (artifact.creatureDetails ?? [])
      .filter(
        (detail) =>
          Number.isFinite(detail.maxProjectedDiameter) &&
          detail.maxProjectedDiameter > 0 &&
          detail.jointIndices.length === (detail.mesh.positions.length / 3) * 4 &&
          detail.weights.length === detail.jointIndices.length,
      )
      .slice()
      .sort((a, b) => b.maxProjectedDiameter - a.maxProjectedDiameter || a.label.localeCompare(b.label));
    const previous = this.choices.get(instanceId);
    let index = 0;
    if (previous?.artifact === artifact) {
      index =
        previous.label === undefined ? 0 : details.findIndex((detail) => detail.label === previous.label) + 1;
      while (
        index < details.length &&
        diameter <= details[index].maxProjectedDiameter * (1 - this.hysteresis)
      )
        index++;
      while (index > 0 && diameter > details[index - 1].maxProjectedDiameter * (1 + this.hysteresis)) index--;
    } else while (index < details.length && diameter <= details[index].maxProjectedDiameter) index++;
    const detail = index ? details[index - 1] : undefined;
    const choice: CreatureDetailChoice = {
      detail,
      metadata: {
        label: detail?.label ?? artifact.creatureGroomDetail ?? "hero",
        projectedDiameter: diameter,
        viewportHeight,
        maxError: null,
        selection: "projected-diameter-hysteresis",
        bounds: "source-envelope-estimate",
      },
    };
    // Refresh insertion order for deterministic least-recently-used eviction.
    this.choices.delete(instanceId);
    this.choices.set(instanceId, { artifact, label: detail?.label, choice });
    while (this.choices.size > this.maxActors) {
      const oldest = this.choices.keys().next().value;
      if (oldest === undefined) break;
      this.choices.delete(oldest);
    }
    return choice;
  }
  inspect(instanceId: string): CreatureDetailChoice | undefined {
    const choice = this.choices.get(instanceId)?.choice;
    return choice ? { detail: choice.detail, metadata: { ...choice.metadata } } : undefined;
  }
  get retainedActors() {
    return this.choices.size;
  }
  clear() {
    this.choices.clear();
  }
}
