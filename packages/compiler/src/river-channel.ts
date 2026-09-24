import type { WaterAuthoring } from "@wrela/model";

type River = NonNullable<WaterAuthoring["river"]>;
type Pair = [number, number];
export type RiverSection = { left: Pair; right: Pair };
const cache = new WeakMap<River, RiverSection[]>();
/** Shared bounded miter joins keep mesh and collision/query support identical at bends. */
export function riverSections(river: River): RiverSection[] {
  const cached = cache.get(river);
  if (cached) return cached;
  const directions = river.points.slice(1).map((point, index): Pair => {
    const previous = river.points[index].position;
    const dx = point.position[0] - previous[0],
      dz = point.position[1] - previous[1];
    const length = Math.hypot(dx, dz);
    return [dx / length, dz / length];
  });
  const sections = river.points.map((point, index) => {
    const before = directions[Math.max(0, index - 1)],
      after = directions[Math.min(index, directions.length - 1)];
    const nx = -before[1] - after[1],
      nz = before[0] + after[0];
    const length = Math.hypot(nx, nz);
    const normal: Pair = length > 1e-6 ? [nx / length, nz / length] : [-after[1], after[0]];
    const scale = Math.min(2, 1 / Math.max(0.5, normal[0] * -after[1] + normal[1] * after[0]));
    const offset: Pair = [(normal[0] * scale * point.width) / 2, (normal[1] * scale * point.width) / 2];
    return {
      left: [point.position[0] - offset[0], point.position[1] - offset[1]] as Pair,
      right: [point.position[0] + offset[0], point.position[1] + offset[1]] as Pair,
    };
  });
  cache.set(river, sections);
  return sections;
}
function barycentric(point: Pair, a: Pair, b: Pair, c: Pair): [number, number, number] | undefined {
  const cross = (u: Pair, v: Pair) => u[0] * v[1] - u[1] * v[0];
  const ab: Pair = [b[0] - a[0], b[1] - a[1]],
    ac: Pair = [c[0] - a[0], c[1] - a[1]],
    ap: Pair = [point[0] - a[0], point[1] - a[1]];
  const determinant = cross(ab, ac);
  if (Math.abs(determinant) < 1e-10) return;
  const u = cross(ap, ac) / determinant,
    v = cross(ab, ap) / determinant;
  return u >= -1e-7 && v >= -1e-7 && u + v <= 1.0000001 ? [1 - u - v, u, v] : undefined;
}
export function channelCoordinates(a: RiverSection, b: RiverSection, point: Pair): Pair | undefined {
  const first = barycentric(point, a.left, a.right, b.left);
  if (first) return [first[1], first[2]];
  const second = barycentric(point, a.right, b.right, b.left);
  if (second) return [second[0] + second[1], second[1] + second[2]];
}
