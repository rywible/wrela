import type {
  Diagnostic,
  EvaluatedScene,
  FrameMeasurements,
  GpuFrameTiming,
  MeshData,
  RenderCompleteness,
  RenderSurface,
  Vec3,
} from "@wrela/model";
import { identityMatrix, inverseMatrix, normalize, normalizeWind, VIEW_MODES } from "@wrela/model";
import { defaultPhysicalAtmosphere } from "./atmosphere";
import { PhysicalAtmosphereGpu } from "./atmosphere-gpu";
import {
  batchSurfaces,
  INSTANCE_FLOATS,
  InstancePacketCache,
  packInstances,
  type SurfaceBatch,
  surfaceInstanceCount,
} from "./batching";
import { encodeCapturePixels } from "./capture-encoding";
import { CLOUD_FORMATION_GLOBAL_OFFSET, packCloudFormations } from "./cloud-formations";
import { packCreatureDeformation, validateCreatureDeformation } from "./creature-deformation";
import { DetailSelector } from "./detail";
import { packFiniteSunSpheres } from "./finite-sun";
import { encodeDisplayPass, encodeShadowPass } from "./frame-passes";
import { glassDrawDistance, isThinGlass } from "./glass";
import { consumeMappedTiming, decodeGpuTiming } from "./gpu-timing";
import {
  GpuVisibility,
  gpuVisibilityBytes,
  prepareGpuVisibilityPipelines,
  projectVisibilityBounds,
  shouldUseGpuVisibility,
} from "./gpu-visibility";
import { IndirectLightingGpu, withIndirectReceivers } from "./indirect";
import { compileLightMask, PointShadowsGpu } from "./local-lighting";
import { MaterialLatticeGpu } from "./material-lattice";
import {
  cachedSurfaceRadiance,
  materialSpecialization,
  simpleDaylightScene,
  specializeMaterialShader,
} from "./material-specialization";
import { cameraBasis, lookAt, multiply, perspective } from "./math";
import {
  GLOBAL_FLOATS,
  OBJECT_FLOATS,
  packReliefSourceVertices,
  packSurface,
  packVertices,
  RELIEF_VERTEX_FLOATS,
  VERTEX_FLOATS,
} from "./packing";
import { PipelineVariants } from "./pipeline-variants";
import { PRIMITIVE_FLOATS, packPrimitive, preparePrimitiveViews } from "./primitive";
import { QUALITY_PROFILES, type RenderQuality } from "./quality";
import { type RenderCompilerOptions, selectRenderProducts, selectWaterAppearance } from "./realization";
import { displayShader, shader } from "./shader";
import { directionalShadow, shadowRadiusForScene } from "./shadows";
import { ShootGeometryPool } from "./shoot-geometry-pool";
import { jitterProjection, TemporalResolve, temporalJitter } from "./temporal";
import {
  createThinCoverageGpu,
  type ThinCoverageGpu,
  thinCoverageTextureBytes,
  uploadThinCoverageGpu,
} from "./thin-coverage";
import { ThinCoveragePool } from "./thin-coverage-pool";
import { finiteSunScene, validateAtmosphereTable } from "./transport";
import { meshVertexBufferLayouts } from "./vertex-layout";
import { ViewportTargets } from "./viewport-targets";
import { selectVisibility, surfaceBounds } from "./visibility";
import { packWaterSpectrumSource, WaterBodyBuffers, waterBodyFloats } from "./water-body";
import { type WaterSpectrumGpu, WaterSpectrumPrograms, waterSpectrumAllocation } from "./water-spectrum-gpu";
import { WaterTemporalResolve } from "./water-temporal";
import { WaterTransportGpu } from "./water-transport";

export type { GpuFrameTiming, RenderCompleteness } from "@wrela/model";
export { cameraBasis, cameraRay, lookAt, multiply, orthographic, perspective } from "./math";
export { GLOBAL_FLOATS, OBJECT_FLOATS, packSurface, packVertices, VERTEX_FLOATS } from "./packing";
export { QUALITY_PROFILES, type RenderQuality } from "./quality";
export { RENDER_KERNEL_VERSION, type RenderCompilerOptions } from "./realization";
export type RendererOptions = {
  /** Diagnostic only: larger (4096-node) static visibility budget for image reference. */
  indirectVisibilityReference?: boolean;
  /** Diagnostic only: matched shading ablations, never a production quality mode. */
  lightingAblation?:
    | "none"
    | "indirect"
    | "sun"
    | "reflection"
    | "shadow"
    | "aerial"
    | "direct"
    | "sky"
    | "foliage-lighting";
  renderCompiler?: RenderCompilerOptions;
  quality?: RenderQuality;
  cloudNoise?: "compiled" | "reference";
  cloudReconstruction?: "temporal" | "full";
  /** Temporal is the production path; spatial/MSAA remain matched diagnostic controls. */
  antialiasing?: "temporal" | "spatial" | "msaa";
  /** Compute is retained as a matched reconstruction reference. */
  temporalResolve?: "raster" | "compute" | "storage";
  /** Experimental exact lattice cache. Opt in after measuring the target workload. */
  materialCache?: boolean;
  materialSpecialization?: boolean;
  /** Matched occupancy-pass experiment. The default keeps the depth prepass. */
  thinCoveragePrepass?: boolean;
  /** Keep conventional shadows for controlled comparisons with nonspherical scenes. */
  finiteSun?: boolean;
  /** Explicit research override; candidate crown products are rejected by default. */
  vegetationCandidates?: boolean;
  /** Matched controls for the compiled woven albedo; other materials are unchanged. */
  wovenIntegration?: "compiled" | "point" | "reference" | "reference-fine";
  maxGpuBytes?: number;
  /** Incremental immutable-geometry upload budget; live frame uniforms are reported separately in total uploads. */
  maxUploadBytesPerFrame?: number;
  pixelRatio?: number;
  /** Explicit scene resolution relative to output, in (0, 1]. Default is the quality profile. */
  resolutionScale?: number;
  onDiagnostic?: (diagnostic: Diagnostic) => void;
};
type MeshResource = {
  id: number;
  vertex: GPUBuffer;
  sourceVertex?: GPUBuffer;
  index: GPUBuffer;
  count: number;
  bytes: number;
  seen: number;
  uploaded: number;
  vertices?: Float32Array;
  sourceVertices?: Float32Array;
};
type CreatureStream = {
  previous: GPUBuffer;
  vertex: GPUBuffer;
  staging: Float32Array;
  mesh: MeshData;
  actor: string;
  indices?: Uint16Array;
  weights?: Float32Array;
  revision?: string;
  data: RenderSurface["deformation"];
  references: number;
  seen: number;
};
type ObjectResource = {
  waterSpectrum?: WaterSpectrumGpu;
  spectrumKey?: string;
  waterBuffer: GPUBuffer;
  waterBytes: number;
  waterState?: RenderSurface["waterState"];
  waterOrigin?: string;
  thinCoverage?: ThinCoverageGpu;
  thinCoverageKey?: string;
  primitive?: { buffer: GPUBuffer; group: GPUBindGroup };
  deformation?: CreatureStream;
  previousPose?: Float32Array;
  uniformData?: Float32Array;
  instanceData?: Float32Array;
  instancePacket?: InstancePacketCache;
  uniform: GPUBuffer;
  skin: GPUBuffer;
  instances: GPUBuffer;
  capacity: number;
  bytes: number;
  group: GPUBindGroup;
  seen: number;
};
type PreparedDraw = {
  water: boolean;
  mesh?: MeshResource;
  object: ObjectResource;
  camera: boolean;
  shadow: boolean;
  count: number;
  start: number;
  indices: number;
  shadowStart: number;
  shadowIndices: number;
  procedural: boolean;
  surfaces: RenderSurface[];
  gpuVisibilityIndex?: number;
};
function sameFloats(previous: Float32Array | undefined, next: Float32Array): boolean {
  if (!previous || previous.length !== next.length) return false;
  for (let i = 0; i < next.length; i++) if (next[i] !== previous[i]) return false;
  return true;
}
const OBJECT_BYTES = OBJECT_FLOATS * 4 + 64 * 64 * 2;
const HDR_FORMAT: GPUTextureFormat = "rgba16float";
type TimingSlot = {
  query: GPUQuerySet;
  resolve: GPUBuffer;
  read: GPUBuffer;
  pending?: Promise<void>;
};
export class IncompleteRenderError extends Error {
  constructor(readonly completeness: RenderCompleteness) {
    super(
      `Render is incomplete: ${completeness.rejected.map(({ id, reason }) => `${id}: ${reason}`).join("; ") || "required geometry is uploading"}`,
    );
    this.name = "IncompleteRenderError";
  }
}

