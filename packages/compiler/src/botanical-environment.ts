import {
  add,
  type BotanicalGrowthState,
  botanicalSpeciesTraits,
  type GrowthEnvironment,
  normalize,
  scale,
  type Vec3,
} from "@wrela/model";

/** Bounded seasonal canopy query. The immutable grid is rebuilt once per growth step.
 * Samples five sky directions, independent of camera and instantaneous sun position. */
export function botanicalEnvironment(
  state: Pick<BotanicalGrowthState, "shoots" | "species">,
  environment: GrowthEnvironment,
) {
  const cells = new Map<string, number>();
  const size = 0.45;
  const key = (p: Vec3) => p.map((v) => Math.floor(v / size)).join(":");
  const traits = botanicalSpeciesTraits[state.species];
  const organArea = traits.leafLength * traits.leafWidth * (state.species === "paper-birch" ? 0.6 : 1);
  const opticalWeight = (count: number, retained: number) =>
    (count * retained * organArea * 0.5) / (size * size);
  for (const shoot of state.shoots) {
    if (shoot.state !== "living" || shoot.foliage.retained <= 0) continue;
    const cell = key(shoot.end);
    cells.set(
      cell,
      Math.min(2, (cells.get(cell) ?? 0) + opticalWeight(shoot.foliage.count, shoot.foliage.retained)),
    );
  }
  const directions: Vec3[] = [
    [0, 1, 0],
    normalize([0.8, 1, 0]),
    normalize([-0.8, 1, 0]),
    normalize([0, 1, 0.8]),
    normalize([0, 1, -0.8]),
  ];
  const neighbors = [...environment.neighbors].sort((a, b) => a.id.localeCompare(b.id));
  return (point: Vec3): { light: number; direction: Vec3 } => {
    let sum = 0;
    let direction: Vec3 = [0, 0, 0];
    for (const ray of directions) {
      // Exclude one shoot's own contribution, not the entire surrounding cell.
      // Skipping nearby foliage lets crowded terminal buds reproduce without competition.
      let depth = Math.max(
        0,
        (cells.get(key(point)) ?? 0) - opticalWeight(state.species === "paper-birch" ? 12 : 80, 1),
      );
      for (let i = 1; i <= 16; i++) depth += cells.get(key(add(point, scale(ray, i * size)))) ?? 0;
      for (const neighbor of neighbors) {
        // Intersection with a canopy ellipsoid; center is the center of its foliage volume.
        const p: Vec3 = [
          (point[0] - neighbor.center[0]) / neighbor.radius,
          (point[1] - neighbor.center[1]) / (neighbor.height * 0.5),
          (point[2] - neighbor.center[2]) / neighbor.radius,
        ];
        const d: Vec3 = [
          ray[0] / neighbor.radius,
          ray[1] / (neighbor.height * 0.5),
          ray[2] / neighbor.radius,
        ];
        const a = d.reduce((s, v) => s + v * v, 0);
        const b = 2 * d.reduce((s, v, i) => s + v * p[i], 0);
        const c = p.reduce((s, v) => s + v * v, 0) - 1;
        const discriminant = b * b - 4 * a * c;
        if (discriminant > 0 && (-b + Math.sqrt(discriminant)) / (2 * a) > 0)
          depth += -Math.log(Math.max(0.001, 1 - neighbor.opacity));
      }
      const light = Math.exp(-depth) * environment.light;
      sum += light;
      direction = add(direction, scale(ray, light));
    }
    // Uniform sky is not a directional cue. Remove the sampling hemisphere's
    // mean before tropism, otherwise every extension accumulates an upward bend.
    const meanY = directions.reduce((total, ray) => total + ray[1], 0) / directions.length;
    direction[1] -= meanY * sum;
    const magnitude = Math.hypot(...direction);
    return {
      light: Math.round((sum / directions.length) * 4096) / 4096,
      direction: magnitude > 1e-8 ? scale(direction, 1 / magnitude) : [0, 0, 0],
    };
  };
}
