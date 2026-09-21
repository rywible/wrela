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
import { identityMatrix, normalize, normalizeWind, VIEW_MODES } from "@wrela/model";
import { batchSurfaces, INSTANCE_FLOATS, packInstances, type SurfaceBatch } from "./batching";
import { DetailSelector } from "./detail";
import { cameraBasis, lookAt, multiply, perspective } from "./math";
import { GLOBAL_FLOATS, OBJECT_FLOATS, packSurface, packVertices, VERTEX_FLOATS } from "./packing";
import { QUALITY_PROFILES, type RenderQuality } from "./quality";
import { displayShader, shader } from "./shader";
import { directionalShadow, shadowRadiusForScene } from "./shadows";
import { selectVisibility } from "./visibility";

export type { GpuFrameTiming, RenderCompleteness } from "@wrela/model";
export { cameraBasis, cameraRay, lookAt, multiply, orthographic, perspective } from "./math";
export { GLOBAL_FLOATS, OBJECT_FLOATS, packSurface, packVertices, VERTEX_FLOATS } from "./packing";
export { QUALITY_PROFILES, type RenderQuality } from "./quality";
export type RendererOptions = {
  quality?: RenderQuality;
  maxGpuBytes?: number;
  /** Incremental immutable-geometry upload budget; live frame uniforms are reported separately in total uploads. */
  maxUploadBytesPerFrame?: number;
  pixelRatio?: number;
  onDiagnostic?: (diagnostic: Diagnostic) => void;
};
type MeshResource = {
  id: number;
  vertex: GPUBuffer;
  index: GPUBuffer;
  count: number;
  bytes: number;
  seen: number;
  uploaded: number;
  vertices?: Float32Array;
};
type ObjectResource = {
  uniform: GPUBuffer;
  skin: GPUBuffer;
  instances: GPUBuffer;
  capacity: number;
  bytes: number;
  group: GPUBindGroup;
  seen: number;
};
const OBJECT_BYTES = OBJECT_FLOATS * 4 + 64 * 64;
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
  private shadowPipeline!: GPURenderPipeline;
  private skyPipelines = new Map<number, GPURenderPipeline>();
  private displayPipeline!: GPURenderPipeline;
  private displayLayout!: GPUBindGroupLayout;
  private displayGroup!: GPUBindGroup;
  private globalBuffer!: GPUBuffer;
  private globalGroup!: GPUBindGroup;
  private shadowGroup!: GPUBindGroup;
  private objectLayout!: GPUBindGroupLayout;
  private depth?: GPUTexture;
  private sceneColor?: GPUTexture;
  private sceneMultisample?: GPUTexture;
  private sceneWidth = 0;
  private sceneHeight = 0;
  private samples = 1;
  private targetBytes = 0;
  private shadowTexture!: GPUTexture;
  private meshes = new Map<MeshData, MeshResource>();
  private objects = new Map<string, ObjectResource>();
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
    if (options.quality && !(options.quality in QUALITY_PROFILES))
      throw new RangeError("Unknown render quality profile");
    for (const value of [options.maxGpuBytes, options.maxUploadBytesPerFrame, options.pixelRatio])
      if (value !== undefined && (!Number.isFinite(value) || value < 0))
        throw new RangeError("Invalid renderer budget or resolution");
    const renderer = new WebGPURenderer(canvas, options);
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
  private async initialize(): Promise<void> {
    if (!navigator.gpu)
      throw new Error("WebGPU is unavailable. Wrela Studio requires a browser with WebGPU support.");
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter)
      throw new Error(
        "No WebGPU adapter is available. Enable hardware acceleration and reopen this workspace.",
      );
    const timestamp = adapter.features.has("timestamp-query");
    const device = await adapter.requestDevice({ requiredFeatures: timestamp ? ["timestamp-query"] : [] });
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
    this.context.configure({
      device,
      format: this.format,
      alphaMode: "opaque",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    });
    device.pushErrorScope("validation");
    const module = device.createShaderModule({
      label: "Wrela procedural beauty / skin / shadow / sky",
      code: shader,
    });
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
        { binding: 1, visibility: GPUShaderStage.FRAGMENT, texture: { sampleType: "depth" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT, sampler: { type: "comparison" } },
      ],
    });
    const shadowLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.VERTEX,
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
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage", minBindingSize: INSTANCE_FLOATS * 4 },
        },
      ],
    });
    const buffers: GPUVertexBufferLayout[] = [
      {
        arrayStride: VERTEX_FLOATS * 4,
        attributes: [
          { shaderLocation: 0, offset: 0, format: "float32x3" },
          { shaderLocation: 1, offset: 12, format: "float32x3" },
          { shaderLocation: 2, offset: 24, format: "float32x3" },
          { shaderLocation: 3, offset: 36, format: "float32x4" },
          { shaderLocation: 4, offset: 52, format: "float32x4" },
        ],
      },
    ];
    const layout = device.createPipelineLayout({ bindGroupLayouts: [globalLayout, this.objectLayout] });
    const pipelines: Promise<void>[] = [];
    for (const samples of new Set([1, this.quality.samples])) {
      for (const procedural of [false, true])
        pipelines.push(
          device
            .createRenderPipelineAsync({
              label: `Scene linear ${procedural ? "procedural" : "solid"} / ${samples} samples`,
              layout,
              vertex: { module, entryPoint: "vertexMain", buffers },
              fragment: {
                module,
                entryPoint: procedural ? "fragmentMain" : "fragmentSolid",
                targets: [{ format: HDR_FORMAT }],
              },
              primitive: { topology: "triangle-list", cullMode: "none" },
              multisample: { count: samples },
              depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
            })
            .then((pipeline) => {
              this.main.set(`${samples}:${procedural}`, pipeline);
            }),
        );
      pipelines.push(
        device
          .createRenderPipelineAsync({
            label: `Analytic sky / ${samples} samples`,
            layout: device.createPipelineLayout({ bindGroupLayouts: [globalLayout] }),
            vertex: { module, entryPoint: "skyVertex" },
            fragment: { module, entryPoint: "skyFragment", targets: [{ format: HDR_FORMAT }] },
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
      device
        .createRenderPipelineAsync({
          label: "Directional shadow",
          layout: device.createPipelineLayout({ bindGroupLayouts: [shadowLayout, this.objectLayout] }),
          vertex: { module, entryPoint: "shadowMain", buffers },
          primitive: { topology: "triangle-list", cullMode: "none" },
          depthStencil: {
            format: "depth32float",
            depthWriteEnabled: true,
            depthCompare: "less",
            depthBias: 2,
            depthBiasSlopeScale: 2,
          },
        })
        .then((pipeline) => {
          this.shadowPipeline = pipeline;
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
    this.globalGroup = device.createBindGroup({
      layout: globalLayout,
      entries: [
        { binding: 0, resource: { buffer: this.globalBuffer } },
        { binding: 1, resource: this.shadowTexture.createView() },
        {
          binding: 2,
          resource: device.createSampler({ compare: "less-equal", minFilter: "linear", magFilter: "linear" }),
        },
      ],
    });
    this.shadowGroup = device.createBindGroup({
      layout: shadowLayout,
      entries: [{ binding: 0, resource: { buffer: this.globalBuffer } }],
    });
    if (timestamp) {
      this.timings = Array.from({ length: 3 }, () => ({
        query: device.createQuerySet({ type: "timestamp", count: 6 }),
        resolve: device.createBuffer({
          size: 48,
          usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
        }),
        read: device.createBuffer({ size: 48, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ }),
      }));
    }
    const error = await device.popErrorScope();
    if (error) throw new Error(`WebGPU pipeline validation: ${error.message}`);
    this.bytes = GLOBAL_FLOATS * 4 + this.quality.shadowSize ** 2 * 4 + this.timings.length * 96;
    if (this.disposed) {
      this.release();
      device.destroy();
      return;
    }
    this.status = "ready";
  }
  private resize(diagnostic: boolean) {
    const dpr = Math.max(
      0.1,
      Math.min(this.options.pixelRatio ?? window.devicePixelRatio ?? 1, this.quality.maxPixelRatio),
    );
    const max = this.device.limits.maxTextureDimension2D;
    const width = Math.max(1, Math.min(max, Math.round(this.canvas.clientWidth * dpr)));
    const height = Math.max(1, Math.min(max, Math.round(this.canvas.clientHeight * dpr)));
    const samples = diagnostic ? 1 : this.quality.samples;
    const bytesPerPixel = 8 + 4 * samples + (samples > 1 ? 8 * samples : 0);
    const fixedBytes = this.quality.shadowSize ** 2 * 4 + GLOBAL_FLOATS * 4 + this.timings.length * 96;
    const targetAllowance =
      Math.max(0, (this.options.maxGpuBytes ?? this.quality.maxGpuBytes) - fixedBytes) * 0.7;
    // Reserve residency capacity before allocating large Retina render targets. Diagnostic pixels stay native.
    const scale = diagnostic
      ? 1
      : Math.min(this.quality.resolutionScale, Math.sqrt(targetAllowance / (width * height * bytesPerPixel)));
    const sceneWidth = Math.max(1, Math.floor(width * scale));
    const sceneHeight = Math.max(1, Math.floor(height * scale));
    if (
      width === this.width &&
      height === this.height &&
      sceneWidth === this.sceneWidth &&
      sceneHeight === this.sceneHeight &&
      samples === this.samples
    )
      return;
    this.depth?.destroy();
    this.sceneColor?.destroy();
    this.sceneMultisample?.destroy();
    this.sceneMultisample = undefined;
    this.bytes -= this.targetBytes;
    this.width = this.canvas.width = width;
    this.height = this.canvas.height = height;
    this.sceneWidth = sceneWidth;
    this.sceneHeight = sceneHeight;
    this.samples = samples;
    this.depth = this.device.createTexture({
      label: "Viewport depth",
      size: [sceneWidth, sceneHeight],
      sampleCount: samples,
      format: "depth32float",
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });
    this.sceneColor = this.device.createTexture({
      label: "Scene linear HDR",
      size: [sceneWidth, sceneHeight],
      format: HDR_FORMAT,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    if (samples > 1)
      this.sceneMultisample = this.device.createTexture({
        label: "Scene geometric anti-aliasing samples",
        size: [sceneWidth, sceneHeight],
        sampleCount: samples,
        format: HDR_FORMAT,
        usage: GPUTextureUsage.RENDER_ATTACHMENT,
      });
    this.displayGroup = this.device.createBindGroup({
      layout: this.displayLayout,
      entries: [
        { binding: 0, resource: this.sceneColor.createView() },
        { binding: 1, resource: this.device.createSampler({ minFilter: "linear", magFilter: "linear" }) },
      ],
    });
    this.targetBytes = sceneWidth * sceneHeight * bytesPerPixel;
    this.bytes += this.targetBytes;
  }
  private evict(required: number) {
    const budget = this.options.maxGpuBytes ?? this.quality.maxGpuBytes;
    const candidates = [...this.meshes.entries()]
      .filter(([, r]) => r.seen !== this.frame)
      .sort((a, b) => a[1].seen - b[1].seen);
    for (const [mesh, resource] of candidates) {
      if (this.bytes + required <= budget && this.frame - resource.seen < 120) break;
      resource.vertex.destroy();
      resource.index.destroy();
      this.bytes -= resource.bytes;
      this.meshes.delete(mesh);
    }
    for (const [id, resource] of this.objects)
      if (
        resource.seen !== this.frame &&
        (this.frame - resource.seen > 120 || this.bytes + required > budget)
      ) {
        resource.uniform.destroy();
        resource.skin.destroy();
        resource.instances.destroy();
        this.objects.delete(id);
        this.bytes -= resource.bytes;
      }
    return this.bytes + required <= budget;
  }
  private mesh(surface: RenderSurface): MeshResource | undefined {
    const existing = this.meshes.get(surface.mesh);
    if (existing) {
      existing.seen = this.frame;
      return this.uploadMesh(surface.mesh, existing);
    }
    if (!surface.mesh.indices.length || !surface.mesh.positions.length) return;
    const bytes = (surface.mesh.positions.length / 3) * VERTEX_FLOATS * 4 + surface.mesh.indices.byteLength;
    if (!this.evict(bytes)) {
      this.report(
        "gpu-budget",
        "The scene exceeded the GPU memory budget. Reduce world residency or mesh quality.",
        "warning",
      );
      return;
    }
    const vertices = packVertices(surface.mesh, surface.skin);
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
    const resource = {
      id: ++this.meshSerial,
      vertex,
      index,
      count: surface.mesh.indices.length,
      bytes,
      seen: this.frame,
      uploaded: 0,
      vertices,
    };
    this.meshes.set(surface.mesh, resource);
    this.bytes += bytes;
    return this.uploadMesh(surface.mesh, resource);
  }
  private uploadMesh(mesh: MeshData, resource: MeshResource): MeshResource | undefined {
    if (!resource.vertices) return resource;
    const vertexBytes = resource.vertices.byteLength;
    while (this.uploadRemaining >= 4 && resource.uploaded < resource.bytes) {
      const isVertex = resource.uploaded < vertexBytes;
      const data = isVertex ? resource.vertices : mesh.indices;
      const offset = isVertex ? resource.uploaded : resource.uploaded - vertexBytes;
      const count = Math.min(Math.floor(this.uploadRemaining / 4) * 4, data.byteLength - offset);
      this.device.queue.writeBuffer(
        isVertex ? resource.vertex : resource.index,
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
    return resource;
  }
  private object(batch: SurfaceBatch, origin?: Vec3): ObjectResource | undefined {
    const surface = batch.surfaces[0];
    let resource = this.objects.get(batch.key);
    const capacity = 2 ** Math.ceil(Math.log2(batch.surfaces.length));
    if (resource && resource.capacity < capacity) {
      resource.uniform.destroy();
      resource.skin.destroy();
      resource.instances.destroy();
      this.bytes -= resource.bytes;
      this.objects.delete(batch.key);
      resource = undefined;
    }
    if (!resource) {
      const bytes = OBJECT_BYTES + capacity * INSTANCE_FLOATS * 4;
      if (!this.evict(bytes)) return;
      const uniform = this.device.createBuffer({
        label: `Surface ${surface.id}`,
        size: OBJECT_FLOATS * 4,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
      });
      const skin = this.device.createBuffer({
        label: `Pose ${surface.id}`,
        size: 64 * 64,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      });
      const instances = this.device.createBuffer({
        label: `Instances ${surface.source}`,
        size: capacity * INSTANCE_FLOATS * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      });
      const group = this.device.createBindGroup({
        layout: this.objectLayout,
        entries: [
          { binding: 0, resource: { buffer: uniform } },
          { binding: 1, resource: { buffer: skin } },
          { binding: 2, resource: { buffer: instances } },
        ],
      });
      resource = { uniform, skin, instances, capacity, bytes, group, seen: this.frame };
      this.objects.set(batch.key, resource);
      this.bytes += bytes;
      this.device.queue.writeBuffer(skin, 0, identityMatrix() as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += 64;
    }
    resource.seen = this.frame;
    const uniforms = packSurface(surface, origin),
      instances = packInstances(batch.surfaces);
    this.device.queue.writeBuffer(resource.uniform, 0, uniforms as Float32Array<ArrayBuffer>);
    this.device.queue.writeBuffer(resource.instances, 0, instances as Float32Array<ArrayBuffer>);
    this.dynamicUploadedBytes += uniforms.byteLength + instances.byteLength;
    if (surface.skin) {
      const pose = surface.skin.matrices.subarray(0, 1024);
      this.device.queue.writeBuffer(resource.skin, 0, pose as Float32Array<ArrayBuffer>);
      this.dynamicUploadedBytes += pose.byteLength;
    }
    return resource;
  }
  render(scene: EvaluatedScene): void {
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
    this.dynamicUploadedBytes = 0;
    this.resize(scene.mode !== "beauty");
    if (this.bytes > (this.options.maxGpuBytes ?? this.quality.maxGpuBytes)) this.evict(0);
    const realization = this.detailSelector.select(
      scene,
      this.height * this.quality.resolutionScale,
      this.quality.detailPixelScale,
    );
    scene = realization.scene;
    this.realizedScene = scene;
    this.completeness.details = realization.details;
    const depth = this.depth,
      sceneColor = this.sceneColor;
    if (!depth || !sceneColor) throw new Error("Render targets are unavailable");
    const global = new Float32Array(GLOBAL_FLOATS);
    const aspect = this.width / this.height;
    const cameraVP = multiply(
      perspective(scene.camera.fov, aspect),
      lookAt(scene.camera.position, scene.camera.target),
    );
    global.set(cameraVP, 0);
    const sun = normalize(scene.environment.sunDirection);
    const light = directionalShadow(
      scene,
      shadowRadiusForScene(scene, this.quality.shadowDistance),
      this.quality.shadowSize,
    );
    global.set(light.matrix, 16);
    const env = scene.environment;
    global.set([...scene.camera.position, 1], 32);
    global.set([...sun, env.sunIntensity], 36);
    global.set([...env.sunColor, light.inverseDepthRange], 40);
    global.set([...env.skyColor, env.fogDensity], 44);
    global.set([...env.horizonColor, env.ambient], 48);
    global.set([...env.groundColor, light.worldTexel], 52);
    global.set([...normalizeWind(env.wind), env.windPhase ?? 0], 56);
    global.set([scene.time, env.exposure, VIEW_MODES.indexOf(scene.mode), scene.grid ? 1 : 0], 60);
    const basis = cameraBasis(scene.camera);
    global.set([...basis.right, 0], 64);
    global.set([...basis.up, 0], 68);
    global.set([...basis.forward, 0], 72);
    const points = env.pointLights?.slice(0, this.quality.maxPointLights) ?? [];
    global.set([aspect, Math.tan((scene.camera.fov * Math.PI) / 360), points.length, this.samples], 76);
    for (let i = 0; i < points.length; i++) {
      global.set([...points[i].position, points[i].intensity], 80 + i * 8);
      global.set([...points[i].color, 0], 84 + i * 8);
    }
    this.device.queue.writeBuffer(this.globalBuffer, 0, global);
    this.dynamicUploadedBytes += global.byteLength;
    const visibility = selectVisibility(scene, cameraVP, light.matrix);
    const required = scene.surfaces
      .filter((surface) => {
        const visible = visibility.get(surface);
        if (!visible) throw new Error("Surface visibility was not evaluated");
        // Diagnostic channels have no lighting dependency and never wait on off-camera shadow casters.
        if (scene.mode !== "beauty") visible.shadow = false;
        if (!visible.camera && !visible.shadow) {
          this.completeness.culled.push(surface.id);
          return false;
        }
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
      const cached = this.meshes.get(surface.mesh);
      if (cached) cached.seen = this.frame;
    }
    const available = required.filter((surface) => {
      if (this.mesh(surface)) return true;
      if (this.meshes.get(surface.mesh)?.vertices) this.completeness.uploading.push(surface.id);
      else
        this.completeness.rejected.push({
          id: surface.id,
          reason: "Geometry could not fit the GPU resource budget",
        });
      return false;
    });
    const batches = batchSurfaces(
      available,
      (surface) => this.meshes.get(surface.mesh)?.id ?? 0,
      (surface) => visibility.get(surface) ?? { camera: true, shadow: !surface.water },
    );
    for (const batch of batches) {
      const cached = this.objects.get(batch.key);
      if (cached) cached.seen = this.frame;
    }
    const draws: {
      mesh: MeshResource;
      object: ObjectResource;
      camera: boolean;
      shadow: boolean;
      count: number;
      start: number;
      indices: number;
      procedural: boolean;
    }[] = [];
    for (const batch of batches) {
      const surface = batch.surfaces[0];
      const mesh = this.meshes.get(surface.mesh);
      if (!mesh) continue;
      const object = this.object(batch, scene.origin);
      if (object) {
        const range = surface.drawRange ?? { start: 0, count: mesh.count };
        draws.push({
          mesh,
          object,
          ...batch.visibility,
          count: batch.surfaces.length,
          start: range.start,
          indices: range.count,
          procedural: !!(
            surface.material.pattern ||
            surface.material.normalStrength ||
            surface.material.layers?.length
          ),
        });
        this.completeness.rendered.push(...batch.surfaces.map(({ id }) => id));
      } else
        this.completeness.rejected.push(
          ...batch.surfaces.map(({ id }) => ({
            id,
            reason: "Instance or material data could not fit the GPU resource budget",
          })),
        );
    }
    this.completeness.complete = !this.completeness.uploading.length && !this.completeness.rejected.length;
    const encoder = this.device.createCommandEncoder({ label: "Wrela frame" });
    const timing = this.timings.find((slot) => !slot.pending);
    const shadow = encoder.beginRenderPass({
      label: "Shadow pass",
      colorAttachments: [],
      depthStencilAttachment: {
        view: this.shadowTexture.createView(),
        depthClearValue: 1,
        depthLoadOp: "clear",
        depthStoreOp: "store",
      },
      ...(timing
        ? {
            timestampWrites: { querySet: timing.query, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 },
          }
        : {}),
    });
    shadow.setPipeline(this.shadowPipeline);
    shadow.setBindGroup(0, this.shadowGroup);
    for (const draw of draws)
      if (draw.shadow) {
        shadow.setBindGroup(1, draw.object.group);
        shadow.setVertexBuffer(0, draw.mesh.vertex);
        shadow.setIndexBuffer(draw.mesh.index, "uint32");
        shadow.drawIndexed(draw.indices, draw.count, draw.start);
      }
    shadow.end();
    const pass = encoder.beginRenderPass({
      label: "Scene linear color and depth",
      colorAttachments: [
        {
          view: (this.sceneMultisample ?? sceneColor).createView(),
          ...(this.sceneMultisample ? { resolveTarget: sceneColor.createView() } : {}),
          clearValue: { r: 0.05, g: 0.07, b: 0.09, a: 1 },
          loadOp: "clear",
          storeOp: this.sceneMultisample ? "discard" : "store",
        },
      ],
      depthStencilAttachment: {
        view: depth.createView(),
        depthClearValue: 1,
        depthLoadOp: "clear",
        depthStoreOp: "discard",
      },
      ...(timing
        ? {
            timestampWrites: { querySet: timing.query, beginningOfPassWriteIndex: 2, endOfPassWriteIndex: 3 },
          }
        : {}),
    });
    const skyPipeline = this.skyPipelines.get(this.samples);
    if (!skyPipeline) throw new Error("Sky pipeline is unavailable");
    pass.setPipeline(skyPipeline);
    pass.setBindGroup(0, this.globalGroup);
    pass.draw(3);
    for (const draw of draws) {
      if (!draw.camera) continue;
      const pipeline = this.main.get(`${this.samples}:${draw.procedural}`);
      if (!pipeline) throw new Error("Material pipeline is unavailable");
      pass.setPipeline(pipeline);
      pass.setBindGroup(1, draw.object.group);
      pass.setVertexBuffer(0, draw.mesh.vertex);
      pass.setIndexBuffer(draw.mesh.index, "uint32");
      pass.drawIndexed(draw.indices, draw.count, draw.start);
    }
    pass.end();
    const display = encoder.beginRenderPass({
      label: "Display transform and anti-aliasing resolve",
      colorAttachments: [
        {
          view: this.context.getCurrentTexture().createView(),
          loadOp: "clear",
          storeOp: "store",
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
        },
      ],
      ...(timing
        ? {
            timestampWrites: { querySet: timing.query, beginningOfPassWriteIndex: 4, endOfPassWriteIndex: 5 },
          }
        : {}),
    });
    display.setPipeline(this.displayPipeline);
    display.setBindGroup(0, this.globalGroup);
    display.setBindGroup(1, this.displayGroup);
    display.draw(3);
    display.end();
    if (timing) {
      encoder.resolveQuerySet(timing.query, 0, 6, timing.resolve, 0);
      encoder.copyBufferToBuffer(timing.resolve, 0, timing.read, 0, 48);
    }
    this.device.queue.submit([encoder.finish()]);
    if (timing) {
      const read = timing.read;
      const generation = this.generation;
      const frame = this.frame;
      timing.pending = read
        .mapAsync(GPUMapMode.READ)
        .then(() => {
          const t = new BigUint64Array(read.getMappedRange());
          if (generation === this.generation) {
            const sample: GpuFrameTiming = {
              frame,
              shadowMs: Number(t[1] - t[0]) / 1e6,
              sceneMs: Number(t[3] - t[2]) / 1e6,
              displayMs: Number(t[5] - t[4]) / 1e6,
              gpuMs: Number(t[5] - t[0]) / 1e6,
            };
            this.completedTimings.push(sample);
            if (this.completedTimings.length > 2048) this.completedTimings.shift();
            this.measurements.gpuMs = sample.gpuMs;
          }
          read.unmap();
        })
        .catch(() => {})
        .finally(() => {
          timing.pending = undefined;
        });
    }
    this.evict(0);
    this.measurements = {
      ...this.measurements,
      cpuMs: performance.now() - start,
      triangles: draws
        .filter((draw) => draw.camera)
        .reduce((sum, draw) => sum + (draw.indices / 3) * draw.count, 0),
      drawCalls: 2 + draws.filter((draw) => draw.camera).length + draws.filter((draw) => draw.shadow).length,
      gpuBytes: this.bytes,
      frame: this.frame,
      qualityProfile: this.qualityProfile,
      uploadedBytes: uploadBudget - this.uploadRemaining + this.dynamicUploadedBytes,
      visibleSurfaces: this.completeness.rendered.length,
      culledSurfaces: this.completeness.culled.length,
      detailSurfaces: realization.details.length,
      ownedTextureBytes: this.targetBytes + this.quality.shadowSize ** 2 * 4,
      bufferBytes: this.bytes - this.targetBytes - this.quality.shadowSize ** 2 * 4,
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
  get needsRender(): boolean {
    return this.completeness.uploading.length > 0;
  }
  async capture(): Promise<Blob> {
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
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (!context) throw new Error("Could not create a capture encoding surface.");
      context.putImageData(new ImageData(pixels, width, height), 0, 0);
      return await new Promise<Blob>((resolve, reject) =>
        canvas.toBlob(
          (blob) => (blob ? resolve(blob) : reject(new Error("Could not encode viewport PNG."))),
          "image/png",
        ),
      );
    } finally {
      readback.destroy();
    }
  }
  private release() {
    for (const mesh of this.meshes.values()) {
      mesh.vertex.destroy();
      mesh.index.destroy();
    }
    for (const object of this.objects.values()) {
      object.uniform.destroy();
      object.skin.destroy();
      object.instances.destroy();
    }
    this.meshes.clear();
    this.objects.clear();
    this.depth?.destroy();
    this.sceneColor?.destroy();
    this.sceneMultisample?.destroy();
    this.shadowTexture?.destroy();
    this.globalBuffer?.destroy();
    for (const timing of this.timings) {
      timing.query.destroy();
      timing.resolve.destroy();
      timing.read.destroy();
    }
    this.timings = [];
    this.completedTimings = [];
    this.main.clear();
    this.skyPipelines.clear();
    this.detailSelector.clear();
    this.realizedScene = undefined;
    this.depth = undefined;
    this.sceneColor = undefined;
    this.sceneMultisample = undefined;
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
