import type { Diagnostic, MeshData } from "./contracts";
import type { Vec3 } from "./math";

/** Correspondence is authored-chart based and survives compatible tessellation changes. */
export type CreatureSurfaceCoordinate = {
  region: string;
  chart: string;
  chartRevision: number;
  coordinates: Vec3;
  /** Optional stable groom-root provenance on vertices generated from a guide. */
  id?: string;
  layer?: string;
};
export type CreatureCorrectiveProduct = {
  id: string;
  region: string;
  joint: string;
  axis: "x" | "y" | "z";
  angle: number;
  vertices: Uint32Array;
  displacements: Float32Array;
};
export type CreatureAnchorResolution = {
  status: "resolved" | "invalid" | "ambiguous";
  position?: Vec3;
  normal?: Vec3;
  residual: number | null;
  confidence: number;
  diagnostics: Diagnostic[];
};

export type GroomRoot = {
  id: string;
  layer: string;
  region: string;
  chart: string;
  chartRevision: number;
  coordinates: Vec3;
};
export type CompiledGroomGuide = {
  rootProjection?: {
    domain: "compiled-body";
    chartPosition: Vec3;
    sourceNode?: string;
    distance: number;
    interval?: [number, number];
  };
  representation?: "tufts" | "ribbons";
  ribbonThickness?: number;
  widthDirection?: Vec3;
  root: GroomRoot;
  points: Vec3[];
  normal: Vec3;
  width: number;
  taper: number;
  material: string;
  rootColor: Vec3;
  tipColor: Vec3;
  stiffness: number;
  damping: number;
};
export type CompiledGroomDetail = {
  label: string;
  /** CPU source-envelope measurements; not a rendered coverage or silhouette guarantee. */
  fidelity?: {
    scope: "rest-guide-envelope";
    sourceGuides: number;
    retainedGuides: number;
    tipBoundsError: Vec3 | null;
    coverageError: null;
    drawGroups: number;
    realizations?: ("tufts" | "ribbons")[];
    maxGuideInterpolationError?: number;
  };
  mesh: MeshData;
  vertexGuideIndices: Uint32Array;
  guideIds: string[];
  cost: { vertices: number; triangles: number; bytes: number; guides: number };
  maxError: null;
};
export type CompiledGroom = {
  key: string;
  guides: CompiledGroomGuide[];
  details: CompiledGroomDetail[];
  diagnostics: Diagnostic[];
  representation: "opaque-tufts" | "opaque-ribbons" | "mixed-opaque";
};
export type CreatureDetail = {
  label: string;
  mesh: MeshData;
  jointIndices: Uint16Array;
  weights: Float32Array;
  maxProjectedDiameter: number;
  maxError: null;
  correctives?: CreatureCorrectiveProduct[];
  groomGuideIndices?: Uint32Array;
};
