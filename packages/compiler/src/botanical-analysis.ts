import { add, type BotanicalGrowthState, scale, sub } from "@wrela/model";

export function analyzeBotanicalGrowth(state: BotanicalGrowthState) {
  const violations: string[] = [];
  const ids = new Set<string>(),
    byId = new Map(state.shoots.map((s) => [s.id, s]));
  let height = 0,
    radius = 0,
    length = 0,
    foliage = 0,
    minimumFoliageHeight = Infinity;
  const orders: Record<string, number> = {};
  for (const shoot of state.shoots) {
    if (ids.has(shoot.id)) violations.push(`duplicate:${shoot.id}`);
    ids.add(shoot.id);
    const parent = shoot.parent ? byId.get(shoot.parent) : undefined;
    if (
      shoot.parent &&
      (!parent ||
        parent.birth >= shoot.birth ||
        Math.hypot(
          ...sub(shoot.start, add(parent.start, scale(sub(parent.end, parent.start), shoot.attachment ?? 1))),
        ) > 1e-8)
    )
      violations.push(`attachment:${shoot.id}`);
    if (
      ![...shoot.start, ...shoot.end, shoot.radius, shoot.tipRadius].every(Number.isFinite) ||
      shoot.radius < shoot.tipRadius ||
      shoot.tipRadius <= 0
    )
      violations.push(`geometry:${shoot.id}`);
    if (shoot.state === "pruned") continue;
    height = Math.max(height, shoot.end[1]);
    radius = Math.max(radius, Math.hypot(shoot.end[0], shoot.end[2]));
    length += Math.hypot(...sub(shoot.end, shoot.start));
    orders[shoot.order] = (orders[shoot.order] ?? 0) + 1;
    foliage += shoot.foliage.count * shoot.foliage.retained;
    if (shoot.foliage.retained > 0) minimumFoliageHeight = Math.min(minimumFoliageHeight, shoot.start[1]);
  }
  for (const entry of state.ledger)
    if (
      entry.end < 0 ||
      Math.abs(entry.start + entry.assimilated - entry.maintenance - entry.growth - entry.lost - entry.end) >
        1e-8
    )
      violations.push(`resources:${entry.step}`);
  return {
    species: state.species,
    seed: state.seed,
    step: state.step,
    height,
    crownRadius: radius,
    crownRatio: height ? Math.max(0, 1 - minimumFoliageHeight / height) : 0,
    woodLength: length,
    foliage,
    shoots: state.shoots.length,
    orders,
    reserve: state.reserve,
    limited: state.limited,
    violations,
  };
}
