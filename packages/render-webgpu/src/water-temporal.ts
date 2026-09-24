import source from "./water-temporal.wgsl" with { type: "text" };
import compositeSource from "./water-temporal-composite.wgsl" with { type: "text" };

/** Quarter-pixel history with full precision identity/depth and a detail-preserving
 * correction. No full-resolution history or feedback copy is needed. */
export class WaterTemporalResolve {
  readonly byteLength: number;
  readonly historyWidth: number;
  readonly historyHeight: number;
  private colors: GPUTexture[];
  private metadata: GPUTexture[];
  private correction: GPUTexture;
  private parameters: GPUBuffer;
  private compute: GPUComputePipeline;
  private composite: GPURenderPipeline;
  private index = 0;
  private valid = false;
  private destroyed = false;
  constructor(
    private device: GPUDevice,
    readonly width: number,
    readonly height: number,
  ) {
    if (
      ![width, height].every((v) => Number.isInteger(v) && v > 0 && v <= device.limits.maxTextureDimension2D)
    )
      throw new RangeError("Invalid water history dimensions");
    this.historyWidth = Math.ceil(width / 2);
    this.historyHeight = Math.ceil(height / 2);
    const texture = (label: string, format: GPUTextureFormat) =>
      device.createTexture({
        label,
        size: [this.historyWidth, this.historyHeight],
        format,
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.STORAGE_BINDING,
      });
    this.colors = [0, 1].map((i) => texture(`Water radiance history ${i}`, "rgba16float"));
    this.metadata = [0, 1].map((i) => texture(`Water depth and identity ${i}`, "rg32float"));
    this.correction = texture("Water temporal correction", "rgba16float");
    this.parameters = device.createBuffer({
      size: 48,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.compute = device.createComputePipeline({
      label: "Water radiance history",
      layout: "auto",
      compute: { module: device.createShaderModule({ code: source }), entryPoint: "main" },
    });
    const module = device.createShaderModule({ code: compositeSource });
    this.composite = device.createRenderPipeline({
      label: "Water history detail-preserving composite",
      layout: "auto",
      vertex: { module, entryPoint: "vertexMain" },
      fragment: {
        module,
        entryPoint: "fragmentMain",
        targets: [
          {
            format: "rgba16float",
            blend: {
              color: { srcFactor: "one", dstFactor: "one", operation: "add" },
              alpha: { srcFactor: "zero", dstFactor: "one", operation: "add" },
            },
          },
        ],
      },
      primitive: { topology: "triangle-list" },
    });
    this.byteLength = this.historyWidth * this.historyHeight * 40 + 48;
  }
  reset() {
    this.valid = false;
  }
  encode(encoder: GPUCommandEncoder, color: GPUTexture, motion: GPUTexture, depth: GPUTexture) {
    if (this.destroyed) throw new Error("Water history was destroyed");
    if (
      [color, motion, depth].some(
        (t) => t.width !== this.width || t.height !== this.height || t.sampleCount !== 1,
      )
    )
      throw new Error("Water history requires matching single-sample targets");
    const read = this.index,
      write = 1 - read;
    const parameters = new Uint32Array(12);
    parameters.set([this.width, this.height, this.historyWidth, this.historyHeight, Number(this.valid)]);
    this.device.queue.writeBuffer(this.parameters, 0, parameters);
    const entries: GPUBindGroupEntry[] = [
      color,
      motion,
      depth,
      this.colors[read],
      this.metadata[read],
      this.colors[write],
      this.metadata[write],
      this.correction,
    ].map((t, binding) => ({ binding, resource: t.createView() }));
    entries.push({ binding: 8, resource: { buffer: this.parameters } });
    const pass = encoder.beginComputePass({ label: "Water radiance history" });
    pass.setPipeline(this.compute);
    pass.setBindGroup(
      0,
      this.device.createBindGroup({ layout: this.compute.getBindGroupLayout(0), entries }),
    );
    pass.dispatchWorkgroups(Math.ceil(this.historyWidth / 8), Math.ceil(this.historyHeight / 8));
    pass.end();
    const apply = encoder.beginRenderPass({
      label: "Water temporal correction",
      colorAttachments: [
        {
          view: color.createView(),
          loadOp: "load",
          storeOp: "store",
        },
      ],
    });
    apply.setPipeline(this.composite);
    apply.setBindGroup(
      0,
      this.device.createBindGroup({
        layout: this.composite.getBindGroupLayout(0),
        entries: [this.correction, this.metadata[write], motion, depth].map((t, binding) => ({
          binding,
          resource: t.createView(),
        })),
      }),
    );
    apply.draw(3);
    apply.end();
    this.index = write;
    this.valid = true;
  }
  destroy() {
    if (this.destroyed) return;
    this.destroyed = true;
    for (const t of [...this.colors, ...this.metadata, this.correction]) t.destroy();
    this.parameters.destroy();
  }
}
