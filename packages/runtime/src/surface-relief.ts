import {
  compileSurfaceRelief,
  createRenderProduct,
  type SurfaceReliefResult,
  type SurfaceReliefReview,
} from "@wrela/compiler";
import {
  type CompiledRenderProduct,
  type CompiledSurface,
  contentKey,
  type Diagnostic,
  type MeshData,
  type RenderSurface,
  type SurfaceRelief,
  type SurfaceReliefAppearance,
} from "@wrela/model";

import { artifactBytes } from "./artifact-memory";

export type RuntimeSurfaceReliefReview = SurfaceReliefReview & {
  source: string;
  material: string;
  surface: string;
  realization: "static-products" | "wind-heuristic" | "unchanged";
  skipped?: string;
  cacheHit: boolean;
};
type Entry = {
  key: string;
  source: MeshData;
  result: SurfaceReliefResult;
  products: CompiledRenderProduct[];
  artifact: CompiledSurface & { reliefAppearance: SurfaceReliefAppearance };
};
const buffers = (mesh: MeshData): ArrayBufferLike[] => [
  mesh.positions.buffer,
  mesh.normals.buffer,
  mesh.indices.buffer,
  ...(mesh.reliefCoordinates ? [mesh.reliefCoordinates.buffer] : []),
  ...(mesh.reliefNormals ? [mesh.reliefNormals.buffer] : []),
  ...(mesh.thinCoverage
    ? [
        mesh.thinCoverage.uv.buffer,
        ...(mesh.thinCoverage.layer ? [mesh.thinCoverage.layer.buffer] : []),
        ...mesh.thinCoverage.levels.map((level) => level.buffer),
      ]
    : []),
  ...(mesh.colors ? [mesh.colors.buffer] : []),
  ...(mesh.wind ? [mesh.wind.buffer] : []),
];
function unchangedReview(mesh: MeshData, relief: SurfaceRelief): SurfaceReliefReview {
  return {
    applied: false,
    sourceTriangles: mesh.indices.length / 3,
    triangles: mesh.indices.length / 3,
    byteLength: [...new Set(buffers(mesh))].reduce((sum, buffer) => sum + buffer.byteLength, 0),
    maxDisplacement: 0,
    coarseDistanceBound: 0,
    targetEdgeLength: relief.targetEdgeLength,
    achievedMaxEdgeLength: 0,
    budgetLimited: false,
    boundaryVertices: 0,
    curvatureLimitedVertices: 0,
    thicknessLimitedVertices: 0,
    orientationBackoffs: 0,
    sourceClosed: false,
    normalError: "unknown",
    patternEdgeLength: 0,
    geometryBandWeights: [0, 0, 0],
    residualBandWeights: [1, 1, 1],
  };
}
function geometryKey(mesh: MeshData): string {
  return contentKey({
    positions: Array.from(mesh.positions),
    normals: Array.from(mesh.normals),
    indices: Array.from(mesh.indices),
    colors: mesh.colors ? Array.from(mesh.colors) : undefined,
    wind: mesh.wind ? Array.from(mesh.wind) : undefined,
    reliefCoordinates: mesh.reliefCoordinates ? Array.from(mesh.reliefCoordinates) : undefined,
    reliefNormals: mesh.reliefNormals ? Array.from(mesh.reliefNormals) : undefined,
    thinCoverage: mesh.thinCoverage
      ? {
          ...mesh.thinCoverage,
          uv: Array.from(mesh.thinCoverage.uv),
          layer: mesh.thinCoverage.layer ? Array.from(mesh.thinCoverage.layer) : undefined,
          levels: mesh.thinCoverage.levels.map((level) => Array.from(level)),
        }
      : undefined,
    bounds: mesh.bounds,
    materialGroups: mesh.materialGroups,
  });
}
function productsFor(
  mesh: MeshData,
  result: SurfaceReliefResult,
  recipe: string,
  triangleBudget: number,
): CompiledRenderProduct[] {
  if (!result.review.applied) return [];
  const geometry = geometryKey(mesh),
    sourceKey = contentKey({ geometry, recipe, triangleBudget, algorithm: "surface-relief-3" });
  const domainKey = `surface-relief-${sourceKey}`;
  const common = {
    sourceKey,
    formatVersion: 1 as const,
    domainKey,
    assumptions: [{ kind: "rigid" as const }],
    dependencies: [
      { kind: "geometry" as const, key: geometry },
      { kind: "material" as const, key: recipe },
    ],
  };
  const near = createRenderProduct({
    ...common,
    kind: "direct-mesh",
    algorithmVersion: "surface-relief-near-3",
    fallbackKey: null,
    errors: [
      { kind: "numeric-bound", metric: "silhouette", maximum: 0, domain: domainKey },
      { kind: "numeric-bound", metric: "depth", maximum: 0, domain: domainKey },
      {
        kind: "unknown",
        reason:
          "Zero bounds compare this identical realized near mesh to itself. Fidelity to ideal authored relief remains uncertified.",
      },
    ],
  });
  const coarse = createRenderProduct({
    ...common,
    kind: "parametric-mesh",
    mesh,
    algorithmVersion: "surface-relief-coarse-3",
    fallbackKey: near.key,
    errors: [
      {
        kind: "real-bound",
        metric: "silhouette",
        maximum: result.review.coarseDistanceBound,
        domain: domainKey,
        numericError: "unknown",
      },
      {
        kind: "unknown",
        reason:
          "The silhouette metric is local Euclidean correspondence before conservative projection, not ray depth. Normal, radiance and temporal errors remain unknown.",
      },
    ],
  });
  return [near, coarse];
}

