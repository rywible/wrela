import type { RenderSurface } from "@wrela/model";

/** Bounded off-screen fallback. These ellipsoids are appearance estimates, never visibility certificates. */
export function compileWaterReflectionProxies(surfaces: readonly RenderSurface[]): Float32Array {
  const groups = new Map<string, { min: number[]; max: number[]; color: number[]; rough: number }>();
  for (const surface of surfaces) {
    if (
      surface.water ||
      surface.waterContact ||
      surface.id.endsWith("-bed") ||
      surface.mesh.bounds.max[0] - surface.mesh.bounds.min[0] > 80
    )
      continue;
    const m = surface.matrix,
      b = surface.mesh.bounds;
    const min = [Infinity, Infinity, Infinity],
      max = [-Infinity, -Infinity, -Infinity];
    for (let corner = 0; corner < 8; corner++) {
      const p = [
        corner & 1 ? b.max[0] : b.min[0],
        corner & 2 ? b.max[1] : b.min[1],
        corner & 4 ? b.max[2] : b.min[2],
      ];
      for (let axis = 0; axis < 3; axis++) {
        const value = m[axis] * p[0] + m[axis + 4] * p[1] + m[axis + 8] * p[2] + m[axis + 12];
        min[axis] = Math.min(min[axis], value);
        max[axis] = Math.max(max[axis], value);
      }
    }
    if (max[1] - min[1] < 0.3) continue;
    const key = surface.instanceId ?? surface.id.split("/", 1)[0],
      old = groups.get(key);
    if (old) {
      if (surface.mesh.thinCoverage) old.color = [...surface.material.color];
      for (let i = 0; i < 3; i++) {
        old.min[i] = Math.min(old.min[i], min[i]);
        old.max[i] = Math.max(old.max[i], max[i]);
      }
    } else
      groups.set(key, { min, max, color: [...surface.material.color], rough: surface.material.roughness });
  }
  const selected = [...groups.values()]
    .sort((a, b) => b.max[1] - b.min[1] - (a.max[1] - a.min[1]))
    .slice(0, 16);
  const data = new Float32Array(selected.length * 12);
  selected.forEach((p, i) => {
    data.set(
      [
        (p.min[0] + p.max[0]) / 2,
        (p.min[1] + p.max[1]) / 2,
        (p.min[2] + p.max[2]) / 2,
        p.rough,
        Math.max(0.1, (p.max[0] - p.min[0]) / 2),
        Math.max(0.1, (p.max[1] - p.min[1]) / 2),
        Math.max(0.1, (p.max[2] - p.min[2]) / 2),
        0,
        ...p.color,
        1,
      ],
      i * 12,
    );
  });
  return data;
}