/** Owns all WebGPU resources. Scene packets and meshes are treated as immutable inputs. */
export class WebGPURenderer {
  private device!: GPUDevice;
  private context!: GPUCanvasContext;
  private format!: GPUTextureFormat;
  private main = new Map<string, GPURenderPipeline>();
  private glassMain = new Map<number, GPURenderPipeline>();
  private reliefPipelines = new Map<GPURenderPipeline, GPURenderPipeline>();
  private reliefDescriptors = new Map<GPURenderPipeline, GPURenderPipelineDescriptor>();
  private thinMain = new Map<string, GPURenderPipeline>();
  private thinDepth = new Map<number, GPURenderPipeline>();
  private waterMain = new Map<string, GPURenderPipeline>();
  private primitiveMain = new Map<string, GPURenderPipeline>();
  private primitiveShadow!: GPURenderPipeline;
  private primitiveLayout!: GPUBindGroupLayout;
  private pointShadows!: PointShadowsGpu;
  private shadowPipeline!: GPURenderPipeline;
  private visibilityPipelines = new Map<number, GPURenderPipeline>();
  private gpuVisibility?: GpuVisibility;
  private gpuVisibilityFrame = -1;
  private skyPipelines = new Map<number, GPURenderPipeline>();
  private displayPipeline!: GPURenderPipeline;
  private displayLayout!: GPUBindGroupLayout;
  private displayGroup!: GPUBindGroup;
  private sunBuffer!: GPUBuffer;
  private atmosphereBuffer!: GPUBuffer;
  private atmosphereKey = "";
  private atmosphereGpu!: PhysicalAtmosphereGpu;
  private waterTransport!: WaterTransportGpu;
  private waterSpectrumPrograms!: WaterSpectrumPrograms;
  private waterBodyBuffers!: WaterBodyBuffers;
  private globalLayout!: GPUBindGroupLayout;
  private shadowSampler!: GPUSampler;
  private initialUploadedBytes = 0;
  private globalBuffer!: GPUBuffer;
  private storagePresentation = false;
  private droppedGpuTimings = 0;
  private materialLattice!: MaterialLatticeGpu;
  private specializedMain = new PipelineVariants<string, GPURenderPipeline>((error) =>
    this.report("pipeline-specialization", String(error)),
  );
  private surfaceCacheMain = new Map<string, GPURenderPipeline>();
  private triangleCacheMain = new Map<string, GPURenderPipeline>();
  private environmentMain = new Map<string, GPURenderPipeline>();
  private skyVisibilityMain = new Map<string, GPURenderPipeline>();
  private indirectLighting!: IndirectLightingGpu;
  private globalGroup!: GPUBindGroup;
  private shadowGroup!: GPUBindGroup;
  private objectLayout!: GPUBindGroupLayout;
  private thinFallback!: ThinCoverageGpu;
  private thinCoveragePool = new ThinCoveragePool();
  private thinSampler!: GPUSampler;
  private temporal?: TemporalResolve;
  private waterTemporal?: WaterTemporalResolve;
  private previousVP?: Float32Array;
  private previousScene?: {
    time: number;
    windPhase: number;
    origin: string;
    mode: string;
    fov: number;
    camera: Vec3;
    forward: Vec3;
    sun: Vec3;
    environment: string;
  };
  private previousSurfaces = new Map<
    string,
    {
      matrix: Float32Array;
      mesh: MeshData;
      appearance: string;
      realization: string | undefined;
      shootSelection?: RenderSurface["shootSelection"];
    }
  >();
  private targets = new ViewportTargets();
  private capturing = false;
  private sceneWidth = 0;
  private sceneHeight = 0;
  private samples = 1;
  private targetBytes = 0;
  private shadowTexture!: GPUTexture;
  private meshes = new Map<MeshData, MeshResource>();
  private shootGeometry = new ShootGeometryPool();
  private waterSceneGeometry = true;
  private objects = new Map<string, ObjectResource>();
  private creatureStreams = new Set<CreatureStream>();
  private width = 0;
  private height = 0;
  private frame = 0;
  private meshSerial = 0;
  private bytes = 0;
  private uploadRemaining = 0;
  private dynamicUploadedBytes = 0;
  private generation = 0;
  private disposed = false;
  private timings: TimingSlot[] = [];
  private completedTimings: GpuFrameTiming[] = [];
  private lastScene?: EvaluatedScene;
  private detailSelector = new DetailSelector();
  /** Current actual geometry, including selected detail levels; picking and captures can use this packet. */
  realizedScene?: EvaluatedScene;
  readonly diagnostics: Diagnostic[] = [];
  completeness: RenderCompleteness = {
    frame: 0,
    complete: false,
    rendered: [],
    culled: [],
    uploading: [],
    rejected: [],
  };
  status: "initializing" | "ready" | "recovering" | "error" | "disposed" = "initializing";
  measurements: FrameMeasurements = {
    cpuMs: 0,
    gpuMs: null,
    triangles: 0,
    drawCalls: 0,
    gpuBytes: 0,
    frame: 0,
    adapter: "",
  };
  private constructor(
    private canvas: HTMLCanvasElement,
    private options: RendererOptions,
  ) {}
  get qualityProfile() {
    return this.options.quality ?? "balanced";
  }
  private get quality() {
    return QUALITY_PROFILES[this.qualityProfile];
  }
  static async create(canvas: HTMLCanvasElement, options: RendererOptions = {}): Promise<WebGPURenderer> {
    if (
      options.wovenIntegration &&
      !["compiled", "point", "reference", "reference-fine"].includes(options.wovenIntegration)
    )
      throw new RangeError("Unknown woven integration mode");
    if (options.quality && !(options.quality in QUALITY_PROFILES))
      throw new RangeError("Unknown render quality profile");
    for (const value of [options.maxGpuBytes, options.maxUploadBytesPerFrame, options.pixelRatio])
      if (value !== undefined && (!Number.isFinite(value) || value < 0))
        throw new RangeError("Invalid renderer budget or resolution");
    if (options.temporalResolve && !["storage", "raster", "compute"].includes(options.temporalResolve))
      throw new RangeError("Unknown temporal reconstruction implementation");
    if (options.antialiasing && !["temporal", "spatial", "msaa"].includes(options.antialiasing))
      throw new RangeError("Unknown antialiasing policy");
    if (
      options.resolutionScale !== undefined &&
      (!Number.isFinite(options.resolutionScale) ||
        options.resolutionScale <= 0 ||
        options.resolutionScale > 1)
    )
      throw new RangeError("Render resolution scale must be in (0, 1]");
    const compiler = options.renderCompiler;
    if (compiler?.geometry && !["direct", "parametric", "analytic", "auto"].includes(compiler.geometry))
      throw new RangeError("Unknown geometry realization policy");
    if (compiler?.water && !["auto", "direct", "regular", "reference"].includes(compiler.water))
      throw new RangeError("Unknown water appearance policy");
    for (const value of [compiler?.shutterSeconds, compiler?.maxGeometryErrorPixels])
      if (value !== undefined && (!Number.isFinite(value) || value < 0))
        throw new RangeError("Invalid rendering compiler quality budget");
    if ((compiler?.shutterSeconds ?? 0) > 4) throw new RangeError("Shutter duration exceeds four seconds");
    if (
      compiler?.finiteSun &&
      (!Number.isFinite(compiler.finiteSun.angularRadius) ||
        compiler.finiteSun.angularRadius < 0.0001 ||
        compiler.finiteSun.angularRadius > 0.1)
    )
      throw new RangeError("Finite sun radius outside supported domain");
    if (compiler?.atmosphere)
      validateAtmosphereTable(
        compiler.atmosphere,
        QUALITY_PROFILES[options.quality ?? "balanced"].maxAppearanceBytes,
      );
    const renderer = new WebGPURenderer(canvas, {
      ...options,
      renderCompiler: compiler
        ? {
            ...compiler,
            atmosphere: compiler.atmosphere
              ? {
                  ...compiler.atmosphere,
                  data: compiler.atmosphere.data.slice(),
                  planetCenter: [...compiler.atmosphere.planetCenter],
                }
              : undefined,
          }
        : undefined,
    });
    try {
      await renderer.initialize();
    } catch (error) {
      renderer.dispose();
      throw error;
    }
    return renderer;
  }
  private report(code: string, message: string, severity: Diagnostic["severity"] = "error") {
    if (this.diagnostics.some((d) => d.code === code && d.message === message)) return;
    const diagnostic: Diagnostic = { code, message, severity };
    this.diagnostics.push(diagnostic);
    if (this.diagnostics.length > 100) this.diagnostics.shift();
    this.options.onDiagnostic?.(diagnostic);
  }
  private async createMeshPipeline(descriptor: GPURenderPipelineDescriptor): Promise<GPURenderPipeline> {
    const generation = this.generation;
    const ordinary = await this.device.createRenderPipelineAsync({
      ...descriptor,
      vertex: { ...descriptor.vertex, buffers: meshVertexBufferLayouts(false) },
    });
    if (!this.disposed && generation === this.generation) this.reliefDescriptors.set(ordinary, descriptor);
    return ordinary;
  }
  private meshPipeline(pipeline: GPURenderPipeline, mesh?: MeshResource): GPURenderPipeline {
    if (!mesh?.sourceVertex) return pipeline;
    let relief = this.reliefPipelines.get(pipeline);
    if (!relief) {
      const descriptor = this.reliefDescriptors.get(pipeline);
      if (!descriptor) throw new Error("Compact relief vertex pipeline is unavailable");
      relief = this.device.createRenderPipeline({
        ...descriptor,
        label: `${descriptor.label} / compact relief`,
        vertex: { ...descriptor.vertex, buffers: meshVertexBufferLayouts(true) },
      });
      this.reliefPipelines.set(pipeline, relief);
    }
    return relief;
  }
  private async initialize(): Promise<void> {
    if (!navigator.gpu)
      throw new Error("WebGPU is unavailable. Wrela Studio requires a browser with WebGPU support.");
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter)
      throw new Error(
        "No WebGPU adapter is available. Enable hardware acceleration and reopen this workspace.",
      );
    const timestamp = adapter.features.has("timestamp-query");
    const requiredFeatures: GPUFeatureName[] = timestamp ? ["timestamp-query"] : [];
    if (adapter.features.has("bgra8unorm-storage")) requiredFeatures.push("bgra8unorm-storage");
    const device = await adapter.requestDevice({ requiredFeatures });
    if (this.disposed) {
      device.destroy();
      return;
    }
    const generation = ++this.generation;
    this.device = device;
    this.measurements.adapter =
      [adapter.info.vendor, adapter.info.architecture, adapter.info.description]
        .filter(Boolean)
        .join(" · ") || "WebGPU adapter";
    device.addEventListener("uncapturederror", (event) => this.report("gpu-validation", event.error.message));
    void device.lost.then(async (info) => {
      if (this.disposed || generation !== this.generation) return;
      this.status = "recovering";
      this.report("device-lost", `Graphics device lost (${info.reason}). Rebuilding resources.`, "warning");
      this.release();
      try {
        await this.initialize();
        if (this.lastScene) this.render(this.lastScene);
      } catch (error) {
        this.status = "error";
        this.report("device-recovery-failed", String(error));
      }
    });
    this.context = this.canvas.getContext("webgpu") as GPUCanvasContext;
    if (!this.context) throw new Error("The canvas could not create a WebGPU context.");
    this.format = navigator.gpu.getPreferredCanvasFormat();
    this.storagePresentation = this.format === "rgba8unorm" || device.features.has("bgra8unorm-storage");
    this.context.configure({
      device,
      format: this.format,
      alphaMode: "opaque",
      usage:
        GPUTextureUsage.RENDER_ATTACHMENT |
        GPUTextureUsage.COPY_SRC |
        (this.storagePresentation ? GPUTextureUsage.STORAGE_BINDING : 0),
    });
    device.pushErrorScope("validation");
    const sceneModuleDescriptor = {
      label: "Wrela procedural beauty / skin / shadow / sky",
      code: (this.options.materialCache !== true
        ? shader.replace("override MATERIAL_CACHE:bool=true;", "const MATERIAL_CACHE:bool=false;")
        : shader
      )
        .replace(
          "const LIGHTING_ABLATION:u32=0u;",
          `const LIGHTING_ABLATION:u32=${Math.max(0, ["none", "indirect", "sun", "reflection", "shadow", "aerial", "direct", "sky", "foliage-lighting"].indexOf(this.options.lightingAblation ?? "none"))}u;`,
        )
        .replace(
          "const INDIRECT_VISIBILITY_STEPS:u32=128u;",
          `const INDIRECT_VISIBILITY_STEPS:u32=${this.options.indirectVisibilityReference ? 4096 : 128}u;`,
        )
        .replace(
          "const WOVEN_MODE:u32=0u;",
          `const WOVEN_MODE:u32=${Math.max(0, ["compiled", "point", "reference", "reference-fine"].indexOf(this.options.wovenIntegration ?? "compiled"))}u;`,
        ),
    };
    const module = device.createShaderModule(sceneModuleDescriptor);
    const compilation = await module.getCompilationInfo();
    for (const message of compilation.messages)
      this.report(
        "shader-compile",
        `scene.wgsl ${message.lineNum}:${message.linePos}: ${message.message}`,
        message.type === "error" ? "error" : message.type === "warning" ? "warning" : "info",
      );
    if (compilation.messages.some((m) => m.type === "error")) {
      await device.popErrorScope();
      throw new Error(
        `Renderer shader compilation failed: ${compilation.messages
          .filter((message) => message.type === "error")
          .map((message) => `scene.wgsl:${message.lineNum}:${message.linePos} ${message.message}`)
          .join("; ")}`,
      );
    }
    const visibility = GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT;
    const globalLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility, buffer: { type: "uniform", minBindingSize: GLOBAL_FLOATS * 4 } },
        {
          binding: 3,
          visibility: GPUShaderStage.FRAGMENT,
          buffer: { type: "read-only-storage", minBindingSize: 16 },
        },
        {
          binding: 4,
          visibility: GPUShaderStage.FRAGMENT,
          buffer: { type: "read-only-storage", minBindingSize: 16 },
        },
        { binding: 5, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        { binding: 6, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "unfilterable-float" } },
        { binding: 7, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
        { binding: 8, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        {
          binding: 9,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 10,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 11,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          sampler: { type: "filtering" },
        },
        {
          binding: 13,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "unfilterable-float", viewDimension: "3d" },
        },
        { binding: 14, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "uniform", minBindingSize: 16 } },
        { binding: 12, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "read-only-storage" } },
        { binding: 16, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        {
          binding: 17,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        { binding: 18, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        {
          binding: 20,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float" },
        },
        {
          binding: 21,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float" },
        },
        {
          binding: 22,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "depth", viewDimension: "2d-array" },
        },
        { binding: 23, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "uniform" } },
        {
          binding: 19,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 15,
          visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "read-only-storage", minBindingSize: 64 },
        },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "depth" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "comparison" } },
      ],
    });
    this.globalLayout = globalLayout;
    const shadowLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility,
          buffer: { type: "uniform", minBindingSize: GLOBAL_FLOATS * 4 },
        },
      ],
    });
    this.objectLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility, buffer: { type: "uniform", minBindingSize: OBJECT_FLOATS * 4 } },
        {
          binding: 1,
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage", minBindingSize: 64 },
        },
        {
          binding: 2,
          visibility,
          buffer: { type: "read-only-storage", minBindingSize: INSTANCE_FLOATS * 4 },
        },
        {
          binding: 3,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "2d-array" },
        },
        { binding: 4, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
        { binding: 5, visibility, buffer: { type: "read-only-storage", minBindingSize: 16 } },
        {
          binding: 6,
          visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "2d-array" },
        },
        { binding: 7, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
    });
    this.thinFallback = createThinCoverageGpu(device);
    this.waterSpectrumPrograms = new WaterSpectrumPrograms(device);
    this.waterBodyBuffers = new WaterBodyBuffers(device);
    this.thinSampler = device.createSampler({
      minFilter: "linear",
      magFilter: "linear",
      mipmapFilter: "linear",
      maxAnisotropy: 4,
    });
    this.primitiveLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility,
          buffer: { type: "read-only-storage", minBindingSize: PRIMITIVE_FLOATS * 4 },
        },
      ],
    });
    const layout = device.createPipelineLayout({ bindGroupLayouts: [globalLayout, this.objectLayout] });
    const primitiveLayout = device.createPipelineLayout({
      bindGroupLayouts: [globalLayout, this.objectLayout, this.primitiveLayout],
    });
    const pipelines: Promise<void>[] = [];
    const specializedModules =
      this.options.materialSpecialization === false
        ? []
        : (["plain", "foliage", "bark"] as const).flatMap((profile) =>
            [false, true].map((daylight) => ({
              profile,
              daylight,
              module: device.createShaderModule({
                label: `Proven ${profile} material / ${daylight}`,
                code: specializeMaterialShader(sceneModuleDescriptor.code, profile, daylight),
              }),
            })),
          );
    for (const samples of new Set([1, this.options.antialiasing === "msaa" ? 4 : this.quality.samples])) {
      pipelines.push(prepareGpuVisibilityPipelines(device, samples));
      for (const specialized of specializedModules)
        for (const thin of specialized.profile === "foliage" ? [false, true] : [false])
          for (const procedural of specialized.profile === "plain" ? [false] : [false, true])
            for (const surfaceCache of specialized.daylight
              ? specialized.profile === "plain"
                ? [0, 4, 5, 6]
                : [0, 4]
              : specialized.profile === "plain"
                ? [0, 1, 2, 3, 4, 5, 6]
                : [0, 3, 4]) {
              this.specializedMain.define(
                `${samples}:${specialized.profile}:${procedural}:${thin}:${specialized.daylight}:${surfaceCache}`,
                () =>
                  this.createMeshPipeline({
                    label: `${specialized.profile} / ${samples} / ${thin ? "thin" : "opaque"}`,
                    layout,
                    vertex: {
                      module: specialized.module,
                      entryPoint: "vertexMain",
                      constants: { RADIANCE_PAIR_ONLY: Number(surfaceCache === 6) },
                    },
                    fragment: {
                      module: specialized.module,
                      entryPoint: thin
                        ? procedural
                          ? this.options.thinCoveragePrepass === false
                            ? "fragmentThinDirectMain"
                            : "fragmentThinMain"
                          : this.options.thinCoveragePrepass === false
                            ? "fragmentThinDirectSolid"
                            : "fragmentThinSolid"
                        : procedural
                          ? "fragmentMain"
                          : "fragmentSolid",
                      targets: [
                        { format: HDR_FORMAT },
                        { format: samples > 1 ? "rgba16float" : "rgba32float" },
                      ],
                      constants: {
                        PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference"),
                        SURFACE_LIGHTING_CACHE: Number(surfaceCache === 1),
                        SURFACE_TRIANGLE_CACHE: Number(surfaceCache === 2),
                        LOCAL_SKY_VISIBILITY: Number(surfaceCache >= 4),
                        COMPILED_REFLECTION_CACHE: Number(surfaceCache >= 5),
                        RADIANCE_PAIR_ONLY: Number(surfaceCache === 6),
                        LOCAL_INDIRECT_LIGHTING: Number(!specialized.daylight && surfaceCache < 3),
                        ...(thin ? { THIN_COVERAGE_MSAA: Number(samples > 1) } : {}),
                      },
                    },
                    primitive: { topology: "triangle-list", cullMode: "none" },
                    multisample: { count: samples },
                    depthStencil: {
                      format: "depth32float",
                      depthWriteEnabled: !thin || this.options.thinCoveragePrepass === false,
                      depthCompare: thin && this.options.thinCoveragePrepass !== false ? "equal" : "less",
                    },
                  }),
              );
            }

      pipelines.push(
        this.createMeshPipeline({
          label: `Thin coverage depth / ${samples} samples`,
          layout,
          vertex: { module, entryPoint: "vertexMain" },
          fragment: {
            module,
            entryPoint: "thinDepthFragment",
            targets: [],
            constants: { THIN_COVERAGE_MSAA: Number(samples > 1) },
          },
          primitive: { topology: "triangle-list", cullMode: "none" },
          multisample: { count: samples },
          depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
        }).then((pipeline) => {
          this.thinDepth.set(samples, pipeline);
        }),
      );
      for (const procedural of [false, true])
        pipelines.push(
          this.createMeshPipeline({
            label: `Thin depth-tested color / ${samples} samples`,
            layout,
            vertex: { module, entryPoint: "vertexMain" },
            fragment: {
              module,
              entryPoint:
                this.options.thinCoveragePrepass === false
                  ? procedural
                    ? "fragmentThinDirectMain"
                    : "fragmentThinDirectSolid"
                  : procedural
                    ? "fragmentThinMain"
                    : "fragmentThinSolid",
              constants: { THIN_COVERAGE_MSAA: Number(samples > 1) },
              targets: [{ format: HDR_FORMAT }, { format: samples > 1 ? "rgba16float" : "rgba32float" }],
            },
            primitive: { topology: "triangle-list", cullMode: "none" },
            multisample: { count: samples },
            depthStencil: {
              format: "depth32float",
              depthWriteEnabled: this.options.thinCoveragePrepass === false,
              depthCompare: this.options.thinCoveragePrepass === false ? "less" : "equal",
            },
          }).then((pipeline) => {
            this.thinMain.set(`${samples}:${procedural}`, pipeline);
          }),
        );
      pipelines.push(
        this.createMeshPipeline({
          label: `Opaque camera visibility / ${samples} samples`,
          layout,
          vertex: { module, entryPoint: "vertexMain" },
          fragment: { module, entryPoint: "visibilityFragment", targets: [] },
          primitive: { topology: "triangle-list", cullMode: "none" },
          multisample: { count: samples },
          depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
        }).then((pipeline) => {
          this.visibilityPipelines.set(samples, pipeline);
        }),
      );
      pipelines.push(
        this.createMeshPipeline({
          label: `Thin glass / ${samples} samples`,
          layout,
          vertex: { module, entryPoint: "vertexMain" },
          fragment: {
            module,
            entryPoint: "fragmentSolid",
            constants: {
              THIN_GLASS_TRANSPARENCY: 1,
              PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference"),
            },
            targets: [
              {
                format: HDR_FORMAT,
                blend: {
                  color: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
                  alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
                },
              },
              { format: samples > 1 ? "rgba16float" : "rgba32float", writeMask: 0 },
            ],
          },
          primitive: { topology: "triangle-list", cullMode: "back" },
          multisample: { count: samples },
          depthStencil: { format: "depth32float", depthWriteEnabled: false, depthCompare: "less-equal" },
        }).then((pipeline) => {
          this.glassMain.set(samples, pipeline);
        }),
      );
      for (const procedural of [false, true])
        for (const surfaceCache of [0, 1, 2, 3, 4])
          pipelines.push(
            this.createMeshPipeline({
              label: `Scene linear ${procedural ? "procedural" : "solid"} / ${samples} samples`,
              layout,
              vertex: { module, entryPoint: "vertexMain" },
              fragment: {
                module,
                entryPoint: procedural ? "fragmentMain" : "fragmentSolid",
                targets: [{ format: HDR_FORMAT }, { format: samples > 1 ? "rgba16float" : "rgba32float" }],
                constants: {
                  PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference"),
                  SURFACE_LIGHTING_CACHE: Number(surfaceCache === 1),
                  SURFACE_TRIANGLE_CACHE: Number(surfaceCache === 2),
                  LOCAL_SKY_VISIBILITY: Number(surfaceCache === 4),
                  LOCAL_INDIRECT_LIGHTING: Number(surfaceCache < 3),
                },
              },
              primitive: { topology: "triangle-list", cullMode: "none" },
              multisample: { count: samples },
              depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
            }).then((pipeline) => {
              (surfaceCache === 4
                ? this.skyVisibilityMain
                : surfaceCache === 3
                  ? this.environmentMain
                  : surfaceCache === 2
                    ? this.triangleCacheMain
                    : surfaceCache === 1
                      ? this.surfaceCacheMain
                      : this.main
              ).set(`${samples}:${procedural}`, pipeline);
            }),
          );
      for (const procedural of [false, true])
        for (const coherent of [false, true])
          pipelines.push(
            this.createMeshPipeline({
              label: `Water ${coherent ? "coherent" : "correlated"} / ${samples} samples`,
              layout,
              vertex: { module, entryPoint: "vertexMain" },
              fragment: {
                module,
                entryPoint: procedural ? "fragmentMain" : "fragmentSolid",
                targets: [{ format: HDR_FORMAT }, { format: samples > 1 ? "rgba16float" : "rgba32float" }],
                constants: {
                  WATER_SURFACE: 1,
                  WATER_COHERENT: Number(coherent),
                  WATER_REFERENCE: Number(this.options.renderCompiler?.water === "reference"),
                  PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference"),
                },
              },
              primitive: { topology: "triangle-list", cullMode: "none" },
              multisample: { count: samples },
              depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
            }).then((pipeline) => {
              this.waterMain.set(`${samples}:${procedural}:${coherent}`, pipeline);
            }),
          );
      pipelines.push(
        this.createMeshPipeline({
          label: `Water compiled spectrum / ${samples} samples`,
          layout,
          vertex: { module, entryPoint: "vertexWaterBody", constants: { WATER_BODY: 1 } },
          fragment: {
            module,
            entryPoint: "fragmentWaterBody",
            targets: [{ format: HDR_FORMAT }, { format: samples > 1 ? "rgba16float" : "rgba32float" }],
            constants: { PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference") },
          },
          primitive: { topology: "triangle-list", cullMode: "none" },
          multisample: { count: samples },
          depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
        }).then((pipeline) => {
          this.waterMain.set(`${samples}:body`, pipeline);
        }),
      );
      for (const procedural of [false, true])
        pipelines.push(
          device
            .createRenderPipelineAsync({
              label: `Analytic primitive / ${samples} samples`,
              layout: primitiveLayout,
              vertex: { module, entryPoint: "primitiveVertex" },
              fragment: {
                module,
                entryPoint: procedural ? "primitiveFragment" : "primitiveSolid",
                targets: [{ format: HDR_FORMAT }, { format: samples > 1 ? "rgba16float" : "rgba32float" }],
                constants: { PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference") },
              },
              primitive: { topology: "triangle-list" },
              multisample: { count: samples },
              depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
            })
            .then((pipeline) => {
              this.primitiveMain.set(`${samples}:${procedural}`, pipeline);
            }),
        );
      pipelines.push(
        device
          .createRenderPipelineAsync({
            label: `Analytic sky / ${samples} samples`,
            layout: device.createPipelineLayout({ bindGroupLayouts: [globalLayout] }),
            vertex: { module, entryPoint: "skyVertex" },
            fragment: {
              module,
              entryPoint: "skyFragment",
              targets: [{ format: HDR_FORMAT }, { format: samples > 1 ? "rgba16float" : "rgba32float" }],
              constants: { PHYSICAL_CLOUD_COMPILED_NOISE: Number(this.options.cloudNoise !== "reference") },
            },
            primitive: { topology: "triangle-list" },
            multisample: { count: samples },
            depthStencil: { format: "depth32float", depthWriteEnabled: false, depthCompare: "less-equal" },
          })
          .then((pipeline) => {
            this.skyPipelines.set(samples, pipeline);
          }),
      );
    }
    pipelines.push(
      this.createMeshPipeline({
        label: "Directional shadow",
        layout: device.createPipelineLayout({ bindGroupLayouts: [shadowLayout, this.objectLayout] }),
        vertex: { module, entryPoint: "shadowMain" },
        fragment: { module, entryPoint: "shadowFragment", targets: [] },
        primitive: { topology: "triangle-list", cullMode: "none" },
        depthStencil: {
          format: "depth32float",
          depthWriteEnabled: true,
          depthCompare: "less",
          depthBias: 2,
          depthBiasSlopeScale: 2,
        },
      }).then((pipeline) => {
        this.shadowPipeline = pipeline;
      }),
    );
    pipelines.push(
      device
        .createRenderPipelineAsync({
          label: "Analytic primitive shadow",
          layout: device.createPipelineLayout({
            bindGroupLayouts: [shadowLayout, this.objectLayout, this.primitiveLayout],
          }),
          vertex: { module, entryPoint: "primitiveShadowVertex" },
          fragment: { module, entryPoint: "primitiveShadow", targets: [] },
          primitive: { topology: "triangle-list" },
          depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
        })
        .then((pipeline) => {
          this.primitiveShadow = pipeline;
        }),
    );
    this.displayLayout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "float" } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "filtering" } },
      ],
    });
    const displayModule = device.createShaderModule({
      label: "Display transform and edge resolve",
      code: displayShader,
    });
    pipelines.push(
      device
        .createRenderPipelineAsync({
          label: "Display",
          layout: device.createPipelineLayout({ bindGroupLayouts: [globalLayout, this.displayLayout] }),
          vertex: { module: displayModule, entryPoint: "vertexMain" },
          fragment: { module: displayModule, entryPoint: "fragmentMain", targets: [{ format: this.format }] },
          primitive: { topology: "triangle-list" },
        })
        .then((pipeline) => {
          this.displayPipeline = pipeline;
        }),
    );
    await Promise.all(pipelines);
    this.globalBuffer = device.createBuffer({
      label: "Frame uniforms",
      size: GLOBAL_FLOATS * 4,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.shadowTexture = device.createTexture({
      label: "Directional shadow depth",
      size: [this.quality.shadowSize, this.quality.shadowSize],
      format: "depth32float",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    const sunData = packFiniteSunSpheres([]);
    const atmosphere = this.options.renderCompiler?.atmosphere ?? defaultPhysicalAtmosphere();
    const atmosphereData = atmosphere.data;
    this.atmosphereKey = atmosphere.key;
    this.atmosphereGpu = await PhysicalAtmosphereGpu.create(device, {
      cloudNoise: this.options.cloudNoise,
      reconstruction: this.options.cloudReconstruction,
      quality: this.qualityProfile,
    });
    this.waterTransport = new WaterTransportGpu(device, 1, 1, 1);
    this.shadowSampler = device.createSampler({
      compare: "less-equal",
      minFilter: "linear",
      magFilter: "linear",
    });
    this.sunBuffer = device.createBuffer({
      label: "Finite sun sphere queries",
      size: sunData.byteLength,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    this.atmosphereBuffer = device.createBuffer({
      label: "Atmosphere optical depth",
      size: atmosphereData.byteLength,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    device.queue.writeBuffer(this.sunBuffer, 0, sunData as Float32Array<ArrayBuffer>);
    device.queue.writeBuffer(this.atmosphereBuffer, 0, atmosphereData as Float32Array<ArrayBuffer>);
    this.pointShadows = new PointShadowsGpu(
      device,
      shadowLayout,
      this.options.quality === "high" ? 256 : this.options.quality === "low" ? 64 : 128,
    );
    this.materialLattice = new MaterialLatticeGpu(device);
    this.indirectLighting = new IndirectLightingGpu(device);
    this.initialUploadedBytes = sunData.byteLength + atmosphereData.byteLength;
    this.rebuildGlobalGroup();
    this.shadowGroup = device.createBindGroup({
      layout: shadowLayout,
      entries: [{ binding: 0, resource: { buffer: this.globalBuffer } }],
    });
    this.droppedGpuTimings = 0;
    if (timestamp) {
      // Keep delayed browser readbacks from exhausting the profiler during dense scenes.
      this.timings = Array.from({ length: 16 }, () => ({
        query: device.createQuerySet({ type: "timestamp", count: 18 }),
        resolve: device.createBuffer({
          size: 144,
          usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
        }),
        read: device.createBuffer({ size: 144, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ }),
      }));
    }
    const error = await device.popErrorScope();
    if (error) throw new Error(`WebGPU pipeline validation: ${error.message}`);
    this.bytes =
      GLOBAL_FLOATS * 4 +
      this.quality.shadowSize ** 2 * 4 +
      this.timings.length * 288 +
      this.sunBuffer.size +
      this.atmosphereBuffer.size +
      this.atmosphereGpu.byteLength +
      this.materialLattice.bytes +
      this.thinFallback.bytes +
      56 +
      this.indirectLighting.bytes +
      this.pointShadows.byteLength;
    if (this.disposed) {
      this.release();
      device.destroy();
      return;
    }
    this.status = "ready";
  }
  private rebuildGlobalGroup() {
    this.globalGroup = this.device.createBindGroup({
      layout: this.globalLayout,
      entries: [
        { binding: 0, resource: { buffer: this.globalBuffer } },
        { binding: 1, resource: this.shadowTexture.createView() },
        { binding: 2, resource: this.shadowSampler },
        { binding: 3, resource: { buffer: this.sunBuffer } },
        { binding: 4, resource: { buffer: this.atmosphereBuffer } },
        { binding: 5, resource: this.waterTransport.colorView },
        { binding: 6, resource: this.waterTransport.depthView },
        { binding: 7, resource: this.waterTransport.sampler },
        { binding: 8, resource: this.atmosphereGpu.skyView },
        { binding: 9, resource: this.atmosphereGpu.aerialRadianceView },
        { binding: 10, resource: this.atmosphereGpu.aerialTransmissionView },
        { binding: 11, resource: this.atmosphereGpu.sampler },
        { binding: 13, resource: this.materialLattice.texture.createView() },
        { binding: 14, resource: { buffer: this.materialLattice.bounds } },
        { binding: 12, resource: { buffer: this.atmosphereGpu.irradianceBuffer } },
        { binding: 15, resource: { buffer: this.indirectLighting.buffer } },
        { binding: 16, resource: this.atmosphereGpu.cloudView },
        { binding: 17, resource: this.atmosphereGpu.cloudNoiseView },
        { binding: 18, resource: this.atmosphereGpu.cloudShadowView },
        { binding: 19, resource: this.atmosphereGpu.cloudLightView },
        { binding: 20, resource: this.atmosphereGpu.reflection.view },
        { binding: 21, resource: this.atmosphereGpu.reflection.lutView },
        { binding: 22, resource: this.pointShadows.view },
        { binding: 23, resource: { buffer: this.pointShadows.parameters } },
      ],
    });
  }
  private resize(diagnostic: boolean, needsWaterTransport = false, compiledWater = false) {
    const dpr = Math.max(
      0.1,
      this.options.pixelRatio ?? Math.min(window.devicePixelRatio ?? 1, this.quality.maxPixelRatio),
    );
    const max = this.device.limits.maxTextureDimension2D;
    const width = Math.max(1, Math.min(max, Math.round(this.canvas.clientWidth * dpr)));
    const height = Math.max(1, Math.min(max, Math.round(this.canvas.clientHeight * dpr)));
    const samples = diagnostic ? 1 : this.options.antialiasing === "msaa" ? 4 : this.quality.samples;
    // Opaque history is resolved before water. Water transport receives a stable,
    // unjittered snapshot, so reflections never inherit an unrelated motion vector.
    const temporal = !diagnostic && samples === 1 && this.options.antialiasing !== "spatial";
    const waterHistory = temporal && compiledWater;
    // Exact temporal identities need32bits only on the single-sample history path.
    const motionBytes = samples > 1 ? 8 : 16;
    const bytesPerPixel =
      (needsWaterTransport ? (this.options.quality === "high" ? 14 : 10) : 0) +
      8 +
      (4 + motionBytes) * samples +
      (samples > 1 ? 8 * samples : 0) +
      (temporal ? 32 : 0) +
      (waterHistory ? 10 : 0);
    const fixedBytes =
      this.quality.shadowSize ** 2 * 4 +
      GLOBAL_FLOATS * 4 +
      this.timings.length * 288 +
      this.sunBuffer.size +
      this.atmosphereBuffer.size +
      this.atmosphereGpu.byteLength +
      this.materialLattice.bytes +
      this.thinFallback.bytes +
      56 +
      this.indirectLighting.bytes +
      this.pointShadows.byteLength;
    if (
      diagnostic &&
      fixedBytes + width * height * bytesPerPixel > (this.options.maxGpuBytes ?? this.quality.maxGpuBytes)
    )
      return false;
    const targetAllowance =
      Math.max(0, (this.options.maxGpuBytes ?? this.quality.maxGpuBytes) - fixedBytes) * 0.7;
    // Reserve residency capacity before allocating large Retina render targets. Diagnostic pixels stay native.
    const scale = diagnostic
      ? 1
      : Math.min(
          this.options.resolutionScale ?? this.quality.resolutionScale,
          Math.sqrt(targetAllowance / (width * height * bytesPerPixel)),
        );
    const sceneWidth = Math.max(1, Math.floor(width * scale));
    const sceneHeight = Math.max(1, Math.floor(height * scale));
    if (
      width === this.width &&
      height === this.height &&
      sceneWidth === this.sceneWidth &&
      sceneHeight === this.sceneHeight &&
      samples === this.samples &&
      temporal === !!this.temporal &&
      waterHistory === !!this.waterTemporal &&
      this.waterTransport?.width === (needsWaterTransport ? sceneWidth : 1) &&
      this.waterTransport?.height === (needsWaterTransport ? sceneHeight : 1)
    )
      return;
    this.temporal?.destroy();
    this.temporal = undefined;
    this.waterTemporal?.destroy();
    this.waterTemporal = undefined;
    this.targets.destroy();
    this.previousVP = undefined;
    this.previousScene = undefined;
    this.previousSurfaces.clear();
    this.waterTransport?.destroy();
    this.bytes -= this.targetBytes;
    this.width = this.canvas.width = width;
    this.height = this.canvas.height = height;
    this.sceneWidth = sceneWidth;
    this.sceneHeight = sceneHeight;
    this.samples = samples;
    this.temporal = temporal ? new TemporalResolve(this.device, sceneWidth, sceneHeight) : undefined;
    this.waterTemporal = waterHistory
      ? new WaterTemporalResolve(this.device, sceneWidth, sceneHeight)
      : undefined;
    this.targets.resize(this.device, sceneWidth, sceneHeight, samples);
    const sceneColor = this.targets.sceneColor;
    if (!sceneColor) throw Error("Viewport color target was not allocated");
    this.displayGroup = this.device.createBindGroup({
      layout: this.displayLayout,
      entries: [
        { binding: 0, resource: sceneColor.createView() },
        { binding: 1, resource: this.device.createSampler({ minFilter: "linear", magFilter: "linear" }) },
      ],
    });
    this.waterTransport = new WaterTransportGpu(
      this.device,
      needsWaterTransport ? sceneWidth : 1,
      needsWaterTransport ? sceneHeight : 1,
      samples,
      this.options.quality === "high" ? 1 : 2,
    );
    this.rebuildGlobalGroup();
    this.targetBytes =
      sceneWidth *
        sceneHeight *
        (8 + (4 + motionBytes) * samples + (samples > 1 ? 8 * samples : 0) + (temporal ? 32 : 0)) +
      this.waterTransport.byteLength +
      (this.waterTemporal?.byteLength ?? 0) +
      (temporal ? 16 : 0);
    this.bytes += this.targetBytes;
  }
  private prepareGpuVisibility(scene: EvaluatedScene, draws: PreparedDraw[], camera: Float32Array) {
    const candidates = draws.filter(
      (draw) =>
        draw.camera && draw.mesh && !draw.surfaces.some((surface) => surface.water || surface.mesh.shoots),
    );
    const count = candidates.reduce((sum, draw) => sum + draw.count, 0);
    const work = candidates.reduce(
      (sum, draw) => sum + draw.indices * draw.count * (draw.surfaces[0]?.skin ? 4 : 1),
      0,
    );
    const skip = () => {
      if (this.gpuVisibility) {
        this.bytes -= this.gpuVisibility.byteLength;
        this.gpuVisibility.destroy();
        this.gpuVisibility = undefined;
      }
      return undefined;
    };
    if (
      (scene.mode !== "beauty" && scene.mode !== "clay") ||
      this.options.renderCompiler?.visibility === false ||
      count < 64 ||
      count > 262_144 ||
      this.sceneWidth * this.sceneHeight > 16_777_216 ||
      work < 250_000
    )
      return skip();
    const bounds = new Map<RenderSurface, ReturnType<typeof surfaceBounds>>();
    for (const draw of candidates)
      for (const surface of draw.surfaces) bounds.set(surface, surfaceBounds(surface, scene.environment));
    const ranked = candidates
      // Filtered thin coverage is not a conservative opaque occluder. Camera
      // alpha-to-coverage and single-sample shadow masks have different samples.
      .filter((draw) =>
        draw.surfaces.every(
          (surface) =>
            !surface.skin &&
            !surface.wind &&
            !surface.water &&
            !surface.mesh.thinCoverage &&
            surface.material.appearance?.family !== "glass",
        ),
      )
      .map((draw) => {
        const area = draw.surfaces.reduce((sum, surface) => {
          const bound = bounds.get(surface);
          if (!bound) return sum;
          const rect = projectVisibilityBounds(bound, camera, this.sceneWidth, this.sceneHeight);
          return (
            sum +
            (rect ? (rect.x1 - rect.x0 + 1) * (rect.y1 - rect.y0 + 1) : this.sceneWidth * this.sceneHeight)
          );
        }, 0);
        return { draw, work: draw.indices * draw.count, area };
      })
      .filter((item) => item.area > this.sceneWidth * this.sceneHeight * 0.001)
      .sort((a, b) => b.area / b.work - a.area / a.work);
    const occluders: PreparedDraw[] = [];
    let occluderWork = 0;
    for (const item of ranked) {
      if (occluders.length >= 64) break;
      if (occluderWork + item.work >= work / 8) continue;
      occluders.push(item.draw);
      occluderWork += item.work;
    }
    if (!shouldUseGpuVisibility(count, work, occluderWork, this.sceneWidth * this.sceneHeight, this.samples))
      return skip();
    const maxInstances = 2 ** Math.ceil(Math.log2(count));
    const maxBatches = 2 ** Math.ceil(Math.log2(candidates.length));
    const uploadBytes = count * (32 + INSTANCE_FLOATS * 4) + candidates.length * 36 + 16;
    // Compaction adds per-frame inputs; never turn a visibility optimization
    // into an unbounded upload or allocation under streaming pressure.
    if (uploadBytes > this.uploadRemaining) return skip();
    if (
      this.gpuVisibility &&
      (this.gpuVisibility.width !== this.sceneWidth ||
        this.gpuVisibility.height !== this.sceneHeight ||
        this.gpuVisibility.samples !== this.samples ||
        this.gpuVisibility.maxInstances < maxInstances ||
        this.gpuVisibility.maxBatches < maxBatches)
    )
      skip();
    if (!this.gpuVisibility) {
      const bytes = gpuVisibilityBytes(
        this.sceneWidth,
        this.sceneHeight,
        this.samples,
        maxInstances,
        maxBatches,
      );
      if (!this.evict(bytes)) return;
      this.gpuVisibility = new GpuVisibility(
        this.device,
        this.sceneWidth,
        this.sceneHeight,
        this.samples,
        maxInstances,
        maxBatches,
      );
      this.bytes += this.gpuVisibility.byteLength;
      this.dynamicUploadedBytes += this.gpuVisibility.initialUploadBytes;
    }
    const resource = this.gpuVisibility;
    resource.prepare(
      camera,
      candidates.map((draw, index) => {
        draw.gpuVisibilityIndex = index;
        return {
          bounds: draw.surfaces.map(
            (surface) => bounds.get(surface) ?? surfaceBounds(surface, scene.environment),
          ),
          instances: packInstances(draw.surfaces, (surface) => this.previousMatrix(surface)),
          indexCount: draw.indices,
          firstIndex: draw.start,
        };
      }),
    );
    this.uploadRemaining -= resource.uploadBytes;
    this.gpuVisibilityFrame = this.frame;
    return { resource, occluders };
  }
  private evict(required: number) {
    const budget = this.options.maxGpuBytes ?? this.quality.maxGpuBytes;
    if (this.bytes + required > budget && this.gpuVisibility && this.gpuVisibilityFrame !== this.frame) {
      this.bytes -= this.gpuVisibility.byteLength;
      this.gpuVisibility.destroy();
      this.gpuVisibility = undefined;
    }
    const candidates = [...this.meshes.entries()]
      .filter(([, r]) => r.seen !== this.frame)
      .sort((a, b) => a[1].seen - b[1].seen);
    for (const [mesh, resource] of candidates) {
      if (this.bytes + required <= budget && this.frame - resource.seen < 120) break;
      resource.vertex.destroy();
      resource.sourceVertex?.destroy();
      resource.index.destroy();
      this.bytes -= resource.bytes;
      this.meshes.delete(mesh);
    }
    for (const [id, resource] of this.objects)
      if (
        resource.seen !== this.frame &&
        (this.frame - resource.seen > 120 || this.bytes + required > budget)
      ) {
        resource.primitive?.buffer.destroy();
        this.releaseCreatureStream(resource);
        resource.uniform.destroy();
        resource.skin.destroy();
        {
          const before = this.waterBodyBuffers.bytes;
          this.waterBodyBuffers.release(resource.waterBuffer);
          this.bytes += this.waterBodyBuffers.bytes - before;
        }
        if (resource.waterSpectrum) {
          const before = this.waterSpectrumPrograms.bytes;
          this.waterSpectrumPrograms.release(resource.waterSpectrum);
          this.bytes += this.waterSpectrumPrograms.bytes - before;
        }
        resource.instances.destroy();
        this.releaseThinCoverage(resource);
        this.objects.delete(id);
        this.bytes -= resource.bytes;
      }
    return this.bytes + required <= budget;
  }
  private releaseCreatureStream(resource: ObjectResource): void {
    const stream = resource.deformation;
    if (!stream) return;
    resource.deformation = undefined;
    if (--stream.references === 0) {
      stream.vertex.destroy();
      stream.previous.destroy();
      this.bytes -= stream.vertex.size + stream.previous.size;
      this.creatureStreams.delete(stream);
    }
  }
  private releaseThinCoverage(resource: ObjectResource) {
    if (!resource.thinCoverageKey) return;
    const before = this.thinCoveragePool.bytes;
    this.thinCoveragePool.release(resource.thinCoverageKey);
    this.bytes += this.thinCoveragePool.bytes - before;
    resource.thinCoverageKey = undefined;
    resource.thinCoverage = undefined;
  }
  private storageMesh(surface: RenderSurface) {
    return surface.skin ? surface.mesh : this.shootGeometry.canonical(surface.mesh);
  }
  private mesh(surface: RenderSurface): MeshResource | undefined {
    const existing = this.meshes.get(this.storageMesh(surface));
    if (existing) {
      existing.seen = this.frame;
      return this.uploadMesh(surface.mesh, existing);
    }
    if (!surface.mesh.indices.length || !surface.mesh.positions.length) return;
    const hasReliefSource = !!surface.mesh.reliefCoordinates || !!surface.mesh.reliefNormals;
    const bytes =
      (surface.mesh.positions.length / 3) *
        (VERTEX_FLOATS + (hasReliefSource ? RELIEF_VERTEX_FLOATS : 0)) *
        4 +
      surface.mesh.indices.byteLength;
    if (!this.evict(bytes)) {
      this.report(
        "gpu-budget",
        "The scene exceeded the GPU memory budget. Reduce world residency or mesh quality.",
        "warning",
      );
      return;
    }
    const vertices = packVertices(surface.mesh, surface.skin);
    const sourceVertices = packReliefSourceVertices(surface.mesh);
    const vertex = this.device.createBuffer({
      label: `Vertices ${surface.source}`,
      size: vertices.byteLength,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    const index = this.device.createBuffer({
      label: `Indices ${surface.source}`,
      size: surface.mesh.indices.byteLength,
      usage: GPUBufferUsage.INDEX | GPUBufferUsage.COPY_DST,
    });
    const sourceVertex = sourceVertices
      ? this.device.createBuffer({
          label: `Relief source vertices ${surface.source}`,
          size: sourceVertices.byteLength,
          usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
        })
      : undefined;
    const resource = {
      id: ++this.meshSerial,
      vertex,
      index,
      count: surface.mesh.indices.length,
      bytes,
      seen: this.frame,
      uploaded: 0,
      vertices,
      sourceVertex,
      sourceVertices,
    };
    this.meshes.set(this.storageMesh(surface), resource);
    this.bytes += bytes;
    return this.uploadMesh(surface.mesh, resource);
  }
  private uploadMesh(mesh: MeshData, resource: MeshResource): MeshResource | undefined {
    if (!resource.vertices) return resource;
    const vertexBytes = resource.vertices.byteLength;
    const sourceBytes = resource.sourceVertices?.byteLength ?? 0;
    while (this.uploadRemaining >= 4 && resource.uploaded < resource.bytes) {
      const isVertex = resource.uploaded < vertexBytes;
      const isSource = !isVertex && resource.uploaded < vertexBytes + sourceBytes;
      const data = isVertex
        ? resource.vertices
        : isSource
          ? (resource.sourceVertices as Float32Array)
          : mesh.indices;
      const offset = isVertex
        ? resource.uploaded
        : isSource
          ? resource.uploaded - vertexBytes
          : resource.uploaded - vertexBytes - sourceBytes;
      const count = Math.min(Math.floor(this.uploadRemaining / 4) * 4, data.byteLength - offset);
      this.device.queue.writeBuffer(
        isVertex ? resource.vertex : isSource ? (resource.sourceVertex as GPUBuffer) : resource.index,
        offset,
        data.buffer as ArrayBuffer,
        data.byteOffset + offset,
        count,
      );
      resource.uploaded += count;
      this.uploadRemaining -= count;
    }
    if (resource.uploaded < resource.bytes) return;
    resource.vertices = undefined;
    resource.sourceVertices = undefined;
    return resource;
  }
  private object(
    batch: SurfaceBatch,
    origin?: Vec3,
    primitiveData?: Float32Array,
  ): ObjectResource | undefined {
    const surface = batch.surfaces[0];
    let resource = this.objects.get(batch.key);
    const capacity =
      2 ** Math.ceil(Math.log2(batch.surfaces.reduce((n, s) => n + surfaceInstanceCount(s), 0)));
    const waterBytes = waterBodyFloats(surface) * 4;
    const hasWaterSpectrum = !!surface.waterState?.spectrum.tiles;
    if (
      resource &&
      (resource.capacity < capacity ||
        resource.waterBytes !== waterBytes ||
        !!resource.waterSpectrum !== hasWaterSpectrum)
    ) {
      resource.primitive?.buffer.destroy();
      this.releaseCreatureStream(resource);
      resource.uniform.destroy();
      resource.skin.destroy();
      {
        const before = this.waterBodyBuffers.bytes;
        this.waterBodyBuffers.release(resource.waterBuffer);
        this.bytes += this.waterBodyBuffers.bytes - before;
      }
      if (resource.waterSpectrum) {
        const before = this.waterSpectrumPrograms.bytes;
        this.waterSpectrumPrograms.release(resource.waterSpectrum);
        this.bytes += this.waterSpectrumPrograms.bytes - before;
      }
      resource.instances.destroy();
      this.releaseThinCoverage(resource);
      this.bytes -= resource.bytes;
      this.objects.delete(batch.key);
      resource = undefined;
    }
    if (!resource) {
      const sourceCoverage = surface.mesh.thinCoverage;
      // Retain before eviction so making room for this object cannot evict its
      // already resident shared texture and invalidate the reservation.
      let thinCoverage = sourceCoverage ? this.thinCoveragePool.retain(sourceCoverage) : undefined;
      const coverageBytes = thinCoverage ? 0 : thinCoverageTextureBytes(sourceCoverage);
      const bytes =
        OBJECT_BYTES +
        capacity * INSTANCE_FLOATS * 4 +
        (primitiveData ? capacity * PRIMITIVE_FLOATS * 4 : 0) +
        0;
      if (
        !this.evict(
          bytes +
            waterBytes +
            coverageBytes +
            (hasWaterSpectrum ? waterSpectrumAllocation(surface.waterState?.spectrum).bytes : 0),
        )
      ) {
        if (thinCoverage && sourceCoverage) {
          const before = this.thinCoveragePool.bytes;
          this.thinCoveragePool.release(sourceCoverage.key);
          this.bytes += this.thinCoveragePool.bytes - before;
        }
        return;
      }
      if (sourceCoverage && !thinCoverage) {
        thinCoverage = this.thinCoveragePool.create(this.device, sourceCoverage);
        this.bytes += coverageBytes;
      }
      const uniform = this.device.createBuffer({
        label: `Surface ${surface.id}`,
        size: OBJECT_FLOATS * 4,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC,
      });
      const skin = this.device.createBuffer({
        label: `Pose ${surface.id}`,
        size: 64 * 64 * 2,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      });
      const instances = this.device.createBuffer({
        label: `Instances ${surface.source}`,
        size: capacity * INSTANCE_FLOATS * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      });
      const beforeBody = this.waterBodyBuffers.bytes;
      const waterBuffer = this.waterBodyBuffers.acquire(surface);
      this.bytes += this.waterBodyBuffers.bytes - beforeBody;
      const spectrumKey = `${surface.waterState?.spectrum.key}:${origin?.join(",")}:${surface.water?.optics?.foamLifetime}:${!!surface.waterState?.domain}`;
      const beforeSpectrum = this.waterSpectrumPrograms.bytes;
      const waterSpectrum =
        hasWaterSpectrum && surface.waterState
          ? this.waterSpectrumPrograms.acquire(
              spectrumKey,
              surface.waterState.spectrum,
              packWaterSpectrumSource(surface, origin),
            )
          : undefined;
      this.bytes += this.waterSpectrumPrograms.bytes - beforeSpectrum;
      const group = this.device.createBindGroup({
        layout: this.objectLayout,
        entries: [
          { binding: 0, resource: { buffer: uniform } },
          { binding: 1, resource: { buffer: skin } },
          { binding: 2, resource: { buffer: instances } },
          { binding: 3, resource: (thinCoverage ?? this.thinFallback).view },
          { binding: 4, resource: this.thinSampler },
          { binding: 5, resource: { buffer: waterBuffer } },
          { binding: 6, resource: waterSpectrum?.view ?? this.waterSpectrumPrograms.fallbackView },
          { binding: 7, resource: this.waterSpectrumPrograms.sampler },
        ],
      });
      const primitive = primitiveData
        ? (() => {
            const buffer = this.device.createBuffer({
              label: `Quadric ${surface.id}`,
              size: capacity * PRIMITIVE_FLOATS * 4,
              usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
            });
            return {
              buffer,
              group: this.device.createBindGroup({
                layout: this.primitiveLayout,
                entries: [{ binding: 0, resource: { buffer } }],
              }),
            };
          })()
        : undefined;
      resource = {
        spectrumKey,
        waterSpectrum,
        waterBuffer,
        waterBytes,
        uniform,
        skin,
        instances,
        capacity,
        bytes,
        group,
        primitive,
        thinCoverage,
        thinCoverageKey: sourceCoverage?.key,
        seen: this.frame,
      };
      this.objects.set(batch.key, resource);
      this.bytes += bytes;
      this.device.queue.writeBuffer(skin, 0, identityMatrix() as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += 64;
    }
    if (hasWaterSpectrum && surface.waterState) {
      const spectrumKey = `${surface.waterState.spectrum.key}:${origin?.join(",")}:${surface.water?.optics?.foamLifetime}:${!!surface.waterState.domain}`;
      if (resource.spectrumKey !== spectrumKey) {
        const before = this.waterSpectrumPrograms.bytes,
          prior = resource.waterSpectrum;
        resource.waterSpectrum = this.waterSpectrumPrograms.acquire(
          spectrumKey,
          surface.waterState.spectrum,
          packWaterSpectrumSource(surface, origin),
          prior,
        );
        this.bytes += this.waterSpectrumPrograms.bytes - before;
        resource.spectrumKey = spectrumKey;
        if (prior !== resource.waterSpectrum)
          resource.group = this.device.createBindGroup({
            layout: this.objectLayout,
            entries: [
              { binding: 0, resource: { buffer: resource.uniform } },
              { binding: 1, resource: { buffer: resource.skin } },
              { binding: 2, resource: { buffer: resource.instances } },
              { binding: 3, resource: (resource.thinCoverage ?? this.thinFallback).view },
              { binding: 4, resource: this.thinSampler },
              { binding: 5, resource: { buffer: resource.waterBuffer } },
              { binding: 6, resource: resource.waterSpectrum!.view },
              { binding: 7, resource: this.waterSpectrumPrograms.sampler },
            ],
          });
      }
    }
    resource.seen = this.frame;
    this.dynamicUploadedBytes += this.waterBodyBuffers.upload(
      resource.waterBuffer,
      surface,
      origin,
      this.waterSceneGeometry,
    );
    resource.waterState = surface.waterState ?? surface.waterContact?.state;
    resource.waterOrigin = origin?.join(",") ?? "0,0,0";
    if (resource.thinCoverage && resource.thinCoverage.uploaded < resource.thinCoverage.bytes) {
      this.uploadRemaining -= uploadThinCoverageGpu(
        this.device,
        resource.thinCoverage,
        surface.mesh.thinCoverage,
        this.uploadRemaining,
      );
      if (resource.thinCoverage.uploaded < resource.thinCoverage.bytes) return;
    }
    const deformation = surface.deformation;
    if (deformation) {
      const actor = surface.instanceId ?? surface.id;
      const compatible = (stream: CreatureStream) =>
        stream.actor === actor &&
        stream.mesh === surface.mesh &&
        stream.indices === surface.skin?.jointIndices &&
        stream.weights === surface.skin?.weights;
      let stream = resource.deformation;
      if (
        !stream ||
        !compatible(stream) ||
        (stream.references > 1 && stream.seen === this.frame && stream.data !== deformation)
      ) {
        this.releaseCreatureStream(resource);
        stream = [...this.creatureStreams].find(
          (item) => compatible(item) && (item.seen !== this.frame || item.data === deformation),
        );
        if (!stream) {
          const bytes = (surface.mesh.positions.length / 3) * VERTEX_FLOATS * 4;
          if (!this.evict(bytes * 2)) return;
          stream = {
            previous: this.device.createBuffer({
              label: `Previous creature deformation ${actor}`,
              size: bytes,
              usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
            }),
            vertex: this.device.createBuffer({
              label: `Creature deformation ${actor}`,
              size: bytes,
              usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
            }),
            staging: packVertices(surface.mesh, surface.skin),
            mesh: surface.mesh,
            actor,
            indices: surface.skin?.jointIndices,
            weights: surface.skin?.weights,
            data: deformation,
            references: 0,
            seen: -1,
          };
          this.creatureStreams.add(stream);
          this.bytes += bytes * 2;
        }
        resource.deformation = stream;
        stream.references++;
      }
      // All material and inspection ranges share one current/previous stream.
      // Advance history once per rendered frame, never once per draw range.
      if (stream.seen !== this.frame) {
        this.device.queue.writeBuffer(stream.previous, 0, stream.staging as Float32Array<ArrayBuffer>);
        this.dynamicUploadedBytes += stream.staging.byteLength;
        stream.seen = this.frame;
      }
      if (stream.revision !== deformation.revision) {
        packCreatureDeformation(surface, stream.staging);
        this.device.queue.writeBuffer(stream.vertex, 0, stream.staging as Float32Array<ArrayBuffer>);
        this.dynamicUploadedBytes += stream.staging.byteLength;
        stream.revision = deformation.revision;
      }
      stream.data = deformation;
    } else this.releaseCreatureStream(resource);
    if (primitiveData && resource.primitive) {
      this.device.queue.writeBuffer(resource.primitive.buffer, 0, primitiveData as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += primitiveData.byteLength;
    }
    resource.instancePacket ??= new InstancePacketCache();
    const uniforms = packSurface(surface, origin),
      instances = resource.instancePacket.pack(
        batch.surfaces,
        (surface) => this.previousMatrix(surface),
        (surface) => this.previousSurfaces.get(surface.id)?.shootSelection,
      );
    if (!sameFloats(resource.uniformData, uniforms)) {
      this.device.queue.writeBuffer(resource.uniform, 0, uniforms as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += uniforms.byteLength;
      resource.uniformData = uniforms;
    }
    if (resource.instanceData !== instances) {
      this.device.queue.writeBuffer(resource.instances, 0, instances as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += instances.byteLength;
      resource.instanceData = instances;
    }
    if (surface.skin) {
      const pose = surface.skin.matrices.subarray(0, 1024);
      this.device.queue.writeBuffer(
        resource.skin,
        4096,
        (resource.previousPose ?? pose) as Float32Array<ArrayBuffer>,
      );
      resource.previousPose = pose.slice();
      this.dynamicUploadedBytes += pose.byteLength;
      this.device.queue.writeBuffer(resource.skin, 0, pose as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += pose.byteLength;
    }
    return resource;
  }
  private appearanceKey(surface: RenderSurface) {
    return JSON.stringify([
      surface.material,
      surface.water,
      surface.wind,
      surface.selected,
      surface.drawRange,
    ]);
  }
  private previousMatrix(surface: RenderSurface) {
    const previous = this.previousSurfaces.get(surface.id);
    return previous?.mesh === surface.mesh &&
      previous.appearance === this.appearanceKey(surface) &&
      previous.realization === surface.selectedRenderProduct?.key
      ? previous.matrix
      : undefined;
  }
  render(scene: EvaluatedScene): void {
    if (scene.mode === "beauty" && scene.surfaces.some(isThinGlass))
      scene = {
        ...scene,
        surfaces: scene.surfaces.map((s) => (isThinGlass(s) ? { ...s, castsShadow: false } : s)),
      };
    this.waterSceneGeometry = scene.surfaces.some((surface) => !surface.water);
    this.lastScene = scene;
    if (this.status !== "ready" || this.disposed) {
      this.completeness = {
        frame: this.frame,
        complete: false,
        rendered: [],
        culled: [],
        uploading: [],
        rejected: scene.surfaces.map(({ id }) => ({ id, reason: `Graphics device is ${this.status}` })),
      };
      return;
    }
    const start = performance.now();
    this.frame++;
    this.completeness = {
      frame: this.frame,
      complete: false,
      rendered: [],
      culled: [],
      uploading: [],
      rejected: [],
    };
    this.uploadRemaining = Math.max(
      4,
      this.options.maxUploadBytesPerFrame ?? this.quality.maxUploadBytesPerFrame,
    );
    const uploadBudget = this.uploadRemaining;
    this.dynamicUploadedBytes = this.initialUploadedBytes;
    this.initialUploadedBytes = 0;
    if (
      this.resize(
        scene.mode !== "beauty" && scene.mode !== "clay",
        scene.surfaces.some(
          (surface) =>
            !!surface.water &&
            (!surface.waterState || this.waterSceneGeometry || surface.waterEffect !== undefined),
        ),
        scene.surfaces.some((surface) => !!surface.waterState),
      ) === false
    ) {
      this.completeness.rejected = scene.surfaces.map(({ id }) => ({
        id,
        reason: "Native diagnostic render targets exceed the GPU memory budget",
      }));
      return;
    }
    const oldIndirectBytes = this.indirectLighting.bytes;
    const indirectUpload = this.indirectLighting.update(
      scene.indirectLighting,
      scene.origin,
      (this.options.maxGpuBytes ?? this.quality.maxGpuBytes) - this.bytes + oldIndirectBytes,
      scene.radianceLighting,
    );
    this.bytes += this.indirectLighting.bytes - oldIndirectBytes;
    this.dynamicUploadedBytes += indirectUpload.uploaded;
    if (indirectUpload.rejected)
      this.options.onDiagnostic?.({
        severity: "warning",
        code: "indirect.memory-budget",
        message: indirectUpload.rejected,
      });
    if (indirectUpload.changed) this.rebuildGlobalGroup();
    const oldLatticeBytes = this.materialLattice.bytes;
    const latticeUpload = this.materialLattice.update(
      this.options.materialCache !== true ? undefined : scene.materialLattice,
      Math.min(
        this.quality.maxAppearanceBytes,
        (this.options.maxGpuBytes ?? this.quality.maxGpuBytes) - this.bytes + oldLatticeBytes,
      ),
    );
    this.bytes += this.materialLattice.bytes - oldLatticeBytes;
    if (latticeUpload) {
      this.dynamicUploadedBytes += latticeUpload;
      this.rebuildGlobalGroup();
    }
    if (this.bytes > (this.options.maxGpuBytes ?? this.quality.maxGpuBytes)) this.evict(0);
    scene = {
      ...scene,
      surfaces: scene.surfaces.filter((surface) => {
        const reason = validateCreatureDeformation(surface);
        if (reason) this.completeness.rejected.push({ id: surface.id, reason });
        return !reason;
      }),
    };
    const products = selectRenderProducts(
      scene,
      this.sceneHeight,
      this.options.renderCompiler,
      this.measurements.adapter,
      navigator.userAgent,
      this.samples,
    );
    scene = products.scene;
    this.completeness.realizations = products.decisions;
    const appearance = selectWaterAppearance(scene, this.options.renderCompiler, this.qualityProfile);
    scene = appearance.scene;
    this.completeness.appearance = appearance.decisions;
    const realization = this.detailSelector.select(scene, this.sceneHeight, this.quality.detailPixelScale, {
      aspect: this.sceneWidth / this.sceneHeight,
      shadowTexelSize:
        (2 * shadowRadiusForScene(scene, this.quality.shadowDistance)) / this.quality.shadowSize,
      allowCandidates: this.options.vegetationCandidates,
    });
    scene = withIndirectReceivers(realization.scene);
    this.realizedScene = scene;
    this.completeness.details = realization.details;
    const depth = this.targets.depth,
      sceneColor = this.targets.sceneColor,
      motion = this.targets.motion;
    if (!depth || !sceneColor || !motion) throw new Error("Render targets are unavailable");
    const global = new Float32Array(GLOBAL_FLOATS);
    const aspect = this.width / this.height;
    let cameraVP = multiply(
      perspective(scene.camera.fov, aspect),
      lookAt(scene.camera.position, scene.camera.target),
    );
    const temporalActive =
      !!this.temporal && !this.capturing && (scene.mode === "beauty" || scene.mode === "clay");
    const previous = this.previousScene;
    const environment = JSON.stringify([
      scene.environment.atmosphere?.key,
      scene.environment.sunColor,
      scene.environment.sunIntensity,
      scene.environment.groundColor,
      scene.environment.pointLights,
      scene.environment.wind,
    ]);
    const origin = JSON.stringify(scene.origin ?? [0, 0, 0]);
    const forward = cameraBasis(scene.camera).forward;
    const sunDirection = normalize(scene.environment.sunDirection);
    if (
      !temporalActive ||
      !previous ||
      previous.origin !== origin ||
      previous.mode !== scene.mode ||
      previous.environment !== environment ||
      scene.time < previous.time ||
      scene.time - previous.time > 0.25 ||
      Math.abs(previous.fov - scene.camera.fov) > 1 ||
      forward.reduce((sum, value, axis) => sum + value * previous.forward[axis], 0) < 0.9 ||
      sunDirection.reduce((sum, value, axis) => sum + value * previous.sun[axis], 0) < 0.98 ||
      Math.hypot(...scene.camera.position.map((v, i) => v - previous.camera[i])) > 5
    ) {
      this.temporal?.reset();
      this.waterTemporal?.reset();
      this.previousSurfaces.clear();
      this.previousVP = undefined;
    }
    const separateWater = scene.surfaces.some((surface) => !!surface.waterState);
    if (temporalActive && !separateWater)
      cameraVP = jitterProjection(cameraVP, this.frame, this.sceneWidth, this.sceneHeight);
    const jitter = temporalActive && !separateWater ? temporalJitter(this.frame) : [0, 0];
    global.set(this.previousVP ?? cameraVP, 168);
    global.set(
      [
        previous?.time ?? scene.time,
        previous?.windPhase ?? scene.environment.windPhase ?? 0,
        (2 * jitter[0]) / this.sceneWidth,
        (-2 * jitter[1]) / this.sceneHeight,
      ],
      184,
    );
    global.set(cameraVP, 0);
    global.set(inverseMatrix(cameraVP) ?? identityMatrix(), 152);
    const sun = normalize(scene.environment.sunDirection);
    const shadowScene =
      (scene.environment.nightFactor ?? 0) > 0.5 && (scene.environment.moonIntensity ?? 0) > 0
        ? {
            ...scene,
            environment: { ...scene.environment, sunDirection: scene.environment.moonDirection ?? [0, 1, 0] },
          }
        : scene;
    const light = directionalShadow(
      shadowScene,
      shadowRadiusForScene(scene, this.quality.shadowDistance),
      this.quality.shadowSize,
    );
    const primitiveData = new Map<RenderSurface, Float32Array>();
    const primitiveViews = preparePrimitiveViews(cameraVP, light.matrix);
    scene = {
      ...scene,
      surfaces: scene.surfaces.map((surface) => {
        if (surface.selectedRenderProduct?.kind !== "analytic-quadric") return surface;
        const packed = packPrimitive(
          surface.selectedRenderProduct.primitive,
          surface.matrix,
          cameraVP,
          light.matrix,
          primitiveViews ?? undefined,
        );
        if (packed) {
          primitiveData.set(surface, packed);
          return surface;
        }
        const fallback = surface.renderProducts?.find((product) => product.kind === "direct-mesh");
        const decision = this.completeness.realizations?.find((item) => item.id === surface.id);
        if (decision && fallback) {
          decision.rejections.push({
            key: decision.key,
            reason: "invalid or ill-conditioned quadric transform",
          });
          decision.kind = "direct-mesh";
          decision.key = fallback.key;
          decision.reason = "numeric transform fallback";
        }
        return { ...surface, selectedRenderProduct: fallback };
      }),
    };
    this.realizedScene = scene;
    const sunRadius = this.options.renderCompiler?.finiteSun?.angularRadius ?? 0.00465;
    const finiteSun =
      this.options.finiteSun === false
        ? { enabled: false, data: packFiniteSunSpheres([]), reason: "conventional shadows requested" }
        : finiteSunScene(scene, sunRadius);
    global[144] = finiteSun.enabled ? sunRadius : 0;
    this.device.queue.writeBuffer(this.sunBuffer, 0, finiteSun.data as Float32Array<ArrayBuffer>);
    this.dynamicUploadedBytes += finiteSun.data.byteLength;
    this.completeness.transport = [
      { kind: "finite-sun", selected: finiteSun.enabled, reason: finiteSun.reason },
    ];
    const atmosphere =
      this.options.renderCompiler?.atmosphere ?? scene.environment.atmosphere ?? defaultPhysicalAtmosphere();
    if (atmosphere.key !== this.atmosphereKey) {
      validateAtmosphereTable(atmosphere, this.quality.maxAppearanceBytes);
      if (!this.evict(Math.max(0, atmosphere.byteLength - this.atmosphereBuffer.size)))
        throw new Error("Atmosphere exceeds renderer memory budget");
      if (this.atmosphereBuffer.size !== atmosphere.byteLength) {
        const replacement = this.device.createBuffer({
          label: "Atmosphere composition",
          size: atmosphere.byteLength,
          usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
        });
        this.bytes += atmosphere.byteLength - this.atmosphereBuffer.size;
        this.atmosphereBuffer.destroy();
        this.atmosphereBuffer = replacement;
        this.rebuildGlobalGroup();
      }
      this.device.queue.writeBuffer(this.atmosphereBuffer, 0, atmosphere.data as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += atmosphere.byteLength;
      this.atmosphereKey = atmosphere.key;
    }
    global[145] = 1;
    global.set(
      atmosphere.planetCenter.map((value, axis) => value - (scene.origin?.[axis] ?? 0)),
      148,
    );
    this.completeness.transport.push({
      kind: "atmosphere",
      selected: true,
      reason: "composition optical-depth table, sky-view and aerial-perspective products",
    });
    global.set(light.matrix, 16);
    const env = scene.environment;
    global.set([...scene.camera.position, 1], 32);
    global.set([...sun, env.sunIntensity], 36);
    global.set([...env.sunColor, light.inverseDepthRange], 40);
    global.set([...env.skyColor, env.fogDensity], 44);
    global.set([...env.horizonColor, env.ambient], 48);
    global.set([...env.groundColor, light.worldTexel], 52);
    global.set([env.cloudCover ?? 0, env.wind[0], env.wind[2], scene.time], 188);
    const cloudscape = env.cloudscape;
    global.set(packCloudFormations(cloudscape), CLOUD_FORMATION_GLOBAL_OFFSET);
    const front = cloudscape?.front;
    global.set([...(env.moonDirection ?? [0, 1, 0]), env.moonIntensity ?? 0], 192);
    global.set(
      [env.nightFactor ?? 0, env.starRotation ?? 0, env.starLatitude ?? 0, env.starNorthOffset ?? 0],
      196,
    );
    global.set(
      [
        cloudscape?.development ?? 0.45,
        cloudscape?.storminess ?? 0,
        cloudscape?.highCloudCover ?? 0,
        front?.strength ?? 0,
      ],
      200,
    );
    global.set([front?.origin[0] ?? 0, front?.origin[1] ?? 0, front?.width ?? 1000, 0], 204);
    global.set([front?.direction[0] ?? 1, front?.direction[1] ?? 0, 0, 0], 208);
    global.set([...normalizeWind(env.wind), env.windPhase ?? 0], 56);
    global.set([scene.time, env.exposure, VIEW_MODES.indexOf(scene.mode), scene.grid ? 1 : 0], 60);
    const basis = cameraBasis(scene.camera);
    global.set([...basis.right, 0], 64);
    global.set([...basis.up, 0], 68);
    global.set([...basis.forward, 0], 72);
    const points = env.pointLights?.slice(0, this.quality.maxPointLights) ?? [];
    global.set(
      [
        aspect,
        Math.tan((scene.camera.fov * Math.PI) / 360),
        points.length,
        temporalActive ? 2 : this.samples,
      ],
      76,
    );
    for (let i = 0; i < points.length; i++) {
      global.set([...points[i].position, points[i].intensity], 80 + i * 8);
      global.set([...points[i].color, (points[i].range ?? 0) ** 2], 84 + i * 8);
    }
    if (points.length)
      scene = {
        ...scene,
        surfaces: scene.surfaces.map((surface) => {
          const copy = {
            ...surface,
            localLightMask: points.length ? compileLightMask(surfaceBounds(surface, env), points) : 0,
          };
          const primitive = primitiveData.get(surface);
          if (primitive) primitiveData.set(copy, primitive);
          return copy;
        }),
      };
    const localShadowView = this.pointShadows.view;
    const localShadowBytes = this.pointShadows.byteLength;
    const localShadowPlans = this.pointShadows.prepare(scene, points);
    if (this.pointShadows.view !== localShadowView) {
      this.bytes += this.pointShadows.byteLength - localShadowBytes;
      this.rebuildGlobalGroup();
    }
    this.realizedScene = scene;
    this.completeness.visibility = [];
    const visibility = selectVisibility(scene, cameraVP, light.matrix, {
      enabled: this.options.renderCompiler?.visibility ?? true,
      onReject: (surface, reason) => this.completeness.visibility?.push({ id: surface.id, reason }),
    });
    for (const surface of scene.surfaces)
      if (surface.castsShadow === false) {
        const state = visibility.get(surface);
        if (state) state.shadow = false;
      }
    for (const plan of localShadowPlans)
      for (const caster of plan.casters) {
        const state = visibility.get(caster);
        if (state) state.shadow = true;
      }
    const required = scene.surfaces
      .filter((surface) => {
        const visible = visibility.get(surface);
        if (!visible) throw new Error("Surface visibility was not evaluated");
        // Diagnostic channels have no lighting dependency and never wait on off-camera shadow casters.
        if (scene.mode !== "beauty" && scene.mode !== "clay") visible.shadow = false;
        if (!visible.camera && !visible.shadow) {
          this.completeness.culled.push(surface.id);
          return false;
        }
        if (primitiveData.has(surface)) return true;
        const range = surface.drawRange ?? { start: 0, count: surface.mesh.indices.length };
        if (
          !Number.isInteger(range.start) ||
          !Number.isInteger(range.count) ||
          range.start < 0 ||
          range.count <= 0 ||
          range.start % 3 ||
          range.count % 3 ||
          range.start + range.count > surface.mesh.indices.length
        ) {
          this.completeness.rejected.push({ id: surface.id, reason: "Invalid or empty geometry draw range" });
          return false;
        }
        return true;
      })
      .sort((a, b) => Number(visibility.get(b)?.camera) - Number(visibility.get(a)?.camera));
    // Mark required meshes before allocation so eviction cannot destroy buffers used by either pass.
    for (const surface of required) {
      if (primitiveData.has(surface) && !localShadowPlans.length) continue;
      const cached = this.meshes.get(this.storageMesh(surface));
      if (cached) cached.seen = this.frame;
    }
    const available = required.filter((surface) => {
      if ((primitiveData.has(surface) && !localShadowPlans.length) || this.mesh(surface)) return true;
      if (this.meshes.get(this.storageMesh(surface))?.vertices) this.completeness.uploading.push(surface.id);
      else
        this.completeness.rejected.push({
          id: surface.id,
          reason: "Geometry could not fit the GPU resource budget",
        });
      return false;
    });
    const batches = batchSurfaces(
      available,
      (surface) => this.meshes.get(this.storageMesh(surface))?.id ?? 0,
      (surface) => visibility.get(surface) ?? { camera: true, shadow: !surface.water },
      localShadowPlans.length > 0,
    );
    for (const batch of batches) {
      const cached = this.objects.get(batch.key);
      if (cached) cached.seen = this.frame;
    }
    const draws: PreparedDraw[] = [];
    for (const batch of batches) {
      const surface = batch.surfaces[0];
      const mesh =
        primitiveData.has(surface) && !localShadowPlans.length
          ? undefined
          : this.meshes.get(this.storageMesh(surface));
      if (!mesh && !primitiveData.has(surface)) continue;
      const packedPrimitives = primitiveData.has(surface)
        ? new Float32Array(batch.surfaces.length * PRIMITIVE_FLOATS)
        : undefined;
      if (packedPrimitives)
        batch.surfaces.forEach((item, index) => {
          const record = primitiveData.get(item);
          if (!record) throw new Error("Missing analytic instance query");
          packedPrimitives.set(record, index * PRIMITIVE_FLOATS);
        });
      const object = this.object(batch, scene.origin, packedPrimitives);
      const coverage = this.objects.get(batch.key)?.thinCoverage;
      if (object) {
        const range = primitiveData.has(surface)
          ? { start: 0, count: 6 }
          : (surface.drawRange ?? { start: 0, count: mesh?.count ?? 0 });
        draws.push({
          mesh,
          object,
          surfaces: batch.surfaces,
          ...batch.visibility,
          count: batch.surfaces.reduce((n, s) => n + surfaceInstanceCount(s), 0),
          water: !!surface.water,
          start: range.start,
          indices: range.count,
          shadowStart: surface.shadowDrawRange?.start ?? range.start,
          shadowIndices: surface.shadowDrawRange?.count ?? range.count,
          procedural: !!(
            surface.material.pattern ||
            surface.material.normalStrength ||
            surface.material.layers?.length
          ),
        });
        this.completeness.rendered.push(...batch.surfaces.map(({ id }) => id));
      } else if (coverage && coverage.uploaded < coverage.bytes) {
        this.completeness.uploading.push(...batch.surfaces.map(({ id }) => id));
      } else
        this.completeness.rejected.push(
          ...batch.surfaces.map(({ id }) => ({
            id,
            reason: "Instance or material data could not fit the GPU resource budget",
          })),
        );
    }
    this.completeness.complete = !this.completeness.uploading.length && !this.completeness.rejected.length;
    const gpuVisibility = this.prepareGpuVisibility(scene, draws, cameraVP);
    const encoder = this.device.createCommandEncoder({ label: "Wrela frame" });
    const timing = this.timings.find((slot) => !slot.pending);
    if (this.timings.length && !timing) this.droppedGpuTimings++;
    const atmosphereUpload = this.atmosphereGpu.encode(
      encoder,
      global,
      this.atmosphereBuffer,
      timing ? { querySet: timing.query, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 } : undefined,
    );
    this.dynamicUploadedBytes += atmosphereUpload;
    this.device.queue.writeBuffer(this.globalBuffer, 0, global);
    this.dynamicUploadedBytes += global.byteLength;
    const spectrumRequests = new Map<WaterSpectrumGpu, string>();
    for (const draw of draws)
      if (draw.camera && draw.object.waterSpectrum) {
        const key = draw.object.spectrumKey ?? "";
        if (draw.object.waterSpectrum.needsUpdate(scene.time, key))
          spectrumRequests.set(draw.object.waterSpectrum, key);
      }
    let spectrumIndex = 0;
    for (const [spectrum, key] of spectrumRequests) {
      const first = spectrumIndex === 0,
        last = ++spectrumIndex === spectrumRequests.size;
      this.dynamicUploadedBytes += spectrum.encode(
        encoder,
        scene.time,
        key,
        timing && (first || last)
          ? {
              querySet: timing.query,
              ...(first ? { beginningOfPassWriteIndex: 14 } : {}),
              ...(last ? { endOfPassWriteIndex: 15 } : {}),
            }
          : undefined,
      );
    }
    const indirectRelit = this.indirectLighting.encodeRelight(encoder, {
      key: `${this.atmosphereGpu.lightingRevision}/${global[51]}/${Array.from(global.subarray(36, 44)).join(",")}/${Array.from(global.subarray(80, 144)).join(",")}`,
      timestamps: timing
        ? { querySet: timing.query, beginningOfPassWriteIndex: 16, endOfPassWriteIndex: 17 }
        : undefined,
      globals: this.globalBuffer,
      atmosphere: this.atmosphereBuffer,
      sky: this.atmosphereGpu.irradianceBuffer,
      cloudShadow: this.atmosphereGpu.cloudShadowView,
      sampler: this.atmosphereGpu.sampler,
    });
    const hasShadowDraws = draws.some((draw) => draw.shadow);
    encodeShadowPass(
      encoder,
      this.shadowTexture,
      timing && hasShadowDraws ? timing.query : undefined,
      (shadow) => {
        shadow.setPipeline(this.shadowPipeline);
        shadow.setBindGroup(0, this.shadowGroup);
        for (const draw of draws)
          if (draw.shadow) {
            shadow.setPipeline(
              draw.object.primitive
                ? this.primitiveShadow
                : this.meshPipeline(this.shadowPipeline, draw.mesh),
            );
            shadow.setBindGroup(1, draw.object.group);
            if (draw.object.primitive) {
              shadow.setBindGroup(2, draw.object.primitive.group);
              shadow.draw(6, draw.count);
            } else if (draw.mesh) {
              shadow.setVertexBuffer(0, draw.object.deformation?.vertex ?? draw.mesh.vertex);
              shadow.setVertexBuffer(1, draw.mesh.sourceVertex ?? draw.mesh.vertex);
              shadow.setVertexBuffer(2, draw.object.deformation?.previous ?? draw.mesh.vertex);
              shadow.setIndexBuffer(draw.mesh.index, "uint32");
              shadow.drawIndexed(draw.shadowIndices, draw.count, draw.shadowStart);
            }
          }
      },
    );
    this.dynamicUploadedBytes += this.pointShadows.encode(
      encoder,
      localShadowPlans,
      global,
      this.completeness.complete,
      (pass, casters) => {
        let count = 0;
        for (const draw of draws) {
          if (!draw.mesh || draw.water || !draw.surfaces.some((surface) => casters.has(surface))) continue;
          pass.setPipeline(this.meshPipeline(this.shadowPipeline, draw.mesh));
          pass.setBindGroup(1, draw.object.group);
          pass.setVertexBuffer(0, draw.object.deformation?.vertex ?? draw.mesh.vertex);
          pass.setVertexBuffer(1, draw.mesh.sourceVertex ?? draw.mesh.vertex);
          pass.setVertexBuffer(2, draw.object.deformation?.previous ?? draw.mesh.vertex);
          pass.setIndexBuffer(draw.mesh.index, "uint32");
          const surface = draw.surfaces[0];
          const range = surface.shadowDrawRange ?? surface.drawRange ?? { start: 0, count: draw.mesh.count };
          pass.drawIndexed(range.count, draw.count, range.start);
          count++;
        }
        return count;
      },
    );
    // These passes remain inside the full-frame timing interval. Shadows use
    // original instance records regardless of camera visibility.
    if (gpuVisibility) {
      const visibilityPass = gpuVisibility.resource.beginDepthPass(encoder);
      const pipeline = this.visibilityPipelines.get(this.samples);
      if (!pipeline) throw new Error("Opaque visibility pipeline is unavailable");
      visibilityPass.setPipeline(pipeline);
      visibilityPass.setBindGroup(0, this.globalGroup);
      for (const draw of gpuVisibility.occluders) {
        if (!draw.mesh) continue;
        visibilityPass.setPipeline(this.meshPipeline(pipeline, draw.mesh));
        visibilityPass.setBindGroup(1, draw.object.group);
        visibilityPass.setVertexBuffer(0, draw.object.deformation?.vertex ?? draw.mesh.vertex);
        visibilityPass.setVertexBuffer(1, draw.mesh.sourceVertex ?? draw.mesh.vertex);
        visibilityPass.setVertexBuffer(2, draw.object.deformation?.previous ?? draw.mesh.vertex);
        visibilityPass.setIndexBuffer(draw.mesh.index, "uint32");
        visibilityPass.drawIndexed(draw.indices, draw.count, draw.start);
      }
      visibilityPass.end();
      gpuVisibility.resource.encode(encoder);
    }
    const hasWater = draws.some((draw) => draw.camera && draw.water);
    const thinDraws = draws.filter(
      (draw) => draw.camera && !draw.water && draw.mesh && draw.object.thinCoverage,
    );
    const bindCameraObject = (pass: GPURenderPassEncoder, draw: PreparedDraw) => {
      pass.setBindGroup(
        1,
        gpuVisibility && draw.gpuVisibilityIndex !== undefined
          ? gpuVisibility.resource.cameraGroup(
              draw.gpuVisibilityIndex,
              this.objectLayout,
              draw.object.uniform,
              draw.object.skin,
              (draw.object.thinCoverage ?? this.thinFallback).view,
              this.thinSampler,
              draw.object.waterBuffer,
              this.waterSpectrumPrograms.fallbackView,
              this.waterSpectrumPrograms.sampler,
            )
          : draw.object.group,
      );
    };
    const submitCameraMesh = (pass: GPURenderPassEncoder, draw: PreparedDraw) => {
      if (!draw.mesh) return;
      pass.setVertexBuffer(0, draw.object.deformation?.vertex ?? draw.mesh.vertex);
      pass.setVertexBuffer(1, draw.mesh.sourceVertex ?? draw.mesh.vertex);
      pass.setVertexBuffer(2, draw.object.deformation?.previous ?? draw.mesh.vertex);
      pass.setIndexBuffer(draw.mesh.index, "uint32");
      if (gpuVisibility && draw.gpuVisibilityIndex !== undefined)
        pass.drawIndexedIndirect(
          gpuVisibility.resource.indirectBuffer,
          gpuVisibility.resource.indirectOffset(draw.gpuVisibilityIndex),
        );
      else pass.drawIndexed(draw.indices, draw.count, draw.start);
    };
    // Resolve stochastic occupancy cheaply before evaluating the expensive
    // material/lighting closure. Main thin draws use the identical vertices and
    // an equality depth test, so each shaded sample belongs to the nearest leaf.
    if (thinDraws.length && this.options.thinCoveragePrepass !== false) {
      const pipeline = this.thinDepth.get(this.samples);
      if (!pipeline) throw Error("Thin coverage depth pipeline is unavailable");
      const thinPass = encoder.beginRenderPass({
        label: "Thin coverage depth",
        colorAttachments: [],
        depthStencilAttachment: {
          view: depth.createView(),
          depthClearValue: 1,
          depthLoadOp: "clear",
          depthStoreOp: "store",
        },
        ...(timing
          ? {
              timestampWrites: {
                querySet: timing.query,
                beginningOfPassWriteIndex: 12,
                endOfPassWriteIndex: 13,
              },
            }
          : {}),
      });
      thinPass.setPipeline(pipeline);
      thinPass.setBindGroup(0, this.globalGroup);
      for (const draw of thinDraws) {
        thinPass.setPipeline(this.meshPipeline(pipeline, draw.mesh));
        bindCameraObject(thinPass, draw);
        submitCameraMesh(thinPass, draw);
      }
      thinPass.end();
    }
    const pass = encoder.beginRenderPass({
      label: "Scene linear color and depth",
      colorAttachments: [
        {
          view: (this.targets.sceneMultisample ?? sceneColor).createView(),
          ...(this.targets.sceneMultisample ? { resolveTarget: sceneColor.createView() } : {}),
          clearValue: { r: 0.05, g: 0.07, b: 0.09, a: 1 },
          loadOp: "clear",
          storeOp: hasWater ? "store" : this.targets.sceneMultisample ? "discard" : "store",
        },
        {
          view: motion.createView(),
          loadOp: "clear",
          storeOp: hasWater || temporalActive ? "store" : "discard",
          clearValue: { r: 0, g: 0, b: 0, a: 0 },
        },
      ],
      depthStencilAttachment: {
        view: depth.createView(),
        depthClearValue: 1,
        depthLoadOp: thinDraws.length && this.options.thinCoveragePrepass !== false ? "load" : "clear",
        depthStoreOp: hasWater || temporalActive ? "store" : "discard",
      },
      ...(timing
        ? {
            timestampWrites: {
              querySet: timing.query,
              beginningOfPassWriteIndex: 4,
              endOfPassWriteIndex: 5,
            },
          }
        : {}),
    });
    const skyPipeline = this.skyPipelines.get(this.samples);
    if (!skyPipeline) throw new Error("Sky pipeline is unavailable");
    pass.setPipeline(skyPipeline);
    pass.setBindGroup(0, this.globalGroup);
    pass.draw(3);
    const glassDraws =
      scene.mode === "beauty"
        ? draws.filter((draw) => draw.camera && !draw.water && draw.mesh && draw.surfaces.every(isThinGlass))
        : [];
    const glassSet = new Set(glassDraws);
    for (const draw of draws) {
      if (!draw.camera || draw.water || glassSet.has(draw)) continue;
      const profile =
        this.options.materialSpecialization === false ? undefined : materialSpecialization(draw.surfaces[0]);
      const surfaceCache =
        profile === "plain" &&
        scene.mode === "beauty" &&
        this.indirectLighting.radianceReady &&
        cachedSurfaceRadiance(draw.surfaces[0])
          ? draw.surfaces[0].mesh.radianceMixtures === false
            ? 6
            : 5
          : !scene.indirectLighting
            ? draw.surfaces[0]?.mesh.skyVisibility
              ? 4
              : 3
            : draw.surfaces[0]?.mesh.indirectProofs
              ? 2
              : draw.surfaces[0]?.indirectSurfaceChart
                ? 1
                : 0;
      const specialized =
        !draw.object.primitive && profile
          ? this.specializedMain.get(
              `${this.samples}:${profile}:${draw.procedural}:${!!draw.object.thinCoverage}:${simpleDaylightScene(scene, finiteSun.enabled)}:${simpleDaylightScene(scene, finiteSun.enabled) && surfaceCache < 4 ? 0 : surfaceCache}`,
            )
          : undefined;
      const pipeline =
        specialized ??
        (draw.object.primitive
          ? this.primitiveMain
          : draw.object.thinCoverage
            ? this.thinMain
            : surfaceCache >= 4
              ? this.skyVisibilityMain
              : surfaceCache === 3
                ? this.environmentMain
                : surfaceCache === 2
                  ? this.triangleCacheMain
                  : surfaceCache === 1
                    ? this.surfaceCacheMain
                    : this.main
        ).get(`${this.samples}:${draw.procedural}`);
      if (!pipeline) throw new Error("Material pipeline is unavailable");
      pass.setPipeline(this.meshPipeline(pipeline, draw.mesh));
      bindCameraObject(pass, draw);
      if (draw.object.primitive) {
        pass.setBindGroup(2, draw.object.primitive.group);
        pass.draw(6, draw.count);
      } else if (draw.mesh) {
        submitCameraMesh(pass, draw);
      }
    }
    if (glassDraws.length) {
      const glassPipeline = this.glassMain.get(this.samples);
      if (!glassPipeline) throw Error("Thin glass pipeline unavailable");
      const distance = (draw: (typeof glassDraws)[number]) =>
        glassDrawDistance(draw.surfaces[0], scene.camera.position);
      glassDraws.sort((a, b) => distance(b) - distance(a));
      for (const draw of glassDraws) {
        pass.setPipeline(this.meshPipeline(glassPipeline, draw.mesh));
        bindCameraObject(pass, draw);
        submitCameraMesh(pass, draw);
      }
    }
    pass.end();
    const opaqueResolved = temporalActive && separateWater && !!this.temporal;
    if (opaqueResolved && this.temporal) {
      const opaque = this.temporal.encode(
        encoder,
        sceneColor,
        motion,
        depth,
        timing
          ? { querySet: timing.query, beginningOfPassWriteIndex: 10, endOfPassWriteIndex: 11 }
          : undefined,
      );
      encoder.copyTextureToTexture({ texture: opaque }, { texture: sceneColor }, [
        this.sceneWidth,
        this.sceneHeight,
      ]);
    }
    if (hasWater) {
      // Legacy water always samples the opaque snapshot, including its clear depth.
      if (
        this.waterSceneGeometry ||
        draws.some((draw) => draw.camera && draw.water && !draw.surfaces[0]?.waterState)
      )
        this.waterTransport.capture(encoder, sceneColor, depth);
      const hasEffects = draws.some(
        (draw) => draw.camera && draw.water && draw.surfaces[0]?.waterEffect !== undefined,
      );
      for (const effectsLayer of hasEffects ? [false, true] : [false]) {
        // Airborne sheets see the completed base water through a separate snapshot.
        // They never sample the attachment they are currently writing.
        if (effectsLayer) this.waterTransport.capture(encoder, sceneColor, depth);
        const waterPass = encoder.beginRenderPass({
          label: effectsLayer ? "Water sheets and spray" : "Water reflection, refraction and absorption",
          ...(timing && !effectsLayer
            ? {
                timestampWrites: {
                  querySet: timing.query,
                  beginningOfPassWriteIndex: 6,
                  endOfPassWriteIndex: 7,
                },
              }
            : {}),
          colorAttachments: [
            {
              view: (this.targets.sceneMultisample ?? sceneColor).createView(),
              ...(this.targets.sceneMultisample ? { resolveTarget: sceneColor.createView() } : {}),
              loadOp: "load",
              storeOp: this.targets.sceneMultisample && !hasEffects ? "discard" : "store",
            },
            {
              view: motion.createView(),
              loadOp: "load",
              storeOp: temporalActive || hasEffects ? "store" : "discard",
            },
          ],
          depthStencilAttachment: {
            view: depth.createView(),
            depthLoadOp: "load",
            depthStoreOp: temporalActive || hasEffects ? "store" : "discard",
          },
        });
        waterPass.setBindGroup(0, this.globalGroup);
        for (const draw of draws) {
          if (
            !draw.camera ||
            !draw.water ||
            !draw.mesh ||
            (draw.surfaces[0]?.waterEffect !== undefined) !== effectsLayer
          )
            continue;
          const waves = draw.surfaces[0]?.water?.waves.filter((wave) => wave.amplitude !== 0) ?? [];
          const first = waves[0];
          const coherent =
            !!first &&
            (waves.every((wave) => wave.speed / wave.wavelength === first.speed / first.wavelength) ||
              waves.every(
                (wave) => wave.wavelength === first.wavelength && wave.direction === first.direction,
              ));
          const waterPipeline = this.waterMain.get(
            draw.surfaces[0]?.waterState
              ? `${this.samples}:body`
              : `${this.samples}:${draw.procedural}:${coherent}`,
          );
          if (!waterPipeline) throw new Error("Water material pipeline unavailable");
          waterPass.setPipeline(this.meshPipeline(waterPipeline, draw.mesh));
          waterPass.setBindGroup(1, draw.object.group);
          waterPass.setVertexBuffer(0, draw.object.deformation?.vertex ?? draw.mesh.vertex);
          waterPass.setVertexBuffer(1, draw.mesh.sourceVertex ?? draw.mesh.vertex);
          waterPass.setVertexBuffer(2, draw.object.deformation?.previous ?? draw.mesh.vertex);
          waterPass.setIndexBuffer(draw.mesh.index, "uint32");
          waterPass.drawIndexed(draw.indices, draw.count, draw.start);
        }
        waterPass.end();
      }
      if (opaqueResolved && this.waterTemporal) {
        this.waterTemporal.encode(encoder, sceneColor, motion, depth);
        this.dynamicUploadedBytes += 48;
      }
    }
    const fusedTemporal =
      temporalActive &&
      !opaqueResolved &&
      this.options.temporalResolve !== "compute" &&
      (this.storagePresentation || this.options.temporalResolve === "raster") &&
      this.sceneWidth === this.width &&
      this.sceneHeight === this.height;
    const presentation = this.context.getCurrentTexture();
    if (fusedTemporal && this.temporal) {
      const stamps = timing
        ? { querySet: timing.query, beginningOfPassWriteIndex: 8, endOfPassWriteIndex: 9 }
        : undefined;
      if (this.options.temporalResolve === "raster")
        this.temporal.encodeDisplay(encoder, sceneColor, motion, depth, presentation, global[61], stamps);
      else this.temporal.encode(encoder, sceneColor, motion, depth, stamps, presentation, global[61]);
    }
    const finalColor =
      temporalActive && this.temporal && !fusedTemporal && !opaqueResolved
        ? this.temporal.encode(
            encoder,
            sceneColor,
            motion,
            depth,
            timing
              ? { querySet: timing.query, beginningOfPassWriteIndex: 10, endOfPassWriteIndex: 11 }
              : undefined,
          )
        : sceneColor;
    if (temporalActive && this.temporal) this.dynamicUploadedBytes += 16;
    if (!fusedTemporal)
      this.displayGroup = this.device.createBindGroup({
        layout: this.displayLayout,
        entries: [
          { binding: 0, resource: finalColor.createView() },
          { binding: 1, resource: this.waterTransport.sampler },
        ],
      });
    this.previousVP = cameraVP.slice();
    this.previousScene = {
      time: scene.time,
      windPhase: scene.environment.windPhase ?? 0,
      origin,
      environment,
      mode: scene.mode,
      fov: scene.camera.fov,
      camera: [...scene.camera.position],
      forward,
      sun: sunDirection,
    };
    this.previousSurfaces = new Map(
      scene.surfaces.map((surface) => [
        surface.id,
        {
          matrix: surface.matrix.slice(),
          mesh: surface.mesh,
          appearance: this.appearanceKey(surface),
          realization: surface.selectedRenderProduct?.key,
          shootSelection: surface.shootSelection,
        },
      ]),
    );
    if (!fusedTemporal) {
      encodeDisplayPass(
        encoder,
        presentation,
        this.displayPipeline,
        this.globalGroup,
        this.displayGroup,
        timing?.query,
      );
    }
    if (timing) {
      encoder.resolveQuerySet(timing.query, 0, 18, timing.resolve, 0);
      encoder.copyBufferToBuffer(timing.resolve, 0, timing.read, 0, 144);
    }
    this.device.queue.submit([encoder.finish()]);
    if (timing) {
      const read = timing.read;
      const generation = this.generation;
      const frame = this.frame;
      timing.pending = read
        .mapAsync(GPUMapMode.READ)
        .then(() =>
          consumeMappedTiming(read, (t) => {
            if (generation === this.generation) {
              const sample = decodeGpuTiming(
                t,
                frame,
                hasWater,
                temporalActive,
                fusedTemporal,
                atmosphereUpload > 0,
                hasShadowDraws,
                thinDraws.length > 0 && this.options.thinCoveragePrepass !== false,
                spectrumRequests.size > 0,
                indirectRelit,
              );
              this.completedTimings.push(sample);
              if (this.completedTimings.length > 2048) this.completedTimings.shift();
              this.measurements.gpuMs = sample.gpuMs;
            }
          }),
        )
        .catch(() => {
          this.droppedGpuTimings++;
        })
        .finally(() => {
          timing.pending = undefined;
        });
    }
    this.evict(0);
    const auxiliaryTextureBytes =
      this.pointShadows.byteLength -
      this.pointShadows.bufferBytes +
      this.materialLattice.bytes -
      16 +
      this.atmosphereGpu.byteLength -
      this.atmosphereGpu.bufferByteLength +
      (this.gpuVisibility
        ? this.gpuVisibility.width * this.gpuVisibility.height * this.gpuVisibility.samples * 4
        : 0);
    const targetTextureBytes = this.targetBytes - (this.temporal ? 16 : 0) - (this.waterTemporal ? 48 : 0);
    const thinTextureBytes =
      this.thinFallback.bytes + 56 + this.thinCoveragePool.bytes + this.waterSpectrumPrograms.textureBytes;
    const waterDraws = draws.filter(
      (draw) => draw.water || draw.surfaces.some((surface) => !!surface.waterContact),
    );
    const waterGeometry = [...new Set(waterDraws.flatMap((draw) => (draw.mesh ? [draw.mesh] : [])))].reduce(
      (sum, mesh) => sum + mesh.bytes,
      0,
    );
    const waterObjects = [...new Set(waterDraws.map((draw) => draw.object))].reduce(
      (sum, object) => sum + object.bytes,
      0,
    );
    const waterResources = {
      transport: this.waterTransport.byteLength,
      history: this.waterTemporal?.byteLength ?? 0,
      spectrum: this.waterSpectrumPrograms.bytes,
      body: this.waterBodyBuffers.bytes,
      geometry: waterGeometry,
      objects: waterObjects,
    };
    this.measurements = {
      ...this.measurements,
      waterResources: { ...waterResources, total: Object.values(waterResources).reduce((a, b) => a + b, 0) },
      waterReconstruction: opaqueResolved && this.waterTemporal && hasWater ? "compact-history" : "spatial",
      reconstruction: temporalActive
        ? fusedTemporal
          ? this.options.temporalResolve === "raster"
            ? "raster"
            : "storage"
          : "compute"
        : "none",
      gpuTimingDroppedFrames: this.droppedGpuTimings,
      localShadowPasses: this.pointShadows.lastPasses,
      localShadowDrawCalls: this.pointShadows.lastDrawCalls,
      materialCacheBytes: Math.max(0, this.materialLattice.bytes - 32),
      cpuMs: performance.now() - start,
      triangles: draws
        .filter((draw) => draw.camera)
        .reduce((sum, draw) => sum + (draw.indices / 3) * draw.count, 0),
      drawCalls:
        2 +
        draws.filter((draw) => draw.camera).length +
        draws.filter((draw) => draw.shadow).length +
        (this.options.thinCoveragePrepass === false ? 0 : thinDraws.length) +
        this.pointShadows.lastDrawCalls,
      gpuBytes: this.bytes,
      frame: this.frame,
      qualityProfile: this.qualityProfile,
      uploadedBytes: uploadBudget - this.uploadRemaining + this.dynamicUploadedBytes,
      visibleSurfaces: this.completeness.rendered.length,
      culledSurfaces: this.completeness.culled.length,
      detailSurfaces: realization.details.length,
      ownedTextureBytes:
        targetTextureBytes + this.quality.shadowSize ** 2 * 4 + auxiliaryTextureBytes + thinTextureBytes,
      bufferBytes:
        this.bytes -
        targetTextureBytes -
        this.quality.shadowSize ** 2 * 4 -
        auxiliaryTextureBytes -
        thinTextureBytes,
      outputResolution: [this.width, this.height],
      renderResolution: [this.sceneWidth, this.sceneHeight],
    };
  }
  /** Each timestamp result has the frame it measured; consumers must not resample a cached value. */
  drainGpuTimings(): GpuFrameTiming[] {
    return this.completedTimings.splice(0).sort((a, b) => a.frame - b.frame);
  }
  async flushGpuTimings(): Promise<void> {
    await Promise.all(this.timings.map((slot) => slot.pending));
  }
  /** Diagnostic/preparation barrier: steady-state measurements exclude shader compilation. */
  async waitForPipelineCompilation(): Promise<void> {
    await this.specializedMain.settle();
  }
  get needsRender(): boolean {
    return this.completeness.uploading.length > 0;
  }
  async capture(): Promise<Blob> {
    this.capturing = true;
    try {
      return await this.captureFrame();
    } finally {
      this.capturing = false;
      this.temporal?.reset();
      this.waterTemporal?.reset();
    }
  }
  private async captureFrame(): Promise<Blob> {
    if (this.status !== "ready" || !this.lastScene)
      throw new Error("Render a scene before capturing a frame.");
    const scene = this.lastScene;
    // Review captures use a canonical detail decision, independent of the preceding interactive camera path.
    this.detailSelector.clear();
    this.render(scene);
    if (this.completeness.rejected.length) throw new IncompleteRenderError(this.completeness);
    // Captures wait for the scene's bounded incremental uploads to finish.
    while (this.needsRender) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      if (this.status !== "ready") throw new Error("Graphics device became unavailable during capture.");
      this.render(scene);
      if (this.completeness.rejected.length) throw new IncompleteRenderError(this.completeness);
    }
    // Copy while the current swapchain texture is valid; presentation may occur during an await.
    const width = this.width,
      height = this.height;
    const rowBytes = Math.ceil((width * 4) / 256) * 256;
    const readback = this.device.createBuffer({
      label: "Capture readback",
      size: rowBytes * height,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const encoder = this.device.createCommandEncoder({ label: "Capture" });
    encoder.copyTextureToBuffer(
      { texture: this.context.getCurrentTexture() },
      { buffer: readback, bytesPerRow: rowBytes },
      [width, height],
    );
    this.device.queue.submit([encoder.finish()]);
    try {
      await readback.mapAsync(GPUMapMode.READ);
      const bytes = new Uint8Array(readback.getMappedRange());
      const pixels = new Uint8ClampedArray(width * height * 4);
      const bgra = this.format.startsWith("bgra");
      for (let y = 0; y < height; y++)
        for (let x = 0; x < width; x++) {
          const source = y * rowBytes + x * 4,
            target = (y * width + x) * 4;
          pixels[target] = bytes[source + (bgra ? 2 : 0)];
          pixels[target + 1] = bytes[source + 1];
          pixels[target + 2] = bytes[source + (bgra ? 0 : 2)];
          pixels[target + 3] = 255;
        }
      readback.unmap();
      return await encodeCapturePixels(pixels, width, height);
    } finally {
      readback.destroy();
    }
  }
  private release() {
    this.gpuVisibility?.destroy();
    this.gpuVisibility = undefined;
    this.visibilityPipelines.clear();
    this.reliefPipelines.clear();
    this.reliefDescriptors.clear();
    this.thinDepth.clear();
    this.thinMain.clear();
    for (const mesh of this.meshes.values()) {
      mesh.vertex.destroy();
      mesh.sourceVertex?.destroy();
      mesh.index.destroy();
    }
    for (const object of this.objects.values()) {
      object.primitive?.buffer.destroy();
      this.releaseCreatureStream(object);
      object.uniform.destroy();
      object.skin.destroy();
      this.waterBodyBuffers.release(object.waterBuffer);
      if (object.waterSpectrum) this.waterSpectrumPrograms.release(object.waterSpectrum);
      object.instances.destroy();
      this.releaseThinCoverage(object);
    }
    this.meshes.clear();
    this.shootGeometry.clear();
    this.objects.clear();
    this.thinFallback?.texture.destroy();
    this.waterSpectrumPrograms?.destroy();
    this.waterBodyBuffers?.destroy();
    this.temporal?.destroy();
    this.temporal = undefined;
    this.waterTemporal?.destroy();
    this.waterTemporal = undefined;
    this.targets.destroy();
    this.previousVP = undefined;
    this.previousScene = undefined;
    this.previousSurfaces.clear();
    this.shadowTexture?.destroy();
    this.pointShadows?.destroy();
    this.globalBuffer?.destroy();
    this.sunBuffer?.destroy();
    this.atmosphereBuffer?.destroy();
    this.atmosphereGpu?.destroy();
    this.materialLattice?.destroy();
    this.specializedMain.clear();
    this.surfaceCacheMain.clear();
    this.triangleCacheMain.clear();
    this.environmentMain.clear();
    this.skyVisibilityMain.clear();
    this.indirectLighting?.dispose();
    this.waterTransport?.destroy();
    this.atmosphereKey = "";
    this.initialUploadedBytes = 0;
    for (const timing of this.timings) {
      timing.query.destroy();
      timing.resolve.destroy();
      timing.read.destroy();
    }
    this.timings = [];
    this.completedTimings = [];
    this.main.clear();
    this.waterMain.clear();
    this.glassMain.clear();
    this.primitiveMain.clear();
    this.skyPipelines.clear();
    this.detailSelector.clear();
    this.realizedScene = undefined;
    this.width = 0;
    this.height = 0;
    this.sceneWidth = 0;
    this.sceneHeight = 0;
    this.targetBytes = 0;
    this.bytes = 0;
  }
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.status = "disposed";
    this.release();
    this.context?.unconfigure();
    this.device?.destroy();
  }
}

export { encodeCapturePixels };
