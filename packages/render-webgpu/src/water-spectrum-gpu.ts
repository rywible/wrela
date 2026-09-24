import type { CompiledWaterSpectrum } from "@wrela/model";

/** Periodic, source-derived wave slopes and their second moments. No authored image assets. */
export const WATER_SPECTRUM_SIZE = 256;
export const WATER_SPECTRUM_LEVELS = 9;
export const waterSpectrumBytes =
  16 +
  6 *
    8 *
    Array.from({ length: WATER_SPECTRUM_LEVELS }, (_, mip) => (WATER_SPECTRUM_SIZE >> mip) ** 2).reduce(
      (a, b) => a + b,
      0,
    );

export function waterSpectrumAllocation(spectrum?: CompiledWaterSpectrum) {
  const size = spectrum?.realization.mapSize ?? 256;
  const layers = spectrum?.realization.layers ?? 6;
  const levels = Math.log2(size) + 1;
  const bytes =
    16 +
    layers * 8 * Array.from({ length: levels }, (_, mip) => (size >> mip) ** 2).reduce((a, b) => a + b, 0) +
    (layers === 6 ? size * size * 16 : 0);
  return { size, layers, levels, bytes };
}

export const waterSpectrumBuildWGSL = /* wgsl */ `
@group(0) @binding(0) var<storage,read> body:array<vec4f>;
@group(0) @binding(1) var<uniform> clock:vec4f;
@group(0) @binding(2) var output:texture_storage_2d_array<rgba16float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(output);if(any(id.xy>=size)||id.z>=3u){return;}
  let period=body[5][id.z];
  let p=(vec2f(id.xy)+0.5)*period/vec2f(size);
  var slope=vec2f(0.0);var height=0.0;var jac=vec3f(0.0);
  for(var i=id.z*18u;i<(id.z+1u)*18u;i++) {
    let at=u32(body[1].w)+(u32(body[1].z)-54u+i)*2u;let phase=body[at];let shape=body[at+1u];
    let angle=dot(phase.xy,p)+phase.z*clock.x+phase.w;
    let h=shape.x*sin(angle);let d=shape.x*cos(angle);
    height+=h;slope+=d*phase.xy;
    jac-=h*body[2].y*vec3f(shape.z*phase.x,shape.z*phase.y,shape.w*phase.y);
  }
  let choppy=textureNumLayers(output)>3u;
  let layer=select(id.z,id.z*2u,choppy);
  textureStore(output,vec2i(id.xy),i32(layer),vec4f(slope,dot(slope,slope),height));
  if(choppy){textureStore(output,vec2i(id.xy),i32(layer+1u),vec4f(jac,0.0));}
}`;
/** Thirty-two adjacent samples share phase rotations; exact carrier model, bounded float drift. */
export const waterSpectrumRecurrenceWGSL =
  waterSpectrumBuildWGSL.slice(0, waterSpectrumBuildWGSL.indexOf("@compute")) +
  /* wgsl */ `
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {
 let size=textureDimensions(output);let segments=size.x/32u;
 if(id.x>=segments*size.y||id.z>=3u){return;}
 let y=id.x/segments;let start=(id.x%segments)*32u;let period=body[5][id.z];
 let p=(vec2f(f32(start),f32(y))+0.5)*period/vec2f(size);
 var phases:array<vec2f,18>;var steps:array<vec2f,18>;
 for(var i=0u;i<18u;i++){
  let at=u32(body[1].w)+(u32(body[1].z)-54u+id.z*18u+i)*2u;let wave=body[at];
  let angle=dot(wave.xy,p)+wave.z*clock.x+wave.w;let step=wave.x*period/f32(size.x);
  phases[i]=vec2f(cos(angle),sin(angle));steps[i]=vec2f(cos(step),sin(step));
 }
 let choppy=textureNumLayers(output)>3u;let layer=select(id.z,id.z*2u,choppy);
 for(var x=0u;x<32u;x++){
  var slope=vec2f(0.0);var height=0.0;var jac=vec3f(0.0);
  for(var i=0u;i<18u;i++){
   let at=u32(body[1].w)+(u32(body[1].z)-54u+id.z*18u+i)*2u;let wave=body[at];let shape=body[at+1u];
   let phase=phases[i];let h=shape.x*phase.y;let d=shape.x*phase.x;
   height+=h;slope+=d*wave.xy;
   if(choppy){jac-=h*body[2].y*vec3f(shape.z*wave.x,shape.z*wave.y,shape.w*wave.y);}
   let step=steps[i];phases[i]=vec2f(phase.x*step.x-phase.y*step.y,phase.y*step.x+phase.x*step.y);
  }
  textureStore(output,vec2i(vec2u(start+x,y)),i32(layer),vec4f(slope,dot(slope,slope),height));
  if(choppy){textureStore(output,vec2i(vec2u(start+x,y)),i32(layer+1u),vec4f(jac,0.0));}
 }
}`;
const reduceWGSL = /* wgsl */ `
@group(0) @binding(0) var source:texture_2d_array<f32>;
@group(0) @binding(1) var output:texture_storage_2d_array<rgba16float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(output);if(any(id.xy>=size)||id.z>=textureNumLayers(output)){return;}
  let p=vec2i(id.xy*2u);let layer=i32(id.z);
  let sum=textureLoad(source,p,layer,0)+textureLoad(source,p+vec2i(1,0),layer,0)
    +textureLoad(source,p+vec2i(0,1),layer,0)+textureLoad(source,p+vec2i(1,1),layer,0);
  textureStore(output,vec2i(id.xy),layer,sum*0.25);
}`;

