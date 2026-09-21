import { generateTerrainPatch, terrainHeight } from "@wrela/compiler";
import {
  contentKey,
  type MeshData,
  normalize,
  type TerrainDefinition,
  type Vec3,
  type WorldDefinition,
} from "@wrela/model";
import { worldPosition } from "./coordinates";
import { type InterestSource, type PatchAddress, type PlannerOptions, planTerrain } from "./planner";
import {
  generatePlacements,
  generatorCompatibility,
  PersistentWorldState,
  type PopulationReport,
  type Region,
  type WorldSave,
} from "./population";
import { BoundedScheduler } from "./scheduler";
export type ResidentPatch = PatchAddress & {
  key: string;
  mesh: MeshData;
  revision: number;
  sourceRevision: number;
};
export type WorldOptions = Partial<PlannerOptions> & {
  resolution?: number;
  maxCacheBytes?: number;
  concurrency?: number;
  generationTimeoutMs?: number;
  maxLiveBytes?: number;
  generate?: (
    terrain: TerrainDefinition,
    patch: PatchAddress,
    resolution: number,
    signal?: AbortSignal,
  ) => Promise<MeshData>;
};
export type GroundQuery =
  | {
      status: "ready";
      height: number;
      normal: Vec3;
      approximation: "resident-triangle-heightfield";
      revision: number;
      analyticHeight: number;
      sampleSpacing: number;
    }
  | { status: "not-resident"; reason: string };
