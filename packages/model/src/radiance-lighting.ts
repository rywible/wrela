import type { Vec3 } from "./math";

/** Automatic, bounded static light transport. This is an irregular set of
 * surface-adjacent samples, not a per-pixel world ray-tracing representation. */
export type RadianceLightingField = {
  key: string;
  revision: number;
  /** Runtime gain for a normalized static-emission transport basis. Uniform
   * positive source-intensity scaling does not change visibility or transport. */
  emissionScale?: number;
  positions: Vec3[];
  /** One-based convex, edge-closed component IDs; zero means no enclosure certificate. */
  enclosed: Uint32Array;
  /** Nine output radiance SH coefficients × 27 input vec4s per sample:
   * 9 visible sky, 9 bounced sky, 8 local lights, emitted radiance. */
  transfer: Float32Array;
  /** Direct emission SH, separated for position-correct static receiver lighting.
   * The full transfer still includes this term for moving receivers/reflections. */
  directEmission?: Float32Array;
  /** Static receiver records: four sample IDs followed by four normalized weights. */
  receivers?: Float32Array;
  /** Three floats per static receiver: local direct emitted irradiance/pi.
   * The GPU separates direct probe emission once per probe, retaining shared
   * diffuse bounce/reflections without repeating probe data per receiver. */
  receiverEmission?: Float32Array;
  /** Planar receiver transport. Each sample has one fixed-normal 27-input
   * transfer; records contain three sample IDs, weights and emitted irradiance.
   * Geometry certificates are compiled away before this GPU-facing product. */
  surfaceDiffuse?: {
    positions: Vec3[];
    transfer: Float32Array;
    receivers: Float32Array;
    /** Immutable compiler certificates retained for neighboring-region reuse.
     * The GPU consumes only positions, transfer and receiver records. */
    patches?: {
      triangle: number;
      normal: Vec3;
      resolution: number;
      offset: number;
      validCells: Uint8Array;
      directEmission: Float32Array;
    }[];
  };
  /** Nine scalar SH coefficients describing visibility of the actual sky. */
  skyVisibility: Float32Array;
  /** Absolute source positions/ranges for the local-light transport basis.
   * A moved source must never consume its old transport. Color/intensity are free. */
  lights: { position: Vec3; range?: number }[];
  report: {
    samples: number;
    reusedSamples?: number;
    reusedGeometry?: boolean;
    reusedEmissionReceivers?: number;
    surfaceSamples?: number;
    surfaceReceivers?: number;
    surfaceRays?: number;
    reusedSurfaceSamples?: number;
    restoredFromStorage?: boolean;
    restoreMs?: number;
    triangles: number;
    rays: number;
    vertices: number;
    unmappedVertices: number;
    deferredReceivers?: number;
    bytes: number;
    buildMs: number;
    activeMs?: number;
    maxSliceMs?: number;
    excluded: { id: string; reason: string }[];
  };
};
