import type { Vec3 } from "@wrela/model";
export type EntityResidency = { id: string; position: Vec3; dormant: boolean; removed: boolean };
export type EntityResidencyPlan = { wanted: Set<string>; regions: Map<string, Vec3> };
/** Stable actor leases share nearby physical regions while respecting the world planner budget. */
export function planEntityResidency(
  entities: EntityResidency[],
  focusId: string,
  regionBudget: number,
): EntityResidencyPlan {
  const focus = entities.find((entity) => entity.id === focusId);
  const wanted = new Set<string>(),
    regions = new Map<string, Vec3>();
  if (!focus) return { wanted, regions };
  const distance = (a: Vec3, b: Vec3) => Math.hypot(a[0] - b[0], a[2] - b[2]);
  const ordered = [...entities].sort(
    (a, b) =>
      Number(b.id === focusId) - Number(a.id === focusId) ||
      distance(a.position, focus.position) - distance(b.position, focus.position) ||
      a.id.localeCompare(b.id),
  );
  for (const entity of ordered) {
    if (
      entity.removed ||
      (entity.id !== focusId && distance(entity.position, focus.position) > (entity.dormant ? 48 : 80))
    )
      continue;
    if (![...regions.values()].some((position) => distance(position, entity.position) <= 24)) {
      if (regions.size >= regionBudget) continue;
      regions.set(entity.id, [...entity.position]);
    }
    wanted.add(entity.id);
  }
  return { wanted, regions };
}