const foamWGSL = /* wgsl */ `
@group(0) @binding(0) var<storage,read> body:array<vec4f>;
@group(0) @binding(1) var<uniform> clock:vec4f;
@group(0) @binding(2) var waves:texture_2d_array<f32>;
@group(0) @binding(3) var history:texture_2d<f32>;
@group(0) @binding(4) var filtering:sampler;
@group(0) @binding(5) var output:texture_storage_2d<rgba16float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
 let size=textureDimensions(output);if(any(id.xy>=size)){return;}
 let uv=(vec2f(id.xy)+0.5)/vec2f(size);let p=uv*body[5].x;
 var jac=vec3f(1.0,0.0,1.0);var height=0.0;
 for(var band=0u;band<3u;band++){
  let coordinate=p/body[5][band];
  jac+=textureSampleLevel(waves,filtering,coordinate,i32(band*2u+1u),0.0).xyz;
  height+=textureSampleLevel(waves,filtering,coordinate,i32(band*2u),0.0).w;
 }
 let compression=1.0-smoothstep(0.68,0.96,jac.x*jac.z-jac.y*jac.y);
 let emission=compression*smoothstep(0.0,0.12,height);
 let first=u32(body[1].w)+(u32(body[1].z)-54u)*2u;
 let longHeight=textureSampleLevel(waves,filtering,uv,0,0.0).w;
 let velocity=body[8].xy+body[first+1u].zw*(-body[first].z)*longHeight*body[2].y;
 let previous=textureSampleLevel(history,filtering,uv-velocity*clock.y/body[5].x,0.0).w*clock.z;
 let dt=clock.y;let decay=exp(-dt/max(0.2,body[8].z));
 let concentration=clamp(previous*decay+(1.0-previous)*emission*(1.0-exp(-dt*3.5)),0.0,1.0);
 let baseJac=textureLoad(waves,vec2i(id.xy),1,0).xyz;
 textureStore(output,vec2i(id.xy),vec4f(baseJac,concentration));
}`;

