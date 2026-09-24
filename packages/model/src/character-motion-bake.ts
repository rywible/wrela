import type { Joint } from "./documents";
import type { Quat, Vec3 } from "./math";

const subtract = (a: Vec3, b: Vec3): Vec3 => a.map((value, axis) => value - b[axis]) as Vec3;
const unit = (value: Vec3): Vec3 => {
  const length = Math.hypot(...value);
  return length > 1e-9 ? (value.map((v) => v / length) as Vec3) : [0, -1, 0];
};
const multiply = (a: Quat, b: Quat): Quat => [
  a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
  a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
  a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
  a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
];
const inverse = (q: Quat): Quat => [-q[0], -q[1], -q[2], q[3]];
function between(from: Vec3, to: Vec3): Quat {
  const a = unit(from),
    b = unit(to),
    dot = a.reduce((sum, value, index) => sum + value * b[index], 0);
  const q: Quat = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0], 1 + dot];
  const length = Math.hypot(...q);
  return length > 1e-8 ? (q.map((value) => value / length) as Quat) : [1, 0, 0, 0];
}
function euler(q: Quat): Vec3 {
  const [x, y, z, w] = q,
    sy = Math.max(-1, Math.min(1, 2 * (x * z + y * w)));
  return Math.abs(sy) < 0.9999999
    ? [
        Math.atan2(2 * (x * w - y * z), 1 - 2 * (x * x + y * y)),
        Math.asin(sy),
        Math.atan2(2 * (z * w - x * y), 1 - 2 * (y * y + z * z)),
      ]
    : [Math.atan2(2 * (y * z + x * w), 1 - 2 * (x * x + z * z)), Math.asin(sy), 0];
}
/** Bounded offline source bake for the zero-rest-rotation Warden chains. No runtime dependency. */
export function bakeLegPose(chain: Joint[], target: Vec3): Vec3[] {
  const points = chain.map((joint) => [...joint.position] as Vec3),
    anchor = [...points[0]] as Vec3;
  const lengths = points.slice(1).map((point, index) => Math.hypot(...subtract(point, points[index])));
  const atLength = (from: Vec3, to: Vec3, length: number): Vec3 => {
    const direction = unit(subtract(to, from));
    return from.map((value, axis) => value + direction[axis] * length) as Vec3;
  };
  for (let iteration = 0; iteration < 40; iteration++) {
    points[points.length - 1] = [...target];
    for (let index = points.length - 2; index >= 0; index--)
      points[index] = atLength(points[index + 1], points[index], lengths[index]);
    points[0] = [...anchor];
    for (let index = 1; index < points.length; index++)
      points[index] = atLength(points[index - 1], points[index], lengths[index - 1]);
  }
  let parent: Quat = [0, 0, 0, 1];
  const rotations = points.slice(0, -1).map((point, index) => {
    const world = between(
      subtract(chain[index + 1].position, chain[index].position),
      subtract(points[index + 1], point),
    );
    const local = multiply(inverse(parent), world);
    parent = world;
    return euler(local);
  });
  rotations.push(euler(inverse(parent)));
  return rotations;
}
export function yawPoint(point: Vec3, yaw: number): Vec3 {
  return [
    Math.cos(yaw) * point[0] + Math.sin(yaw) * point[2],
    point[1],
    -Math.sin(yaw) * point[0] + Math.cos(yaw) * point[2],
  ];
}

/** Exact two-bone source pose with a chosen anatomical bend plane. The target
 * clamps to the reachable shell; runtime contacts report any remaining error. */
export function bakeTwoBonePose(chain: Joint[], target: Vec3, pole: Vec3): Vec3[] {
  if (chain.length !== 3) throw Error("A two-bone pose requires three joints");
  if (![...chain.flatMap((joint) => joint.position), ...target, ...pole].every(Number.isFinite))
    throw Error("A two-bone pose requires finite positions");
  const [root, middle, end] = chain.map((joint) => joint.position);
  const a = Math.hypot(...subtract(middle, root)),
    b = Math.hypot(...subtract(end, middle));
  if (a < 1e-8 || b < 1e-8) throw Error("A two-bone pose needs nonzero bones");
  const delta = subtract(target, root),
    direction = unit(delta);
  const distance = Math.max(Math.abs(a - b) + 1e-6, Math.min(a + b - 1e-6, Math.hypot(...delta)));
  const along = (a * a - b * b + distance * distance) / (2 * distance);
  const bend = Math.sqrt(Math.max(0, a * a - along * along));
  const poleDelta = subtract(pole, root),
    projection = poleDelta.reduce((sum, value, axis) => sum + value * direction[axis], 0);
  let bendDirection = poleDelta.map((value, axis) => value - direction[axis] * projection) as Vec3;
  if (Math.hypot(...bendDirection) < 1e-8) {
    const fallback: Vec3 = Math.abs(direction[1]) < 0.9 ? [0, 1, 0] : [0, 0, 1];
    const dot = fallback.reduce((sum, value, axis) => sum + value * direction[axis], 0);
    bendDirection = fallback.map((value, axis) => value - direction[axis] * dot) as Vec3;
  }
  const perpendicular = unit(bendDirection);
  const knee = root.map(
    (value, axis) => value + direction[axis] * along + perpendicular[axis] * bend,
  ) as Vec3;
  const foot = root.map((value, axis) => value + direction[axis] * distance) as Vec3;
  const upper = between(subtract(middle, root), subtract(knee, root));
  const lower = between(subtract(end, middle), subtract(foot, knee));
  return [euler(upper), euler(multiply(inverse(upper), lower)), euler(inverse(lower))];
}
