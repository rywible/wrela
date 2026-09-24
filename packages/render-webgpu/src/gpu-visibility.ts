import type { Bounds } from "@wrela/model";

import { INSTANCE_FLOATS } from "./batching";

export type VisibilityBatch = {
  bounds: Bounds[];
  /** Packed matrix/identity/motion records, one per conservative bound. */
  instances: Float32Array;
  indexCount: number;
  firstIndex: number;
  baseVertex?: number;
};
export type ProjectedBounds = { x0: number; y0: number; x1: number; y1: number; nearest: number };
const INSTANCE_BYTES = INSTANCE_FLOATS * 4;
const align = (size: number) => Math.ceil(size / 256) * 256;

/** Pixel coverage is rounded outwards. Behind/through the near plane and any
 * ambiguous projection stay visible; no previous-frame depth is accepted. */
export function projectVisibilityBounds(
  bounds: Bounds,
  matrix: Float32Array,
  width: number,
  height: number,
): ProjectedBounds | null {
  if (
    matrix.length !== 16 ||
    ![...matrix, ...bounds.min, ...bounds.max].every(Number.isFinite) ||
    bounds.min.some((value, axis) => value > bounds.max[axis])
  )
    return null;
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity,
    nearest = Infinity;
  for (let corner = 0; corner < 8; corner++) {
    const p = [
      bounds[corner & 1 ? "max" : "min"][0],
      bounds[corner & 2 ? "max" : "min"][1],
      bounds[corner & 4 ? "max" : "min"][2],
    ];
    const clip = [0, 1, 2, 3].map(
      (row) => matrix[row] * p[0] + matrix[row + 4] * p[1] + matrix[row + 8] * p[2] + matrix[row + 12],
    );
    const error = 128 * 2 ** -23 * Math.max(1, ...clip.map(Math.abs), ...p.map(Math.abs));
    if (clip[3] <= error || clip[2] <= error || clip[2] >= clip[3] - error) return null;
    const x = ((clip[0] / clip[3]) * 0.5 + 0.5) * width;
    const y = (0.5 - (clip[1] / clip[3]) * 0.5) * height;
    // Ratio uncertainty grows near the eye. A pixel is a minimum raster margin.
    const pixelError = 1 + (error / (clip[3] - error)) * Math.max(width, height);
    x0 = Math.min(x0, x - pixelError);
    y0 = Math.min(y0, y - pixelError);
    x1 = Math.max(x1, x + pixelError);
    y1 = Math.max(y1, y + pixelError);
    nearest = Math.min(nearest, (clip[2] - error) / (clip[3] + error));
  }
  if (x0 < 0 || y0 < 0 || x1 >= width || y1 >= height || !Number.isFinite(nearest)) return null;
  return { x0: Math.floor(x0), y0: Math.floor(y0), x1: Math.ceil(x1), y1: Math.ceil(y1), nearest };
}

export function visibilityLevels(width: number, height: number) {
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width * height > 16_777_216
  )
    throw new Error("Invalid visibility extent");
  const levels: { width: number; height: number; offset: number }[] = [];
  let offset = 0;
  for (;;) {
    levels.push({ width, height, offset });
    offset += width * height;
    if (width === 1 && height === 1) break;
    width = Math.ceil(width / 2);
    height = Math.ceil(height / 2);
  }
  return { levels, values: offset };
}
export function gpuVisibilityBytes(
  width: number,
  height: number,
  samples: number,
  maxInstances: number,
  maxBatches: number,
): number {
  const { levels, values } = visibilityLevels(width, height);
  return (
    width * height * samples * 4 +
    values * 4 +
    maxInstances * (32 + INSTANCE_BYTES * 2) +
    maxBatches * (20 + 16 + 256) +
    levels.length * 32
  );
}
/** A scheduling estimate, not measured evidence. Small/cheap workloads bypass
 * the prepass and compaction entirely. Full-screen depth reduction and multisample
 * depth traffic scale with target area even when few objects are hidden.
 * The caller also budgets occluder work. */
