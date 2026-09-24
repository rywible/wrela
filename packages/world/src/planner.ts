import { MAX_WORLD_COORDINATE, type Vec3 } from "@wrela/model";

import { worldPosition } from "./coordinates";
export type InterestSource = {
  id: string;
  position: Vec3;
  visualRadius: number;
  collisionRadius: number;
  priority?: number;
};
export type PatchAddress = {
  x: number;
  z: number;
  size: number;
  level: number;
  id: string;
  collision: boolean;
  stitch: { north?: boolean; east?: boolean; south?: boolean; west?: boolean };
};
export type PlannerOptions = { baseSize: number; levels: number; maxPatches: number; maxRadius: number };
export const defaultPlanner: PlannerOptions = { baseSize: 32, levels: 3, maxPatches: 160, maxRadius: 768 };
const key = (x: number, z: number, size: number) => `${x}:${z}:${size}`;
function distance(x: number, z: number, size: number, p: Vec3) {
  return Math.hypot(Math.max(x - p[0], 0, p[0] - x - size), Math.max(z - p[2], 0, p[2] - z - size));
}
function adjacent(a: PatchAddress, b: PatchAddress) {
  return (
    ((a.x + a.size === b.x || b.x + b.size === a.x) &&
      Math.max(a.z, b.z) < Math.min(a.z + a.size, b.z + b.size)) ||
    ((a.z + a.size === b.z || b.z + b.size === a.z) &&
      Math.max(a.x, b.x) < Math.min(a.x + a.size, b.x + b.size))
  );
}
export function planTerrain(
  interests: InterestSource[],
  options: Partial<PlannerOptions> = {},
  previous: Set<string> = new Set(),
): PatchAddress[] {
  const o = { ...defaultPlanner, ...options };
  if (
    !Number.isFinite(o.baseSize) ||
    o.baseSize < 4 ||
    o.baseSize > 65536 ||
    !Number.isInteger(o.levels) ||
    o.levels < 0 ||
    o.levels > 8 ||
    !Number.isInteger(o.maxPatches) ||
    o.maxPatches < 4 ||
    o.maxPatches > 256 ||
    !Number.isFinite(o.maxRadius) ||
    o.maxRadius <= 0 ||
    o.maxRadius > 8192
  )
    throw new RangeError("Invalid terrain planner budget");
  if (interests.length > 8) throw new RangeError("At most eight independent interest sources are supported");
  for (const interest of interests) {
    worldPosition(interest.position);
    if (
      interest.priority !== undefined &&
      (!Number.isFinite(interest.priority) || interest.priority <= 0 || interest.priority > 100)
    )
      throw new RangeError("Invalid interest priority");
    if (
      ![interest.visualRadius, interest.collisionRadius].every(
        (r) => Number.isFinite(r) && r >= 0 && r <= o.maxRadius,
      )
    )
      throw new RangeError("Interest radius outside budget");
  }
  const rootSize = o.baseSize * 2 ** o.levels;
  const previouslySplit = new Set<string>();
  for (const id of previous) {
    const [px, pz, patchSize] = id.split(":").map(Number);
    if (![px, pz, patchSize].every(Number.isFinite) || patchSize <= 0) continue;
    for (let size = patchSize * 2; size <= rootSize; size *= 2)
      previouslySplit.add(key(Math.floor(px / size) * size, Math.floor(pz / size) * size, size));
  }
  const patches = new Map<string, PatchAddress>();
  function make(x: number, z: number, size: number, level: number): PatchAddress {
    if (
      x < -MAX_WORLD_COORDINATE ||
      z < -MAX_WORLD_COORDINATE ||
      x + size > MAX_WORLD_COORDINATE ||
      z + size > MAX_WORLD_COORDINATE
    )
      throw new RangeError("Requested terrain region crosses the supported world boundary");
    return {
      x,
      z,
      size,
      level,
      id: key(x, z, size),
      collision: interests.some(
        (i) => i.collisionRadius > 0 && distance(x, z, size, i.position) <= i.collisionRadius,
      ),
      stitch: {},
    };
  }
  for (const i of interests) {
    const r = Math.max(i.visualRadius, i.collisionRadius);
    for (let z = Math.floor((i.position[2] - r) / rootSize) * rootSize; z <= i.position[2] + r; z += rootSize)
      for (
        let x = Math.floor((i.position[0] - r) / rootSize) * rootSize;
        x <= i.position[0] + r;
        x += rootSize
      )
        if (distance(x, z, rootSize, i.position) <= r) {
          patches.set(key(x, z, rootSize), make(x, z, rootSize, o.levels));
          if (patches.size > o.maxPatches) throw new RangeError("Interest coverage exceeds patch budget");
        }
  }
  if (patches.size > o.maxPatches)
    throw new RangeError("Interest coverage exceeds patch budget; reduce radius or increase base size");
  for (;;) {
    const candidates = [...patches.values()]
      .filter((p) => p.level > 0)
      .map((p) => ({
        p,
        score: Math.min(
          ...interests.map(
            (i) =>
              distance(p.x, p.z, p.size, i.position) /
              (p.size * (previouslySplit.has(p.id) ? 1.8 : 1.5)) /
              (i.priority ?? 1),
          ),
        ),
      }))
      .filter(({ score }) => score < 1)
      .sort(
        (a, b) =>
          Number(b.p.collision) - Number(a.p.collision) ||
          Math.floor(a.score * 20) - Math.floor(b.score * 20) ||
          Number(previouslySplit.has(b.p.id)) - Number(previouslySplit.has(a.p.id)) ||
          a.p.id.localeCompare(b.p.id),
      );
    if (!candidates.length || patches.size + 3 > o.maxPatches) break;
    const p = candidates[0].p;
    patches.delete(p.id);
    const s = p.size / 2;
    for (const dx of [0, s])
      for (const dz of [0, s]) {
        const child = make(p.x + dx, p.z + dz, s, p.level - 1);
        patches.set(child.id, child);
      }
  }
  // Coarsen fine siblings until every shared edge differs by at most one level.
  for (;;) {
    const list = [...patches.values()];
    let fine: PatchAddress | undefined;
    outer: for (const a of list)
      for (const b of list)
        if (a.level + 1 < b.level && adjacent(a, b)) {
          fine = a;
          break outer;
        }
    if (!fine) break;
    const size = fine.size * 2,
      x = Math.floor(fine.x / size) * size,
      z = Math.floor(fine.z / size) * size;
    for (const p of list)
      if (p.x >= x && p.z >= z && p.x + p.size <= x + size && p.z + p.size <= z + size) patches.delete(p.id);
    const parent = make(x, z, size, fine.level + 1);
    patches.set(parent.id, parent);
  }
  const list = [...patches.values()].sort(
    (a, b) => Number(b.collision) - Number(a.collision) || b.level - a.level || a.id.localeCompare(b.id),
  );
  for (const a of list)
    for (const b of list)
      if (b.level === a.level + 1 && adjacent(a, b)) {
        if (a.x === b.x + b.size) a.stitch.west = true;
        if (a.x + a.size === b.x) a.stitch.east = true;
        if (a.z === b.z + b.size) a.stitch.north = true;
        if (a.z + a.size === b.z) a.stitch.south = true;
      }
  return list;
}