export class WorldSession {
  readonly persistence: PersistentWorldState;
  private interests = new Map<string, InterestSource>();
  private scheduler: BoundedScheduler<MeshData>;
  private installed: ResidentPatch[] = [];
  private requested: PatchAddress[] = [];
  private requestedKeys = new Map<string, string>();
  private cache = new Map<string, { mesh: MeshData; bytes: number }>();
  private cacheBytes = 0;
  private generation = 0;
  private disposed = false;
  private pending: Promise<void> = Promise.resolve();
  private criticalPending: Promise<void> = Promise.resolve();
  private prepared = new Map<string, ResidentPatch>();
  private reservations = new Map<symbol, number>();
  private collisionGeneration = 0;
  private sourceRevision = 1;
  private readySourceRevision = 1;
  private failures: string[] = [];
  private populationReport?: PopulationReport;
  private readyTerrain: TerrainDefinition;
  private desiredSignature = "";
  private requestedRevision = 0;
  private publishedRevision = 0;
  constructor(
    readonly world: WorldDefinition,
    private terrain: TerrainDefinition,
    readonly options: WorldOptions = {},
  ) {
    if (
      options.resolution !== undefined &&
      (!Number.isInteger(options.resolution) ||
        options.resolution < 4 ||
        options.resolution > 64 ||
        options.resolution % 2 !== 0)
    )
      throw new RangeError("Terrain resolution must be even and between 4 and 64");
    for (const budget of [options.maxCacheBytes, options.maxLiveBytes])
      if (budget !== undefined && (!Number.isFinite(budget) || budget < 0))
        throw new RangeError("Invalid terrain memory budget");
    this.world = structuredClone(world);
    this.terrain = structuredClone(terrain);
    this.options = { ...options };
    this.persistence = new PersistentWorldState(
      world.id,
      world.generatorVersion,
      generatorCompatibility(world, terrain),
    );
    this.readyTerrain = structuredClone(terrain);
    this.scheduler = new BoundedScheduler(
      options.concurrency ?? 2,
      options.maxPatches ?? 192,
      options.generationTimeoutMs ?? 10000,
    );
  }
  get metrics() {
    return {
      ...this.scheduler.metrics,
      resident: this.installed.length,
      requested: this.requested.length,
      cacheBytes: this.cacheBytes,
      cacheEntries: this.cache.size,
      liveBytes: this.liveBytes(),
      generatingBytes: [...this.reservations.values()].reduce((sum, bytes) => sum + bytes, 0),
      revision: this.publishedRevision,
      residencyRevision: this.requestedRevision,
      sourceRevision: this.readySourceRevision,
      requestedSourceRevision: this.sourceRevision,
      collisionRevision: this.collisionGeneration,
      collisionPending: this.missing(true).length > 0,
      requestedRevision: this.requestedRevision,
      pending: this.missing(false).length > 0 || this.readySourceRevision !== this.sourceRevision,
      failures: [...this.failures],
      population: this.populationReport ? structuredClone(this.populationReport) : undefined,
    };
  }
  setInterest(interest: InterestSource) {
    worldPosition(interest.position);
    this.interests.set(interest.id, structuredClone(interest));
  }
  interestSources(): InterestSource[] {
    return structuredClone([...this.interests.values()]);
  }
  setInterests(interests: InterestSource[]) {
    planTerrain(interests, this.options);
    this.interests = new Map(interests.map((interest) => [interest.id, structuredClone(interest)]));
  }
  removeInterest(id: string) {
    this.interests.delete(id);
  }
  residentPatches(): readonly ResidentPatch[] {
    return this.installed;
  }
  collisionPatches(): readonly ResidentPatch[] {
    return this.installed.filter((patch) => patch.collision);
  }
  private missing(collisionOnly: boolean): string[] {
    const available = new Set(
      this.installed.filter((patch) => !collisionOnly || patch.collision).map((patch) => patch.key),
    );
    return this.requested
      .filter(
        (patch) =>
          (!collisionOnly || patch.collision) && !available.has(this.requestedKeys.get(patch.id) ?? ""),
      )
      .map((patch) => patch.id);
  }
  update(): void {
    if (this.disposed) throw new Error("World disposed");
    const plan = planTerrain(
      [...this.interests.values()],
      this.options,
      new Set(this.installed.map((p) => p.id)),
    );
    const signature = contentKey({ terrain: this.terrain, plan });
    if (signature === this.desiredSignature) return;
    this.desiredSignature = signature;
    const generation = ++this.generation;
    this.requestedRevision = generation;
    this.requested = plan;
    this.prepared = new Map();
    this.failures = [];
    const terrain = structuredClone(this.terrain),
      resolution = this.options.resolution ?? 16;
    const sourceRevision = this.sourceRevision;
    const keys = plan.map((patch) => this.patchKey(terrain, patch, resolution));
    this.requestedKeys = new Map(plan.map((patch, index) => [patch.id, keys[index]]));
    const sourceChanged = this.readySourceRevision !== sourceRevision;
    const groups = sourceChanged ? [plan.map((_, index) => index)] : publicationGroups(plan, this.installed);
    const preparedPatch = (key: string) => {
      const patch = this.prepared.get(key);
      if (!patch) throw new Error("Terrain publication is missing a prepared product");
      return patch;
    };
    const published = new Set<number>();
    let completed = false;
    const publishReady = () => {
      if (generation !== this.generation || this.disposed || completed) return;
      for (let groupIndex = 0; groupIndex < groups.length; groupIndex++) {
        if (published.has(groupIndex)) continue;
        const group = groups[groupIndex];
        if (group.some((index) => !this.prepared.has(keys[index]))) continue;
        const products = group.map((index) => preparedPatch(keys[index]));
        const before = this.collisionSignature();
        this.installed = this.installed.filter((old) => !products.some((patch) => overlaps(old, patch)));
        this.installed.push(...products);
        this.installed = this.installed.filter((old) => plan.some((patch) => overlaps(old, patch)));
        this.publishedRevision++;
        if (before !== this.collisionSignature()) this.collisionGeneration++;
        published.add(groupIndex);
      }
      if (published.size === groups.length) {
        const before = this.collisionSignature();
        this.installed = plan.map((_, index) => preparedPatch(keys[index]));
        if (before !== this.collisionSignature()) this.collisionGeneration++;
        this.readyTerrain = terrain;
        this.readySourceRevision = sourceRevision;
        this.prepared.clear();
        completed = true;
      }
    };
    this.scheduler.cancelExcept(new Set(keys));
    const jobs = plan.map(async (patch, index) => {
      const key = keys[index];
      const cached = this.cache.get(key);
      const resident = this.installed.find((candidate) => candidate.key === key);
      let mesh = cached?.mesh ?? resident?.mesh;
      if (cached) {
        this.cache.delete(key);
        this.cache.set(key, cached);
      }
      if (!mesh)
        mesh = await this.scheduler.request(
          key,
          patch.collision ? 1000 + patch.level : patch.level + 1,
          async (signal) => {
            signal.throwIfAborted();
            const reservation = Symbol(key),
              estimated = (resolution + 1) ** 2 * 24 + resolution ** 2 * 24;
            this.reserve(reservation, estimated);
            try {
              let result: MeshData;
              if (this.options.generate)
                result = await this.options.generate(terrain, patch, resolution, signal);
              else {
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
                signal.throwIfAborted();
                result = generateTerrainPatch(
                  terrain,
                  patch.x,
                  patch.z,
                  patch.size,
                  resolution,
                  patch.stitch,
                );
              }
              signal.throwIfAborted();
              this.reservations.delete(reservation);
              this.reserve(reservation, meshBytes(result));
              return result;
            } finally {
              this.reservations.delete(reservation);
            }
          },
          { urgent: patch.collision },
        );
      if (generation !== this.generation || this.disposed) return;
      this.prepared.set(key, { ...patch, key, mesh, revision: generation, sourceRevision });
      this.cacheMesh(key, mesh);
      publishReady();
    });
    const recordFailure = (error: unknown) => {
      if (generation === this.generation && !this.disposed) {
        this.failures.push(String(error));
        this.desiredSignature = "";
      }
    };
    this.pending = Promise.all(jobs).then(() => {
      publishReady();
    }, recordFailure);
    const criticalIndices = new Set(
      groups.filter((group) => group.some((index) => plan[index].collision)).flat(),
    );
    this.criticalPending = Promise.all(jobs.filter((_, index) => criticalIndices.has(index))).then(
      () => {
        publishReady();
      },
      () => {},
    );
  }
  private collisionSignature() {
    return this.installed
      .filter((patch) => patch.collision)
      .map((patch) => patch.key)
      .sort()
      .join("|");
  }
  private liveBytes() {
    const meshes = new Set<MeshData>();
    for (const patch of this.installed) meshes.add(patch.mesh);
    for (const patch of this.prepared.values()) meshes.add(patch.mesh);
    for (const entry of this.cache.values()) meshes.add(entry.mesh);
    return (
      [...meshes].reduce((sum, mesh) => sum + meshBytes(mesh), 0) +
      [...this.reservations.values()].reduce((sum, bytes) => sum + bytes, 0)
    );
  }
  private reserve(id: symbol, bytes: number) {
    const budget = this.options.maxLiveBytes ?? 64 * 1024 * 1024;
    while (this.cache.size && this.liveBytes() + bytes > budget) {
      const first = this.cache.entries().next().value;
      if (!first) break;
      this.cacheBytes -= first[1].bytes;
      this.cache.delete(first[0]);
    }
    if (this.liveBytes() + bytes > budget)
      throw new Error("Terrain live-memory budget exceeded; reduce residency or terrain resolution");
    this.reservations.set(id, bytes);
  }
  private patchKey(terrain: TerrainDefinition, patch: PatchAddress, resolution: number) {
    const margin = (patch.size / resolution) * 2;
    const interventions = terrain.interventions
      .filter(
        (edit) =>
          edit.kind !== "clearing" &&
          edit.center[0] + edit.radius >= patch.x - margin &&
          edit.center[0] - edit.radius <= patch.x + patch.size + margin &&
          edit.center[1] + edit.radius >= patch.z - margin &&
          edit.center[1] - edit.radius <= patch.z + patch.size + margin,
      )
      .map(({ id: _id, ...geometry }) => geometry);
    return contentKey({
      compiler: "terrain-2",
      terrain: {
        seed: terrain.seed,
        amplitude: terrain.amplitude,
        frequency: terrain.frequency,
        octaves: terrain.octaves,
        baseHeight: terrain.baseHeight,
        interventions,
      },
      patch: { x: patch.x, z: patch.z, size: patch.size, stitch: patch.stitch },
      resolution,
    });
  }
  private cacheMesh(key: string, mesh: MeshData) {
    if (this.cache.has(key)) return;
    const bytes = mesh.positions.byteLength + mesh.normals.byteLength + mesh.indices.byteLength;
    const budget = this.options.maxCacheBytes ?? 32 * 1024 * 1024;
    if (bytes > budget) return;
    this.cache.set(key, { mesh, bytes });
    this.cacheBytes += bytes;
    while (this.cacheBytes > budget) {
      const first = this.cache.entries().next().value;
      if (!first) break;
      this.cacheBytes -= first[1].bytes;
      this.cache.delete(first[0]);
    }
  }
  async prepare(
    options: { timeoutMs?: number; scope?: "collision" | "visual" } = {},
  ): Promise<{ ready: boolean; missing: string[] }> {
    this.update();
    const generation = this.generation;
    const critical = options.scope === "collision";
    let timer: ReturnType<typeof setTimeout> | undefined;
    const completed = await Promise.race([
      (critical ? this.criticalPending : this.pending).then(() => true),
      new Promise<boolean>((resolve) => {
        timer = setTimeout(() => resolve(false), options.timeoutMs ?? 10000);
      }),
    ]);
    if (timer) clearTimeout(timer);
    const missing = this.missing(critical);
    const ready =
      completed &&
      generation === this.generation &&
      !missing.length &&
      (critical || this.readySourceRevision === this.sourceRevision);
    return { ready, missing: ready ? [] : missing };
  }
  queryGround(x: number, z: number): GroundQuery {
    const patch = this.installed.find(
      (p) => p.collision && x >= p.x && x <= p.x + p.size && z >= p.z && z <= p.z + p.size,
    );
    if (!patch)
      return { status: "not-resident", reason: "Prepare a physical interest region before moving here" };
    const row = Math.round(Math.sqrt(patch.mesh.positions.length / 3)),
      resolution = row - 1;
    const u = ((x - patch.x) / patch.size) * resolution,
      v = ((z - patch.z) / patch.size) * resolution;
    const ix = Math.min(resolution - 1, Math.max(0, Math.floor(u))),
      iz = Math.min(resolution - 1, Math.max(0, Math.floor(v)));
    const tx = u - ix,
      tz = v - iz,
      index = iz * row + ix,
      vertices = patch.mesh.positions,
      step = patch.size / resolution;
    const a = vertices[index * 3 + 1],
      b = vertices[(index + 1) * 3 + 1],
      c = vertices[(index + row) * 3 + 1],
      d = vertices[(index + row + 1) * 3 + 1];
    const first = tx + tz <= 1;
    const height = first ? a + (b - a) * tx + (c - a) * tz : d + (c - d) * (1 - tx) + (b - d) * (1 - tz);
    return {
      status: "ready",
      height,
      normal: normalize([-(first ? b - a : d - c) / step, 1, -(first ? c - a : d - b) / step]),
      analyticHeight: terrainHeight(this.readyTerrain, x, z),
      sampleSpacing: step,
      approximation: "resident-triangle-heightfield",
      revision: patch.revision,
    };
  }

