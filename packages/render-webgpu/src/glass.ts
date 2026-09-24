import type { RenderSurface, Vec3 } from "@wrela/model";

export function isThinGlass(surface: RenderSurface) {
  return surface.material.appearance?.family === "glass" && surface.material.appearance.transmission > 0;
}
const centers = new WeakMap<RenderSurface["mesh"], Map<string, Vec3>>();
/** Sort the actual material draw range, not the full multi-material assembly bounds. */
export function glassDrawDistance(surface: RenderSurface, camera: Vec3) {
  const mesh = surface.mesh,
    range = surface.drawRange ?? { start: 0, count: mesh.indices.length },
    key = `${range.start}:${range.count}`;
  let map = centers.get(mesh);
  if (!map) {
    map = new Map();
    centers.set(mesh, map);
  }
  let center = map.get(key);
  if (!center) {
    const min = [Infinity, Infinity, Infinity],
      max = [-Infinity, -Infinity, -Infinity];
    for (let i = range.start; i < range.start + range.count; i++) {
      const index = mesh.indices[i] * 3;
      for (let a = 0; a < 3; a++) {
        min[a] = Math.min(min[a], mesh.positions[index + a]);
        max[a] = Math.max(max[a], mesh.positions[index + a]);
      }
    }
    center = min.map((v, i) => (v + max[i]) * 0.5) as Vec3;
    map.set(key, center);
  }
  const m = surface.matrix,
    p = center;
  return Math.hypot(
    ...[0, 1, 2].map((i) => m[i] * p[0] + m[i + 4] * p[1] + m[i + 8] * p[2] + m[i + 12] - camera[i]),
  );
}
