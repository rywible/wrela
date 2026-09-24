import {
  PHYSICAL_AERIAL_SIZE,
  PHYSICAL_ATMOSPHERE_FRAME_FLOATS,
  PHYSICAL_CLOUD_LIGHT_SIZE,
  PHYSICAL_CLOUD_NOISE_SIZE,
  PHYSICAL_CLOUD_SHADOW_SIZE,
  PHYSICAL_CLOUD_VIEW_SIZE,
  PHYSICAL_DIFFUSE_SIZE,
  PHYSICAL_SKY_SIZE,
  physicalAtmosphereComputeWGSL,
  physicalCloudComputeWGSL,
  physicalCloudLightComputeWGSL,
  physicalCloudNoiseComputeWGSL,
  physicalCloudResolveWGSL,
  physicalCloudTemporalComputeWGSL,
  physicalMultipleScatteringComputeWGSL,
} from "./atmosphere";
import { CLOUD_FORMATION_FRAME_OFFSET, CLOUD_FORMATION_GLOBAL_OFFSET } from "./cloud-formations";
import { canReuseCloudHistory, cloudHistoryMaxAge } from "./cloud-history";
import { EnvironmentReflectionGpu, reflectionBytes } from "./environment-reflection";
import { irradianceBuildWGSL } from "./irradiance";
import { GLOBAL_FLOATS } from "./packing";
import type { RenderQuality } from "./quality";

const CLOUD_QUALITY = {
  low: { size: [320, 240], skySize: [256, 128], steps: 64, skySteps: 48, lightSteps: 6 },
  balanced: { size: PHYSICAL_CLOUD_VIEW_SIZE, skySize: [384, 192], steps: 384, skySteps: 64, lightSteps: 12 },
  high: { size: [1440, 900], skySize: PHYSICAL_SKY_SIZE, steps: 1024, skySteps: 96, lightSteps: 16 },
} as const;

/** Rebuild bounded lighting products when their exact inputs change.
 * Clear-air products are cached by their dependencies; cloud views use
 * bounded-age, wind-aware reconstruction with a full tracing control. */
