import type { Vec3 } from "./math";

/** Bounded static diffuse transport with optional local reflection context.
 * Coefficients encode irradiance/pi from visible sky and 1–3 surface bounces. */
export type IndirectLightingField = {
  key: string;
  revision: number;
  /** Probe positions in absolute world coordinates; rendering subtracts scene.origin. */
  origin: Vec3;
  spacing: Vec3;
  dimensions: Vec3;
  /** 60 floats/probe: 9 RGB SH vectors, 6 directional visibility moment vectors.
   * Fourth component of coefficient0 is1 only after that probe is complete.
   * Unused moment lanes 38,39,42 store the bounded XYZ placement offset. */
  data: Float32Array;
  /** Compiler BVH in probe-origin-relative coordinates. Receiver-to-probe rays
   * reject visibility through geometry instead of relying on moments alone. */
  visibility?: {
    nodes: Float32Array;
    triangles: Float32Array;
    /** Cell header (triangle list, count, general region root, static surface root), followed by
     * triangle indices and optional receiver-region trees. Negative counts retain
     * bounded BVH traversal. Region leaves keep exact tests at uncertain edges. */
    cells?: Float32Array;
  };
  /** 9 output SH coefficients × (9 incident sky coefficients + sun) × RGB/padding.
   * Geometry/material transfer is relit on GPU from the actual atmosphere. */
  transfer?: Float32Array;
  /** Low-frequency local reflection context. Nine vec4/probe: RGB is incoming
   * radiance from geometry (unconvolved SH); W is directional sky visibility SH.
   * Sharp scene reflections are outside this bounded representation. */
  reflections?: { data: Float32Array; transfer?: Float32Array };
  /** Immutable source identity for the surface-constrained visibility product.
   * Only the exact mesh, material range and absolute pose may consume it. */
  receivers?: readonly {
    id: string;
    positions: Float32Array;
    indices: Uint32Array;
    matrix: readonly number[];
    start: number;
    count: number;
  }[];
  /** Experimental planar receiver cache: geometry-only blend weights with
   * optional relit diffuse/reflection data. The weights-only control preserves
   * per-probe angular clamps; resolved reflections blend SH before clamping. */
  surfaceCache?: {
    /** vec4 header [patches,samples,weightsOffset,flagsOffset], four vec4/patch,
     * second header [sampleMetadataOffset,radianceOffset,0,0]; two vec4/sample
     * (eight weights), packed per-tile flags, optional [normal.xyz,baseProbe]
     * per sample and ten GPU-resolved vec4/sample (diffuse, nine reflected SH).
     * Chart origins are relative to field.origin, never large absolute floats. */
    data: Float32Array;
    sources: readonly {
      id: string;
      positions: Float32Array;
      normals: Float32Array;
      indices: Uint32Array;
      matrix: readonly number[];
      start: number;
      count: number;
    }[];
    report: {
      patches: number;
      samples: number;
      tiles: number;
      admitted: number;
      rejected: { boundary: number; visibility: number; interpolation: number };
      buildMs: number;
      excluded: { id: string; reason: string }[];
    };
  };
  /** Conservative static triangle visibility certificates for curved/normal-mapped
   * receivers. Indexed geometry and material coordinates are retained unchanged. */
  triangleCache?: {
    sources: readonly {
      id: string;
      positions: Float32Array;
      normals: Float32Array;
      indices: Uint32Array;
      colors?: Float32Array;
      sourceIds?: string[];
      materialCoordinates?: Float32Array;
      matrix: readonly number[];
      start: number;
      count: number;
      mesh: import("./contracts").MeshData;
    }[];
    report: {
      triangles: number;
      admitted: number;
      clear: number;
      blocked: number;
      bytes: number;
      regionBytes: number;
      avoidedBuilds: number;
      rejectedCost: number;
      coneReuse: number;
      coneBuilds: number;
      candidates: number;
      buildMs: number;
      excluded: { id: string; reason: string }[];
    };
  };
  completedProbes: number;
  totalProbes: number;
  report: {
    status: "building" | "ready" | "refused";
    triangles: number;
    rays: number;
    source: "constant-sky-and-directional-sun" | "physical-sky-and-directional-sun";
    bounces: 1 | 2 | 3;
    excluded: { id: string; reason: string }[];
    buildMs: number;
    maxSliceMs: number;
    relocatedProbes?: number;
    reason?: string;
  };
};
