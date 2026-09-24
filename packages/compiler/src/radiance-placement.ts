import type { Vec3 } from "@wrela/model";
import type { IndirectGeometry } from "./indirect-query";

const add = (a: Vec3, b: Vec3, scale = 1): Vec3 => a.map((v, i) => v + b[i] * scale) as Vec3;
const distance = (a: Vec3, b: Vec3) => Math.hypot(...a.map((v, i) => v - b[i]));

/** Bounded spatial surface sampling. Coverage is validated independently by
 * lighting-reference-check; this does not claim a visibility/error certificate. */
export function* radianceSamplePositions(
  geometry: IndirectGeometry,
  center: Vec3,
  budget: number,
): Generator<void, Vec3[]> {
  const cells = new Map<string, { position: Vec3; normal: Vec3 }>();
  // Spatial sampling, not mesh density, controls the lighting budget. Both
  // sides support two-sided room shells and ordinary outward-facing meshes.
  for (const t of geometry.triangles) {
    const divisions = Math.min(
      16,
      Math.max(1, Math.ceil(Math.max(Math.hypot(...t.ab), Math.hypot(...t.ac)) / 2)),
    );
    for (let y = 0; y < divisions; y++)
      for (let x = 0; x < divisions - y; x++) {
        const p = add(add(t.a, t.ab, (x + 1 / 3) / divisions), t.ac, (y + 1 / 3) / divisions);
        if (distance(p, center) > 48) continue;
        const key = [...p.map((v) => Math.floor(v / 1.5)), ...t.normal.map((v) => Math.round(v))].join(",");
        if (!cells.has(key)) cells.set(key, { position: p, normal: t.normal });
      }
    yield;
  }
  const candidates = [...cells.values()];
  const positions: Vec3[] = [];
  const nearest = new Float64Array(candidates.length).fill(Infinity);
  const importance = candidates.map((c) => 1 / (1 + distance(c.position, center) ** 2 / 144));
  let chosen = candidates.reduce((best, _c, i) => (importance[i] > (importance[best] ?? -1) ? i : best), 0);
  for (let pair = 0; pair < Math.floor(budget / 2) && candidates.length; pair++) {
    const candidate = candidates[chosen];
    positions.push(
      add(candidate.position, candidate.normal, 0.025),
      add(candidate.position, candidate.normal, -0.025),
    );
    let next = -1,
      score = -1;
    for (let i = 0; i < candidates.length; i++) {
      const d = candidates[i].position.reduce((sum, v, a) => sum + (v - candidate.position[a]) ** 2, 0);
      nearest[i] = Math.min(nearest[i], d);
      const priority = nearest[i] * importance[i];
      if (priority > score) {
        score = priority;
        next = i;
      }
      if (i % 512 === 511) yield;
    }
    if (score < 0.000001) break;
    chosen = next;
  }
  return positions;
}
