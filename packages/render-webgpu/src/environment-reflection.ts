/** Reusable split-sum GGX products. The source is the same sun-free physical sky
 * consumed by diffuse irradiance. Directional sunlight remains a separate term. */
export const REFLECTION_SIZE = [256, 128] as const;
export const REFLECTION_LEVELS = 8;
export const REFLECTION_LUT_SIZE = 64;
export const reflectionBytes =
  Array.from(
    { length: REFLECTION_LEVELS },
    (_, mip) => Math.max(1, REFLECTION_SIZE[0] >> mip) * Math.max(1, REFLECTION_SIZE[1] >> mip) * 8,
  ).reduce((a, b) => a + b, 0) +
  REFLECTION_LUT_SIZE ** 2 * 8;
const sampling = /* wgsl */ `
const PI:f32=3.141592653589793;
fn radicalInverse(i:u32)->f32 {return f32(reverseBits(i))*2.3283064365386963e-10;}
fn ggxHalf(u:vec2f,rough:f32)->vec3f {
 let a2=rough*rough*rough*rough;let c=sqrt((1.0-u.y)/(1.0+(a2-1.0)*u.y));
 let s=sqrt(max(0.0,1.0-c*c));let p=2.0*PI*u.x;return vec3f(s*cos(p),s*sin(p),c);
}
fn skyUV(n:vec3f)->vec2f {
 let e=asin(clamp(n.y,-1.0,1.0))/(0.5*PI);
 return vec2f(atan2(n.z,n.x)/(2.0*PI)+0.5,0.5+0.5*sign(e)*sqrt(abs(e)));
}
`;
export const reflectionBuildWGSL = /* wgsl */ `
${sampling}
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var linearSampler:sampler;
@group(0) @binding(2) var destination:texture_storage_2d<rgba16float,write>;
override ROUGHNESS:f32=0.0;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
 let size=textureDimensions(destination);if(any(id.xy>=size)){return;}
 let uv=(vec2f(id.xy)+0.5)/vec2f(size);let a=(uv.x-0.5)*2.0*PI;
 let v=uv.y*2.0-1.0;let e=sign(v)*v*v*0.5*PI;
 let n=vec3f(cos(e)*cos(a),sin(e),cos(e)*sin(a));
 if(ROUGHNESS==0.0){textureStore(destination,id.xy,vec4f(textureSampleLevel(source,linearSampler,uv,0.0).xyz,1));return;}
 let t=normalize(cross(select(vec3f(0,1,0),vec3f(1,0,0),abs(n.y)>0.95),n));let b=cross(n,t);
 var sum=vec3f(0);var weight=0.0;
 for(var i=0u;i<128u;i++){
  let h0=ggxHalf(vec2f((f32(i)+0.5)/128.0,radicalInverse(i)),ROUGHNESS);
  let h=t*h0.x+b*h0.y+n*h0.z;let l=2.0*dot(n,h)*h-n;let nl=max(dot(n,l),0.0);
  sum+=textureSampleLevel(source,linearSampler,skyUV(l),0.0).xyz*nl;weight+=nl;
 }
 textureStore(destination,id.xy,vec4f(sum/max(weight,1e-6),1));
}
`;
export const reflectionLutWGSL = /* wgsl */ `
${sampling}
@group(0) @binding(0) var destination:texture_storage_2d<rgba16float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
 let size=textureDimensions(destination);if(any(id.xy>=size)){return;}
 let uv=(vec2f(id.xy)+0.5)/vec2f(size);let nv=max(uv.x,0.001);let rough=uv.y;
 let view=vec3f(sqrt(1.0-nv*nv),0,nv);let a2=rough*rough*rough*rough;var sum=vec2f(0);var bands=vec3f(0);
 for(var i=0u;i<512u;i++){
  let h=ggxHalf(vec2f((f32(i)+0.5)/512.0,radicalInverse(i)),rough);
  // Zonal SH eigenvalues of the same N=V GGX prefilter used for the sky.
  // Constant radiance is preserved; no per-pixel sampling is introduced.
  let cosine=2.0*h.z*h.z-1.0;let weight=max(cosine,0.0);
  bands+=vec3f(1.0,cosine,0.5*(3.0*cosine*cosine-1.0))*weight;
  let vh=max(dot(view,h),0.0);let l=2.0*vh*h-view;let nl=l.z;
  if(nl>0.0){
   // Height-correlated Smith, matching brdfGGX. G*V.H / (N.H*N.V).
   let sv=sqrt(a2+(1.0-a2)*nv*nv);let sl=sqrt(a2+(1.0-a2)*nl*nl);
   let visibility=2.0*nl*vh/max(h.z*(nl*sv+nv*sl),1e-8);
   let fc=pow(1.0-vh,5.0);sum+=vec2f(1.0-fc,fc)*visibility;
  }
 }
 textureStore(destination,id.xy,vec4f(sum/512.0,bands.yz/max(bands.x,0.000001)));
}
`;
export const environmentReflectionWGSL = /* wgsl */ `
@group(0) @binding(20) var environmentReflection:texture_2d<f32>;
@group(0) @binding(21) var environmentBrdf:texture_2d<f32>;
fn physicalSkyRoughReflection(ray:vec3f,rough:f32)->vec3f {
 if(LIGHTING_ABLATION==3u){return vec3f(0);}
 let elevation=asin(clamp(ray.y,-1.0,1.0))/(0.5*ATMOSPHERE_PI);
 let uv=vec2f(atan2(ray.z,ray.x)/(2.0*ATMOSPHERE_PI)+0.5,0.5+0.5*sign(elevation)*sqrt(abs(elevation)));
 return textureSampleLevel(environmentReflection,physicalAtmosphereSampler,uv,clamp(rough,0.0,1.0)*7.0).xyz;
}
fn environmentSpecularWeight(nv:f32,rough:f32,f0:vec3f)->vec3f {
 let uv=clamp(vec2f(nv,rough),vec2f(0.5/64.0),vec2f(1.0-0.5/64.0));
 let split=textureSampleLevel(environmentBrdf,physicalAtmosphereSampler,uv,0.0).xy;
 return f0*split.x+vec3f(split.y);
}
fn environmentReflectionBands(rough:f32)->vec3f {
 return vec3f(1.0,textureSampleLevel(environmentBrdf,physicalAtmosphereSampler,vec2f(0.5,clamp(rough,0.5/64.0,1.0-0.5/64.0)),0.0).zw);
}
`;
export class EnvironmentReflectionGpu {
  readonly texture: GPUTexture;
  readonly lut: GPUTexture;
  readonly view: GPUTextureView;
  readonly lutView: GPUTextureView;
  readonly byteLength = reflectionBytes;
  private readonly pipelines: GPUComputePipeline[];
  private readonly groups: GPUBindGroup[];
  private readonly lutPipeline: GPUComputePipeline;
  private readonly lutGroup: GPUBindGroup;
  private ready = false;
  constructor(device: GPUDevice, source: GPUTextureView, sampler: GPUSampler) {
    this.texture = device.createTexture({
      label: "GGX environment reflection",
      size: [...REFLECTION_SIZE],
      mipLevelCount: REFLECTION_LEVELS,
      format: "rgba16float",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC,
    });
    this.lut = device.createTexture({
      label: "Height-correlated GGX integration",
      size: [REFLECTION_LUT_SIZE, REFLECTION_LUT_SIZE],
      format: "rgba16float",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC,
    });
    this.view = this.texture.createView();
    this.lutView = this.lut.createView();
    const module = device.createShaderModule({ label: "GGX sky prefilter", code: reflectionBuildWGSL });
    this.pipelines = Array.from({ length: REFLECTION_LEVELS }, (_, mip) =>
      device.createComputePipeline({
        label: `GGX reflection level ${mip}`,
        layout: "auto",
        compute: { module, entryPoint: "main", constants: { ROUGHNESS: mip / (REFLECTION_LEVELS - 1) } },
      }),
    );
    this.groups = this.pipelines.map((pipeline, mip) =>
      device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: source },
          { binding: 1, resource: sampler },
          { binding: 2, resource: this.texture.createView({ baseMipLevel: mip, mipLevelCount: 1 }) },
        ],
      }),
    );
    this.lutPipeline = device.createComputePipeline({
      label: "GGX environment BRDF integration",
      layout: "auto",
      compute: { module: device.createShaderModule({ code: reflectionLutWGSL }), entryPoint: "main" },
    });
    this.lutGroup = device.createBindGroup({
      layout: this.lutPipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: this.lutView }],
    });
  }
  encode(pass: GPUComputePassEncoder) {
    if (!this.ready) {
      pass.setPipeline(this.lutPipeline);
      pass.setBindGroup(0, this.lutGroup);
      pass.dispatchWorkgroups(8, 8);
      this.ready = true;
    }
    for (let mip = 0; mip < REFLECTION_LEVELS; mip++) {
      pass.setPipeline(this.pipelines[mip]);
      pass.setBindGroup(0, this.groups[mip]);
      pass.dispatchWorkgroups(
        Math.ceil(Math.max(1, REFLECTION_SIZE[0] >> mip) / 8),
        Math.ceil(Math.max(1, REFLECTION_SIZE[1] >> mip) / 8),
      );
    }
  }
  destroy() {
    this.texture.destroy();
    this.lut.destroy();
  }
}