export class WaterSpectrumPrograms {
  readonly build: GPUComputePipeline;
  readonly reduce: GPUComputePipeline;
  readonly foam: GPUComputePipeline;
  readonly foamFallback: GPUTexture;
  readonly fallback: GPUTexture;
  readonly fallbackView: GPUTextureView;
  readonly sampler: GPUSampler;
  private readonly shared = new Map<
    string,
    { gpu: WaterSpectrumGpu; body: GPUBuffer; refs: number; bytes: number }
  >();
  get bytes() {
    return [...this.shared.values()].reduce((sum, item) => sum + item.bytes, 0);
  }
  get textureBytes() {
    return [...this.shared.values()].reduce((sum, item) => sum + item.gpu.allocation.bytes - 16, 0);
  }
  acquire(key: string, spectrum: CompiledWaterSpectrum, data: Float32Array, previous?: WaterSpectrumGpu) {
    let entry = this.shared.get(key);
    if (entry && entry.gpu === previous) return entry.gpu;
    if (entry) {
      entry.refs++;
      if (previous) this.release(previous);
      return entry.gpu;
    }
    const prior = [...this.shared].find(([, value]) => value.gpu === previous);
    if (
      prior &&
      prior[1].refs === 1 &&
      prior[1].body.size === data.byteLength &&
      prior[1].gpu.allocation.bytes === waterSpectrumAllocation(spectrum).bytes
    ) {
      this.shared.delete(prior[0]);
      entry = prior[1];
      this.shared.set(key, entry);
      this.device.queue.writeBuffer(entry.body, 0, data as Float32Array<ArrayBuffer>);
      return entry.gpu;
    }
    if (previous) this.release(previous);
    const body = this.device.createBuffer({
      label: "Shared spectrum source",
      size: data.byteLength,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });
    this.device.queue.writeBuffer(body, 0, data as Float32Array<ArrayBuffer>);
    const gpu = new WaterSpectrumGpu(this, body, spectrum);
    this.shared.set(key, { gpu, body, refs: 1, bytes: gpu.allocation.bytes + data.byteLength });
    return gpu;
  }
  release(gpu: WaterSpectrumGpu) {
    for (const [key, entry] of this.shared)
      if (entry.gpu === gpu) {
        if (--entry.refs === 0) {
          entry.gpu.destroy();
          entry.body.destroy();
          this.shared.delete(key);
        }
        return;
      }
  }
  constructor(
    readonly device: GPUDevice,
    readonly synthesis: "direct" | "recurrence" = "direct",
  ) {
    this.build = device.createComputePipeline({
      label: "Water spectrum synthesis",
      layout: "auto",
      compute: {
        module: device.createShaderModule({
          code: synthesis === "recurrence" ? waterSpectrumRecurrenceWGSL : waterSpectrumBuildWGSL,
        }),
        entryPoint: "main",
      },
    });
    this.reduce = device.createComputePipeline({
      label: "Water slope moments",
      layout: "auto",
      compute: { module: device.createShaderModule({ code: reduceWGSL }), entryPoint: "main" },
    });
    this.foam = device.createComputePipeline({
      label: "Persistent advected ocean foam",
      layout: "auto",
      compute: { module: device.createShaderModule({ code: foamWGSL }), entryPoint: "main" },
    });
    this.foamFallback = device.createTexture({
      size: [1, 1],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING,
    });
    this.fallback = device.createTexture({
      size: [1, 1, 6],
      format: "rgba16float",
      usage: GPUTextureUsage.TEXTURE_BINDING,
    });
    this.fallbackView = this.fallback.createView({ dimension: "2d-array" });
    this.sampler = device.createSampler({
      minFilter: "linear",
      magFilter: "linear",
      mipmapFilter: "linear",
      addressModeU: "repeat",
      addressModeV: "repeat",
    });
  }
  destroy() {
    for (const entry of this.shared.values()) {
      entry.gpu.destroy();
      entry.body.destroy();
    }
    this.shared.clear();
    this.fallback.destroy();
    this.foamFallback.destroy();
  }
}