export class PhysicalAtmosphereGpu {
  lightingRevision = 0;
  readonly reflection: EnvironmentReflectionGpu;
  readonly irradianceBuffer: GPUBuffer;
  private readonly irradiancePipeline: GPUComputePipeline;
  private readonly irradianceGroup: GPUBindGroup;
  readonly skyView: GPUTextureView;
  readonly cloudView: GPUTextureView;
  private readonly rawCloudView: GPUTextureView;
  private readonly historyViews: GPUTextureView[];
  private readonly momentViews: GPUTextureView[];
  private readonly historyFrame: GPUBuffer;
  private readonly rayQueue: GPUBuffer;
  private readonly cloudTracePipeline: GPUComputePipeline;
  private readonly cloudFullPipeline: GPUComputePipeline;
  private readonly historyData = new Float32Array(PHYSICAL_ATMOSPHERE_FRAME_FLOATS + 4);
  private readonly previousFrame = new Float32Array(PHYSICAL_ATMOSPHERE_FRAME_FLOATS);
  private historyIndex = 0;
  private historyPhase = 0;
  private historyValid = false;
  readonly cloudNoiseView: GPUTextureView;
  readonly cloudShadowView: GPUTextureView;
  readonly cloudLightView: GPUTextureView;
  private readonly cloudAmbientView: GPUTextureView;
  private readonly clearSkyView: GPUTextureView;
  readonly aerialRadianceView: GPUTextureView;
  readonly aerialTransmissionView: GPUTextureView;
  readonly sampler: GPUSampler;
  readonly byteLength: number;
  readonly bufferByteLength: number;
  readonly frameUploadBytes = PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4;
  private readonly viewSize: readonly [number, number];
  private readonly skySize: readonly [number, number];
  private readonly textures: GPUTexture[];
  private readonly frame: GPUBuffer;
  private readonly layout: GPUBindGroupLayout;
  private readonly skyPipeline: GPUComputePipeline;
  private readonly aerialPipeline: GPUComputePipeline;
  private readonly diffusePipeline: GPUComputePipeline;
  private readonly diffuseView: GPUTextureView;
  private readonly cloudLayout: GPUBindGroupLayout;
  private readonly cloudSkyPipeline: GPUComputePipeline;
  private readonly cloudViewPipeline: GPUComputePipeline;
  private readonly cloudShadowPipeline: GPUComputePipeline;
  private readonly cloudLightPipeline: GPUComputePipeline;
  private readonly cloudLightLayout: GPUBindGroupLayout;
  private cloudLightBind: GPUBindGroup | null = null;
  private readonly cloudNoisePipeline: GPUComputePipeline;
  private readonly cloudResolvePipeline: GPUComputePipeline;
  private readonly cloudResolveBind: GPUBindGroup[];
  private readonly cloudNoiseBind: GPUBindGroup;
  private noiseReady = false;
  private cloudBind: GPUBindGroup[] = [];
  private diffuseBind: GPUBindGroup | null = null;
  private diffuseKey = "";
  private skyKey = "";
  private clearKey = "";
  private viewKey = "";
  private shadowKey = "";
  private lightGridKey = "";
  private aerialKey = "";
  private table: GPUBuffer | null = null;
  private bind: GPUBindGroup | null = null;
  private readonly frameData = new Float32Array(PHYSICAL_ATMOSPHERE_FRAME_FLOATS);
  private readonly cloudViewBasis = new Float32Array(16);
  private cloudViewBasisValid = false;
  private constructor(
    private readonly device: GPUDevice,
    module: GPUShaderModule,
    diffuseModule: GPUShaderModule,
    cloudModule: GPUShaderModule,
    cloudNoiseModule: GPUShaderModule,
    cloudResolveModule: GPUShaderModule,
    cloudLightModule: GPUShaderModule,
    cloudTemporalModule: GPUShaderModule,
    readonly cloudNoiseMode: "compiled" | "reference",
    quality: RenderQuality,
    readonly reconstruction: "temporal" | "full",
  ) {
    const cloudQuality = CLOUD_QUALITY[quality];
    const viewSize = cloudQuality.size;
    this.viewSize = viewSize;
    this.skySize = cloudQuality.skySize;
    this.bufferByteLength =
      512 +
      viewSize[0] * Math.ceil(viewSize[1] / 16) * 16 * 4 +
      this.historyData.byteLength +
      this.frameUploadBytes +
      9 * 16;
    this.byteLength =
      reflectionBytes +
      viewSize[0] * viewSize[1] * 24 +
      512 +
      viewSize[0] * Math.ceil(viewSize[1] / 16) * 16 * 4 +
      (PHYSICAL_ATMOSPHERE_FRAME_FLOATS + 4) * 4 +
      (PHYSICAL_DIFFUSE_SIZE[0] * PHYSICAL_DIFFUSE_SIZE[1] +
        (PHYSICAL_SKY_SIZE[0] * PHYSICAL_SKY_SIZE[1] + this.skySize[0] * this.skySize[1]) +
        2 * viewSize[0] * viewSize[1] +
        2 * PHYSICAL_AERIAL_SIZE[0] * PHYSICAL_AERIAL_SIZE[1] * PHYSICAL_AERIAL_SIZE[2]) *
        8 +
      PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4 +
      9 * 16 +
      PHYSICAL_CLOUD_SHADOW_SIZE[0] * PHYSICAL_CLOUD_SHADOW_SIZE[1] * 4 +
      PHYSICAL_CLOUD_LIGHT_SIZE[0] * PHYSICAL_CLOUD_LIGHT_SIZE[1] * PHYSICAL_CLOUD_LIGHT_SIZE[2] * 16 +
      PHYSICAL_CLOUD_NOISE_SIZE[0] * PHYSICAL_CLOUD_NOISE_SIZE[1] * PHYSICAL_CLOUD_NOISE_SIZE[2] * 4;
    const texture = (label: string, size: readonly number[], dimension: GPUTextureDimension) =>
      device.createTexture({
        label,
        size: [...size],
        dimension,
        format: "rgba16float",
        usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
      });
    this.textures = [
      texture("Physical atmosphere sky view", this.skySize, "2d"),
      texture("Physical atmosphere aerial radiance", PHYSICAL_AERIAL_SIZE, "3d"),
      texture("Physical atmosphere aerial transmission", PHYSICAL_AERIAL_SIZE, "3d"),
      texture("Physical atmosphere diffuse transport", PHYSICAL_DIFFUSE_SIZE, "2d"),
      texture("Physical atmosphere clear sky", PHYSICAL_SKY_SIZE, "2d"),
      texture("Physical atmosphere view clouds", viewSize, "2d"),
      texture("Physical atmosphere resolved view clouds", viewSize, "2d"),
    ];
    [
      this.skyView,
      this.aerialRadianceView,
      this.aerialTransmissionView,
      this.diffuseView,
      this.clearSkyView,
      this.rawCloudView,
      this.cloudView,
    ] = this.textures.map((t) => t.createView());
    const secondHistory = texture("Cloud transport history", viewSize, "2d");
    this.textures.push(secondHistory);
    this.historyViews = [this.rawCloudView, secondHistory.createView()];
    this.momentViews = [0, 1].map((index) => {
      const moments = device.createTexture({
        label: `Cloud extinction moments ${index}`,
        size: [...viewSize],
        format: "rgba16float",
        usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
      });
      this.textures.push(moments);
      return moments.createView();
    });
    this.rayQueue = device.createBuffer({
      label: "Cloud ray work queue",
      size: 512 + viewSize[0] * Math.ceil(viewSize[1] / 16) * 16 * 4,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    this.historyFrame = device.createBuffer({
      label: "Cloud semantic motion",
      size: this.historyData.byteLength,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    const noiseTexture = device.createTexture({
      label: "Compiled procedural cloud noise",
      size: [...PHYSICAL_CLOUD_NOISE_SIZE],
      dimension: "3d",
      format: "rgba8unorm",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.textures.push(noiseTexture);
    this.cloudNoiseView = noiseTexture.createView();
    const shadowTexture = device.createTexture({
      label: "Compiled cloud sun transmission",
      size: [...PHYSICAL_CLOUD_SHADOW_SIZE],
      format: "rgba8unorm",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.textures.push(shadowTexture);
    this.cloudShadowView = shadowTexture.createView();
    const lightTexture = texture("Shared cloud optical depth", PHYSICAL_CLOUD_LIGHT_SIZE, "3d");
    this.textures.push(lightTexture);
    this.cloudLightView = lightTexture.createView();
    const ambientTexture = texture("Shared cloud incident diffuse light", PHYSICAL_CLOUD_LIGHT_SIZE, "3d");
    this.textures.push(ambientTexture);
    this.cloudAmbientView = ambientTexture.createView();
    this.cloudNoisePipeline = device.createComputePipeline({
      label: "Compile periodic cloud noise",
      layout: "auto",
      compute: { module: cloudNoiseModule, entryPoint: "main" },
    });
    this.cloudNoiseBind = device.createBindGroup({
      layout: this.cloudNoisePipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: this.cloudNoiseView }],
    });
    this.cloudResolvePipeline = device.createComputePipeline({
      label: "Resolve physical cloud view",
      layout: "auto",
      compute: { module: cloudResolveModule, entryPoint: "main" },
    });
    this.cloudResolveBind = this.historyViews.map((view) =>
      device.createBindGroup({
        layout: this.cloudResolvePipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: view },
          { binding: 1, resource: this.cloudView },
        ],
      }),
    );
    this.sampler = device.createSampler({
      label: "Physical atmosphere linear lookup",
      minFilter: "linear",
      mipmapFilter: "linear",
      magFilter: "linear",
      addressModeU: "repeat",
      addressModeV: "clamp-to-edge",
      addressModeW: "clamp-to-edge",
    });
    this.reflection = new EnvironmentReflectionGpu(device, this.skyView, this.sampler);
    this.cloudLightLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 5,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "2d" },
        },
        {
          binding: 23,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "3d" },
        },
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 6, visibility: GPUShaderStage.COMPUTE, sampler: { type: "filtering" } },
        {
          binding: 17,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 20,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "3d" },
        },
      ],
    });
    this.cloudLightPipeline = device.createComputePipeline({
      label: "Shared cloud optical depth build",
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.cloudLightLayout] }),
      compute: {
        module: cloudLightModule,
        entryPoint: "main",
        constants: {
          PHYSICAL_CLOUD_COMPILED_NOISE: Number(cloudNoiseMode === "compiled"),
          PHYSICAL_CLOUD_LIGHT_STEPS: cloudQuality.lightSteps,
        },
      },
    });
    this.irradianceBuffer = device.createBuffer({
      label: "Preintegrated sky irradiance",
      size: 9 * 16,
      usage: GPUBufferUsage.STORAGE,
    });
    this.irradiancePipeline = device.createComputePipeline({
      label: "Sky diffuse convolution",
      layout: "auto",
      compute: { module: device.createShaderModule({ code: irradianceBuildWGSL }), entryPoint: "main" },
    });
    this.irradianceGroup = device.createBindGroup({
      layout: this.irradiancePipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: this.skyView },
        { binding: 1, resource: this.sampler },
        { binding: 2, resource: { buffer: this.irradianceBuffer } },
      ],
    });
    this.frame = device.createBuffer({
      label: "Physical atmosphere frame",
      size: this.frameUploadBytes,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.layout = device.createBindGroupLayout({
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        {
          binding: 2,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "2d" },
        },
        {
          binding: 3,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "3d" },
        },
        {
          binding: 4,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "3d" },
        },
        { binding: 5, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "float" } },
        {
          binding: 19,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        { binding: 6, visibility: GPUShaderStage.COMPUTE, sampler: { type: "filtering" } },
      ],
    });
    const layout = device.createPipelineLayout({ bindGroupLayouts: [this.layout] });
    this.skyPipeline = device.createComputePipeline({
      label: "Physical atmosphere sky build",
      layout,
      compute: { module, entryPoint: "physicalSkyBuild" },
    });
    this.diffusePipeline = device.createComputePipeline({
      label: "Physical atmosphere diffuse build",
      layout: "auto",
      compute: { module: diffuseModule, entryPoint: "physicalDiffuseBuild" },
    });
    this.aerialPipeline = device.createComputePipeline({
      label: "Physical atmosphere aerial build",
      layout,
      compute: { module, entryPoint: "physicalAerialBuild" },
    });
    this.cloudLayout = device.createBindGroupLayout({
      entries: [
        { binding: 22, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "float" } },
        { binding: 24, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "unfilterable-float" } },
        {
          binding: 25,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "2d" },
        },
        { binding: 26, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 27, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        {
          binding: 9,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 10,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 21,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "float" } },
        {
          binding: 3,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "2d" },
        },
        {
          binding: 4,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba16float", viewDimension: "2d" },
        },
        { binding: 5, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "float" } },
        { binding: 6, visibility: GPUShaderStage.COMPUTE, sampler: { type: "filtering" } },
        {
          binding: 17,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
        {
          binding: 18,
          visibility: GPUShaderStage.COMPUTE,
          storageTexture: { access: "write-only", format: "rgba8unorm", viewDimension: "2d" },
        },
        {
          binding: 19,
          visibility: GPUShaderStage.COMPUTE,
          texture: { sampleType: "float", viewDimension: "3d" },
        },
      ],
    });
    const cloudLayout = device.createPipelineLayout({ bindGroupLayouts: [this.cloudLayout] });
    this.cloudSkyPipeline = device.createComputePipeline({
      label: "Physical cloud reflection sky build",
      layout: cloudLayout,
      compute: {
        module: cloudModule,
        entryPoint: "physicalCloudSkyBuild",
        constants: {
          PHYSICAL_CLOUD_COMPILED_NOISE: Number(cloudNoiseMode === "compiled"),
          PHYSICAL_CLOUD_VIEW_STEPS: cloudQuality.skySteps,
        },
      },
    });
    this.cloudViewPipeline = device.createComputePipeline({
      label: "Physical cloud camera view build",
      layout: cloudLayout,
      compute: {
        module: cloudTemporalModule,
        entryPoint: "physicalCloudTemporalBuild",
        constants: {
          PHYSICAL_CLOUD_COMPILED_NOISE: Number(cloudNoiseMode === "compiled"),
          PHYSICAL_CLOUD_VIEW_STEPS: cloudQuality.steps,
          PHYSICAL_CLOUD_FORMATION_STEPS: quality === "low" ? 64 : quality === "high" ? 1024 : 512,
          PHYSICAL_CLOUD_PHASE_SPREAD: quality === "balanced" ? 0.6 : 0,
          PHYSICAL_CLOUD_TARGET_STEP: quality === "low" ? 65 : 15,
        },
      },
    });
    this.cloudTracePipeline = device.createComputePipeline({
      label: "Trace compacted cloud rays",
      layout: cloudLayout,
      compute: {
        module: cloudTemporalModule,
        entryPoint: "physicalCloudTraceQueued",
        constants: {
          PHYSICAL_CLOUD_COMPILED_NOISE: Number(cloudNoiseMode === "compiled"),
          PHYSICAL_CLOUD_VIEW_STEPS: cloudQuality.steps,
          PHYSICAL_CLOUD_FORMATION_STEPS: quality === "low" ? 64 : quality === "high" ? 1024 : 512,
          PHYSICAL_CLOUD_PHASE_SPREAD: quality === "balanced" ? 0.6 : 0,
          PHYSICAL_CLOUD_TARGET_STEP: quality === "low" ? 65 : 15,
        },
      },
    });
    this.cloudFullPipeline = device.createComputePipeline({
      label: "Trace complete cloud view",
      layout: cloudLayout,
      compute: {
        module: cloudTemporalModule,
        entryPoint: "physicalCloudFullBuild",
        constants: {
          PHYSICAL_CLOUD_COMPILED_NOISE: Number(cloudNoiseMode === "compiled"),
          PHYSICAL_CLOUD_VIEW_STEPS: cloudQuality.steps,
          PHYSICAL_CLOUD_FORMATION_STEPS: quality === "low" ? 64 : quality === "high" ? 1024 : 512,
          PHYSICAL_CLOUD_PHASE_SPREAD: quality === "balanced" ? 0.6 : 0,
          PHYSICAL_CLOUD_TARGET_STEP: quality === "low" ? 65 : 15,
        },
      },
    });
    this.cloudShadowPipeline = device.createComputePipeline({
      label: "Shared cloud light field",
      layout: cloudLayout,
      compute: {
        module: cloudModule,
        entryPoint: "physicalCloudShadowBuild",
        constants: { PHYSICAL_CLOUD_COMPILED_NOISE: Number(cloudNoiseMode === "compiled") },
      },
    });
  }
  static async create(
    device: GPUDevice,
    options: {
      cloudNoise?: "compiled" | "reference";
      quality?: RenderQuality;
      reconstruction?: "temporal" | "full";
    } = {},
  ): Promise<PhysicalAtmosphereGpu> {
    const modules = [
      physicalAtmosphereComputeWGSL,
      physicalMultipleScatteringComputeWGSL,
      physicalCloudComputeWGSL,
      physicalCloudNoiseComputeWGSL,
      physicalCloudResolveWGSL,
      physicalCloudLightComputeWGSL,
      physicalCloudTemporalComputeWGSL,
    ].map((code, index) =>
      device.createShaderModule({ label: `Physical atmosphere lookup builder ${index}`, code }),
    );
    for (const module of modules) {
      const messages = (await module.getCompilationInfo()).messages.filter(
        (message) => message.type === "error",
      );
      if (messages.length)
        throw new Error(
          messages.map((message) => `${message.lineNum}:${message.linePos} ${message.message}`).join("\n"),
        );
    }
    return new PhysicalAtmosphereGpu(
      device,
      modules[0],
      modules[1],
      modules[2],
      modules[3],
      modules[4],
      modules[5],
      modules[6],
      options.cloudNoise ?? "compiled",
      options.quality ?? "balanced",
      options.reconstruction ?? "temporal",
    );
  }
  /** Camera globals are render-relative; formation coordinates are atmosphere-local. */
  encode(
    encoder: GPUCommandEncoder,
    globals: Float32Array,
    table: GPUBuffer,
    timestampWrites?: GPUComputePassTimestampWrites,
  ): number {
    if (globals.length < GLOBAL_FLOATS) throw new RangeError("Atmosphere requires complete frame globals");
    [32, 36, 40, 148, 64, 68, 72, 76, 52, 188, 192, 196, 200, 204, 208].forEach((offset, index) => {
      this.frameData.set(globals.subarray(offset, offset + 4), index * 4);
    });
    this.frameData.set(
      globals.subarray(CLOUD_FORMATION_GLOBAL_OFFSET, GLOBAL_FLOATS),
      CLOUD_FORMATION_FRAME_OFFSET,
    );
    // Atmospheric scattering shares cloud visibility. The camera cloud product
    // follows orientation/FOV, while reflected clouds remain world-oriented.
    const values = Array.from(this.frameData);
    const quantize = (value: number, step: number) => Math.round(value / step);
    const orientation = [16, 17, 18, 20, 21, 22, 24, 25, 26, 28, 29];
    const key = (indices: number[]) => JSON.stringify(indices.map((index) => values[index]));
    const formationState = values
      .slice(CLOUD_FORMATION_FRAME_OFFSET)
      .map((value, index) =>
        index === 1 ? quantize(value, 0.01) : index === 4 ? quantize(value, 0.005) : value,
      );
    // The clear atmosphere changes with altitude and celestial/weather light,
    // not with a horizontal camera move. Quantization bounds update lag below
    // visible angular/photometric changes during ordinary playback.
    const atmosphereKey = JSON.stringify([
      quantize(values[1] - values[13], 5),
      ...values.slice(12, 15),
      ...values.slice(4, 7).map((v) => quantize(v, 0.01)),
      quantize(values[7], 0.01),
      ...values.slice(8, 11).map((v) => quantize(v, 0.01)),
      ...values.slice(32, 35),
      ...values.slice(40, 43).map((v) => quantize(v, 0.01)),
      quantize(values[43], 0.002),
      quantize(values[44], 0.02),
    ]);
    // Wind and cloud morphology are held only for subpixel changes.
    const hasClouds =
      values[36] > 0.001 || values[50] > 0.001 || values[CLOUD_FORMATION_FRAME_OFFSET + 4] > 0.001;
    const cloudState = hasClouds
      ? [
          quantize(values[36], 0.005),
          quantize(values[37] * values[39], 0.25),
          quantize(values[38] * values[39], 0.25),
          ...values.slice(48, 52).map((v) => quantize(v, 0.01)),
          quantize(values[52], 25),
          quantize(values[53], 25),
          quantize(values[54], 25),
          ...values.slice(56, 58).map((v) => quantize(v, 0.01)),
          ...formationState,
        ]
      : [0];
    // The display projects current rays into the cached cloud basis. Rebuild
    // after one metre of parallax or three degrees of frustum exposure.
    const viewCamera = [0, 1, 2].map((index) => quantize(values[index], 1));
    const basis = globals.subarray(64, 80);
    const oldBasis = this.cloudViewBasis;
    const dotBasis = (offset: number) =>
      basis[offset] * oldBasis[offset] +
      basis[offset + 1] * oldBasis[offset + 1] +
      basis[offset + 2] * oldBasis[offset + 2];
    const viewTurned =
      !this.cloudViewBasisValid || dotBasis(0) < 0.99863 || dotBasis(4) < 0.99863 || dotBasis(8) < 0.99863;
    // Thin decks affect sky/reflections, but cannot change low-cloud columns.
    const lowFormationState = [...formationState.slice(0, 3), ...formationState.slice(8)];
    const lightGridKey = JSON.stringify([
      ...values.slice(12, 15),
      ...values.slice(32, 35),
      ...values.slice(4, 7).map((v) => quantize(v, 0.01)),
      ...values.slice(40, 43).map((v) => quantize(v, 0.01)),
      values[44] > 0.5,
      quantize(values[36], 0.02),
      quantize(values[37] * values[39], 4),
      quantize(values[38] * values[39], 4),
      quantize(values[48], 0.02),
      quantize(values[49], 0.02),
      quantize(values[51], 0.02),
      quantize(values[52], 100),
      quantize(values[53], 100),
      quantize(values[54], 100),
      quantize(values[56], 0.02),
      quantize(values[57], 0.02),
      Math.floor(values[0] / 250),
      Math.floor(values[2] / 250),
      ...lowFormationState,
    ]);
    const shadowKey = `${lightGridKey}/${Math.floor(values[0] / 125)}/${Math.floor(values[2] / 125)}`;
    // Air depends on the shared shadow field, not subpixel density drift or
    // the unshadowed upper sheet. Reuse it until that field actually changes.
    const clearKey = `${atmosphereKey}/${values[36] > 0.001}/${lightGridKey}/${quantize(values[0], 4)}/${quantize(values[2], 4)}`;
    const skyKey = `${clearKey}/${JSON.stringify(cloudState)}`;
    const aerialKey = `${clearKey}/${quantize(values[0], 1)}/${quantize(values[2], 1)}/${key(orientation)}`;
    const viewKey = `${clearKey}/${JSON.stringify(cloudState)}/${JSON.stringify(viewCamera)}/${key([28, 29])}`;
    const changedTable = this.table !== table;
    const rebuildClear = changedTable || this.clearKey !== clearKey;
    const rebuildSky = changedTable || this.skyKey !== skyKey;
    const rebuildAerial = changedTable || this.aerialKey !== aerialKey;
    const rebuildView = hasClouds && (changedTable || this.viewKey !== viewKey || viewTurned);
    if (rebuildView || !this.cloudViewBasisValid) {
      this.cloudViewBasis.set(basis);
      this.cloudViewBasisValid = true;
    }
    globals.set(this.cloudViewBasis, 212);
    const rebuildLightGrid = values[36] > 0.001 && (changedTable || this.lightGridKey !== lightGridKey);
    const rebuildShadow = changedTable || this.shadowKey !== shadowKey || rebuildLightGrid;
    if (
      !rebuildClear &&
      !rebuildSky &&
      !rebuildAerial &&
      !rebuildView &&
      !rebuildLightGrid &&
      !rebuildShadow &&
      this.bind
    )
      return 0;
    this.clearKey = clearKey;
    this.skyKey = skyKey;
    this.aerialKey = aerialKey;
    this.viewKey = viewKey;
    this.shadowKey = shadowKey;
    this.lightGridKey = lightGridKey;
    this.device.queue.writeBuffer(this.frame, 0, this.frameData as Float32Array<ArrayBuffer>);
    let reuseHistory = false;
    if (rebuildView) {
      const old = this.previousFrame;
      const reuse =
        this.reconstruction === "temporal" &&
        this.historyValid &&
        !changedTable &&
        canReuseCloudHistory(old, this.frameData);
      reuseHistory = reuse;
      this.historyIndex = 1 - this.historyIndex;
      this.historyData.set(old);
      const schedule = new Uint32Array(this.historyData.buffer, PHYSICAL_ATMOSPHERE_FRAME_FLOATS * 4, 4);
      schedule.set([this.historyPhase++ % 8, Number(reuse), cloudHistoryMaxAge(old, this.frameData), 0]);
      this.device.queue.writeBuffer(this.historyFrame, 0, this.historyData);
      this.previousFrame.set(this.frameData);
      this.historyValid = true;
    }
    if (changedTable || !this.bind) {
      this.diffuseKey = "";
      this.table = table;
      this.diffuseBind = this.device.createBindGroup({
        layout: this.diffusePipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: this.frame } },
          { binding: 1, resource: { buffer: table } },
          { binding: 2, resource: this.diffuseView },
        ],
      });
      this.bind = this.device.createBindGroup({
        layout: this.layout,
        entries: [
          { binding: 0, resource: { buffer: this.frame } },
          { binding: 1, resource: { buffer: table } },
          { binding: 2, resource: this.clearSkyView },
          { binding: 3, resource: this.aerialRadianceView },
          { binding: 4, resource: this.aerialTransmissionView },
          { binding: 5, resource: this.diffuseView },
          { binding: 6, resource: this.sampler },
          { binding: 19, resource: this.cloudLightView },
        ],
      });
      this.cloudBind = this.historyViews.map((view, index) =>
        this.device.createBindGroup({
          layout: this.cloudLayout,
          entries: [
            { binding: 22, resource: this.historyViews[1 - index] },
            { binding: 24, resource: this.momentViews[1 - index] },
            { binding: 25, resource: this.momentViews[index] },
            { binding: 26, resource: { buffer: this.historyFrame } },
            { binding: 27, resource: { buffer: this.rayQueue } },
            { binding: 9, resource: this.aerialRadianceView },
            { binding: 10, resource: this.aerialTransmissionView },
            { binding: 21, resource: this.cloudAmbientView },
            { binding: 0, resource: { buffer: this.frame } },
            { binding: 1, resource: { buffer: table } },
            { binding: 2, resource: this.clearSkyView },
            { binding: 3, resource: this.skyView },
            { binding: 4, resource: view },
            { binding: 5, resource: this.diffuseView },
            { binding: 6, resource: this.sampler },
            { binding: 17, resource: this.cloudNoiseView },
            { binding: 18, resource: this.cloudShadowView },
            { binding: 19, resource: this.cloudLightView },
          ],
        }),
      );
      this.cloudLightBind = this.device.createBindGroup({
        layout: this.cloudLightLayout,
        entries: [
          { binding: 5, resource: this.diffuseView },
          { binding: 23, resource: this.cloudAmbientView },
          { binding: 0, resource: { buffer: this.frame } },
          { binding: 1, resource: { buffer: table } },
          { binding: 6, resource: this.sampler },
          { binding: 17, resource: this.cloudNoiseView },
          { binding: 20, resource: this.cloudLightView },
        ],
      });
    }
    const groundKey = `${globals[52]}/${globals[53]}/${globals[54]}`;
    const rebuildDiffuse = this.diffuseKey !== groundKey;
    const buildAtmosphere = rebuildClear || rebuildAerial,
      buildClouds = rebuildSky || rebuildView || rebuildShadow;
    if (rebuildSky || rebuildShadow) this.lightingRevision++;
    const buildNoise = !this.noiseReady;
    const passCount =
      Number(buildNoise) +
      Number(rebuildDiffuse) +
      Number(buildAtmosphere) +
      Number(rebuildLightGrid) +
      Number(buildClouds) +
      Number(rebuildView) +
      Number(rebuildSky);
    // Clear the work queue before opening the timestamped compute interval.
    // On Metal a mid-interval clear introduces a blit encoder boundary and can
    // leave the cross-pass timestamp pair unavailable on reuse-only updates.
    if (rebuildView && reuseHistory) encoder.clearBuffer(this.rayQueue, 0, 512);
    let passIndex = 0;
    const begin = (label: string) =>
      encoder.beginComputePass({
        label,
        timestampWrites:
          timestampWrites && (passIndex === 0 || passIndex === passCount - 1)
            ? {
                querySet: timestampWrites.querySet,
                ...(passIndex === 0
                  ? { beginningOfPassWriteIndex: timestampWrites.beginningOfPassWriteIndex }
                  : {}),
                ...(passIndex === passCount - 1
                  ? { endOfPassWriteIndex: timestampWrites.endOfPassWriteIndex }
                  : {}),
              }
            : undefined,
      });
    const end = (pass: GPUComputePassEncoder) => {
      pass.end();
      passIndex++;
    };
    if (buildNoise) {
      const pass = begin("Compile procedural cloud noise");
      pass.setPipeline(this.cloudNoisePipeline);
      pass.setBindGroup(0, this.cloudNoiseBind);
      pass.dispatchWorkgroups(
        Math.ceil(PHYSICAL_CLOUD_NOISE_SIZE[0] / 4),
        Math.ceil(PHYSICAL_CLOUD_NOISE_SIZE[1] / 4),
        Math.ceil(PHYSICAL_CLOUD_NOISE_SIZE[2] / 4),
      );
      end(pass);
      this.noiseReady = true;
    }
    if (rebuildDiffuse) {
      if (!this.diffuseBind) throw new Error("Diffuse atmosphere bindings are missing");
      const pass = begin("Physical atmosphere composition/ground diffuse construction");
      pass.setPipeline(this.diffusePipeline);
      pass.setBindGroup(0, this.diffuseBind);
      pass.dispatchWorkgroups(
        Math.ceil(PHYSICAL_DIFFUSE_SIZE[0] / 8),
        Math.ceil(PHYSICAL_DIFFUSE_SIZE[1] / 8),
      );
      end(pass);
      this.diffuseKey = groundKey;
    }
    if (rebuildLightGrid) {
      if (!this.cloudLightBind) throw new Error("Cloud light bindings are missing");
      const pass = begin("Shared cloud optical depth construction");
      pass.setPipeline(this.cloudLightPipeline);
      pass.setBindGroup(0, this.cloudLightBind);
      pass.dispatchWorkgroups(
        Math.ceil(PHYSICAL_CLOUD_LIGHT_SIZE[0] / 4),
        Math.ceil(PHYSICAL_CLOUD_LIGHT_SIZE[1] / 4),
        Math.ceil(PHYSICAL_CLOUD_LIGHT_SIZE[2] / 4),
      );
      end(pass);
    }
    // Separate passes transition completed storage products to sampled use.
    if (buildAtmosphere) {
      const pass = begin("Physical atmosphere lookup construction");
      pass.setBindGroup(0, this.bind);
      if (rebuildClear) {
        pass.setPipeline(this.skyPipeline);
        pass.dispatchWorkgroups(Math.ceil(PHYSICAL_SKY_SIZE[0] / 8), Math.ceil(PHYSICAL_SKY_SIZE[1] / 8));
      }
      if (rebuildAerial) {
        pass.setPipeline(this.aerialPipeline);
        pass.dispatchWorkgroups(
          Math.ceil(PHYSICAL_AERIAL_SIZE[0] / 4),
          Math.ceil(PHYSICAL_AERIAL_SIZE[1] / 4),
          Math.ceil(PHYSICAL_AERIAL_SIZE[2] / 4),
        );
      }
      end(pass);
    }
    if (buildClouds) {
      if (!this.cloudBind.length) throw new Error("Cloud atmosphere bindings are missing");
      const pass = begin("Physical atmosphere cloud construction");
      pass.setBindGroup(0, this.cloudBind[this.historyIndex]);
      if (rebuildSky) {
        pass.setPipeline(this.cloudSkyPipeline);
        pass.dispatchWorkgroups(Math.ceil(this.skySize[0] / 8), Math.ceil(this.skySize[1] / 8));
      }
      if (rebuildView) {
        pass.setPipeline(reuseHistory ? this.cloudViewPipeline : this.cloudFullPipeline);
        pass.dispatchWorkgroups(Math.ceil(this.viewSize[0] / 8), Math.ceil(this.viewSize[1] / 8));
        if (reuseHistory) {
          pass.setPipeline(this.cloudTracePipeline);
          pass.dispatchWorkgroups(Math.ceil((this.viewSize[0] * 16) / 64), Math.ceil(this.viewSize[1] / 16));
        }
      }
      if (rebuildShadow) {
        pass.setPipeline(this.cloudShadowPipeline);
        pass.dispatchWorkgroups(
          Math.ceil(PHYSICAL_CLOUD_SHADOW_SIZE[0] / 8),
          Math.ceil(PHYSICAL_CLOUD_SHADOW_SIZE[1] / 8),
        );
      }
      end(pass);
    }
    if (rebuildView) {
      const pass = begin("Resolve physical cloud view");
      pass.setPipeline(this.cloudResolvePipeline);
      pass.setBindGroup(0, this.cloudResolveBind[this.historyIndex]);
      pass.dispatchWorkgroups(Math.ceil(this.viewSize[0] / 8), Math.ceil(this.viewSize[1] / 8));
      end(pass);
    }
    if (rebuildSky) {
      const pass = begin("Sky irradiance integration");
      pass.setPipeline(this.irradiancePipeline);
      pass.setBindGroup(0, this.irradianceGroup);
      pass.dispatchWorkgroups(9);
      this.reflection.encode(pass);
      end(pass);
    }
    return this.frameUploadBytes + (rebuildView ? this.historyData.byteLength : 0);
  }
  destroy(): void {
    for (const texture of this.textures) texture.destroy();
    this.frame.destroy();
    this.historyFrame.destroy();
    this.rayQueue.destroy();
    this.irradianceBuffer.destroy();
    this.reflection.destroy();
    this.bind = null;
    this.cloudBind = [];
    this.diffuseBind = null;
    this.diffuseKey = "";
    this.skyKey = "";
    this.clearKey = "";
    this.viewKey = "";
    this.shadowKey = "";
    this.lightGridKey = "";
    this.aerialKey = "";
    this.cloudViewBasisValid = false;
    this.table = null;
  }
}
