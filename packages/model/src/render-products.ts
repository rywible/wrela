import type { MeshData } from "./contracts";
import type { Vec3 } from "./math";

/** Units never compose implicitly. Measured RMS is not a maximum-error certificate. */
export type RenderErrorMetric =
  | "depth"
  | "silhouette"
  | "normal-angle"
  | "linear-radiance"
  | "transmittance"
  | "temporal";
export type ErrorEvidence =
  | { kind: "numeric-bound"; metric: RenderErrorMetric; maximum: number; domain: string }
  | {
      kind: "real-bound";
      metric: RenderErrorMetric;
      maximum: number;
      domain: string;
      numericError: "unknown";
    }
  | {
      kind: "measured";
      metric: RenderErrorMetric;
      rms: number;
      maximum: number;
      domain: string;
      reference: string;
      samples: number;
    }
  | { kind: "unknown"; reason: string };
export type RenderAssumption =
  | { kind: "rigid" }
  | { kind: "opaque" }
  | { kind: "roughness-range"; minimum: number; maximum: number }
  | { kind: "carrier-relation"; phaseBasisKey: string; relation: "coherent" | "independent" }
  | { kind: "complete-periods"; minimum: number }
  | { kind: "pose-revision"; revision: string }
  | { kind: "finite-source"; geometry: "sphere"; angularRadius: number }
  | { kind: "matrix-condition"; maximum: number };
export type RenderDependency = {
  kind: "geometry" | "material" | "binding" | "motion" | "water" | "lighting" | "atmosphere" | "product";
  key: string;
};
export type RenderProductMetadata = {
  key: string;
  sourceKey: string;
  algorithmVersion: string;
  formatVersion: 1;
  domainKey: string;
  assumptions: RenderAssumption[];
  errors: ErrorEvidence[];
  /** Owned binary payload only. Direct products reference their surface's existing mesh. */
  byteLength: number;
  fallbackKey: string | null;
  dependencies: RenderDependency[];
};
export type AnalyticQuadric = {
  center: Vec3;
  radii: Vec3;
  rotation: Vec3;
  nodeId: string;
  material?: string;
};
export type QuadricPrimitive = AnalyticQuadric;
export type CompiledRenderProduct = RenderProductMetadata &
  (
    | { kind: "direct-mesh" }
    | { kind: "analytic-quadric"; primitive: AnalyticQuadric }
    | { kind: "parametric-mesh"; mesh: MeshData }
  );
/** Cost observations are adapter-specific and cannot confer mathematical validity. */
export type RenderCostObservation = {
  productKey: string;
  adapter: string;
  browser: string;
  kernelVersion: string;
  fixture: string;
  gpuP50Ms: number;
  gpuP95Ms: number;
  preparationMs: number;
};
export type RealizationRejectionReason =
  | "unsupported"
  | "assumption"
  | "error-budget"
  | "unmeasured-cost"
  | "memory-budget"
  | "stale-dependency";
export type SelectedRealization = {
  id: string;
  key: string;
  kind: CompiledRenderProduct["kind"];
  reason: string;
  rejections: { key: string; reason: string }[];
};