export function shouldUseGpuVisibility(
  instances: number,
  candidateVertexWork: number,
  occluderVertexWork: number,
  pixels = 640 * 360,
  samples = 1,
): boolean {
  return (
    instances >= 64 &&
    candidateVertexWork >= Math.max(250_000, pixels * (2 + samples)) &&
    occluderVertexWork > 0 &&
    occluderVertexWork * 8 < candidateVertexWork
  );
}

const copySource = (samples: number) => `
@group(0) @binding(0) var source: ${samples > 1 ? "texture_depth_multisampled_2d" : "texture_depth_2d"};
@group(0) @binding(1) var<storage,read_write> depths: array<f32>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id: vec3u) {
 let size = textureDimensions(source); if(any(id.xy >= size)){return;}
 var farthest = 0.0;
 for(var sample=0u; sample<${samples}u; sample++) {
  let value = textureLoad(source, vec2i(id.xy), ${samples > 1 ? "i32(sample)" : "0"});
  farthest = max(farthest, select(1.0, value, value >= 0.0 && value <= 1.0));
 }
 depths[id.y*size.x+id.x] = farthest;
}`;
const reduceSource = `
@group(0) @binding(0) var<storage,read_write> depths: array<f32>;
@group(0) @binding(1) var<uniform> source: vec4u;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id: vec3u) {
 let size = (source.xy+vec2u(1))/2u; if(any(id.xy >= size)){return;}
 var farthest=0.0;
 for(var y=0u;y<2u;y++){for(var x=0u;x<2u;x++){
  let p=id.xy*2u+vec2u(x,y);
  if(all(p<source.xy)){farthest=max(farthest,depths[source.z+p.y*source.x+p.x]);}
 }}
 depths[source.w+id.y*size.x+id.x]=farthest;
}`;
const compactSource = `
struct Item { rect:vec4u, nearest:f32, draw:u32, source:u32, visible:u32 }
struct Command { indexCount:u32, instanceCount:atomic<u32>, firstIndex:u32, baseVertex:i32, firstInstance:u32 }
@group(0) @binding(0) var<storage,read> depths:array<f32>;
@group(0) @binding(1) var<storage,read> levels:array<vec4u>;
@group(0) @binding(2) var<storage,read> items:array<Item>;
@group(0) @binding(3) var<storage,read> sourceInstances:array<f32>;
@group(0) @binding(4) var<storage,read_write> survivors:array<f32>;
@group(0) @binding(5) var<storage,read_write> commands:array<Command>;
@group(0) @binding(6) var<storage,read> draws:array<vec4u>;
@group(0) @binding(7) var<uniform> params:vec4u;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u){
 if(id.x>=params.x){return;} let item=items[id.x]; var visible=item.visible!=0u;
 if(!visible){
  let span=max(item.rect.z-item.rect.x+1u,item.rect.w-item.rect.y+1u);
  var level=0u; var scale=1u;
  while(level+1u<params.y && scale*2u<=span){level++;scale*=2u;}
  let shape=levels[level]; let low=item.rect.xy/scale; let high=item.rect.zw/scale;
  if(any(high>=shape.xy)){visible=true;}
  else {for(var y=low.y;y<=high.y;y++){for(var x=low.x;x<=high.x;x++){
   // Strict depth margin preserves ties, near contacts, and quantization.
   if(!(item.nearest>depths[shape.z+y*shape.x+x]+0.00002)){visible=true;}
  }}}
 }
 if(visible){
  let destination=atomicAdd(&commands[item.draw].instanceCount,1u);
  let offset=draws[item.draw].x+destination*${INSTANCE_FLOATS}u;
  for(var i=0u;i<${INSTANCE_FLOATS}u;i++){survivors[offset+i]=sourceInstances[item.source*${INSTANCE_FLOATS}u+i];}
 }
}`;

type VisibilityPipelines = {
  copy: GPUComputePipeline;
  reduce: GPUComputePipeline;
  compact: GPUComputePipeline;
};
const pipelineCache = new WeakMap<GPUDevice, Map<number, VisibilityPipelines>>();
/** Compile during renderer initialization, outside gameplay and the render loop. */
export async function prepareGpuVisibilityPipelines(device: GPUDevice, samples: number): Promise<void> {
  let known = pipelineCache.get(device);
  if (!known) {
    known = new Map();
    pipelineCache.set(device, known);
  }
  if (known.has(samples)) return;
  const compile = (label: string, code: string) =>
    device.createComputePipelineAsync({
      label,
      layout: "auto",
      compute: { module: device.createShaderModule({ label, code }), entryPoint: "main" },
    });
  const [copy, reduce, compact] = await Promise.all([
    compile("Copy opaque maximum sample depth", copySource(samples)),
    compile("Reduce opaque Hi-Z", reduceSource),
    compile("Cull and compact camera instances", compactSource),
  ]);
  known.set(samples, { copy, reduce, compact });
}

