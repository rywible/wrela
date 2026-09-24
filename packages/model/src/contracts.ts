import type { CreatureDefinition, CreatureMaterial } from "./creature";
import type {
  CompiledGroom,
  CreatureAnchorResolution,
  CreatureCorrectiveProduct,
  CreatureDetail,
  CreatureSurfaceCoordinate,
} from "./creature-products";
import type { Joint, MaterialDefinition, Motion, WaterDefinition } from "./documents";
import type { Cloudscape } from "./environment-authoring";
import type { Bounds, Vec3 } from "./math";
import type { CharacterPerformance } from "./performance";
import type { CompiledRenderProduct, SelectedRealization } from "./render-products";
import type { SurfaceAppearance } from "./surface-appearance";
import type { SurfaceReliefAppearance } from "./surface-relief";
import type { ThinCoverage } from "./thin-coverage";
import type { OpaqueVisibilityFacts } from "./visibility-products";
import type { WaterRenderState } from "./water-body";
import type { WaterPhaseProduct } from "./water-phase";
export type Diagnostic = {
  severity: "error" | "warning" | "info";
  code: string;
  message: string;
  document?: string;
  node?: string;
  path?: string | (string | number)[];
};
export type Quality = "interactive" | "review" | "export";
export type MeshData = {
  /** Reusable botanical geometry in shoot coordinates; transforms map it into
   * the owning tree. Bounds enclose every occurrence, not only the template. */
  shoots?: {
    transforms: Float32Array;
    anchors: Float32Array;
    motion: Float32Array;
    skyVisibility?: Float32Array;
    sourceIds: string[];
    templateBounds: Bounds;
  };
  /** Optional authored surface frame coordinates; packed into unused skin slots on rigid wood. */
  materialCoordinates?: Float32Array;
  /** Compiler-owned static triangle visibility proof, four floats per vertex.
   * Provoking vertices carry a certificate for all triangles that share them. */
  indirectProofs?: Float32Array;
  /** Compiled local sky visibility: front bent direction times exposure, then
   * back exposure. Four floats per vertex; no diffuse bounce is stored. */
  skyVisibility?: Float32Array;
  /** Compiled radiance sample IDs: two front, two back; zero means unavailable.
   * A .5 fraction on the first ID of a pair certifies a closed convex receiver region. */
  /** First IDs >=1024 address four-sample records in the field. Otherwise each
   * second ID may encode first-sample weight as fraction 0.125 + 0.25*w. */
  radianceProbes?: Float32Array;
  /** Compiler verified that every triangle has a constant, geometric front normal. */
  radianceFlatNormals?: boolean;
  /** At least one receiver uses four-sample storage records rather than a pair. */
  radianceMixtures?: boolean;
  positions: Float32Array;
  normals: Float32Array;
  indices: Uint32Array;
  sourceIds?: string[];
  materialGroups?: { material: string; start: number; count: number }[];
  bounds: Bounds;
  colors?: Float32Array;
  /** Per-vertex branch phase/amplitude and leaf phase/amplitude; amplitudes are local metres. */
  wind?: Float32Array;
  thinCoverage?: ThinCoverage;
  /** Undisplaced authored coordinates/frame for phase-coherent relief realization changes. */
  reliefCoordinates?: Float32Array;
  reliefNormals?: Float32Array;
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
  renderProducts?: CompiledRenderProduct[];
  opaqueVisibility?: OpaqueVisibilityFacts;
  material: string;
  diagnostics: Diagnostic[];
};
/** Same authored generator with a cheaper realization. The threshold is a heuristic, not an error proof. */
export type MeshDetail = {
  vegetation?: import("./vegetation-products").VegetationCrownProduct;
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
  performance?: CharacterPerformance;
  jointIndices: Uint16Array;
  weights: Float32Array;
  /** Editable anatomical source retained for runtime controls and diagnostics. */
  creature?: CreatureDefinition;
  /** Exact editable character identity used to reject stale source-grounded picks. */
  creatureSourceKey?: string;
  creatureCoordinates?: (CreatureSurfaceCoordinate | null)[];
  creatureRegions?: (string | null)[];
  creatureCorrectives?: CreatureCorrectiveProduct[];
  creatureAnchors?: ({ id: string } & CreatureAnchorResolution)[];
  creatureMaterials?: MaterialDefinition[];
  /** Generated neutral-color fiber material -> live authored optical response. */
  creatureGroomMaterialSources?: Record<string, string>;
  creatureGroom?: CompiledGroom;
  creatureGroomDetail?: string;
  creatureBodyVertexCount?: number;
  creatureDetails?: CreatureDetail[];
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
  /** Scene-linear emitted radiance, independent of illumination. */
  emission?: { color: Vec3; intensity: number };
  appearance?: SurfaceAppearance;
  color: Vec3;
  secondary: Vec3;
  roughness: number;
  metallic: number;
  pattern: number;
  scale: number;
  normalStrength: number;
  creature?: CreatureMaterial;
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
  /** Frame-local subset of compiled shoot occurrences; bits 1/2 mean camera/sun. */
  shootSelection?: { indices: Uint32Array; masks: Uint8Array };
  /** Frame-local light influence product; absent keeps all source lights. */
  localLightMask?: number;
  /** Frame-local proof that this exact static mesh belongs to the GI source. */
  staticIndirectReceiver?: boolean;
  /** Renderer-assigned, one-based chart index; source identity must match. */
  indirectSurfaceChart?: number;
  /** Automatic transition when a compiled local sky product becomes available. */
  skyVisibilityWeight?: number;
  /** Visibility-tested local radiance samples for moving/alternate realizations. */
  radianceProbes?: [number, number, number, number];
  /** Uniform four-sample receiver weights for moving/alternate realizations. */
  radianceWeights?: [number, number, number, number];
  radianceWeight?: number;
  /** Non-occluding visual emitters/transmitting proxies opt out explicitly. */
  castsShadow?: boolean;
  /** Explicit source mobility excludes articulated rigid parts from static transport caches. */
  lightingMobility?: "dynamic";
  instanceId?: string;
  id: string;
  source: string;
  mesh: MeshData;
  /** Index elements, not bytes. Material groups share the same immutable geometry. */
  drawRange?: { start: number; count: number };
  /** Independent light-view ownership within the same uploaded mesh. */
  shadowDrawRange?: { start: number; count: number };
  details?: (MeshDetail & { drawRange?: { start: number; count: number } })[];
  matrix: Float32Array;
  material: RenderMaterial;
  reliefAppearance?: SurfaceReliefAppearance;
  /** Compiled source attribution on this exact vertex layout, for review and picking. */
  creatureInspection?: {
    regions: (string | null)[];
    coordinates: (CreatureSurfaceCoordinate | null)[];
    groomVertexStart?: number;
    materialId?: string;
    albedoColors?: Float32Array;
  };
  wind?: number;
  skin?: { jointIndices: Uint16Array; weights: Float32Array; matrices: Float32Array };
  /** Rest-space dynamic correction; mesh topology and immutable base buffers stay stable. */
  deformation?: {
    revision: string;
    vertexIndices?: Uint32Array;
    positionDeltas: Float32Array;
    normalDeltas?: Float32Array;
    maxDisplacement: number;
  };
  creatureDetail?: {
    label: string;
    projectedDiameter: number;
    viewportHeight: number;
    maxError: null;
    selection: "projected-diameter-hysteresis";
    bounds: "source-envelope-estimate";
  };
  renderProducts?: CompiledRenderProduct[];
  selectedRenderProduct?: CompiledRenderProduct;
  opaqueVisibility?: OpaqueVisibilityFacts;
  water?: WaterDefinition;
  waterState?: WaterRenderState;
  /** Index into the water source's compiled local sheet/impact effects. */
  waterEffect?: number;
  waterReflections?: Float32Array;
  waterContact?: { water: WaterDefinition; state: WaterRenderState };
  waterPhases?: WaterPhaseProduct;
  waterAppearance?: {
    mode: "auto" | "direct" | "regular" | "reference";
    shutterSeconds?: number;
    quality?: "low" | "balanced" | "high";
  };
  waterApproximation?: { spacing: number; maxHeightError: number };
  selected?: boolean;
};
export type Camera = { position: Vec3; target: Vec3; fov: number };
export type RenderEnvironment = {
  wetness?: number;
  cloudCover?: number;
  cloudscape?: Cloudscape;
  moonDirection?: Vec3;
  moonIntensity?: number;
  nightFactor?: number;
  starRotation?: number;
  starLatitude?: number;
  starNorthOffset?: number;
  atmosphere?: {
    key: string;
    data: Float32Array;
    byteLength: number;
    /** Planet center in world metres; GPU lookup tables use kilometres. */
    planetCenter: Vec3;
  };
  windPhase?: number;
  pointLights?: { position: Vec3; color: Vec3; intensity: number; range?: number; shadows?: boolean }[];
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
  "clay",
  "thickness",
  "fiber-direction",
  "regions",
  "shading-normals",
  "indirect-lighting",
  "indirect-cache",
] as const;
/** Stable, exact 8-bit identity colors. Black remains reserved for background. */
export function identityColor(id: string): Vec3 {
  let hash = 2166136261;
  for (let i = 0; i < id.length; i++) hash = Math.imul(hash ^ id.charCodeAt(i), 16777619);
  return [((hash >>> 16) & 255) / 255, ((hash >>> 8) & 255) / 255, Math.max(1, hash & 255) / 255];
}
export type EvaluatedScene = {
  radianceLighting?: import("./radiance-lighting").RadianceLightingField;
  indirectLighting?: import("./indirect-lighting").IndirectLightingField;
  materialLattice?: import("./material-lattice").MaterialLattice;
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
  transport?: { kind: "finite-sun" | "atmosphere"; selected: boolean; reason: string }[];
  frame: number;
  complete: boolean;
  rendered: string[];
  culled: string[];
  uploading: string[];
  rejected: { id: string; reason: string }[];
  realizations?: SelectedRealization[];
  appearance?: {
    id: string;
    kind: "auto" | "direct" | "regular" | "reference" | "spectrum";
    reason: string;
  }[];
  visibility?: { id: string; reason: string }[];
  details?: { id: string; label: string; selection: "projected-size"; maxError: null }[];
};
export type GpuFrameTiming = {
  /** Observed overlapping intervals, relative to the earliest GPU pass start. Not exclusive costs. */
  intervals?: { pass: string; startMs: number; endMs: number }[];
  frame: number;
  atmosphereMs?: number;
  indirectMs?: number;
  thinDepthMs?: number;
  waterMs?: number;
  temporalMs?: number;
  shadowMs: number;
  sceneMs: number;
  displayMs: number;
  gpuMs: number;
};
export type FrameMeasurements = {
  reconstruction?: "storage" | "raster" | "compute" | "none";
  gpuTimingDroppedFrames?: number;
  materialCacheBytes?: number;
  localShadowPasses?: number;
  localShadowDrawCalls?: number;
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
  /** Current water draws, including bed geometry and shared transport; excludes generic scene targets. */
  waterResources?: {
    transport: number;
    history: number;
    spectrum: number;
    body: number;
    geometry: number;
    objects: number;
    total: number;
  };
  waterReconstruction?: "spatial" | "compact-history";
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