export class WaterSpectrumGpu {
  readonly texture: GPUTexture;
  readonly view: GPUTextureView;
  private readonly clock: GPUBuffer;
  private readonly buildGroup: GPUBindGroup;
  private readonly mipGroups: GPUBindGroup[];
  private lastKey = "";
  private lastTime = NaN;
  private sourceKey = "";
  readonly allocation;
  readonly foamView: GPUTextureView;
  private foamHistory?: GPUTexture;
  private foamOutput?: GPUTexture;
  private foamGroup?: GPUBindGroup;
  constructor(
    private readonly programs: WaterSpectrumPrograms,
    body: GPUBuffer,
    spectrum?: CompiledWaterSpectrum,
  ) {
    const device = programs.device;
    const allocation = waterSpectrumAllocation(spectrum);
    this.allocation = allocation;
    this.texture = device.createTexture({
      label: "Water slopes and unresolved variance",
      size: [allocation.size, allocation.size, allocation.layers],
      mipLevelCount: allocation.levels,
      format: "rgba16float",
      usage:
        GPUTextureUsage.TEXTURE_BINDING |
        GPUTextureUsage.STORAGE_BINDING |
        GPUTextureUsage.COPY_SRC |
        GPUTextureUsage.COPY_DST,
    });
    this.view = this.texture.createView({ dimension: "2d-array" });
    this.clock = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const mipView = (mip: number) =>
      this.texture.createView({ dimension: "2d-array", baseMipLevel: mip, mipLevelCount: 1 });
    this.buildGroup = device.createBindGroup({
      layout: programs.build.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: body } },
        { binding: 1, resource: { buffer: this.clock } },
        { binding: 2, resource: mipView(0) },
      ],
    });
    this.mipGroups = Array.from({ length: allocation.levels - 1 }, (_, mip) =>
      device.createBindGroup({
        layout: programs.reduce.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: mipView(mip) },
          { binding: 1, resource: mipView(mip + 1) },
        ],
      }),
    );
    if (allocation.layers === 6) {
      this.foamHistory = device.createTexture({
        label: "Ocean foam history",
        size: [allocation.size, allocation.size],
        format: "rgba16float",
        usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
      });
      this.foamOutput = device.createTexture({
        label: "Ocean foam step",
        size: [allocation.size, allocation.size],
        format: "rgba16float",
        usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.COPY_SRC,
      });
      this.foamView = this.foamHistory.createView();
      this.foamGroup = device.createBindGroup({
        layout: programs.foam.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: body } },
          { binding: 1, resource: { buffer: this.clock } },
          { binding: 2, resource: this.view },
          { binding: 3, resource: this.foamView },
          { binding: 4, resource: programs.sampler },
          { binding: 5, resource: this.foamOutput.createView() },
        ],
      });
    } else this.foamView = programs.foamFallback.createView();
  }
  needsUpdate(time: number, sourceKey: string) {
    return this.lastKey !== `${time}:${sourceKey}`;
  }
  encode(
    encoder: GPUCommandEncoder,
    time: number,
    sourceKey: string,
    timestampWrites?: GPUComputePassTimestampWrites,
  ) {
    const key = `${time}:${sourceKey}`;
    if (key === this.lastKey) return 0;
    this.lastKey = key;
    this.programs.device.queue.writeBuffer(
      this.clock,
      0,
      new Float32Array([
        time,
        Number.isFinite(this.lastTime) && time > this.lastTime && time - this.lastTime < 0.25
          ? time - this.lastTime
          : 1 / 60,
        Number(this.sourceKey === sourceKey && time > this.lastTime && time - this.lastTime < 0.25),
        0,
      ]),
    );
    this.lastTime = time;
    this.sourceKey = sourceKey;
    const pass = encoder.beginComputePass({ label: "Water spectrum and slope filtering", timestampWrites });
    pass.setPipeline(this.programs.build);
    pass.setBindGroup(0, this.buildGroup);
    if (this.programs.synthesis === "recurrence")
      pass.dispatchWorkgroups(Math.ceil((this.allocation.size * this.allocation.size) / 32 / 64), 1, 3);
    else pass.dispatchWorkgroups(this.allocation.size / 8, this.allocation.size / 8, 3);
    pass.end();
    if (this.foamGroup && this.foamOutput && this.foamHistory) {
      const foam = encoder.beginComputePass({ label: "Ocean foam residence and advection" });
      foam.setPipeline(this.programs.foam);
      foam.setBindGroup(0, this.foamGroup);
      foam.dispatchWorkgroups(this.allocation.size / 8, this.allocation.size / 8);
      foam.end();
      encoder.copyTextureToTexture({ texture: this.foamOutput }, { texture: this.foamHistory }, [
        this.allocation.size,
        this.allocation.size,
      ]);
      encoder.copyTextureToTexture(
        { texture: this.foamOutput },
        { texture: this.texture, origin: [0, 0, 1] },
        [this.allocation.size, this.allocation.size],
      );
    }
    const reduce = encoder.beginComputePass({ label: "Filtered wave and foam moments" });
    reduce.setPipeline(this.programs.reduce);
    for (let mip = 1; mip < this.allocation.levels; mip++) {
      reduce.setBindGroup(0, this.mipGroups[mip - 1]);
      reduce.dispatchWorkgroups(
        Math.max(1, (this.allocation.size >> mip) / 8),
        Math.max(1, (this.allocation.size >> mip) / 8),
        this.allocation.layers,
      );
    }
    reduce.end();
    return 16;
  }
  destroy() {
    this.texture.destroy();
    this.clock.destroy();
    this.foamHistory?.destroy();
    this.foamOutput?.destroy();
  }
}