/** Current-frame opaque Hi-Z and per-draw survivor compaction. Camera commands
 * consume compacted records; light/shadow commands retain the original records.
 * No visibility readback participates in submission. */
export class GpuVisibility {
  readonly depthTexture: GPUTexture;
  readonly indirectBuffer: GPUBuffer;
  readonly survivorBuffer: GPUBuffer;
  readonly byteLength: number;
  readonly initialUploadBytes: number;
  uploadBytes = 0;
  private readonly depthBuffer: GPUBuffer;
  private readonly levelsBuffer: GPUBuffer;
  private readonly itemsBuffer: GPUBuffer;
  private readonly sourceBuffer: GPUBuffer;
  private readonly drawsBuffer: GPUBuffer;
  private readonly paramsBuffer: GPUBuffer;
  private readonly copyPipeline: GPUComputePipeline;
  private readonly reducePipeline: GPUComputePipeline;
  private readonly compactPipeline: GPUComputePipeline;
  private readonly copyGroup: GPUBindGroup;
  private readonly compactGroup: GPUBindGroup;
  private readonly reductions: { buffer: GPUBuffer; group: GPUBindGroup; width: number; height: number }[] =
    [];
  private readonly groups = new WeakMap<GPUBuffer, Map<string, GPUBindGroup>>();
  private offsets: { offset: number; size: number }[] = [];
  private itemCount = 0;
  private prepared = false;
  private depthReady = false;
  constructor(
    private readonly device: GPUDevice,
    readonly width: number,
    readonly height: number,
    readonly samples: number,
    readonly maxInstances: number,
    readonly maxBatches: number,
  ) {
    if (
      ![1, 4].includes(samples) ||
      !Number.isInteger(maxInstances) ||
      maxInstances < 1 ||
      maxInstances > 262144 ||
      !Number.isInteger(maxBatches) ||
      maxBatches < 1 ||
      maxBatches > maxInstances
    )
      throw new Error("Invalid visibility capacity");
    const plan = visibilityLevels(width, height);
    this.byteLength = gpuVisibilityBytes(width, height, samples, maxInstances, maxBatches);
    this.initialUploadBytes = (plan.levels.length * 2 - 1) * 16;
    const buffer = (label: string, size: number, usage: number) =>
      device.createBuffer({ label, size, usage });
    const storage = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST;
    this.depthTexture = device.createTexture({
      label: "Current opaque visibility depth",
      size: [width, height],
      sampleCount: samples,
      format: "depth32float",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.depthBuffer = buffer("Hi-Z maximum depths", plan.values * 4, GPUBufferUsage.STORAGE);
    this.levelsBuffer = buffer("Hi-Z layout", plan.levels.length * 16, storage);
    this.itemsBuffer = buffer("Projected visibility bounds", maxInstances * 32, storage);
    this.sourceBuffer = buffer("Visibility source instances", maxInstances * INSTANCE_BYTES, storage);
    this.survivorBuffer = buffer(
      "Compacted camera instances",
      maxInstances * INSTANCE_BYTES + maxBatches * 256,
      GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
    );
    this.indirectBuffer = buffer(
      "Compacted indexed draw commands",
      maxBatches * 20,
      storage | GPUBufferUsage.INDIRECT | GPUBufferUsage.COPY_SRC,
    );
    this.drawsBuffer = buffer("Visibility draw offsets", maxBatches * 16, storage);
    this.paramsBuffer = buffer("Visibility counts", 16, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
    device.queue.writeBuffer(
      this.levelsBuffer,
      0,
      new Uint32Array(plan.levels.flatMap((level) => [level.width, level.height, level.offset, 0])),
    );
    const pipeline = (label: string, code: string) =>
      device.createComputePipeline({
        label,
        layout: "auto",
        compute: { module: device.createShaderModule({ label, code }), entryPoint: "main" },
      });
    const prepared = pipelineCache.get(device)?.get(samples);
    this.copyPipeline = prepared?.copy ?? pipeline("Copy opaque maximum sample depth", copySource(samples));
    this.reducePipeline = prepared?.reduce ?? pipeline("Reduce opaque Hi-Z", reduceSource);
    this.compactPipeline = prepared?.compact ?? pipeline("Cull and compact camera instances", compactSource);
    this.copyGroup = device.createBindGroup({
      layout: this.copyPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: this.depthTexture.createView() },
        { binding: 1, resource: { buffer: this.depthBuffer } },
      ],
    });
    for (let i = 1; i < plan.levels.length; i++) {
      const previous = plan.levels[i - 1],
        level = plan.levels[i];
      const uniform = buffer("Hi-Z reduction domain", 16, GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST);
      device.queue.writeBuffer(
        uniform,
        0,
        new Uint32Array([previous.width, previous.height, previous.offset, level.offset]),
      );
      this.reductions.push({
        buffer: uniform,
        width: level.width,
        height: level.height,
        group: device.createBindGroup({
          layout: this.reducePipeline.getBindGroupLayout(0),
          entries: [
            { binding: 0, resource: { buffer: this.depthBuffer } },
            { binding: 1, resource: { buffer: uniform } },
          ],
        }),
      });
    }
    this.compactGroup = device.createBindGroup({
      layout: this.compactPipeline.getBindGroupLayout(0),
      entries: [
        this.depthBuffer,
        this.levelsBuffer,
        this.itemsBuffer,
        this.sourceBuffer,
        this.survivorBuffer,
        this.indirectBuffer,
        this.drawsBuffer,
        this.paramsBuffer,
      ].map((buffer, binding) => ({ binding, resource: { buffer } })),
    });
  }
  prepare(matrix: Float32Array, batches: VisibilityBatch[]): void {
    if (batches.length > this.maxBatches) throw new Error("Visibility draw capacity exceeded");
    const count = batches.reduce((sum, batch) => sum + batch.bounds.length, 0);
    if (count > this.maxInstances) throw new Error("Visibility instance capacity exceeded");
    const packed = new ArrayBuffer(count * 32),
      uints = new Uint32Array(packed),
      floats = new Float32Array(packed);
    const instances = new Float32Array(count * INSTANCE_FLOATS),
      commands = new Uint32Array(batches.length * 5),
      draws = new Uint32Array(batches.length * 4);
    let item = 0,
      destination = 0;
    this.offsets = [];
    for (let draw = 0; draw < batches.length; draw++) {
      const batch = batches[draw];
      if (
        batch.bounds.length === 0 ||
        batch.instances.length !== batch.bounds.length * INSTANCE_FLOATS ||
        !Number.isInteger(batch.indexCount) ||
        batch.indexCount <= 0 ||
        !Number.isInteger(batch.firstIndex) ||
        batch.firstIndex < 0
      )
        throw new Error("Invalid visibility batch");
      destination = align(destination);
      this.offsets.push({ offset: destination, size: batch.bounds.length * INSTANCE_BYTES });
      draws[draw * 4] = destination / 4;
      destination += batch.bounds.length * INSTANCE_BYTES;
      commands.set([batch.indexCount, 0, batch.firstIndex, (batch.baseVertex ?? 0) >>> 0, 0], draw * 5);
      instances.set(batch.instances, item * INSTANCE_FLOATS);
      for (const bounds of batch.bounds) {
        const projected = projectVisibilityBounds(bounds, matrix, this.width, this.height),
          offset = item * 8;
        if (projected) {
          uints.set([projected.x0, projected.y0, projected.x1, projected.y1], offset);
          floats[offset + 4] = projected.nearest;
        }
        uints[offset + 5] = draw;
        uints[offset + 6] = item;
        uints[offset + 7] = projected ? 0 : 1;
        item++;
      }
    }
    const write = (buffer: GPUBuffer, data: ArrayBuffer | Uint32Array | Float32Array) => {
      if (data.byteLength) this.device.queue.writeBuffer(buffer, 0, data as ArrayBuffer);
    };
    write(this.itemsBuffer, packed);
    write(this.sourceBuffer, instances);
    write(this.indirectBuffer, commands);
    write(this.drawsBuffer, draws);
    write(this.paramsBuffer, new Uint32Array([count, this.reductions.length + 1, 0, 0]));
    this.uploadBytes = packed.byteLength + instances.byteLength + commands.byteLength + draws.byteLength + 16;
    this.itemCount = count;
    this.prepared = true;
    this.depthReady = false;
  }
  beginDepthPass(encoder: GPUCommandEncoder): GPURenderPassEncoder {
    if (!this.prepared) throw new Error("Prepare current visibility inputs before the opaque prepass");
    this.depthReady = true;
    return encoder.beginRenderPass({
      label: "Current opaque visibility prepass",
      colorAttachments: [],
      depthStencilAttachment: {
        view: this.depthTexture.createView(),
        depthClearValue: 1,
        depthLoadOp: "clear",
        depthStoreOp: "store",
      },
    });
  }
  encode(encoder: GPUCommandEncoder): void {
    if (!this.prepared || !this.depthReady)
      throw new Error("Current-frame opaque depth is required for visibility");
    const pass = encoder.beginComputePass({ label: "Current-frame Hi-Z and survivor compaction" });
    pass.setPipeline(this.copyPipeline);
    pass.setBindGroup(0, this.copyGroup);
    pass.dispatchWorkgroups(Math.ceil(this.width / 8), Math.ceil(this.height / 8));
    pass.setPipeline(this.reducePipeline);
    for (const reduction of this.reductions) {
      pass.setBindGroup(0, reduction.group);
      pass.dispatchWorkgroups(Math.ceil(reduction.width / 8), Math.ceil(reduction.height / 8));
    }
    if (this.itemCount) {
      pass.setPipeline(this.compactPipeline);
      pass.setBindGroup(0, this.compactGroup);
      pass.dispatchWorkgroups(Math.ceil(this.itemCount / 64));
    }
    pass.end();
    this.prepared = false;
    this.depthReady = false;
  }
  cameraGroup(
    draw: number,
    layout: GPUBindGroupLayout,
    uniform: GPUBuffer,
    skin: GPUBuffer,
    thinCoverage?: GPUTextureView,
    thinSampler?: GPUSampler,
    waterBuffer?: GPUBuffer,
    waterSpectrum?: GPUTextureView,
    waterSpectrumSampler?: GPUSampler,
  ): GPUBindGroup {
    const slice = this.offsets[draw];
    if (!slice) throw new Error("Unknown visibility draw");
    const key = `${slice.offset}:${slice.size}`;
    let known = this.groups.get(uniform);
    if (!known) {
      known = new Map();
      this.groups.set(uniform, known);
    }
    let group = known.get(key);
    if (!group) {
      group = this.device.createBindGroup({
        layout,
        entries: [
          { binding: 0, resource: { buffer: uniform } },
          { binding: 1, resource: { buffer: skin } },
          { binding: 2, resource: { buffer: this.survivorBuffer, ...slice } },
          ...(waterBuffer ? [{ binding: 5, resource: { buffer: waterBuffer } }] : []),
          ...(waterSpectrum && waterSpectrumSampler
            ? [
                { binding: 6, resource: waterSpectrum },
                { binding: 7, resource: waterSpectrumSampler },
              ]
            : []),
          ...(thinCoverage && thinSampler
            ? [
                { binding: 3, resource: thinCoverage },
                { binding: 4, resource: thinSampler },
              ]
            : []),
        ],
      });
      if (known.size >= 32) known.clear();
      known.set(key, group);
    }
    return group;
  }
  indirectOffset(draw: number): number {
    return draw * 20;
  }
  destroy(): void {
    this.depthTexture.destroy();
    for (const buffer of [
      this.depthBuffer,
      this.levelsBuffer,
      this.itemsBuffer,
      this.sourceBuffer,
      this.survivorBuffer,
      this.indirectBuffer,
      this.drawsBuffer,
      this.paramsBuffer,
      ...this.reductions.map((item) => item.buffer),
    ])
      buffer.destroy();
  }
}