/** Immutable source mesh + recipe identity. Limits include retained original coarse
 * buffers once across entries; oversized failures are weakly memoized, never rebuilt every frame. */
export class SurfaceReliefCache {
  private identities = new WeakMap<MeshData, number>();
  private nextIdentity = 1;
  private entries = new Map<string, Entry>();
  private refused = new WeakMap<
    MeshData,
    Map<string, { review: SurfaceReliefReview; requiredBytes: number }>
  >();
  private pinned = new Set<string>();
  private pinnedBytes: number | undefined;
  // Immutable entry membership has the same exact shared-buffer accounting in
  // every frame. Keep only small signatures, never additional artifact references.
  private admissionBytes = new Map<string, number>();
  private reports = new Map<string, RuntimeSurfaceReliefReview>();
  private warnings = new Map<string, Diagnostic>();
  private retainedBytes = 0;
  compilations = 0;
  hits = 0;
  /** Expensive immutable-artifact accounting passes, exposed alongside compilation/cache counters. */
  admissionMeasurements = 0;
  constructor(
    readonly maxBytes = 32 * 1024 * 1024,
    readonly maxTriangles = 12_000,
  ) {
    if (!Number.isFinite(maxBytes) || maxBytes < 0 || !Number.isInteger(maxTriangles) || maxTriangles < 1)
      throw new Error("Invalid runtime surface relief budget");
  }
  get byteLength() {
    return this.retainedBytes;
  }
  get size() {
    return this.entries.size;
  }
  get reviews(): RuntimeSurfaceReliefReview[] {
    return [...this.reports.values()];
  }
  get diagnostics(): Diagnostic[] {
    return [...this.warnings.values()];
  }
  /** Reuse the host's artifact accounting to deduplicate coarse buffers against installed geometry. */
  get artifacts(): CompiledSurface[] {
    return [...this.entries.values()].map((entry) => entry.artifact);
  }
  beginFrame() {
    this.reports.clear();
    this.warnings.clear();
    this.pinned.clear();
    this.pinnedBytes = undefined;
  }
  private identity(mesh: MeshData): number {
    let id = this.identities.get(mesh);
    if (id === undefined) {
      id = this.nextIdentity++;
      this.identities.set(mesh, id);
    }
    return id;
  }
  private measure() {
    this.retainedBytes = artifactBytes(this.artifacts);
  }
  private canAdmit(requiredBytes: number): boolean {
    if (this.pinned.size >= 64 || requiredBytes > this.maxBytes) return false;
    if (this.pinnedBytes === undefined) {
      const signature = [...this.pinned].sort().join("\0");
      this.pinnedBytes = this.admissionBytes.get(signature);
      if (this.pinnedBytes === undefined) {
        const pinned = [...this.entries.values()].filter((entry) => this.pinned.has(entry.key));
        this.pinnedBytes = artifactBytes(pinned.map((entry) => entry.artifact));
        this.admissionMeasurements++;
        while (this.admissionBytes.size >= 64)
          this.admissionBytes.delete(this.admissionBytes.keys().next().value as string);
        this.admissionBytes.set(signature, this.pinnedBytes);
      }
    }
    return requiredBytes + this.pinnedBytes <= this.maxBytes;
  }
  private refuse(mesh: MeshData, recipe: string, review: SurfaceReliefReview, requiredBytes: number) {
    const memo =
      this.refused.get(mesh) ?? new Map<string, { review: SurfaceReliefReview; requiredBytes: number }>();
    while (memo.size >= 8) memo.delete(memo.keys().next().value as string);
    memo.set(recipe, { review, requiredBytes });
    this.refused.set(mesh, memo);
  }
  private record(
    surface: RenderSurface,
    material: string,
    review: SurfaceReliefReview,
    realization: RuntimeSurfaceReliefReview["realization"],
    cacheHit: boolean,
    skipped?: string,
  ) {
    const key = `${surface.source}/${material}/${this.identity(surface.mesh)}/${skipped ?? "applied"}`;
    if (this.reports.size < 256 || this.reports.has(key))
      this.reports.set(key, {
        ...review,
        source: surface.source,
        material,
        surface: surface.id,
        realization,
        cacheHit,
        ...(skipped ? { skipped } : {}),
      });
  }
  skip(surface: RenderSurface, material: string, relief: SurfaceRelief, reason: string): RenderSurface {
    this.record(surface, material, unchangedReview(surface.mesh, relief), "unchanged", false, reason);
    const key = `${surface.source}/${material}/${reason}`;
    if (this.warnings.size < 256)
      this.warnings.set(key, {
        severity: "warning",
        code: `surface-relief.${reason}`,
        message: `Physical surface relief retained original geometry: ${reason.replaceAll("-", " ")}.`,
        document: surface.source,
        node: material,
      });
    return surface;
  }
  apply(surface: RenderSurface, material: string): RenderSurface {
    const relief = surface.material.appearance?.relief;
    if (!relief || relief.amplitude === 0) return surface;
    if (surface.skin) return this.skip(surface, material, relief, "skin-unsupported");
    if (surface.deformation) return this.skip(surface, material, relief, "deformation-unsupported");
    if (surface.water) return this.skip(surface, material, relief, "water-unsupported");
    if (new Set(surface.mesh.materialGroups?.map((group) => group.material) ?? []).size > 1)
      return this.skip(surface, material, relief, "mixed-material-unsupported");
    if (
      surface.drawRange &&
      (surface.drawRange.start !== 0 || surface.drawRange.count !== surface.mesh.indices.length)
    )
      return this.skip(surface, material, relief, "partial-draw-range-unsupported");
    const recipe = contentKey(relief),
      key = `${this.identity(surface.mesh)}/${recipe}`;
    const refused = this.refused.get(surface.mesh)?.get(recipe);
    if (refused && !this.canAdmit(refused.requiredBytes)) {
      const unchanged = this.skip(surface, material, relief, "cache-budget");
      this.record(surface, material, refused.review, "unchanged", true, "cache-budget");
      this.hits++;
      return unchanged;
    }
    let entry = this.entries.get(key);
    const cacheHit = !!entry;
    if (entry) {
      this.entries.delete(key);
      this.entries.set(key, entry);
      this.hits++;
    } else {
      const result = compileSurfaceRelief(surface.mesh, relief, { maxTriangles: this.maxTriangles });
      this.compilations++;
      const products = productsFor(surface.mesh, result, recipe, this.maxTriangles);
      entry = {
        key,
        source: surface.mesh,
        result,
        products,
        artifact: {
          kind: "surface",
          id: key,
          key,
          material,
          mesh: result.mesh,
          renderProducts: products,
          reliefAppearance: result.appearance,
          diagnostics: result.diagnostics,
        },
      };
      const requiredBytes = artifactBytes([entry.artifact]);
      if (requiredBytes > this.maxBytes || !this.canAdmit(requiredBytes)) {
        const review = { ...result.review, applied: false, budgetLimited: true };
        this.refuse(surface.mesh, recipe, review, requiredBytes);
        this.skip(surface, material, relief, "cache-budget");
        this.record(surface, material, review, "unchanged", false, "cache-budget");
        return surface;
      }
      this.refused.get(surface.mesh)?.delete(recipe);
      // A newly realized entry can replace a former membership identity. Never
      // reuse a previous byte total across residency changes.
      this.admissionBytes.clear();
      this.entries.set(key, entry);
      this.measure();
      while (this.retainedBytes > this.maxBytes || this.entries.size > 64) {
        const victim = [...this.entries.keys()].find(
          (candidate) => candidate !== key && !this.pinned.has(candidate),
        );
        if (!victim) throw new Error("Surface relief admission exceeded its bounded cache");
        this.entries.delete(victim);
        this.measure();
      }
    }
    if (!this.pinned.has(key)) {
      this.pinned.add(key);
      this.pinnedBytes = undefined;
    }
    for (const diagnostic of entry.result.diagnostics) {
      const id = `${surface.source}/${material}/${diagnostic.code}`;
      if (this.warnings.size < 256)
        this.warnings.set(id, { ...diagnostic, document: surface.source, node: material });
    }
    if (!entry.result.review.applied) {
      this.record(surface, material, entry.result.review, "unchanged", cacheHit);
      return surface;
    }
    const wind = surface.wind !== undefined || surface.mesh.wind !== undefined;
    this.record(
      surface,
      material,
      entry.result.review,
      wind ? "wind-heuristic" : "static-products",
      cacheHit,
    );
    return {
      ...surface,
      mesh: entry.result.mesh,
      reliefAppearance: entry.result.appearance,
      drawRange: surface.drawRange ? { start: 0, count: entry.result.mesh.indices.length } : undefined,
      renderProducts: wind ? undefined : entry.products,
      selectedRenderProduct: undefined,
      opaqueVisibility: undefined,
      details: wind
        ? [
            {
              label: "surface-relief-original-coarse-heuristic",
              mesh: surface.mesh,
              maxProjectedDiameter: 96,
              maxError: null,
            },
            ...(surface.details ?? []),
          ]
        : undefined,
    };
  }
  clear() {
    this.entries.clear();
    this.retainedBytes = 0;
    this.identities = new WeakMap();
    this.refused = new WeakMap();
    this.reports.clear();
    this.warnings.clear();
    this.pinned.clear();
    this.pinnedBytes = undefined;
    this.admissionBytes.clear();
  }
}
