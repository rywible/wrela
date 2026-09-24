import computeSource from "./temporal.wgsl" with { type: "text" };
import displaySource from "./temporal-display.wgsl" with { type: "text" };
import resolveSource from "./temporal-resolve.wgsl" with { type: "text" };

const source = `${resolveSource}\n${computeSource}`;
/** Eight-sample low-discrepancy pixel jitter. Capture/diagnostic rendering uses zero. */
export function temporalJitter(frame: number): [number, number] {
  const radical = (n: number, base: number) => {
    let value = 0,
      weight = 1 / base;
    while (n) {
      value += (n % base) * weight;
      n = Math.floor(n / base);
      weight /= base;
    }
    return value;
  };
  const index = (frame % 8) + 1;
  return [radical(index, 2) - 0.5, radical(index, 3) - 0.5];
}
export function jitterProjection(matrix: Float32Array, frame: number, width: number, height: number) {
  const result = matrix.slice(),
    j = temporalJitter(frame);
  for (let column = 0; column < 4; column++) {
    result[column * 4] += ((2 * j[0]) / width) * matrix[column * 4 + 3];
    result[column * 4 + 1] -= ((2 * j[1]) / height) * matrix[column * 4 + 3];
  }
  return result;
}
/** Owns two scene-linear color/history pairs. No history aliases an active output. */
export class TemporalResolve {
  readonly byteLength: number;
  private colors: GPUTexture[];
  private metadata: GPUTexture[];
  private pipeline: GPUComputePipeline;
  private storagePipelines = new Map<GPUTextureFormat, GPUComputePipeline>();
  private displayPipelines = new Map<GPUTextureFormat, GPURenderPipeline>();
  private uniforms: GPUBuffer;
  private index = 0;
  private valid = false;
  constructor(
    private device: GPUDevice,
    readonly width: number,
    readonly height: number,
  ) {
    this.colors = [0, 1].map((i) =>
      device.createTexture({
        label: `Temporal color ${i}`,
        size: [width, height],
        format: "rgba16float",
        usage:
          GPUTextureUsage.STORAGE_BINDING |
          GPUTextureUsage.COPY_SRC |
          GPUTextureUsage.TEXTURE_BINDING |
          GPUTextureUsage.RENDER_ATTACHMENT,
      }),
    );
    this.metadata = [0, 1].map((i) =>
      device.createTexture({
        label: `Temporal depth and identity ${i}`,
        size: [width, height],
        format: "rg32float",
        usage:
          GPUTextureUsage.STORAGE_BINDING |
          GPUTextureUsage.COPY_SRC |
          GPUTextureUsage.TEXTURE_BINDING |
          GPUTextureUsage.RENDER_ATTACHMENT,
      }),
    );
    this.uniforms = device.createBuffer({
      size: 16,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.pipeline = device.createComputePipeline({
      label: "Temporal reconstruction",
      layout: "auto",
      compute: { module: device.createShaderModule({ code: source }), entryPoint: "main" },
    });
    this.byteLength = width * height * 32 + 16;
  }
  reset() {
    this.valid = false;
  }
  encode(
    encoder: GPUCommandEncoder,
    color: GPUTexture,
    motion: GPUTexture,
    depth: GPUTexture,
    timestampWrites?: GPUComputePassTimestampWrites,
    target?: GPUTexture,
    exposure = 1,
  ) {
    const read = this.index,
      write = 1 - read;
    let pipeline = this.pipeline;
    if (target) {
      if (
        target.width !== this.width ||
        target.height !== this.height ||
        !["rgba8unorm", "bgra8unorm"].includes(target.format)
      )
        throw new Error("Storage presentation requires a matching unorm target");
      let fused = this.storagePipelines.get(target.format);
      if (!fused) {
        const code = `${resolveSource}\n@group(0) @binding(7) var presentation:texture_storage_2d<${target.format},write>;\n${computeSource.replace("textureStore(outputMeta,vec2i(id.xy),r.metadata);", "textureStore(outputMeta,vec2i(id.xy),r.metadata); textureStore(presentation,vec2i(id.xy),displayResolved(r.color));")}`;
        fused = this.device.createComputePipeline({
          label: "Tiled temporal reconstruction and display",
          layout: "auto",
          compute: { module: this.device.createShaderModule({ code }), entryPoint: "main" },
        });
        this.storagePipelines.set(target.format, fused);
      }
      pipeline = fused;
    }
    const parameters = new ArrayBuffer(16);
    new Uint32Array(parameters).set([this.width, this.height, Number(this.valid)]);
    new Float32Array(parameters)[3] = exposure;
    this.device.queue.writeBuffer(this.uniforms, 0, parameters);
    const entries: GPUBindGroupEntry[] = [
      color,
      motion,
      depth,
      this.colors[read],
      this.metadata[read],
      this.colors[write],
      this.metadata[write],
    ].map((t, binding) => ({ binding, resource: t.createView() }));
    if (target) entries.push({ binding: 7, resource: target.createView() });
    // Accepted history taps are explicitly filtered.
    entries.push({ binding: 8, resource: { buffer: this.uniforms } });
    const pass = encoder.beginComputePass({ label: "Temporal reconstruction", timestampWrites });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, this.device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries }));
    pass.dispatchWorkgroups(Math.ceil(this.width / 8), Math.ceil(this.height / 8));
    pass.end();
    this.index = write;
    this.valid = true;
    return this.colors[write];
  }
  /** Native-resolution resolve and display share one render pass and history evaluation. */
  encodeDisplay(
    encoder: GPUCommandEncoder,
    color: GPUTexture,
    motion: GPUTexture,
    depth: GPUTexture,
    target: GPUTexture,
    exposure: number,
    timestampWrites?: GPURenderPassTimestampWrites,
  ) {
    if (target.width !== this.width || target.height !== this.height)
      throw new Error("Fused temporal display requires matching render and output resolution");
    let pipeline = this.displayPipelines.get(target.format);
    if (!pipeline) {
      const module = this.device.createShaderModule({ code: `${resolveSource}\n${displaySource}` });
      pipeline = this.device.createRenderPipeline({
        label: "Temporal reconstruction and display",
        layout: "auto",
        vertex: { module, entryPoint: "vertexMain" },
        fragment: {
          module,
          entryPoint: "fragmentMain",
          targets: [{ format: "rgba16float" }, { format: "rg32float" }, { format: target.format }],
        },
        primitive: { topology: "triangle-list" },
      });
      this.displayPipelines.set(target.format, pipeline);
    }
    const read = this.index,
      write = 1 - read;
    const parameters = new ArrayBuffer(16);
    new Uint32Array(parameters).set([this.width, this.height, Number(this.valid)]);
    new Float32Array(parameters)[3] = exposure;
    this.device.queue.writeBuffer(this.uniforms, 0, parameters);
    const entries: GPUBindGroupEntry[] = [color, motion, depth, this.colors[read], this.metadata[read]].map(
      (texture, binding) => ({ binding, resource: texture.createView() }),
    );
    entries.push({ binding: 8, resource: { buffer: this.uniforms } });
    const pass = encoder.beginRenderPass({
      label: "Temporal reconstruction and display",
      timestampWrites,
      colorAttachments: [this.colors[write], this.metadata[write], target].map((texture) => ({
        view: texture.createView(),
        loadOp: "clear",
        storeOp: "store",
        clearValue: [0, 0, 0, 0],
      })),
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, this.device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries }));
    pass.draw(3);
    pass.end();
    this.index = write;
    this.valid = true;
    return this.colors[write];
  }
  destroy() {
    for (const t of [...this.colors, ...this.metadata]) t.destroy();
    this.uniforms.destroy();
  }
}
