import type { Vec3 } from "./math";

export type WorldPathGeometry = { points: Vec3[]; cornerRadius?: number };
export type WorldPathSample = { position: Vec3; yaw: number };

const mix = (a: Vec3, b: Vec3, t: number): Vec3 => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];
const distance = (a: Vec3, b: Vec3) => Math.hypot(b[0] - a[0], b[2] - a[2]);

/** Round each corner inside its adjacent segments. The bounded quadratic curves
 * cannot overshoot authored elevations or leave the control-point convex hull. */
export function worldPathPolyline(path: WorldPathGeometry): Vec3[] {
  const radius = path.cornerRadius ?? 0;
  if (radius <= 0 || path.points.length < 3) return path.points.map((point) => [...point]);
  const result: Vec3[] = [[...path.points[0]]];
  const append = (point: Vec3) => {
    if (distance(result[result.length - 1], point) > 1e-8) result.push(point);
  };
  for (let index = 1; index < path.points.length - 1; index++) {
    const a = path.points[index - 1],
      b = path.points[index],
      c = path.points[index + 1];
    const before = distance(a, b),
      after = distance(b, c);
    const trim = Math.min(radius, before / 2, after / 2);
    if (trim < 1e-8) {
      append([...b]);
      continue;
    }
    const start = mix(b, a, trim / before),
      end = mix(b, c, trim / after);
    append(start);
    for (let step = 1; step <= 12; step++) {
      const t = step / 12;
      append(mix(mix(start, b, t), mix(b, end, t), t));
    }
  }
  append([...path.points[path.points.length - 1]]);
  return result;
}

export function worldPathLength(points: readonly Vec3[]): number {
  return points.slice(1).reduce((sum, point, index) => sum + distance(points[index], point), 0);
}

export function worldPathSampleCount(path: WorldPathGeometry, spacing: number): number {
  if (!Number.isFinite(spacing) || spacing <= 0) throw new RangeError("Path spacing must be positive");
  if ((path.cornerRadius ?? 0) > 0)
    return Math.max(1, Math.ceil(worldPathLength(worldPathPolyline(path)) / spacing)) + 1;
  return (
    1 +
    path.points
      .slice(1)
      .reduce(
        (sum, point, index) => sum + Math.max(1, Math.ceil(distance(path.points[index], point) / spacing)),
        0,
      )
  );
}

/** Uniform arc-length sampling, shared by curved surface modules and review.
 * Returned elevation is authored geometry; traversal callers sample final terrain. */
export function sampleWorldPolyline(points: readonly Vec3[], count: number): WorldPathSample[] {
  if (points.length < 2 || !Number.isInteger(count) || count < 2 || count > 4096)
    throw new RangeError("Path sampling requires two points and 2–4096 samples");
  const lengths = points.slice(1).map((point, index) => distance(points[index], point));
  const total = lengths.reduce((sum, length) => sum + length, 0);
  let segment = 0,
    traversed = 0;
  return Array.from({ length: count }, (_, index) => {
    const d = (total * index) / (count - 1);
    while (segment < lengths.length - 1 && d > traversed + lengths[segment]) traversed += lengths[segment++];
    const a = points[segment],
      b = points[segment + 1];
    return {
      position: mix(a, b, lengths[segment] ? (d - traversed) / lengths[segment] : 0),
      yaw: Math.atan2(b[0] - a[0], b[2] - a[2]),
    };
  });
}

/** Preserve legacy straight-segment identities; curved routes sample by arc length. */
export function sampleWorldPath(path: WorldPathGeometry, spacing: number, limit = 256): WorldPathSample[] {
  const count = worldPathSampleCount(path, spacing);
  if (count > limit) throw new RangeError(`Path exceeds the ${limit} sample budget`);
  if ((path.cornerRadius ?? 0) > 0) return sampleWorldPolyline(worldPathPolyline(path), count);
  const samples: WorldPathSample[] = [];
  for (let segment = 1; segment < path.points.length; segment++) {
    const a = path.points[segment - 1],
      b = path.points[segment];
    const steps = Math.max(1, Math.ceil(distance(a, b) / spacing));
    for (let index = segment === 1 ? 0 : 1; index <= steps; index++)
      samples.push({ position: mix(a, b, index / steps), yaw: Math.atan2(b[0] - a[0], b[2] - a[2]) });
  }
  return samples;
}
