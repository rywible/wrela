import {
  add,
  BOTANICAL_GROWTH_VERSION,
  type BotanicalDevelopment,
  type BotanicalGrowthState,
  botanicalSpeciesTraits,
  contentKey,
  cross,
  type GrowthBud,
  type GrowthShoot,
  normalize,
  scale,
  sub,
  type Vec3,
} from "@wrela/model";

import { botanicalRandom } from "./botanical-branch";
import { botanicalEnvironment } from "./botanical-environment";

export const MAX_GROWTH_SHOOTS = 8192;
/** Source identity excludes requested age: a checkpoint can continue into the future. */
export function botanicalGrowthKey(seed: number, source: BotanicalDevelopment): string {
  return contentKey({
    version: source.version,
    algorithm: "development-solver-8",
    traits: botanicalSpeciesTraits[source.species],
    seed,
    species: source.species,
    environment: source.environment,
    events: [...source.events].sort((a, b) => a.step - b.step || a.id.localeCompare(b.id)),
  });
}
export function seedBotanicalGrowth(seed: number, source: BotanicalDevelopment): BotanicalGrowthState {
  const random = (name: string) => botanicalRandom(seed, `individual/${name}`);
  const vigor = 0.82 + random("vigor") * 0.36;
  return {
    version: BOTANICAL_GROWTH_VERSION,
    sourceKey: botanicalGrowthKey(seed, source),
    seed,
    species: source.species,
    step: 0,
    reserve: 1.4,
    individual: {
      vigor,
      branching: vigor * (0.9 + random("branching") * 0.2),
      angle: (random("angle") - 0.5) * 0.24,
      foliage: vigor * (0.85 + random("foliage") * 0.3),
    },
    environment: structuredClone(source.environment),
    shoots: [],
    buds: [
      {
        id: "trunk",
        parent: null,
        birth: 0,
        order: 0,
        direction: [0, 1, 0],
        terminal: true,
        state: "active",
        lastExtension: 0,
        reason: "seed",
      },
    ],
    appliedEvents: [],
    ledger: [],
    limited: false,
  };
}

/** One synchronous, immutable step. Decisions read only the start-of-step graph.
 * Resource units are a causal source/sink model, not a claim of carbon physiology. */
