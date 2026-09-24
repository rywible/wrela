import {
  type IndirectGeometry,
  type IndirectLighting,
  type IndirectSurfaceCacheOptions,
  indirectGeometrySteps,
  indirectProbePosition,
  indirectProbeSteps,
  indirectSurfaceCacheSteps,
  indirectSurfaceExclusion,
  indirectTriangleCacheSteps,
  indirectVisibilityCellSteps,
  indirectVisibilitySteps,
  relocateIndirectProbe,
} from "@wrela/compiler";
import {
  type Bounds,
  contentKey,
  type EvaluatedScene,
  type IndirectLightingField,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";

export type IndirectLightingOptions = {
  dimensions?: Vec3;
  /** Relight compiled sky transfer from the renderer atmosphere. Explicit lighting
   * retains the constant-source numerical/reference path by default. */
  physicalSky?: boolean;
  /** Explicit absolute-world probe volume; default encloses current static scene. */
  bounds?: Bounds;
  /** Keep finite probe density around the camera target in large worlds. The
   * snapped absolute volume reuses transport during small camera movements. */
  cameraVolume?: { radius: number; halfHeight: number; snap: number };
  lighting?: IndirectLighting;
  samples?: number;
  skySamples?: number;
  /** Compile local radiance and directional sky visibility alongside diffuse GI. */
  reflections?: boolean;
  /** Static diffuse surface bounces, default two; increases build work, not shading queries. */
  bounces?: 1 | 2 | 3;
  /** Bounded static probe placement; segment visibility remains authoritative. */
  relocate?: boolean;
  /** Experimental surface-constrained visibility trees. Disabled by default:
   * world captures did not establish a repeatable saving for their build cost. */
  surfaceVisibility?: boolean;
  /** Compiler cache is automatic for supported receivers. Overrides, including
   * false for the uncached reference, are diagnostic controls rather than quality modes. */
  surfaceCache?: IndirectSurfaceCacheOptions | false;
  maxTriangles?: number;
  /** Cooperative CPU slice target, never a hard real-time deadline. */
  sliceMs?: number;
};
const sleep = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
export function sceneIndirectLighting(scene: EvaluatedScene): IndirectLighting {
  const e = scene.environment,
    length = Math.hypot(...e.sunDirection);
  return {
    sunDirection: e.sunDirection.map((v) => v / Math.max(length, 1e-12)) as Vec3,
    sunRadiance: e.sunColor.map((v) => Math.max(0, v * e.sunIntensity)) as Vec3,
    // Explicit authored constant-sky surrogate. Physical sky LUT readback is a
    // different source contract; this does not silently claim identical lighting.
    skyRadiance: e.skyColor.map((v) => Math.max(0, v * e.ambient * 0.15)) as Vec3,
  };
}
/** One active bounded field. Changes cancel the old build and immediately remove
 * stale illumination. Geometry/BVH work and ray batches cooperatively yield to
 * interaction; completed probes publish independently with monotonic revisions. */
export class IndirectLightingCache {
  private ids = new WeakMap<object, number>();
  private nextId = 1;
  private generation = 0;
  private sourceKey = "";
  private current?: IndirectLightingField;
  private pending: Promise<void> = Promise.resolve();
  private disposed = false;
  private failure?: string;
  private pendingReport?: IndirectLightingField["report"];
  builds = 0;
  get field() {
    return this.current;
  }
  get error() {
    return this.failure;
  }
  get report() {
    return this.current?.report ?? this.pendingReport;
  }
  get byteLength() {
    return this.current
      ? this.current.data.byteLength +
          (this.current.visibility?.nodes.byteLength ?? 0) +
          (this.current.visibility?.triangles.byteLength ?? 0) +
          (this.current.visibility?.cells?.byteLength ?? 0) +
          (this.current.transfer?.byteLength ?? 0) +
          (this.current.reflections?.data.byteLength ?? 0) +
          (this.current.reflections?.transfer?.byteLength ?? 0) +
          (this.current.surfaceCache?.data.byteLength ?? 0) +
          (this.current.triangleCache?.report.bytes ?? 0)
      : 0;
  }
  private id(value: object) {
    let id = this.ids.get(value);
    if (id === undefined) {
      id = this.nextId++;
      this.ids.set(value, id);
    }
    return id;
  }
  update(scene: EvaluatedScene, options: IndirectLightingOptions = {}): IndirectLightingField | undefined {
    if (this.disposed) return;
    options = { ...options, surfaceCache: options.surfaceCache ?? {} };
    const lighting = options.lighting ?? sceneIndirectLighting(scene),
      origin = scene.origin ?? [0, 0, 0];
    if (!options.bounds && options.cameraVolume) {
      const { radius, halfHeight, snap } = options.cameraVolume;
      if (
        ![radius, halfHeight, snap].every((v) => Number.isFinite(v) && v > 0) ||
        snap > Math.min(radius, halfHeight) * 2
      )
        throw Error("Invalid indirect camera volume");
      const center = scene.camera.target.map((v, axis) => Math.round((v + origin[axis]) / snap) * snap);
      const extent = [radius, halfHeight, radius];
      options = {
        ...options,
        bounds: {
          min: center.map((v, axis) => v - extent[axis]) as Vec3,
          max: center.map((v, axis) => v + extent[axis]) as Vec3,
        },
      };
    }
    if (
      ![...lighting.sunDirection, ...lighting.sunRadiance, ...lighting.skyRadiance].every(Number.isFinite) ||
      [...lighting.sunRadiance, ...lighting.skyRadiance].some((v) => v < 0) ||
      Math.abs(Math.hypot(...lighting.sunDirection) - 1) > 1e-5
    )
      throw Error("Indirect lighting requires finite nonnegative sources and a unit sun direction");
    const physicalSky = options.physicalSky ?? !options.lighting;
    const source = physicalSky ? "physical-sky-and-directional-sun" : "constant-sky-and-directional-sun";
    const dimensions = options.dimensions ?? [8, 4, 8];
    const total = dimensions.reduce((a, b) => a * b, 1);
    if (dimensions.some((n) => !Number.isInteger(n) || n < 2 || n > 32) || total > 2048)
      throw Error("Indirect probe grid exceeds bounded allocation");
    if (![1, 2, 3].includes(options.bounces ?? 2)) throw Error("Invalid indirect bounce budget");
    const absolute = scene.surfaces.map(
      (surface): RenderSurface => ({ ...surface, matrix: surface.matrix.slice() }),
    );
    const key = contentKey({
      algorithm: "local-light-transport-9",
      lighting: physicalSky ? { sunDirection: lighting.sunDirection } : lighting,
      options,
      surfaces: absolute.map((s) =>
        indirectSurfaceExclusion(s)
          ? { id: s.id, exclusion: indirectSurfaceExclusion(s) }
          : {
              id: s.id,
              positions: this.id(s.mesh.positions),
              indices: this.id(s.mesh.indices),
              cacheReceiver:
                options.surfaceCache !== false
                  ? {
                      normals: this.id(s.mesh.normals),
                      sourceIds: s.mesh.sourceIds ? this.id(s.mesh.sourceIds) : 0,
                      materialCoordinates: s.mesh.materialCoordinates
                        ? this.id(s.mesh.materialCoordinates)
                        : 0,
                      normalStrength: s.material.normalStrength,
                      appearance: s.material.appearance,
                      creature: s.material.creature,
                      layers: s.material.layers,
                      relief: s.reliefAppearance,
                      renderProduct: s.selectedRenderProduct?.kind,
                    }
                  : undefined,
              colors: s.mesh.colors ? this.id(s.mesh.colors) : 0,
              matrix: Array.from(s.matrix, (v, i) => (i >= 12 && i < 15 ? v + origin[i - 12] : v)),
              range: s.drawRange,
              color: s.material.color,
              metallic: s.material.metallic,
              family: s.material.appearance?.family,
              excluded: !!(
                s.skin ||
                s.deformation ||
                s.wind ||
                s.mesh.wind ||
                s.water ||
                "thinCoverage" in s.mesh
              ),
            },
      ),
    });
    if (key === this.sourceKey) return this.current;
    this.sourceKey = key;
    const token = ++this.generation;
    this.current = undefined;
    this.failure = undefined;
    this.pendingReport = {
      status: "building",
      triangles: 0,
      rays: 0,
      source,
      bounces: options.bounces ?? 2,
      excluded: [],
      buildMs: 0,
      maxSliceMs: 0,
    };
    this.builds++;
    this.pending = this.build(absolute, lighting, dimensions, total, key, token, options, [
      ...origin,
    ] as Vec3).catch((error) => {
      if (token === this.generation) {
        this.failure = String(error);
        this.current = undefined;
        if (this.pendingReport)
          this.pendingReport = { ...this.pendingReport, status: "refused", reason: this.failure };
      }
    });
    return this.current;
  }
  private async build(
    surfaces: RenderSurface[],
    lighting: IndirectLighting,
    dimensions: Vec3,
    total: number,
    key: string,
    token: number,
    options: IndirectLightingOptions,
    renderOrigin: Vec3,
  ) {
    const physicalSky = options.physicalSky ?? !options.lighting;
    const started = performance.now(),
      sliceMs = Math.max(0.25, Math.min(8, options.sliceMs ?? 2));
    let maxSliceMs = 0,
      sliceStart = performance.now();
    const run = async <T>(steps: Generator<void, T>): Promise<T | undefined> => {
      while (token === this.generation && !this.disposed) {
        // Preserve the same deadline across cheap completed probes; one timer
        // per probe would add several seconds of browser timer-clamp latency.
        if (performance.now() - sliceStart >= sliceMs) {
          maxSliceMs = Math.max(maxSliceMs, performance.now() - sliceStart);
          await sleep();
          sliceStart = performance.now();
          if (token !== this.generation || this.disposed) return;
        }
        let step = steps.next();
        while (!step.done && performance.now() - sliceStart < sliceMs) step = steps.next();
        maxSliceMs = Math.max(maxSliceMs, performance.now() - sliceStart);
        if (step.done) return step.value;
      }
      return;
    };
    await sleep();
    sliceStart = performance.now();
    const geometry = await run<IndirectGeometry>(
      indirectGeometrySteps(surfaces, { maxTriangles: options.maxTriangles, origin: renderOrigin }),
    );
    if (!geometry || token !== this.generation) return;
    const bounds = options.bounds ?? {
      min: geometry.bounds.min.map((v, i) => v - Math.max(0.1, (geometry.bounds.max[i] - v) * 0.03)) as Vec3,
      max: geometry.bounds.max.map((v, i) => v + Math.max(0.1, (v - geometry.bounds.min[i]) * 0.03)) as Vec3,
    };
    if (
      ![...bounds.min, ...bounds.max].every(Number.isFinite) ||
      bounds.max.some((v, i) => v <= bounds.min[i])
    )
      throw Error("Invalid indirect probe volume");
    const spacing = bounds.max.map((v, i) => (v - bounds.min[i]) / (dimensions[i] - 1)) as Vec3;
    const field: IndirectLightingField = {
      key,
      revision: 0,
      origin: [...bounds.min],
      spacing,
      dimensions: [...dimensions],
      data: new Float32Array(total * 60),
      receivers: options.surfaceVisibility
        ? surfaces
            .filter((s) => !indirectSurfaceExclusion(s))
            .map((s) => ({
              id: s.id,
              positions: s.mesh.positions,
              indices: s.mesh.indices,
              matrix: Array.from(s.matrix, (v, i) => (i >= 12 && i < 15 ? v + renderOrigin[i - 12] : v)),
              start: s.drawRange?.start ?? 0,
              count: s.drawRange?.count ?? s.mesh.indices.length,
            }))
        : undefined,
      ...(physicalSky ? { transfer: new Float32Array(total * 360) } : {}),
      ...(options.reflections !== false
        ? {
            reflections: {
              data: new Float32Array(total * 36),
              ...(physicalSky ? { transfer: new Float32Array(total * 360) } : {}),
            },
          }
        : {}),
      completedProbes: 0,
      totalProbes: total,
      report: {
        status: "building",
        triangles: geometry.triangles.length,
        rays: 0,
        source: physicalSky ? "physical-sky-and-directional-sun" : "constant-sky-and-directional-sun",
        bounces: options.bounces ?? 2,
        excluded: geometry.report.excluded,
        buildMs: 0,
        maxSliceMs,
      },
    };
    if (options.relocate !== false) {
      const placement = function* () {
        let moved = 0;
        for (let index = 0; index < total; index++) {
          const offset = relocateIndirectProbe(geometry, indirectProbePosition(field, index), spacing);
          for (let a = 0; a < 3; a++) field.data[index * 60 + [38, 39, 42][a]] = offset[a];
          if (offset.some((v) => v !== 0)) moved++;
          yield;
        }
        return moved;
      };
      field.report.relocatedProbes = await run(placement());
    }
    field.visibility = await run(indirectVisibilitySteps(geometry, field.origin));
    if (field.visibility)
      field.visibility.cells = await run(
        indirectVisibilityCellSteps(geometry, field, 48, true, options.surfaceVisibility),
      );
    if (token !== this.generation) return;
    this.current = field;
    const maxDistance = Math.max(1, Math.hypot(...bounds.max.map((v, i) => v - bounds.min[i])) * 3),
      samples = options.samples ?? 128;
    for (let index = 0; index < total; index++) {
      const data = await run(
        indirectProbeSteps(geometry, indirectProbePosition(field, index), lighting, {
          samples,
          bounces: options.bounces ?? 2,
          skySamples: options.skySamples,
          maxDistance,
          seed: index,
          transfer: field.transfer?.subarray(index * 360, (index + 1) * 360),
          reflections: field.reflections
            ? {
                data: field.reflections.data.subarray(index * 36, (index + 1) * 36),
                transfer: field.reflections.transfer?.subarray(index * 360, (index + 1) * 360),
              }
            : undefined,
        }),
      );
      if (!data || token !== this.generation) return;
      for (const lane of [38, 39, 42]) data[lane] = field.data[index * 60 + lane];
      field.data.set(data, index * 60);
      field.completedProbes++;
      field.revision++;
      // Primary probe rays; secondary visibility rays are bounded by the disclosed
      // bounce budget and skySamples, not silently presented as this count.
      field.report.rays = field.completedProbes * samples;
      field.report.buildMs = performance.now() - started;
      field.report.maxSliceMs = maxSliceMs;
    }
    if (options.surfaceCache !== false) {
      const cache = await run(
        indirectSurfaceCacheSteps(geometry, field, surfaces, options.surfaceCache, renderOrigin),
      );
      if (!cache || token !== this.generation) return;
      // An empty cache must not select a more expensive shader or relight work.
      if (cache.report.admitted > 0) field.surfaceCache = cache;
      field.revision++;
      const triangles = await run(indirectTriangleCacheSteps(geometry, field, surfaces, renderOrigin));
      if (!triangles || token !== this.generation) return;
      if (triangles.report.admitted > 0) field.triangleCache = triangles;
      field.revision++;
    }
    if (token === this.generation) {
      field.report.status = "ready";
      field.report.buildMs = performance.now() - started;
      field.report.maxSliceMs = maxSliceMs;
    }
  }
  async waitReady(): Promise<IndirectLightingField> {
    const key = this.sourceKey;
    await this.pending;
    if (key !== this.sourceKey) throw Error("Indirect build replaced while waiting");
    if (!this.current || this.current.report.status !== "ready")
      throw Error(this.failure ?? "Indirect field unavailable");
    return this.current;
  }
  invalidate() {
    this.generation++;
    this.sourceKey = "";
    this.current = undefined;
    this.failure = undefined;
    this.pendingReport = undefined;
  }
  dispose() {
    this.disposed = true;
    this.invalidate();
    this.ids = new WeakMap();
  }
}
