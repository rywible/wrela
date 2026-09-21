import type { Joint, Motion, WaterDefinition } from "./documents";
import type { Bounds, Vec3 } from "./math";
export type Diagnostic = {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
  document?: string;
  node?: string;
};
export type Quality = "interactive" | "review" | "export";
export type MeshData = {
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  sourceIds?: string[];
  materialGroups?: { material: string; start: number; count: number }[];
  bounds: Bounds;
  colors?: Float32Array;
  /** Measured or explicitly unknown fidelity of this realization; never an inferred distance guarantee. */
  fidelity?: {
    sampleSpacing: Vec3;
    requestedMaxError?: number;
    maxError: number | null;
    unresolvedFeatures: string[];
  };
};
export type CompiledSurface = {
  kind: "surface";
  id: string;
  key: string;
  mesh: MeshData;
  details?: MeshDetail[];
  material: string;
  diagnostics: Diagnostic[];
};
/** Same authored generator with a cheaper realization. The threshold is a heuristic, not an error proof. */
export type MeshDetail = {
  label: string;
  mesh: MeshData;
  maxProjectedDiameter: number;
  maxError: null;
};
export type CompiledCharacter = {
  kind: "character";
  id: string;
  key: string;
  mesh: MeshData;
  material: string;
  joints: Joint[];
  motions: Motion[];
  jointIndices: Uint16Array;
  weights: Float32Array;
  diagnostics: Diagnostic[];
};
export type CompiledVegetation = {
  kind: "vegetation";
  id: string;
  key: string;
  surfaces: CompiledSurface[];
  windResponse: number;
  /** Local-space maximum wind displacement at every supported response. */
  maxDisplacement: number;
  bounds: Bounds;
  diagnostics: Diagnostic[];
};
export type SurfaceArtifact = CompiledSurface | CompiledCharacter | CompiledVegetation;
export type RenderMaterial = {
  color: Vec3;
  secondary: Vec3;
  roughness: number;
  metallic: number;
  pattern: number;
  scale: number;
  normalStrength: number;
  /** Local coordinates remain attached to the undeformed surface. World coordinates survive rebasing. */
  domain?: "local" | "world";
  layers?: {
    color: Vec3;
    roughness: number;
    metallic: number;
    coverage: number;
    slopeBias: number;
    noiseScale: number;
    normalStrength: number;
  }[];
};
export type RenderSurface = {
  instanceId?: string;
  id: string;
  source: string;
  mesh: MeshData;
  /** Index elements, not bytes. Material groups share the same immutable geometry. */
  drawRange?: { start: number; count: number };
  details?: (MeshDetail & { drawRange?: { start: number; count: number } })[];
  matrix: Float32Array;
  material: RenderMaterial;
  wind?: number;
  skin?: { jointIndices: Uint16Array; weights: Float32Array; matrices: Float32Array };
  water?: WaterDefinition;
  waterApproximation?: { spacing: number; maxHeightError: number };
  selected?: boolean;
};
export type Camera = { position: Vec3; target: Vec3; fov: number };
export type RenderEnvironment = {
  windPhase?: number;
  pointLights?: { position: Vec3; color: Vec3; intensity: number }[];
  sunDirection: Vec3;
  sunColor: Vec3;
  sunIntensity: number;
  ambient: number;
  skyColor: Vec3;
  horizonColor: Vec3;
  groundColor: Vec3;
  fogDensity: number;
  wind: Vec3;
  exposure: number;
};
export const VIEW_MODES = [
  "beauty",
  "normals",
  "depth",
  "lod",
  "binding",
  "silhouette",
  "identity",
  "albedo",
  "roughness",
  "metallic",
] as const;
/** Stable, exact 8-bit identity colors. Black remains reserved for background. */
export function identityColor(id: string): Vec3 {
  let hash = 2166136261;
  for (let i = 0; i < id.length; i++) hash = Math.imul(hash ^ id.charCodeAt(i), 16777619);
  return [((hash >>> 16) & 255) / 255, ((hash >>> 8) & 255) / 255, Math.max(1, hash & 255) / 255];
}
export type EvaluatedScene = {
  surfaces: RenderSurface[];
  camera: Camera;
  environment: RenderEnvironment;
  time: number;
  mode: (typeof VIEW_MODES)[number];
  grid: boolean;
  /** Absolute position subtracted from render-space coordinates. */
  origin?: Vec3;
  /** Optional world-space directional coverage; subject views otherwise fit a stable camera-distance band. */
  shadowRadius?: number;
};
export type RenderCompleteness = {
  frame: number;
  complete: boolean;
  rendered: string[];
  culled: string[];
  uploading: string[];
  rejected: { id: string; reason: string }[];
  details?: { id: string; label: string; selection: "projected-size"; maxError: null }[];
};
export type GpuFrameTiming = {
  frame: number;
  shadowMs: number;
  sceneMs: number;
  displayMs: number;
  gpuMs: number;
};
export type FrameMeasurements = {
  cpuMs: number;
  gpuMs: number | null;
  triangles: number;
  drawCalls: number;
  gpuBytes: number;
  frame: number;
  adapter: string;
  qualityProfile?: "low" | "balanced" | "high";
  uploadedBytes?: number;
  visibleSurfaces?: number;
  culledSurfaces?: number;
  detailSurfaces?: number;
  /** Estimated owned allocations; excludes browser swapchain and driver-private/deferred-release memory. */
  ownedTextureBytes?: number;
  bufferBytes?: number;
  outputResolution?: [number, number];
  renderResolution?: [number, number];
};
export type CaptureMetadata = {
  revision: number;
  key: string;
  stage: string;
  camera: Camera;
  tick: number;
  quality: Quality;
  environment: string;
  missing: string[];
};
export function identityMatrix(): Float32Array {
  return new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
}
export function transformMatrix(position: Vec3, scale = 1): Float32Array {
  const m = identityMatrix();
  m[0] = m[5] = m[10] = scale;
  m[12] = position[0];
  m[13] = position[1];
  m[14] = position[2];
  return m;
}