export function advanceBotanicalGrowth(
  previous: BotanicalGrowthState,
  source: BotanicalDevelopment,
): BotanicalGrowthState {
  if (
    previous.version !== BOTANICAL_GROWTH_VERSION ||
    previous.sourceKey !== botanicalGrowthKey(previous.seed, source)
  )
    throw Error("Growth checkpoint does not match its source");
  if (previous.step >= 64) throw Error("Growth step limit reached");
  const next = structuredClone(previous);
  next.step++;
  const step = next.step,
    traits = botanicalSpeciesTraits[next.species];
  const random = (key: string) => botanicalRandom(next.seed, key);
  next.shoots.sort((a, b) => a.birth - b.birth || a.id.localeCompare(b.id));
  next.buds.sort((a, b) => a.id.localeCompare(b.id));
  const shoots = new Map(next.shoots.map((shoot) => [shoot.id, shoot]));
  for (const event of [...source.events]
    .filter((event) => event.step === step)
    .sort((a, b) => a.id.localeCompare(b.id))) {
    next.appliedEvents.push(event.id);
    if (event.kind === "environment") {
      next.environment = structuredClone(event.environment);
      continue;
    }
    if (!shoots.has(event.organ))
      throw Error(`Growth event ${event.id} refers to missing organ ${event.organ}`);
    const affected = new Set([event.organ]);
    for (const shoot of next.shoots) if (shoot.parent && affected.has(shoot.parent)) affected.add(shoot.id);
    for (const id of affected) {
      const shoot = shoots.get(id);
      if (!shoot) throw Error("Missing affected shoot");
      shoot.state = event.kind === "prune" ? "pruned" : "dead";
      shoot.foliage.retained = 0;
    }
    for (const bud of next.buds)
      if (bud.parent && affected.has(bud.parent)) {
        bud.state = "dead";
        bud.reason = "removed";
      }
  }
  const exposure = botanicalEnvironment(next, next.environment);
  let assimilation = next.shoots.length ? 0 : 0.7 * next.environment.light;
  let requestedMaintenance = 0;
  const continued = new Set(
    next.shoots
      .filter(
        (shoot) => shoot.state === "living" && shoot.parent && shoots.get(shoot.parent)?.axis === shoot.axis,
      )
      .map((shoot) => shoot.parent),
  );
  for (const shoot of next.shoots) {
    if (shoot.state !== "living") continue;
    const light = exposure(shoot.end).light;
    shoot.vigor = light;
    shoot.shadeSteps = light < traits.shadeTolerance ? shoot.shadeSteps + 1 : 0;
    const previousRetention = shoot.foliage.retained;
    const foliageAge = step - shoot.foliage.birth;
    shoot.foliage.retained =
      next.species === "paper-birch"
        ? Number(foliageAge === 0)
        : Math.max(0, 1 - Math.max(0, foliageAge - traits.retention + 1) / 2);
    // Deciduous foliage renews on living short shoots; old bare stems do not sprout arbitrary leaves.
    if (next.species === "paper-birch" && shoot.habit === "short" && !continued.has(shoot.id)) {
      shoot.foliage.birth = step;
      shoot.foliage.retained = 1;
    }
    assimilation +=
      ((next.species === "paper-birch" ? previousRetention : shoot.foliage.retained) *
        light *
        0.22 *
        shoot.foliage.count) /
      (next.species === "lodgepole-pine" ? 80 : 12);
    requestedMaintenance +=
      next.species === "paper-birch"
        ? 0.0003 + shoot.radius * 0.015 + shoot.foliage.count * previousRetention * 0.00006
        : 0.006 + shoot.radius * 0.05;
    if (shoot.shadeSteps >= 4 && shoot.order > 0) {
      shoot.state = "dead";
      shoot.foliage.retained = 0;
    }
  }
  // Death propagates before any new bud decision. Dead wood remains until explicitly pruned.
  for (const shoot of next.shoots) {
    const parent = shoot.parent ? shoots.get(shoot.parent) : undefined;
    if (shoot.parent && !parent) throw Error("Missing shoot parent");
    if (parent && parent.state !== "living") {
      shoot.state = parent.state;
      shoot.foliage.retained = 0;
    }
  }
  assimilation *= Math.sqrt(next.environment.water * next.environment.fertility);
  const available = next.reserve + assimilation;
  const maintenance = Math.min(available, requestedMaintenance);
  const budget = Math.max(0, available - maintenance) * 0.88;
  const requests: {
    bud: GrowthBud;
    light: number;
    direction: Vec3;
    length: number;
    cost: number;
    priority: number;
  }[] = [];
  for (const bud of next.buds) {
    if (bud.state === "dead") continue;
    const parent = bud.parent ? shoots.get(bud.parent) : undefined;
    if (parent && parent.state !== "living") {
      bud.state = "dead";
      bud.reason = "removed";
      continue;
    }
    if (bud.habit === "short" && parent?.habit === "short") {
      bud.state = "dormant";
      bud.reason = "short-shoot";
      continue;
    }
    const local = exposure(parent?.end ?? [0, 0, 0]);
    if (local.light < traits.shadeTolerance) {
      bud.state = "dormant";
      bud.reason = "shade";
      continue;
    }
    const dominance = bud.terminal ? 1 : 1 - traits.apicalControl * Math.exp(-(step - bud.birth) * 0.65);
    if (!bud.terminal && random(`${bud.id}/activation/${step}`) > dominance * next.individual.branching) {
      bud.state = "dormant";
      bud.reason = "apical-control";
      continue;
    }
    const length =
      bud.habit === "short"
        ? 0.0025 * next.individual.vigor
        : (traits.extension *
            next.individual.vigor *
            (0.8 + random(`${bud.id}/${step}/length`) * 0.4) *
            traits.lateralLength ** bud.order *
            (0.45 + local.light * 0.55) *
            Math.sqrt(next.environment.water)) /
          ((1 + Math.max(0, step - 24) * 0.018) *
            (next.species === "paper-birch" && bud.order === 0 ? 1 + Math.max(0, step - 8) * 0.12 : 1));
    const jitter: Vec3 = [
      (random(`${bud.id}/${step}/x`) - 0.5) * 0.12,
      0,
      (random(`${bud.id}/${step}/z`) - 0.5) * 0.12,
    ];
    const direction = normalize(
      add(
        add(bud.direction, jitter),
        add(scale(local.direction, traits.phototropism * 0.2), [
          0,
          traits.gravity *
            (bud.order === 0
              ? 1 - bud.direction[1]
              : (next.species === "lodgepole-pine" ? 0.08 : 0.55) - bud.direction[1]),
          0,
        ]),
      ),
    );
    requests.push({
      bud,
      light: local.light,
      direction,
      length,
      cost: (bud.habit === "short" ? 0.025 : 0.12) + length * 0.7,
      priority: (bud.order === 0 ? 3 : 1 / (1 + bud.order * 0.4)) * local.light,
    });
  }
  const demand = requests.reduce((sum, request) => sum + request.cost * request.priority, 0);
  let spent = 0;
  for (const request of requests) {
    const { bud, light, direction } = request;
    const fraction = Math.min(1, (budget * request.priority) / Math.max(demand, 1e-9));
    if (fraction < 0.22) {
      bud.state = "dormant";
      bud.reason = "resources";
      continue;
    }
    if (next.shoots.length >= MAX_GROWTH_SHOOTS) {
      next.limited = true;
      bud.state = "dormant";
      bud.reason = "capacity";
      continue;
    }
    const cost = request.cost * fraction;
    spent += cost;
    const parent = bud.parent ? shoots.get(bud.parent) : undefined;
    if (bud.parent && !parent) throw Error("Missing bud parent");
    const start: Vec3 = parent
      ? add(parent.start, scale(sub(parent.end, parent.start), bud.attachment ?? 1))
      : [0, 0, 0];
    const id = `${bud.id}.${step}`;
    const shoot: GrowthShoot = {
      id,
      parent: bud.parent,
      attachment: bud.attachment ?? 1,
      habit: bud.habit ?? "long",
      axis: bud.id,
      birth: step,
      order: bud.order,
      start,
      end: add(start, scale(direction, request.length * fraction)),
      direction,
      radius: 0.0018,
      tipRadius: 0.0012,
      state: "living",
      vigor: light,
      shadeSteps: 0,
      foliage: {
        birth: step,
        count:
          bud.habit === "short"
            ? 3
            : (next.species === "lodgepole-pine" ? 2 : 1) *
              Math.round(
                (next.species === "lodgepole-pine" ? 40 : 12) *
                  next.individual.foliage *
                  (0.65 + 0.35 * fraction),
              ),
        retained: 1,
        tint: 0.85 + random(`${id}/cohort`) * 0.15,
      },
    };
    next.shoots.push(shoot);
    shoots.set(id, shoot);
    bud.parent = id;
    bud.attachment = 1;
    bud.direction = direction;
    bud.terminal = true;
    bud.state = "active";
    bud.lastExtension = step;
    bud.reason = "extended";
    if (next.species === "paper-birch" && bud.order > 0 && bud.habit !== "short") {
      // Proximal axillary short shoots bear renewed foliage without extending
      // another long axis. The attachment survives meshing, edits and saves.
      next.buds.push({
        id: `${id}/short`,
        parent: id,
        attachment: 0.2 + random(`${id}/short-position`) * 0.25,
        habit: "short",
        birth: step,
        order: Math.min(traits.maxOrder, bud.order + 1),
        direction: normalize(
          add(
            scale(direction, 0.4),
            scale(
              normalize(cross(direction, Math.abs(direction[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0])),
              random(`${id}/short-side`) < 0.5 ? -1 : 1,
            ),
          ),
        ),
        terminal: true,
        state: "dormant",
        lastExtension: step,
        reason: "apical-control",
      });
    }
    if (
      bud.habit === "short" ||
      bud.order >= traits.maxOrder ||
      step - bud.birth < 1 ||
      (bud.order > 0 && (step + bud.birth) % (2 + bud.order) !== 0)
    )
      continue;
    const count = bud.order === 0 ? traits.whorl : 1;
    const u = normalize(cross(direction, Math.abs(direction[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
    const v = cross(direction, u);
    for (let i = 0; i < count; i++) {
      const angle = step * 2.399963 + (i * Math.PI * 2) / count + random(`${id}/${i}/angle`) * 0.32;
      const lateral = add(scale(u, Math.cos(angle)), scale(v, Math.sin(angle)));
      const inclination = traits.branchAngle + next.individual.angle;
      next.buds.push({
        id: `${id}/${i}`,
        parent: id,
        birth: step,
        order: bud.order + 1,
        direction: normalize(
          add(scale(direction, Math.cos(inclination)), scale(lateral, Math.sin(inclination))),
        ),
        terminal: false,
        state: "dormant",
        lastExtension: step,
        reason: "apical-control",
      });
    }
  }
  // Secondary growth: support descendant foliage demand, with monotonic wood radii.
  const area = new Map<string, number>();
  for (let i = next.shoots.length - 1; i >= 0; i--) {
    const shoot = next.shoots[i];
    if (shoot.state === "pruned") continue;
    const own = shoot.state === "living" ? 0.0000025 * (0.4 + shoot.foliage.retained) : 0;
    const descendant = area.get(shoot.id) ?? 0;
    shoot.tipRadius = Math.max(shoot.tipRadius, Math.sqrt(descendant + own * 0.3));
    shoot.radius = Math.max(shoot.radius, Math.sqrt(descendant + own) * 1.08);
    if (shoot.parent) area.set(shoot.parent, (area.get(shoot.parent) ?? 0) + descendant + own);
  }
  const reserve = Math.max(0, available - maintenance - spent);
  next.reserve = Math.min(20, reserve);
  next.ledger.push({
    step,
    start: previous.reserve,
    assimilated: assimilation,
    maintenance,
    growth: spent,
    end: next.reserve,
    lost: Math.max(0, reserve - 20),
  });
  return next;
}
export function growBotanical(
  seed: number,
  source: BotanicalDevelopment,
  checkpoint?: BotanicalGrowthState,
): BotanicalGrowthState {
  let state = checkpoint ?? seedBotanicalGrowth(seed, source);
  if (
    state.seed !== seed ||
    state.sourceKey !== botanicalGrowthKey(seed, source) ||
    state.step > source.steps
  )
    throw Error("Incompatible growth checkpoint");
  while (state.step < source.steps) state = advanceBotanicalGrowth(state, source);
  return state;
}