  async teleport(
    position: Vec3,
    options: { id?: string; visualRadius?: number; collisionRadius?: number; timeoutMs?: number } = {},
  ) {
    this.setInterest({
      id: options.id ?? "player",
      position,
      visualRadius: options.visualRadius ?? 256,
      collisionRadius: options.collisionRadius ?? 32,
      priority: 2,
    });
    const result = await this.prepare(options);
    return { ...result, position: result.ready ? position : null };
  }
  replaceTerrain(terrain: TerrainDefinition) {
    this.persistence.updateCompatibility(generatorCompatibility(this.world, terrain));
    if (contentKey(this.terrain) !== contentKey(terrain)) this.sourceRevision++;
    this.terrain = structuredClone(terrain);
    this.update();
  }
  placements(region: Region, limit = 4096) {
    return generatePlacements(this.world, this.readyTerrain, region, this.persistence.overrides, limit, {
      report: (report) => {
        this.populationReport = report;
      },
    });
  }
  save(): WorldSave {
    return this.persistence.save();
  }
  loadSave(save: WorldSave) {
    this.persistence.load(save);
  }
  dispose() {
    this.disposed = true;
    this.generation++;
    this.scheduler.dispose();
    this.installed = [];
    this.cache.clear();
    this.prepared.clear();
    this.cacheBytes = 0;
  }
}

