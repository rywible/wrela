/** Conservative normalized-UV footprint, including odd trailing rows and columns.
 * Adjacent footprints deliberately overlap when a source pixel straddles them. */
export const waterDepthReduceWGSL = `
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var output:texture_storage_2d<r32float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u){
 let size=textureDimensions(output);if(any(id.xy>=size)){return;}
 let sourceSize=textureDimensions(source);
 let first=id.xy*sourceSize/size;
 let end=min(((id.xy+1u)*sourceSize+size-1u)/size,sourceSize);
 var depth=1.0;
 for(var y=first.y;y<end.y;y++){for(var x=first.x;x<end.x;x++){
  depth=min(depth,textureLoad(source,vec2i(vec2u(x,y)),0).r);
 }}
 textureStore(output,vec2i(id.xy),vec4f(depth,0,0,0));
}`;
const resolvePipelines = new WeakMap<GPUDevice, Map<number, GPUComputePipeline>>();

/** An immutable opaque-pass snapshot. Water samples these textures while the
 * main color/depth attachments remain writable, avoiding attachment feedback. */
export class WaterTransportGpu {
  readonly colorView: GPUTextureView;
  readonly depthView: GPUTextureView;
  readonly sampler: GPUSampler;
  readonly byteLength: number;
  private readonly color: GPUTexture;
  private readonly depth: GPUTexture;
  private readonly pipeline: GPUComputePipeline;
  private readonly groups = new WeakMap<GPUTexture, GPUBindGroup>();
  private destroyed = false;
  private readonly hierarchy: GPUComputePipeline;
  private readonly mipGroups: GPUBindGroup[];
  readonly depthLevels: number;
  readonly colorWidth: number;

