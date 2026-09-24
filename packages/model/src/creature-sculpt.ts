import type { CreatureSculpt } from "./creature";
import { add, dot, scale, sub, type Vec3 } from "./math";

/** Shared source-space field evaluation: compiler, repair fitting and review use identical support. */
export function sculptRotate(p: Vec3, r: Vec3, inverse = false): Vec3 {
  let [x, y, z] = p;
  for (const axis of inverse ? [2, 1, 0] : [0, 1, 2]) {
    const angle = r[axis] * (inverse ? -1 : 1),
      c = Math.cos(angle),
      s = Math.sin(angle);
    if (axis === 0) [y, z] = [y * c - z * s, y * s + z * c];
    if (axis === 1) [x, z] = [x * c + z * s, -x * s + z * c];
    if (axis === 2) [x, y] = [x * c - y * s, x * s + y * c];
  }
  return [x, y, z];
}
export function sculptMetric(stroke: CreatureSculpt, p: Vec3): Vec3 {
  const rotated = sculptRotate(p, stroke.support?.rotation ?? [0, 0, 0], true);
  const radii = stroke.support?.radii ?? [stroke.radius, stroke.radius, stroke.radius];
  return rotated.map((v, i) => v / radii[i]) as Vec3;
}
export function sculptSupport(stroke: CreatureSculpt, point: Vec3) {
  const path = stroke.path ?? [stroke.center];
  let distance = Infinity,
    nearest = path[0];
  for (let i = 0; i < Math.max(1, path.length - 1); i++) {
    const a = path[i],
      b = path[Math.min(i + 1, path.length - 1)];
    const segment = sculptMetric(stroke, sub(b, a)),
      offset = sculptMetric(stroke, sub(point, a));
    const denominator = dot(segment, segment);
    const t = denominator > 1e-20 ? Math.max(0, Math.min(1, dot(offset, segment) / denominator)) : 0;
    const candidate = add(a, scale(sub(b, a), t));
    const d = Math.hypot(...sculptMetric(stroke, sub(point, candidate)));
    if (d < distance) {
      distance = d;
      nearest = candidate;
    }
  }
  return {
    distance,
    nearest,
    weight: distance < 1 ? (1 - distance * distance) ** Math.max(1, stroke.falloff) : 0,
  };
}
function oneDisplacement(stroke: CreatureSculpt, point: Vec3): Vec3 {
  const { weight, nearest } = sculptSupport(stroke, point);
  if (!weight) return [0, 0, 0];
  let vector = stroke.displacement;
  const amount = Math.hypot(...vector);
  if (stroke.mode === "flatten" && amount > 1e-12) {
    const normal = scale(vector, 1 / amount);
    vector = scale(normal, Math.max(-amount, Math.min(amount, -dot(sub(point, nearest), normal))));
  } else if (stroke.mode === "inflate") {
    const radial = sub(point, nearest),
      length = Math.hypot(...radial);
    vector = length > 1e-12 ? scale(radial, amount / length) : [0, 0, 0];
  }
  return scale(vector, stroke.strength * weight);
}
export function creatureSculptDisplacement(stroke: CreatureSculpt, point: Vec3): Vec3 {
  const delta = oneDisplacement(stroke, point);
  // Reflect the entire field, including its orientation and curve, rather than just its center.
  const asymmetric = (stroke.path ?? [stroke.center]).some((p) => Math.abs(p[0]) > 1e-8);
  if (stroke.mirror && asymmetric) {
    const reflected = oneDisplacement(stroke, [-point[0], point[1], point[2]]);
    delta[0] -= reflected[0];
    delta[1] += reflected[1];
    delta[2] += reflected[2];
  }
  return delta;
}
export function evaluateCreatureSculpt(strokes: CreatureSculpt[], region: string, point: Vec3): Vec3 {
  let result: Vec3 = [...point];
  for (const stroke of strokes)
    if (stroke.region === region) result = add(result, creatureSculptDisplacement(stroke, point));
  return result;
}
