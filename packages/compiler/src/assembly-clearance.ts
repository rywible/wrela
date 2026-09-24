import { type Bounds, cross, dot, sub, type Vec3 } from "@wrela/model";

/** Triangle/box SAT, including the edge cross products that reject diagonal false positives. */
function triangleInBox(points: [Vec3, Vec3, Vec3], half: Vec3): boolean {
  const edges = points.map((point, index) => sub(points[(index + 1) % 3], point));
  const boxAxes: Vec3[] = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ];
  const axes = [
    ...boxAxes,
    cross(edges[0], edges[1]),
    ...edges.flatMap((edge) => boxAxes.map((axis) => cross(edge, axis))),
  ];
  return axes.every((axis) => {
    const projections = points.map((point) => dot(point, axis));
    const radius = Math.abs(axis[0]) * half[0] + Math.abs(axis[1]) * half[1] + Math.abs(axis[2]) * half[2];
    return Math.min(...projections) <= radius && Math.max(...projections) >= -radius;
  });
}

/**
 * Test an outward-wound, closed module against an open reserved volume. Flat
 * triangle positions are used before Float32 conversion so doorway tangency
 * does not acquire a false obstruction from mesh storage quantization.
 * The winding test also detects a reservation wholly enclosed by a solid.
 */
export function assemblyModuleObstructs(
  positions: readonly number[],
  start: number,
  end: number,
  bounds: Bounds,
): boolean {
  const center = bounds.min.map((value, axis) => (value + bounds.max[axis]) / 2) as Vec3;
  const half = bounds.min.map((value, axis) =>
    Math.max(0, (bounds.max[axis] - value) / 2 - Math.min(1e-6, (bounds.max[axis] - value) * 1e-4)),
  ) as Vec3;
  let winding = 0;
  for (let index = start; index < end; index += 9) {
    const points = [0, 3, 6].map((offset) =>
      sub([positions[index + offset], positions[index + offset + 1], positions[index + offset + 2]], center),
    ) as [Vec3, Vec3, Vec3];
    if (triangleInBox(points, half)) return true;
    const [a, b, c] = points,
      lengths = points.map((point) => Math.hypot(...point));
    // Van Oosterom/Strackee solid angle; no ray-edge or ray-vertex ambiguity.
    winding +=
      2 *
      Math.atan2(
        dot(a, cross(b, c)),
        lengths[0] * lengths[1] * lengths[2] +
          dot(a, b) * lengths[2] +
          dot(b, c) * lengths[0] +
          dot(c, a) * lengths[1],
      );
  }
  return Math.abs(winding) > Math.PI * 2;
}