  constructor(
    private readonly device: GPUDevice,
    readonly width: number,
    readonly height: number,
    readonly samples: number,
    readonly horizontalScale: 1 | 2 = 1,
  ) {
    if (
      ![width, height].every(
        (value) => Number.isInteger(value) && value > 0 && value <= device.limits.maxTextureDimension2D,
      ) ||
      ![1, 4].includes(samples)
    )
      throw new RangeError("Invalid water transport snapshot dimensions or sample count");
    this.colorWidth = Math.ceil(width / horizontalScale);
    this.depthLevels = Math.floor(Math.log2(Math.max(width, height))) + 1;
    this.byteLength =
      this.colorWidth * height * 8 +
      Array.from(
        { length: this.depthLevels },
        (_, mip) => Math.max(1, width >> mip) * Math.max(1, height >> mip) * 4,
      ).reduce((a, b) => a + b, 0);
    this.color = device.createTexture({
      label: "Opaque color for water transport",
      size: [this.colorWidth, height],
      format: "rgba16float",
      usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.STORAGE_BINDING,
    });
    this.depth = device.createTexture({
      label: "Nearest opaque depth for water transport",
      size: [width, height],
      mipLevelCount: this.depthLevels,
      format: "r32float",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
    });
    this.hierarchy = device.createComputePipeline({
      label: "Conservative water depth hierarchy",
      layout: "auto",
      compute: { entryPoint: "main", module: device.createShaderModule({ code: waterDepthReduceWGSL }) },
    });
    this.mipGroups = Array.from({ length: this.depthLevels - 1 }, (_, mip) =>
      device.createBindGroup({
        layout: this.hierarchy.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: this.depth.createView({ baseMipLevel: mip, mipLevelCount: 1 }) },
          { binding: 1, resource: this.depth.createView({ baseMipLevel: mip + 1, mipLevelCount: 1 }) },
        ],
      }),
    );
    this.colorView = this.color.createView();
    this.depthView = this.depth.createView();
    this.sampler = device.createSampler({
      minFilter: "linear",
      magFilter: "linear",
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
    });
    let pipelines = resolvePipelines.get(device);
    if (!pipelines) {
      pipelines = new Map();
      resolvePipelines.set(device, pipelines);
    }
    let pipeline = pipelines.get(samples * 10 + horizontalScale);
    if (!pipeline) {
      const layout = device.createBindGroupLayout({
        entries: [
          {
            binding: 0,
            visibility: GPUShaderStage.COMPUTE,
            texture: { sampleType: "depth", multisampled: samples > 1 },
          },
          {
            binding: 1,
            visibility: GPUShaderStage.COMPUTE,
            storageTexture: { access: "write-only", format: "r32float" },
          },
          ...(horizontalScale === 2
            ? [
                { binding: 2, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "float" as const } },
                {
                  binding: 3,
                  visibility: GPUShaderStage.COMPUTE,
                  storageTexture: { access: "write-only" as const, format: "rgba16float" as const },
                },
              ]
            : []),
        ],
      });
      const module = device.createShaderModule({
        label: "Resolve nearest opaque depth",
        code: `
@group(0) @binding(0) var source: ${samples > 1 ? "texture_depth_multisampled_2d" : "texture_depth_2d"};
@group(0) @binding(1) var depthOutput: texture_storage_2d<r32float,write>;
${horizontalScale === 2 ? "@group(0) @binding(2) var colorSource:texture_2d<f32>; @group(0) @binding(3) var colorOutput:texture_storage_2d<rgba16float,write>;" : ""}
@compute @workgroup_size(8,8) fn resolve(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(depthOutput);
  if(any(id.xy>=size)) { return; }
  var nearest=1.0;
  ${samples > 1 ? `for(var sample=0;sample<${samples};sample++) { nearest=min(nearest,textureLoad(source,vec2i(id.xy),sample)); }` : "nearest=textureLoad(source,vec2i(id.xy),0);"}
  textureStore(depthOutput,vec2i(id.xy),vec4f(nearest,0.0,0.0,0.0));
  ${
    horizontalScale === 2
      ? `if(id.x%2u==0u){
    let other=vec2i(vec2u(min(id.x+1u,size.x-1u),id.y));
    var nextDepth=1.0;
    ${samples > 1 ? `for(var s=0;s<${samples};s++){nextDepth=min(nextDepth,textureLoad(source,other,s));}` : "nextDepth=textureLoad(source,other,0);"}
    let a=textureLoad(colorSource,vec2i(id.xy),0);let b=textureLoad(colorSource,other,0);
    let sameSurface=abs(nextDepth-nearest)<max(0.0000002,(1.0-min(nextDepth,nearest))*0.025);
    let color=select(select(a,b,nextDepth<nearest),(a+b)*0.5,sameSurface);
    textureStore(colorOutput,vec2i(vec2u(id.x/2u,id.y)),color);
  }`
      : ""
  }
}`,
      });
      pipeline = device.createComputePipeline({
        label: "Nearest depth snapshot",
        layout: device.createPipelineLayout({ bindGroupLayouts: [layout] }),
        compute: { module, entryPoint: "resolve" },
      });
      pipelines.set(samples * 10 + horizontalScale, pipeline);
    }
    this.pipeline = pipeline;
  }

  capture(encoder: GPUCommandEncoder, opaqueColor: GPUTexture, opaqueDepth: GPUTexture): void {
    if (this.destroyed) throw new Error("Water transport snapshots were destroyed");
    if (
      opaqueColor.width !== this.width ||
      opaqueColor.height !== this.height ||
      opaqueColor.sampleCount !== 1 ||
      opaqueColor.format !== "rgba16float" ||
      opaqueDepth.width !== this.width ||
      opaqueDepth.height !== this.height ||
      opaqueDepth.sampleCount !== this.samples ||
      opaqueDepth.format !== "depth32float"
    )
      throw new Error("Water transport snapshot does not match opaque attachments");
    if (this.horizontalScale === 1)
      encoder.copyTextureToTexture({ texture: opaqueColor }, { texture: this.color }, [
        this.width,
        this.height,
      ]);
    let group = this.groups.get(opaqueDepth);
    if (!group) {
      group = this.device.createBindGroup({
        layout: this.pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: opaqueDepth.createView() },
          { binding: 1, resource: this.depth.createView({ baseMipLevel: 0, mipLevelCount: 1 }) },
          ...(this.horizontalScale === 2
            ? [
                { binding: 2, resource: opaqueColor.createView() },
                { binding: 3, resource: this.colorView },
              ]
            : []),
        ],
      });
      this.groups.set(opaqueDepth, group);
    }
    const pass = encoder.beginComputePass({ label: "Resolve opaque depth for water" });
    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, group);
    pass.dispatchWorkgroups(Math.ceil(this.width / 8), Math.ceil(this.height / 8));
    pass.end();
    const reduce = encoder.beginComputePass({ label: "Water reflection depth hierarchy" });
    reduce.setPipeline(this.hierarchy);
    for (let mip = 1; mip < this.depthLevels; mip++) {
      reduce.setBindGroup(0, this.mipGroups[mip - 1]);
      reduce.dispatchWorkgroups(
        Math.ceil(Math.max(1, this.width >> mip) / 8),
        Math.ceil(Math.max(1, this.height >> mip) / 8),
      );
    }
    reduce.end();
  }

  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.color.destroy();
    this.depth.destroy();
  }
}
