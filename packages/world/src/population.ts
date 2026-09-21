import { terrainHeight, terrainNormal } from "@wrela/compiler";
import {
  contentKey,
  coordinateHash,
  MAX_WORLD_COORDINATE,
  random01,
  type TerrainDefinition,
  type Vec3,
  type WorldDefinition,
} from "@wrela/model";
import { z } from "zod";
import { worldPosition } from "./coordinates";
export type Placement = { id: string; definition: string; position: Vec3; rotation: number; scale: number };
export type Region = { minX: number; minZ: number; maxX: number; maxZ: number };
export type PopulationReport = {
  policy: "stable-canonical-cells-per-rule";
  limit: number;
  canonicalCells: number;
  evaluated: number;
  emitted: number;
  saturated: boolean;
  perRule: Record<string, number>;
};
export type PersistentOverride = {
  removed?: boolean;
  position?: Vec3;
  rotation?: Vec3;
  state?: Record<string, number | string | boolean>;
  definition?: string;
  scale?: number;
};
export type DormantEntity = {
  id: string;
  definition: string;
  position: Vec3;
  velocity: Vec3;
  state: Record<string, number | string | boolean>;
};
export type WorldSave = {
  version: 1;
  world: string;
  generatorVersion: string;
  generatorKey: string;
  overrides: [string, PersistentOverride][];
  dormant: DormantEntity[];
};
const savedVector = z.tuple([
  z.number().finite().min(-1e9).max(1e9),
  z.number().finite().min(-1e9).max(1e9),
  z.number().finite().min(-1e9).max(1e9),
]);
const savedState = z
  .record(z.string().max(128), z.union([z.number().finite(), z.string().max(4096), z.boolean()]))
  .refine((state) => Object.keys(state).length <= 64, "Too many entity state fields");
const overrideSchema = z
  .object({
    removed: z.boolean().optional(),
    position: savedVector.optional(),
    rotation: savedVector.optional(),
    state: savedState.optional(),
    definition: z.string().min(1).max(100).optional(),
    scale: z.number().finite().positive().max(1000).optional(),
  })
  .strict();
const dormantSchema = z
  .object({
    id: z.string().min(1).max(512),
    definition: z.string().min(1).max(100),
    position: savedVector,
    velocity: savedVector,
    state: savedState,
  })
  .strict();
const saveSchema = z
  .object({
    version: z.literal(1),
    world: z.string().max(100),
    generatorVersion: z.string().max(100),
    generatorKey: z.string().min(1).max(128),
    overrides: z.array(z.tuple([z.string().min(1).max(512), overrideSchema])).max(100000),
    dormant: z.array(dormantSchema).max(100000),
  })
  .strict();
