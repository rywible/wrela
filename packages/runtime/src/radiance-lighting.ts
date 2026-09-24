import {
  compileRadianceLightingSteps,
  cookRadianceSteps,
  indirectSurfaceExclusion,
  type RadianceLightingProduct,
  radianceReceiverMixture,
  radianceSourceKey,
  restoreRadianceSteps,
  yieldCompilation,
} from "@wrela/compiler";
import type { EvaluatedScene, MeshData, RenderSurface, Vec3 } from "@wrela/model";
import { radianceNeighborEstimate, radianceRetainedBytes } from "./radiance-memory";
import { browserRadianceStore, type RadianceProductStore } from "./radiance-store";

type ReceiverBinding = {
  key: string;
  ids: [number, number, number, number];
  weights: [number, number, number, number];
};
type Preparation = { product: RadianceLightingProduct; source?: string; restored: boolean };
type Prefetch = { key: string; promise: Promise<Preparation | undefined>; started: number };

function emissionScale(surfaces: readonly RenderSurface[]): number {
  let scale = 0;
  for (const surface of surfaces)
    if (!indirectSurfaceExclusion(surface))
      scale = Math.max(scale, surface.material.emission?.intensity ?? 0);
  return scale;
}

/** Geometry work is cooperative and cancelled on scene edits. Radiometric edits
 * (sky, light color/intensity) do not compile geometry or upload new meshes. */