function meshBytes(mesh: MeshData) {
  return mesh.positions.byteLength + mesh.normals.byteLength + mesh.indices.byteLength;
}
function overlaps(a: PatchAddress, b: PatchAddress) {
  return (
    Math.max(a.x, b.x) < Math.min(a.x + a.size, b.x + b.size) &&
    Math.max(a.z, b.z) < Math.min(a.z + a.size, b.z + b.size)
  );
}
function adjacent(a: PatchAddress, b: PatchAddress) {
  return (
    ((a.x + a.size === b.x || b.x + b.size === a.x) &&
      Math.max(a.z, b.z) < Math.min(a.z + a.size, b.z + b.size)) ||
    ((a.z + a.size === b.z || b.z + b.size === a.z) &&
      Math.max(a.x, b.x) < Math.min(a.x + a.size, b.x + b.size))
  );
}
/** Replace overlapping coverage together. If an existing shared edge changes
 * sampling frequency, publish both sides together so residency never opens a seam. */
function publicationGroups(plan: PatchAddress[], installed: ResidentPatch[]): number[][] {
  const parents = plan.map((_, index) => index);
  const root = (index: number): number => {
    if (parents[index] !== index) parents[index] = root(parents[index]);
    return parents[index];
  };
  const join = (a: number, b: number) => {
    parents[root(a)] = root(b);
  };
  const old = plan.map((patch) => installed.filter((resident) => overlaps(patch, resident)));
  for (let a = 0; a < plan.length; a++)
    for (let b = a + 1; b < plan.length; b++) {
      if (old[a].some((patch) => old[b].includes(patch))) join(a, b);
      else if (adjacent(plan[a], plan[b])) {
        const desiredSpacing = Math.max(plan[a].size, plan[b].size);
        const changesEdge = (resident: PatchAddress, neighbor: PatchAddress) => {
          if (!adjacent(resident, neighbor)) return false;
          const stitched =
            resident.x + resident.size === neighbor.x
              ? resident.stitch.east
              : neighbor.x + neighbor.size === resident.x
                ? resident.stitch.west
                : resident.z + resident.size === neighbor.z
                  ? resident.stitch.south
                  : resident.stitch.north;
          return resident.size * (stitched ? 2 : 1) !== desiredSpacing;
        };
        if (
          old[a].some((resident) => changesEdge(resident, plan[b])) ||
          old[b].some((resident) => changesEdge(resident, plan[a]))
        )
          join(a, b);
      }
    }
  const groups = new Map<number, number[]>();
  for (let index = 0; index < plan.length; index++) {
    const id = root(index),
      group = groups.get(id) ?? [];
    group.push(index);
    groups.set(id, group);
  }
  return [...groups.values()];
}