export function generatorCompatibility(world: WorldDefinition, terrain: TerrainDefinition): string {
  return contentKey({
    world: world.id,
    version: world.generatorVersion,
    populations: world.populations,
    terrain: {
      id: terrain.id,
      seed: terrain.seed,
      amplitude: terrain.amplitude,
      frequency: terrain.frequency,
      octaves: terrain.octaves,
      baseHeight: terrain.baseHeight,
    },
  });
}
export class PersistentWorldState {
  readonly overrides = new Map<string, PersistentOverride>();
  readonly dormant = new Map<string, DormantEntity>();
  constructor(
    readonly world: string,
    readonly generatorVersion: string,
    private generatorKey = contentKey({ world, generatorVersion }),
  ) {}
  save(): WorldSave {
    return structuredClone({
      version: 1,
      world: this.world,
      generatorVersion: this.generatorVersion,
      generatorKey: this.generatorKey,
      overrides: [...this.overrides],
      dormant: [...this.dormant.values()],
    });
  }
  validate(input: WorldSave): WorldSave {
    const save = saveSchema.parse(input);
    if (
      save.world !== this.world ||
      save.generatorVersion !== this.generatorVersion ||
      save.generatorKey !== this.generatorKey
    )
      throw new Error("Runtime save is incompatible with this world generator fingerprint");
    if (
      new Set(save.overrides.map(([id]) => id)).size !== save.overrides.length ||
      new Set(save.dormant.map((entity) => entity.id)).size !== save.dormant.length
    )
      throw new Error("Runtime save contains duplicate identities");
    return save;
  }
  load(input: WorldSave) {
    const save = this.validate(input);
    // Validate the complete save before replacing either collection.
    this.overrides.clear();
    this.dormant.clear();
    for (const [id, value] of save.overrides) this.overrides.set(id, value);
    for (const entity of save.dormant) this.dormant.set(entity.id, entity);
  }
  updateCompatibility(key: string) {
    if (key !== this.generatorKey && (this.overrides.size || this.dormant.size))
      throw new Error(
        "Generator rules changed while persistent runtime changes exist; export the runtime save and reset it before changing generation",
      );
    this.generatorKey = key;
  }
  clear() {
    this.overrides.clear();
    this.dormant.clear();
  }
  retain(placement: Placement, state: Record<string, number | string | boolean> = {}) {
    this.overrides.set(
      placement.id,
      overrideSchema.parse({
        position: placement.position,
        rotation: [0, placement.rotation, 0],
        definition: placement.definition,
        scale: placement.scale,
        state,
      }),
    );
  }
  sleep(entity: DormantEntity) {
    const validated = dormantSchema.parse(entity);
    this.dormant.set(validated.id, validated);
  }
  wake(id: string) {
    const value = this.dormant.get(id);
    if (value) this.dormant.delete(id);
    return value;
  }
}
export function generatePlacements(
  world: WorldDefinition,
  terrain: TerrainDefinition,
  region: Region,
  overrides: ReadonlyMap<string, PersistentOverride> = new Map(),
  limit = 4096,
  options: { report?: (report: PopulationReport) => void } = {},
): Placement[] {
  worldPosition([region.minX, 0, region.minZ]);
  worldPosition([region.maxX, 0, region.maxZ]);
  if (!Number.isInteger(limit) || limit < 1 || limit > 65536) throw new RangeError("Invalid placement limit");
  if (region.maxX < region.minX || region.maxZ < region.minZ)
    throw new RangeError("Invalid placement region");
  const result: Placement[] = [],
    emitted = new Set<string>();
  let evaluated = 0,
    canonicalCells = 0;
  const finish = () => {
    options.report?.({
      policy: "stable-canonical-cells-per-rule",
      limit,
      canonicalCells,
      evaluated,
      emitted: result.length,
      saturated: result.length === limit,
      perRule: Object.fromEntries(
        world.populations.map((rule) => [
          rule.id,
          result.filter((placement) =>
            placement.id.startsWith(`${world.id}/${world.generatorVersion}/${rule.id}/`),
          ).length,
        ]),
      ),
    });
    return result;
  };
  const prefix = `${world.id}/${world.generatorVersion}/`;
  const rules = new Map(world.populations.map((rule) => [rule.id, rule]));
  function candidate(
    rule: WorldDefinition["populations"][number],
    x: number,
    z: number,
  ): Placement | undefined {
    if (++evaluated > 65536) throw new RangeError("Placement request exceeds bounded canonical-cell budget");
    const id = `${prefix}${rule.id}/${x}/${z}`;
    if (emitted.has(id)) return;
    const seed = coordinateHash(x, z, rule.seed);
    const override = overrides.get(id);
    if (override?.removed || (!override && random01(seed) > rule.density)) return;
    const px = (x + random01(seed ^ 0x21341)) * rule.spacing,
      pz = (z + random01(seed ^ 0x71371)) * rule.spacing;
    if (Math.abs(px) > MAX_WORLD_COORDINATE || Math.abs(pz) > MAX_WORLD_COORDINATE) return;
    const actualX = override?.position?.[0] ?? px,
      actualZ = override?.position?.[2] ?? pz;
    if (actualX < region.minX || actualX >= region.maxX || actualZ < region.minZ || actualZ >= region.maxZ)
      return;
    const height = override?.position?.[1] ?? terrainHeight(terrain, px, pz);
    if (
      !override &&
      (height < rule.minHeight ||
        height > rule.maxHeight ||
        1 - terrainNormal(terrain, px, pz)[1] > rule.maxSlope ||
        terrain.interventions.some(
          (edit) =>
            edit.kind === "clearing" && Math.hypot(px - edit.center[0], pz - edit.center[1]) < edit.radius,
        ))
    )
      return;
    return {
      id,
      definition: override?.definition ?? rule.definition,
      position: override?.position ?? [px, height, pz],
      rotation: override?.rotation?.[1] ?? random01(seed ^ 0x1947) * Math.PI * 2,
      scale: override?.scale ?? 0.8 + random01(seed ^ 0x9342) * 0.4,
    };
  }
  // Relocated objects belong to their current region, even when their canonical
  // generating cell is no longer resident. Prioritize meaningful saved changes
  // before filling the optional procedural population budget.
  if (overrides.size > 100000) throw new RangeError("Runtime override budget exceeded");
  const relocated = [...overrides]
    .filter(
      ([id, override]) =>
        id.startsWith(prefix) &&
        override.position &&
        !override.removed &&
        override.position[0] >= region.minX &&
        override.position[0] < region.maxX &&
        override.position[2] >= region.minZ &&
        override.position[2] < region.maxZ,
    )
    .sort(([a], [b]) => a.localeCompare(b));
  for (const [id] of relocated) {
    const parts = id.slice(prefix.length).split("/");
    if (parts.length !== 3) continue;
    const rule = rules.get(parts[0]),
      x = Number(parts[1]),
      z = Number(parts[2]);
    if (
      !rule ||
      !Number.isSafeInteger(x) ||
      !Number.isSafeInteger(z) ||
      id !== `${prefix}${rule.id}/${x}/${z}`
    )
      continue;
    const placement = candidate(rule, x, z);
    if (placement) {
      result.push(placement);
      emitted.add(placement.id);
    }
    if (result.length > limit) throw new RangeError("Persistent population exceeds placement budget");
  }
  if (result.length === limit) return finish();
  // Canonical cells have a stable, location-independent lottery rank. Selecting
  // the same rank threshold across a moving region preserves most overlapping
  // identities, while covering the entire region instead of its first rows.
  // Round-robin admission gives each authored rule a share before sparse rules
  // donate their unused budget to denser rules. Definition order has no effect.
  canonicalCells = evaluated;
  const candidates = [...world.populations]
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((rule) => {
      const cells: { x: number; z: number; rank: number }[] = [];
      const minX = Math.floor(region.minX / rule.spacing),
        maxX = Math.ceil(region.maxX / rule.spacing),
        minZ = Math.floor(region.minZ / rule.spacing),
        maxZ = Math.ceil(region.maxZ / rule.spacing);
      canonicalCells += (maxX - minX) * (maxZ - minZ);
      if (canonicalCells > 65536)
        throw new RangeError("Placement request exceeds bounded canonical-cell budget");
      for (let z = minZ; z < maxZ; z++)
        for (let x = minX; x < maxX; x++)
          cells.push({ x, z, rank: coordinateHash(x, z, rule.seed ^ 0x59c113) });
      cells.sort((a, b) => a.rank - b.rank || a.z - b.z || a.x - b.x);
      let next = 0;
      return () => {
        while (next < cells.length) {
          const cell = cells[next++],
            placement = candidate(rule, cell.x, cell.z);
          if (placement) return placement;
        }
        return undefined;
      };
    });
  while (result.length < limit) {
    let any = false;
    for (const next of candidates) {
      const placement = next();
      if (!placement) continue;
      any = true;
      result.push(placement);
      if (result.length === limit) break;
    }
    if (!any) break;
  }
  return finish();
}