export class RadianceLightingCache {
  constructor(private readonly store: RadianceProductStore | null = browserRadianceStore() ?? null) {}
  storageHits = 0;
  storageWrites = 0;
  storageLoadMs = 0;
  storageWriteMs = 0;
  storageError?: string;
  private storagePending: Promise<void> = Promise.resolve();
  private emissions = new Map<string, NonNullable<RenderSurface["material"]["emission"]>>();
  /** Diagnostic barrier for persistence checks; rendering never waits for a write. */
  async waitForStorage() {
    await this.storagePending;
  }
  private async run<T>(
    steps: Generator<void, T>,
    generation: number | (() => boolean),
    slice?: (ms: number) => void,
  ): Promise<T | undefined> {
    const current = typeof generation === "number" ? () => generation === this.generation : generation;
    while (!this.disposed && current()) {
      const started = performance.now();
      let step = steps.next();
      while (!step.done && performance.now() - started < 2) step = steps.next();
      slice?.(performance.now() - started);
      if (step.done) return step.value;
      await yieldCompilation();
    }
  }
  private persist(product: RadianceLightingProduct, source: string, generation: number) {
    const store = this.store;
    if (!store) return;
    this.storagePending = this.storagePending
      .then(async () => {
        const started = performance.now();
        const cooked = await this.run(cookRadianceSteps(product, source), generation);
        if (!cooked || this.disposed || generation !== this.generation) return;
        if (await store.put(cooked)) this.storageWrites++;
        this.storageWriteMs = performance.now() - started;
      })
      .catch((error) => {
        this.storageError = String(error);
      });
  }
  private sources(scene: EvaluatedScene): RenderSurface[] {
    return scene.surfaces.map((surface) => ({
      ...surface,
      matrix: surface.matrix.slice(),
      material: {
        ...surface.material,
        color: [...surface.material.color] as Vec3,
        emission: this.emissions.get(surface.id),
        appearance: surface.material.appearance ? { ...surface.material.appearance } : undefined,
      },
    }));
  }
  private receiverPosition(s: RenderSurface, origin: Vec3): Vec3 {
    const b = s.mesh.bounds,
      m = s.matrix,
      local = b.min.map((v, a) => (v + b.max[a]) * 0.5);
    return [0, 1, 2].map(
      (a) => m[a] * local[0] + m[4 + a] * local[1] + m[8 + a] * local[2] + m[12 + a] + origin[a],
    ) as Vec3;
  }
  private *prepareReceivers(
    sources: RenderSurface[],
    origin: Vec3,
    product: RadianceLightingProduct,
  ): Generator<void, Map<string, ReceiverBinding>> {
    const receivers = new Map<string, ReceiverBinding>();
    if (!product.field.positions.length) return receivers;
    for (const surface of sources) {
      if (surface.water || product.meshes.has(surface.id)) continue;
      const position = this.receiverPosition(surface, origin);
      const mixture = radianceReceiverMixture(product.geometry, product.field.positions, position);
      receivers.set(surface.id, {
        key: position.join(","),
        ids: mixture.slice(0, 4) as ReceiverBinding["ids"],
        weights: mixture.slice(4) as ReceiverBinding["weights"],
      });
      yield;
    }
    return receivers;
  }
  private async prepare(
    sources: RenderSurface[],
    origin: Vec3,
    center: Vec3,
    lights: RadianceLightingProduct["field"]["lights"],
    reuse: RadianceLightingProduct | undefined,
    key: string,
    valid: () => boolean,
    background = false,
  ): Promise<Preparation | undefined> {
    const countBuild = () => {
      if (background) this.prefetchBuilds++;
      else this.builds++;
    };
    if (!this.store) countBuild();
    await yieldCompilation();
    if (this.disposed || !valid()) return;
    let source: string | undefined, restored: RadianceLightingProduct | undefined;
    let activeMs = 0,
      maxSliceMs = 0;
    const recordSlice = (ms: number) => {
      activeMs += ms;
      maxSliceMs = Math.max(maxSliceMs, ms);
    };
    let restoreMs = 0;
    if (this.store) {
      const started = performance.now();
      try {
        source = await radianceSourceKey(sources, origin, center, lights);
        if (this.disposed || !valid()) return;
        const cooked = await this.store.get(source);
        if (cooked)
          restored = await this.run(restoreRadianceSteps(cooked, source, sources, key), valid, recordSlice);
        restoreMs = performance.now() - started;
        this.storageLoadMs = restoreMs;
      } catch (error) {
        this.storageError = String(error);
        if (source) await this.store.remove(source).catch(() => {});
      }
    }
    if (this.disposed || !valid()) return;
    if (!restored && this.store) countBuild();
    const product =
      restored ??
      (await this.run(
        compileRadianceLightingSteps(sources, { origin, center, lights, reuse, key }),
        valid,
        recordSlice,
      ));
    if (!product || this.disposed || !valid()) return;
    const receivers = await this.run(this.prepareReceivers(sources, origin, product), valid, recordSlice);
    if (!receivers || this.disposed || !valid()) return;
    this.regionReceivers.set(product, receivers);
    Object.assign(product.field.report, {
      activeMs,
      maxSliceMs,
      restoredFromStorage: !!restored,
      restoreMs: restored ? restoreMs : undefined,
    });
    if (restored) this.storageHits++;
    return { product, source, restored: !!restored };
  }
  private retain(key: string, product: RadianceLightingProduct) {
    this.regions.delete(key);
    this.regions.set(key, product);
    while (this.regions.size > 4 || this.byteLength > 32 * 1024 * 1024) {
      const oldest = [...this.regions].find(([, value]) => value !== this.product)?.[0];
      if (oldest === undefined) break;
      this.regions.delete(oldest);
      this.prefetched.delete(oldest);
    }
  }
  private prefetch?: Prefetch;
  private prefetched = new Set<string>();
  private previousCamera?: Vec3;
  private lightKey = "";
  private preparationSerial = 0;
  prefetchBuilds = 0;
  prefetchHits = 0;
  prefetchWaitMs = 0;
  /** Diagnostic barrier; movement never waits synchronously for preparation. */
  async waitForPrefetch() {
    await this.prefetch?.promise;
  }
  private prepareAhead(
    scene: EvaluatedScene,
    camera: Vec3,
    previous: Vec3 | undefined,
    origin: Vec3,
    lights: RadianceLightingProduct["field"]["lights"],
  ) {
    const product = this.product,
      anchor = this.anchor;
    if (
      !product ||
      !anchor ||
      !previous ||
      this.building ||
      this.prefetch ||
      product.field.report.bytes + radianceNeighborEstimate(product) > 32 * 1024 * 1024
    )
      return;
    const delta = camera.map((v, a) => v - previous[a]);
    const step = Math.hypot(...delta);
    // Ignore teleports. Predict one neighboring region from actual translation,
    // never camera rotation; a stationary scene schedules no speculative work.
    if (step < 0.002 || step > 6) return;
    const axis = delta.reduce((best, v, a) => (Math.abs(v) > Math.abs(delta[best]) ? a : best), 0);
    const direction = Math.sign(delta[axis]);
    if ((camera[axis] - anchor[axis]) * direction < 2) return;
    const center = [...anchor] as Vec3;
    center[axis] += direction * 16;
    const key = JSON.stringify([this.geometryKey, center, lights]);
    if (this.regions.has(key)) return;
    const sources = this.sources(scene);
    const geometryKey = this.geometryKey,
      lightKey = this.lightKey;
    const job: Prefetch = { key, promise: Promise.resolve(undefined), started: performance.now() };
    this.prefetch = job;
    const valid = () =>
      this.prefetch === job && this.geometryKey === geometryKey && this.lightKey === lightKey;
    job.promise = this.prepare(
      sources,
      [...origin],
      center,
      lights,
      product,
      `radiance-${this.identity}-prefetch-${++this.preparationSerial}`,
      valid,
      true,
    )
      .then((prepared) => {
        if (prepared && valid()) {
          this.retain(key, prepared.product);
          if (this.regions.has(key)) this.prefetched.add(key);
          if (prepared.source && !prepared.restored)
            this.persist(prepared.product, prepared.source, this.generation);
        }
        return prepared;
      })
      .catch(() => undefined)
      .finally(() => {
        if (this.prefetch === job) this.prefetch = undefined;
      });
  }
  private ids = new WeakMap<object, number>();
  private serial = 0;
  private key = "";
  private geometryKey = "";
  private anchor?: Vec3;
  private regions = new Map<string, RadianceLightingProduct>();
  regionCacheHits = 0;
  get retainedRegions() {
    return this.regions.size;
  }
  private building = false;
  /** The visible field may still be the preceding region while this is true. */
  get preparingRegion() {
    return this.building;
  }
  private readonly identity = crypto.randomUUID();
  private generation = 0;
  private product?: RadianceLightingProduct;
  private pending: Promise<void> = Promise.resolve();
  private disposed = false;
  private readyAt = 0;
  private originals = new WeakMap<
    MeshData,
    {
      mesh: MeshData;
      drawRange?: RenderSurface["drawRange"];
      shadowDrawRange?: RenderSurface["shadowDrawRange"];
    }
  >();
  private attached = new Map<string, { source: MeshData; mesh: MeshData }>();
  private regionAttachments = new WeakMap<
    RadianceLightingProduct,
    Map<string, { source: MeshData; mesh: MeshData }>
  >();
  private receivers = new Map<string, ReceiverBinding>();
  private regionReceivers = new WeakMap<RadianceLightingProduct, Map<string, ReceiverBinding>>();
  builds = 0;
  error?: string;
  get report() {
    return this.product?.field.report;
  }
  get byteLength() {
    return radianceRetainedBytes([...this.regions.values(), ...(this.product ? [this.product] : [])]);
  }
  private id(object: object) {
    let id = this.ids.get(object);
    if (id === undefined) {
      id = ++this.serial;
      this.ids.set(object, id);
    }
    return id;
  }
  /** Strip our derived streams before another compiler cache examines identity. */
  strip(scene: EvaluatedScene) {
    scene.surfaces = scene.surfaces.map((s) => {
      const original = this.originals.get(s.mesh);
      return original || s.radianceProbes
        ? {
            ...s,
            ...(original ?? {}),
            radianceProbes: undefined,
            radianceWeights: undefined,
            radianceWeight: undefined,
          }
        : s;
    });
    delete scene.radianceLighting;
  }
  apply(scene: EvaluatedScene) {
    if (this.disposed) return;
    if (scene.indirectLighting) {
      this.invalidate();
      return;
    }
    const origin = scene.origin ?? [0, 0, 0];
    const camera = scene.camera.position.map((v, a) => v + origin[a]) as Vec3;
    const previousCamera = this.previousCamera;
    this.previousCamera = camera;
    const anchor = this.anchor;
    const center =
      anchor && camera.every((v, a) => Math.abs(v - anchor[a]) <= 12)
        ? anchor
        : (camera.map((v) => Math.round(v / 16) * 16) as Vec3);
    const lights = (scene.environment.pointLights ?? [])
      .slice(0, 8)
      .map((l) => ({ position: l.position.map((v, a) => v + origin[a]) as Vec3, range: l.range }));
    const lightKey = JSON.stringify(lights);
    if (lightKey !== this.lightKey) this.prefetch = undefined;
    this.lightKey = lightKey;
    const gain = emissionScale(scene.surfaces);
    const emissions = new Map<string, NonNullable<RenderSurface["material"]["emission"]>>();
    for (const surface of scene.surfaces) {
      const source = surface.material.emission;
      if (!source || indirectSurfaceExclusion(surface)) continue;
      // An all-off edit keeps the preceding relative source basis and applies
      // zero gain. Initially dormant sources use a unit basis; a later change
      // in their relative strengths correctly rebuilds that basis.
      emissions.set(surface.id, {
        color: [...source.color] as Vec3,
        intensity: gain > 0 ? source.intensity / gain : (this.emissions.get(surface.id)?.intensity ?? 1),
      });
    }
    this.emissions = emissions;
    const sourceKeys = scene.surfaces.flatMap((s) =>
      indirectSurfaceExclusion(s)
        ? []
        : [
            [
              s.id,
              this.id(s.mesh.positions),
              this.id(s.mesh.normals),
              this.id(s.mesh.indices),
              s.mesh.colors ? this.id(s.mesh.colors) : 0,
              s.mesh.materialCoordinates ? this.id(s.mesh.materialCoordinates) : 0,
              s.mesh.sourceIds ? this.id(s.mesh.sourceIds) : 0,
              s.mesh.bounds,
              s.reliefAppearance ? this.id(s.reliefAppearance) : 0,
              s.mesh.reliefCoordinates ? this.id(s.mesh.reliefCoordinates) : 0,
              s.selectedRenderProduct?.kind,
              Array.from(s.matrix, (v, i) => v + (i >= 12 && i < 15 ? origin[i - 12] : 0)),
              s.drawRange,
              s.material.color,
              s.material.metallic,
              emissions.get(s.id),
            ],
          ],
    );
    sourceKeys.sort((a, b) => String(a[0]).localeCompare(String(b[0])));
    const geometryKey = JSON.stringify(sourceKeys);
    const key = JSON.stringify([geometryKey, center, lights]);
    const compatible = this.geometryKey === geometryKey;
    if (this.key !== key && !(compatible && this.building)) {
      this.key = key;
      this.anchor = center;
      this.geometryKey = geometryKey;
      this.building = true;
      if (!compatible) {
        this.prefetch = undefined;
        this.prefetched.clear();
        this.regions.clear();
        this.product = undefined;
        this.receivers.clear();
        this.attached = new Map();
      }
      // Keep still-valid sky/emission transport while moving lights or camera
      // anchors are rebuilt. The GPU rejects each moved point-light basis.
      this.error = undefined;
      const generation = ++this.generation;
      const sources = this.sources(scene);
      const cached = this.regions.get(key);
      const reusable = compatible ? this.product : undefined;
      if (cached) {
        this.regions.delete(key);
        this.regions.set(key, cached);
        this.product = cached;
        this.building = false;
        this.readyAt = performance.now() - 180;
        this.receivers = this.regionReceivers.get(cached) ?? new Map();
        this.attached = this.regionAttachments.get(cached) ?? new Map();
        this.regionCacheHits++;
        if (this.prefetched.delete(key)) this.prefetchHits++;
        this.pending = Promise.resolve();
      } else {
        const ahead = this.prefetch?.key === key ? this.prefetch : undefined;
        if (!ahead) this.prefetch = undefined;
        const waitStarted = performance.now();
        const valid = () => generation === this.generation;
        const prepare = () =>
          this.prepare(
            sources,
            [...origin],
            center,
            lights,
            reusable,
            `radiance-${this.identity}-${generation}`,
            valid,
          );
        const preparation = ahead
          ? ahead.promise.then((ready) => ready ?? (valid() ? prepare() : undefined))
          : prepare();
        this.pending = (async () => {
          const prepared = await preparation;
          if (!prepared || this.disposed || !valid()) return;
          const { product, source, restored } = prepared;
          if (ahead) {
            this.prefetchHits++;
            this.prefetched.delete(key);
            this.prefetchWaitMs = performance.now() - waitStarted;
          }
          this.product = product;
          this.building = false;
          this.receivers = this.regionReceivers.get(product) ?? new Map();
          this.attached = this.regionAttachments.get(product) ?? new Map();
          this.regionAttachments.set(product, this.attached);
          this.readyAt = performance.now() - (reusable || restored ? 180 : 0);
          this.retain(key, product);
          if (!ahead && source && !restored) this.persist(product, source, generation);
        })().catch((e) => {
          if (generation === this.generation) {
            this.error = String(e);
            this.building = false;
            if (!compatible) this.product = undefined;
          }
        });
      }
    }
    this.prepareAhead(scene, camera, previousCamera, [...origin], lights);
    const product = this.product;
    if (!product?.field.positions.length) return;
    product.field.emissionScale = gain;
    scene.radianceLighting = product.field;
    const weight = Math.min(1, Math.max(0, (performance.now() - this.readyAt) / 180));
    const live = new Set<string>();
    const receiverDeadline = performance.now() + 1.5;
    let queries = 0;
    product.field.report.deferredReceivers = 0;
    scene.surfaces = scene.surfaces.map((s) => {
      if (s.water) return s;
      live.add(s.id);
      const compiled = product.meshes.get(s.id);
      if (compiled?.radianceProbes && !s.skin && !s.wind && !s.mesh.wind) {
        const attached = this.attached.get(s.id);
        let mesh = attached?.source === s.mesh ? attached.mesh : undefined;
        const receiver = product.receivers.get(s.id);
        if (!mesh) {
          mesh = receiver?.sources
            ? { ...compiled }
            : {
                ...s.mesh,
                radianceProbes: compiled.radianceProbes,
                radianceFlatNormals: compiled.radianceFlatNormals,
                radianceMixtures: compiled.radianceMixtures,
              };
          if (receiver?.sources && receiver.weights && s.mesh.skyVisibility) {
            const sky = new Float32Array((mesh.positions.length / 3) * 4);
            for (let v = 0; v < sky.length / 4; v++)
              for (let a = 0; a < 4; a++)
                for (let j = 0; j < 3; j++)
                  sky[v * 4 + a] +=
                    s.mesh.skyVisibility[receiver.sources[v * 3 + j] * 4 + a] * receiver.weights[v * 3 + j];
            mesh.skyVisibility = sky;
          }
          this.attached.set(s.id, { source: s.mesh, mesh });
          this.originals.set(mesh, {
            mesh: s.mesh,
            drawRange: s.drawRange,
            shadowDrawRange: s.shadowDrawRange,
          });
        }
        const ids = product.fallbacks.get(s.id) ?? ([0, 0, 0, 0] as [number, number, number, number]);
        return {
          ...s,
          mesh,
          ...(receiver?.sources ? { drawRange: undefined, shadowDrawRange: undefined } : {}),
          radianceProbes: ids,
          radianceWeight: weight,
        };
      }
      const position = this.receiverPosition(s, origin);
      const receiverKey = position.join(",");
      let cached = this.receivers.get(s.id);
      if (cached?.key !== receiverKey) {
        // Large moving/windy populations are admitted across frames instead of
        // doing hundreds of BVH queries on a single extraction stack.
        if (queries >= 16 || (queries > 0 && performance.now() >= receiverDeadline)) {
          product.field.report.deferredReceivers = (product.field.report.deferredReceivers ?? 0) + 1;
          return s;
        }
        queries++;
        const mixture = radianceReceiverMixture(product.geometry, product.field.positions, position);
        cached = {
          key: receiverKey,
          ids: mixture.slice(0, 4) as [number, number, number, number],
          weights: mixture.slice(4) as [number, number, number, number],
        };
        this.receivers.set(s.id, cached);
      }
      return { ...s, radianceProbes: cached.ids, radianceWeights: cached.weights, radianceWeight: weight };
    });
    for (const id of this.receivers.keys()) if (!live.has(id)) this.receivers.delete(id);
  }
  async waitReady() {
    await this.pending;
    if (this.error) throw Error(this.error);
    if (!this.product) throw Error("Radiance lighting unavailable");
    const remaining = this.readyAt + 180 - performance.now();
    if (remaining > 0) await new Promise<void>((r) => setTimeout(r, remaining));
    return this.product;
  }
  private invalidate() {
    if (this.key) {
      this.key = "";
      this.geometryKey = "";
      this.anchor = undefined;
      this.previousCamera = undefined;
      this.prefetch = undefined;
      this.prefetched.clear();
      this.regions.clear();
      this.building = false;
      this.generation++;
      this.product = undefined;
      this.receivers.clear();
      this.attached = new Map();
      this.regionAttachments = new WeakMap();
      this.regionReceivers = new WeakMap();
    }
  }
  dispose() {
    this.disposed = true;
    this.invalidate();
  }
}
