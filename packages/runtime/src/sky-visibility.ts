import {
  compileSkyVisibilitySteps,
  indirectSurfaceExclusion,
  type SkyVisibilityProduct,
  skyReceiverExclusion,
} from "@wrela/compiler";
import type { EvaluatedScene, MeshData, RenderSurface, Vec3 } from "@wrela/model";

type Source = { surface: RenderSurface; key: string; min: number[]; max: number[] };
/** One automatic local sky product. Immutable source identities and absolute
 * transforms make camera motion, lighting edits and origin rebases free. */
export class SkyVisibilityCache {
  private ids = new WeakMap<object, number>();
  private serial = 0;
  private key = "";
  private generation = 0;
  private product?: SkyVisibilityProduct;
  private pending: Promise<void> = Promise.resolve();
  private originals = new WeakMap<MeshData, MeshData>();
  private ready = new WeakMap<MeshData, number>();
  private cached = new Map<string, { key: string; mesh: MeshData }>();
  private disposed = false;
  error?: string;
  builds = 0;
  get report() {
    return this.product?.report;
  }
  get byteLength() {
    const arrays = new Set([...this.cached.values()].map((c) => c.mesh.skyVisibility));
    for (const mesh of this.product?.meshes.values() ?? []) arrays.add(mesh.skyVisibility);
    return [...arrays].reduce((sum, a) => sum + (a?.byteLength ?? 0), 0);
  }
  private id(o: object) {
    let id = this.ids.get(o);
    if (id === undefined) {
      id = ++this.serial;
      this.ids.set(o, id);
    }
    return id;
  }
  private source(s: RenderSurface): RenderSurface {
    if (!s.mesh.skyVisibility) return s;
    let mesh = this.originals.get(s.mesh);
    if (!mesh) {
      const { skyVisibility: _, ...rest } = s.mesh;
      mesh = rest;
      this.originals.set(s.mesh, mesh);
    }
    return { ...s, mesh, skyVisibilityWeight: undefined };
  }
  private sourceKey(s: RenderSurface, origin: Vec3) {
    return JSON.stringify([
      s.id,
      this.id(s.mesh),
      this.id(s.mesh.positions),
      this.id(s.mesh.normals),
      this.id(s.mesh.indices),
      s.drawRange,
      skyReceiverExclusion(s),
      Array.from(s.matrix, (v, i) => v + (i >= 12 && i < 15 ? origin[i - 12] : 0)),
    ]);
  }
  private bounds(surface: RenderSurface, key: string, origin: Vec3): Source {
    const min = [Infinity, Infinity, Infinity],
      max = [-Infinity, -Infinity, -Infinity],
      m = surface.matrix,
      b = surface.mesh.bounds;
    for (let corner = 0; corner < 8; corner++) {
      const p = [0, 1, 2].map((a) => (corner & (1 << a) ? b.max[a] : b.min[a]));
      for (let a = 0; a < 3; a++) {
        const v = m[a] * p[0] + m[4 + a] * p[1] + m[8 + a] * p[2] + m[12 + a] + origin[a];
        min[a] = Math.min(min[a], v);
        max[a] = Math.max(max[a], v);
      }
    }
    return { surface, key, min, max };
  }
  apply(scene: EvaluatedScene): void {
    scene.surfaces = scene.surfaces.map((s) => this.source(s));
    if (this.disposed) return;
    if (scene.indirectLighting) {
      if (this.key) {
        this.key = "";
        this.generation++;
        this.product = undefined;
        this.cached.clear();
      }
      return;
    }
    const origin = scene.origin ?? [0, 0, 0];
    const sourceKeys = scene.surfaces.map((s) =>
      indirectSurfaceExclusion(s)
        ? JSON.stringify([s.id, indirectSurfaceExclusion(s)])
        : this.sourceKey(s, origin),
    );
    const key = JSON.stringify(sourceKeys);
    if (key !== this.key) {
      this.key = key;
      this.product = undefined;
      this.error = undefined;
      const token = ++this.generation;
      this.builds++;
      const sources = scene.surfaces.map((s) => ({ ...s, matrix: s.matrix.slice() }));
      this.pending = (async () => {
        // No compilation on the extraction/render stack. Cancellation precedes
        // even source analysis when a host is disposed promptly.
        await new Promise<void>((r) => setTimeout(r, 0));
        if (token !== this.generation || this.disposed) return;
        const admitted = sources.flatMap((s, i) =>
          indirectSurfaceExclusion(s) ? [] : [this.bounds(s, sourceKeys[i], origin)],
        );
        const reuse = new Map<string, MeshData>(),
          dependencies = new Map<string, string>();
        if (admitted.length > 2048) throw Error("Sky visibility exceeds 2048 static sources");
        let analysisStarted = performance.now();
        for (const receiver of admitted) {
          if (performance.now() - analysisStarted >= 2) {
            await new Promise<void>((r) => setTimeout(r, 0));
            analysisStarted = performance.now();
          }
          if (token !== this.generation || this.disposed) return;
          if (skyReceiverExclusion(receiver.surface)) continue;
          // Distant roofs and cave walls still block sky. Until the compiler
          // certifies directional dependencies, every resident occluder matters.
          const near = admitted;
          const dependency = JSON.stringify([receiver.key, ...near.map((s) => s.key).sort()]);
          dependencies.set(receiver.surface.id, dependency);
          const cached = this.cached.get(receiver.surface.id);
          if (cached?.key === dependency) reuse.set(receiver.surface.id, cached.mesh);
        }
        if (token !== this.generation || this.disposed) return;
        if (reuse.size)
          this.product = {
            meshes: reuse,
            report: { vertices: 0, reusedVertices: 0, rays: 0, bytes: 0, buildMs: 0, excluded: [] },
          };
        const steps = compileSkyVisibilitySteps(sources, [...origin], reuse);
        let activeMs = 0,
          maxSliceMs = 0;
        while (token === this.generation && !this.disposed) {
          const start = performance.now();
          let step = steps.next();
          while (!step.done && performance.now() - start < 2) step = steps.next();
          const elapsed = performance.now() - start;
          activeMs += elapsed;
          maxSliceMs = Math.max(maxSliceMs, elapsed);
          if (step.done) {
            step.value.report.activeMs = activeMs;
            step.value.report.maxSliceMs = maxSliceMs;
            this.product = step.value;
            this.cached.clear();
            for (const s of sources) {
              const mesh = this.product.meshes.get(s.id);
              if (mesh) {
                this.originals.set(mesh, s.mesh);
                if (!this.ready.has(mesh)) this.ready.set(mesh, performance.now());
                const dependency = dependencies.get(s.id);
                if (dependency === undefined) throw Error("Missing sky dependency key");
                this.cached.set(s.id, { key: dependency, mesh });
              }
            }
            return;
          }
          await new Promise<void>((r) => setTimeout(r, 0));
        }
      })().catch((e) => {
        if (token === this.generation) {
          this.error = String(e);
          this.product = undefined;
        }
      });
    }
    if (this.product) {
      scene.surfaces = scene.surfaces.map((s) => {
        const mesh = this.product?.meshes.get(s.id);
        const blend = mesh
          ? Math.min(1, Math.max(0, (performance.now() - (this.ready.get(mesh) ?? performance.now())) / 180))
          : 0;
        return mesh ? { ...s, mesh, skyVisibilityWeight: blend } : s;
      });
    }
  }
  async waitReady() {
    await this.pending;
    if (this.error) throw Error(this.error);
    if (!this.product) throw Error("Sky visibility unavailable");
    const latest = Math.max(0, ...[...this.product.meshes.values()].map((m) => this.ready.get(m) ?? 0));
    const remaining = latest + 180 - performance.now();
    if (remaining > 0) await new Promise<void>((r) => setTimeout(r, remaining));
    return this.product;
  }
  dispose() {
    this.disposed = true;
    this.generation++;
    this.product = undefined;
    this.cached.clear();
  }
}
