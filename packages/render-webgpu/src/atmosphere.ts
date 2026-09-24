import cloudNoiseSource from "./atmosphere-cloud-noise.wgsl" with { type: "text" };
import cloudReprojectionSource from "./atmosphere-cloud-reprojection.wgsl" with { type: "text" };
import cloudTransportSource from "./atmosphere-cloud-transport.wgsl" with { type: "text" };
import cloudSource from "./atmosphere-clouds.wgsl" with { type: "text" };
import multipleSource from "./atmosphere-multiple.wgsl" with { type: "text" };
import atmosphereCore from "./atmosphere-sky.wgsl" with { type: "text" };
import { CLOUD_FRAME_FLOATS } from "./cloud-formations";
import defaultTable from "./default-atmosphere.json";
import { environmentReflectionWGSL } from "./environment-reflection";
import { irradianceBasisWGSL } from "./irradiance";

const atmosphereSource = `${cloudTransportSource}\n${atmosphereCore}\n${cloudNoiseSource}\n${cloudSource}`;

// Reuse the camera's already-built air transport for clouds in that frustum.
// Reflections and distances beyond the froxel domain keep direct integration.
const cloudAtmosphereSource = atmosphereSource
  .replace(
    `fn physicalCloudAir(world:vec3f,ray:vec3f,distance:f32,useView:bool)->PhysicalAtmosphereTransport {
  return physicalIntegrateAtmosphere(world,ray,distance*0.001,12u);
}`,
    `override PHYSICAL_CLOUD_CACHED_AIR:bool=true;
@group(0) @binding(9) var cloudAirRadiance:texture_3d<f32>;
@group(0) @binding(10) var cloudAirTransmission:texture_3d<f32>;
fn physicalCloudAir(world:vec3f,ray:vec3f,distance:f32,useView:bool)->PhysicalAtmosphereTransport {
  if(!PHYSICAL_CLOUD_CACHED_AIR||!useView||distance>ATMOSPHERE_AERIAL_DISTANCE) {
    return physicalIntegrateAtmosphere(world,ray,distance*0.001,12u);
  }
  let f=physicalFrame();let forward=dot(ray,f.forward.xyz);
  let ndc=vec2f(dot(ray,f.right.xyz)/(forward*f.viewport.x*f.viewport.y),dot(ray,f.up.xyz)/(forward*f.viewport.y));
  let uv=vec3f(ndc*vec2f(0.5,-0.5)+0.5,pow(distance/ATMOSPHERE_AERIAL_DISTANCE,1.0/3.0));
  let halfTexel=vec3f(0.5)/vec3f(textureDimensions(cloudAirRadiance));
  let bounded=clamp(uv,halfTexel,vec3f(1.0)-halfTexel);
  return PhysicalAtmosphereTransport(
    textureSampleLevel(cloudAirRadiance,physicalAtmosphereSampler,bounded,0.0).xyz,
    textureSampleLevel(cloudAirTransmission,physicalAtmosphereSampler,bounded,0.0).xyz);
}`,
  )
  .replace(
    `fn physicalCloudSkyFill(world:vec3f)->vec3f {
  return physicalIntegrateAtmosphere(world,vec3f(0.0,1.0,0.0),200.0,24u).radiance;
}`,
    `fn physicalCloudSkyFill(world:vec3f)->vec3f {
  return physicalCloudClearSky(vec3f(0.0,1.0,0.0));
}`,
  );

const defaultAtmosphere = {
  ...defaultTable,
  data: new Float32Array(defaultTable.data),
  planetCenter: defaultTable.planetCenter as [number, number, number],
};
/** Offline-compiled default composition for manually assembled renderer scenes.
 * Ordinary runtime scenes provide their cached authored composition instead. */
export function defaultPhysicalAtmosphere() {
  return defaultAtmosphere;
}

export const PHYSICAL_DIFFUSE_SIZE = [16, 32] as const;
export const PHYSICAL_DIFFUSE_ORDERS = 16;
/** Finite isotropic closure: first is unit-solar mean radiance, feedback is the
 * fraction of an isotropic field returned by passive atmosphere and ground. */
export function finiteScatteringSeries(
  first: number,
  feedback: number,
  orders = PHYSICAL_DIFFUSE_ORDERS,
): number {
  if (
    !Number.isFinite(first) ||
    first < 0 ||
    !Number.isFinite(feedback) ||
    feedback < 0 ||
    feedback > 1 ||
    !Number.isInteger(orders) ||
    orders < 1 ||
    orders > PHYSICAL_DIFFUSE_ORDERS
  )
    throw new RangeError("Invalid diffuse-scattering closure");
  let sum = first,
    term = first;
  for (let order = 1; order < orders; order++) {
    term *= feedback;
    sum += term;
  }
  return sum;
}
export const PHYSICAL_SKY_SIZE = [512, 256] as const;
/** Camera-frustum product: about one texel per 1.4 review pixels at 1024×768.
 * Fixed allocation/work cap; larger viewports upscale this bounded product. */
export const PHYSICAL_CLOUD_VIEW_SIZE = [768, 512] as const;
export const PHYSICAL_CLOUD_SHADOW_SIZE = [256, 256] as const;
export const PHYSICAL_CLOUD_SHADOW_EXTENT = 32000;
export const PHYSICAL_CLOUD_LIGHT_SIZE = [128, 128, 16] as const;
export const PHYSICAL_AERIAL_SIZE = [24, 16, 32] as const;
export const PHYSICAL_AERIAL_DISTANCE = 20000;
export const PHYSICAL_ATMOSPHERE_FRAME_FLOATS = CLOUD_FRAME_FLOATS;
/** CPU reference for scene-linear homogeneous aerial perspective. */
export function applyAerialPerspective(
  radiance: readonly [number, number, number],
  sourceRadiance: readonly [number, number, number],
  opticalDepth: number,
  extinction: number,
): [number, number, number] {
  if (
    ![...radiance, ...sourceRadiance, opticalDepth, extinction].every(Number.isFinite) ||
    opticalDepth < 0 ||
    extinction < 0
  )
    throw new RangeError("Invalid aerial-perspective input");
  const transmission = Math.exp(-opticalDepth * extinction);
  return [0, 1, 2].map((i) => radiance[i] * transmission + sourceRadiance[i] * (1 - transmission)) as [
    number,
    number,
    number,
  ];
}
export function rayleighPhase(cosine: number): number {
  if (!Number.isFinite(cosine) || Math.abs(cosine) > 1) throw new RangeError("Invalid scattering angle");
  return (3 * (1 + cosine * cosine)) / (16 * Math.PI);
}
export function miePhase(cosine: number, anisotropy: number): number {
  if (
    !Number.isFinite(cosine) ||
    Math.abs(cosine) > 1 ||
    !Number.isFinite(anisotropy) ||
    Math.abs(anisotropy) > 0.95
  )
    throw new RangeError("Invalid scattering domain");
  return (1 - anisotropy ** 2) / (4 * Math.PI * (1 + anisotropy ** 2 - 2 * anisotropy * cosine) ** 1.5);
}
const lookupWGSL = /* wgsl */ `
@group(0) @binding(8) var physicalSkyTexture:texture_2d<f32>;
@group(0) @binding(9) var physicalAerialRadianceTexture:texture_3d<f32>;
@group(0) @binding(10) var physicalAerialTransmissionTexture:texture_3d<f32>;
@group(0) @binding(11) var physicalAtmosphereSampler:sampler;
@group(0) @binding(16) var physicalCloudViewTexture:texture_2d<f32>;
@group(0) @binding(18) var physicalCloudShadowTexture:texture_2d<f32>;
fn physicalCloudShadowCached(world:vec3f,ray:vec3f)->f32 {
  if(physicalFrame().cloud.x<=0.001||ray.y<=0.0) {return 0.0;}
  let frame=physicalFrame();
  let castRay=normalize(select(frame.sun.xyz,frame.moon.xyz,frame.skyCycle.x>0.5));
  let center=floor(frame.camera.xz/125.0)*125.0;
  let uv=(world.xz-center)/32000.0+0.5;
  if(dot(ray,castRay)<0.999||any(uv<vec2f(0.0))||any(uv>vec2f(1.0))) {
    return physicalCloudShadow(world,ray);
  }
  let halfTexel=vec2f(0.5)/vec2f(textureDimensions(physicalCloudShadowTexture));
  return textureSampleLevel(physicalCloudShadowTexture,physicalAtmosphereSampler,
    clamp(uv,halfTexel,vec2f(1.0)-halfTexel),0.0).x;
}
fn physicalCachedSunTransmittance(world:vec3f)->vec3f {
  if(LIGHTING_ABLATION==2u){return vec3f(1.0); }
  let sun=normalize(physicalFrame().sun.xyz);
  return physicalSunTransmissionAt(physicalPlanetPoint(world),sun)*
    (1.0-physicalCloudShadowCached(world,sun)*0.8);
}
fn physicalSkySample(ray:vec3f)->vec4f {
  let elevation=asin(clamp(ray.y,-1.0,1.0))/(0.5*ATMOSPHERE_PI);
  let uv=vec2f(atan2(ray.z,ray.x)/(2.0*ATMOSPHERE_PI)+0.5,0.5+0.5*sign(elevation)*sqrt(abs(elevation)));
  return textureSampleLevel(physicalSkyTexture,physicalAtmosphereSampler,uv,0.0);
}
fn physicalSkyLookup(ray:vec3f)->vec3f {return physicalSkySample(ray).xyz;}
fn physicalSkyWithoutSun(world:vec3f,ray:vec3f)->vec3f { return physicalSkyLookup(ray); }
${environmentReflectionWGSL}
@group(0) @binding(12) var<storage,read> skyIrradiance:array<vec4f>;
${irradianceBasisWGSL}
fn physicalDiffuseSky(normal:vec3f)->vec3f {
 var value=vec3f(0.0);
 for(var i=0u;i<9u;i++){value+=skyIrradiance[i].xyz*irradianceBasis(normal,i);}
 return max(value,vec3f(0.0));
}
fn physicalStarHash(cell:vec2u)->u32 {
  var h=cell.x*0x9e3779b9u+cell.y*0x85ebca6bu+0x27d4eb2du;
  h=(h^(h>>16u))*0x7feb352du;
  h=(h^(h>>15u))*0x846ca68bu;
  return h^(h>>16u);
}
// Cube-face cells avoid the pole singularity of a longitude/latitude star map.
fn physicalStars(ray:vec3f)->vec3f {
  if(g.skyCycle.x<=0.001||ray.y<=0.0) {return vec3f(0.0);}
  let latitude=g.skyCycle.z;
  let north=g.skyCycle.w;
  let pole=vec3f(cos(latitude)*sin(north),sin(latitude),cos(latitude)*cos(north));
  let angle=g.skyCycle.y;
  let starRay=ray*cos(angle)+cross(pole,ray)*sin(angle)+pole*dot(pole,ray)*(1.0-cos(angle));
  let axis=abs(starRay);
  var face=0u;
  var uv=vec2f(0.0);
  if(axis.x>=axis.y&&axis.x>=axis.z) {
    face=select(0u,1u,starRay.x<0.0);
    uv=starRay.zy/axis.x;
  } else if(axis.y>=axis.z) {
    face=select(2u,3u,starRay.y<0.0);
    uv=starRay.xz/axis.y;
  } else {
    face=select(4u,5u,starRay.z<0.0);
    uv=starRay.xy/axis.z;
  }
  let grid=(uv*0.5+0.5)*256.0;
  let hash=physicalStarHash(vec2u(floor(grid))+vec2u(face*347u,face*719u));
  if((hash&4095u)>13u) {return vec3f(0.0);}
  let center=vec2f(0.2+0.6*f32((hash>>10u)&255u)/255.0,
    0.2+0.6*f32((hash>>18u)&255u)/255.0);
  let disk=1.0-smoothstep(0.04,0.26,length(fract(grid)-center));
  let brightness=0.035+0.14*f32((hash>>26u)&63u)/63.0;
  let color=mix(vec3f(1.0,0.82,0.68),vec3f(0.72,0.84,1.0),f32((hash>>7u)&255u)/255.0);
  return color*brightness*disk*g.skyCycle.x*smoothstep(0.0,0.18,ray.y);
}
fn physicalSky(world:vec3f,ray:vec3f)->vec3f {
  let sphere=physicalSkySample(ray);
  var sky=sphere.xyz;
  var cloudTransmission=sphere.w;
  let frame=physicalFrame();
  let forward=dot(ray,g.cloudViewForward.xyz);
  if((frame.cloud.x>0.001||frame.cloudShape.z>0.001||physicalCloudLayers().x>0.001)&&forward>0.0) {
    let ndc=vec2f(dot(ray,g.cloudViewRight.xyz)/(forward*g.cloudViewViewport.x*g.cloudViewViewport.y),dot(ray,g.cloudViewUp.xyz)/(forward*g.cloudViewViewport.y));
    let uv=ndc*vec2f(0.5,-0.5)+0.5;
    if(all(uv>=vec2f(0.0))&&all(uv<=vec2f(1.0))) {
      let halfTexel=vec2f(0.5)/vec2f(textureDimensions(physicalCloudViewTexture));
      let cloudSample=textureSampleLevel(physicalCloudViewTexture,physicalAtmosphereSampler,clamp(uv,halfTexel,vec2f(1.0)-halfTexel),0.0);
      let edge=min(min(uv.x,1.0-uv.x),min(uv.y,1.0-uv.y));
      let detailWeight=smoothstep(0.0,0.045,edge);
      sky=mix(sphere.xyz,cloudSample.xyz,detailWeight);
      cloudTransmission=mix(sphere.w,cloudSample.w,detailWeight);
    }
  }
  let radius=compiledAtmosphereTable[2].w;
  let disk=smoothstep(cos(radius+0.0001),cos(max(0.00001,radius-0.0001)),dot(ray,normalize(frame.sun.xyz)));
  if(disk>0.0) {
    sky+=frame.sunlight.xyz*frame.sun.w*physicalSunTransmissionAt(physicalPlanetPoint(world),ray)*disk*cloudTransmission/(ATMOSPHERE_PI*sin(radius)*sin(radius));
  }
  let moon=normalize(frame.moon.xyz);
  let moonRadius=0.0048;
  let moonDisk=smoothstep(cos(moonRadius+0.0003),cos(moonRadius-0.0003),dot(ray,moon));
  if(moonDisk>0.0) {
    sky+=vec3f(0.72,0.82,1.0)*frame.moon.w*physicalSunTransmissionAt(physicalPlanetPoint(world),moon)*
      moonDisk*cloudTransmission*0.05/(ATMOSPHERE_PI*sin(moonRadius)*sin(moonRadius));
  }
  let moonHalo=pow(max(0.0,dot(ray,moon)),800.0);
  sky+=vec3f(0.48,0.65,1.0)*frame.moon.w*moonHalo*(0.35+0.65*cloudTransmission);
  sky+=physicalStars(ray)*cloudTransmission;
  return sky;
}
fn physicalAerialPerspective(radiance:vec3f,origin:vec3f,end:vec3f)->vec3f {
  if(LIGHTING_ABLATION==5u){return radiance; }
  let frame=physicalFrame(); let delta=end-origin; let distance=length(delta);
  if(distance<0.0001) { return radiance; }
  let forward=dot(delta,frame.forward.xyz);
  if(forward<=0.0) { return radiance; }
  let ndc=vec2f(dot(delta,frame.right.xyz)/(forward*frame.viewport.x*frame.viewport.y),dot(delta,frame.up.xyz)/(forward*frame.viewport.y));
  let uv=vec3f(ndc*vec2f(0.5,-0.5)+0.5,pow(clamp(distance/ATMOSPHERE_AERIAL_DISTANCE,0.0,1.0),1.0/3.0));
  // The sampler wraps sky azimuth. Aerial coordinates instead clamp to texel
  // centers so offscreen endpoints can never wrap to the opposite image edge.
  let halfTexel=vec3f(0.5)/vec3f(textureDimensions(physicalAerialRadianceTexture));
  let bounded=clamp(uv,halfTexel,vec3f(1.0)-halfTexel);
  let source=textureSampleLevel(physicalAerialRadianceTexture,physicalAtmosphereSampler,bounded,0.0).xyz;
  let transmission=textureSampleLevel(physicalAerialTransmissionTexture,physicalAtmosphereSampler,bounded,0.0).xyz;
  return radiance*transmission+source;
}
`;
export function createAtmosphereWGSL(group: number, binding: number): string {
  if (group !== 0 || !Number.isInteger(binding) || binding < 0 || binding >= 32)
    throw new RangeError("Invalid shader binding");
  return `@group(${group}) @binding(${binding}) var<storage,read> compiledAtmosphereTable:array<vec4f>;
${atmosphereSource}
fn physicalFrame()->PhysicalAtmosphereFrame {
  return PhysicalAtmosphereFrame(g.camera,g.sun,g.sunlight,g.planetCenter,g.right,g.up,g.forward,g.viewport,g.ground,g.cloud,g.moon,g.skyCycle,g.cloudShape,g.frontOrigin,g.frontDirection);
}
fn physicalCloudForms()->vec4f {return g.cloudForms;}
fn physicalCloudLayers()->vec4f {return g.cloudLayers;}
fn physicalGrowth(index:u32,lobe:u32)->PhysicalCloudGrowth {return g.cloudGrowth[index*6u+lobe];}
fn physicalFormation(index:u32)->PhysicalCloudFormation {return g.formations[index];}
// Fragment entry points only sample sky/aerial products; no fragment performs
// view integration, so this unreachable common-helper dependency needs no LUT binding.
fn physicalDiffuseRadiance(point:vec3f,sun:vec3f)->vec3f { return vec3f(0.0); }
${lookupWGSL}`;
}
export const physicalAtmosphereComputeWGSL = /* wgsl */ `
@group(0) @binding(0) var<uniform> atmosphereFrame:PhysicalAtmosphereUniform;
@group(0) @binding(1) var<storage,read> compiledAtmosphereTable:array<vec4f>;
@group(0) @binding(2) var skyOutput:texture_storage_2d<rgba16float,write>;
@group(0) @binding(3) var aerialRadianceOutput:texture_storage_3d<rgba16float,write>;
@group(0) @binding(4) var aerialTransmissionOutput:texture_storage_3d<rgba16float,write>;
@group(0) @binding(5) var diffuseTexture:texture_2d<f32>;
@group(0) @binding(6) var physicalAtmosphereSampler:sampler;
${atmosphereSource}
fn physicalFrame()->PhysicalAtmosphereFrame { return atmosphereFrame.frame; }
fn physicalCloudForms()->vec4f {return atmosphereFrame.cloudForms;}
fn physicalCloudLayers()->vec4f {return atmosphereFrame.cloudLayers;}
fn physicalGrowth(index:u32,lobe:u32)->PhysicalCloudGrowth {return atmosphereFrame.cloudGrowth[index*6u+lobe];}
fn physicalFormation(index:u32)->PhysicalCloudFormation {return atmosphereFrame.formations[index];}

fn physicalDiffuseRadiance(point:vec3f,sun:vec3f)->vec3f {
  let c=compiledAtmosphereTable[0]; let radius=length(point);
  let height=clamp(radius-c.x,0.0,c.y); let cosine=clamp(dot(point/max(radius,0.001),sun),-1.0,1.0);
  let unit=vec2f(0.5+0.5*sign(cosine)*sqrt(abs(cosine)),pow(height/c.y,0.25));
  let size=vec2f(textureDimensions(diffuseTexture));
  let uv=(unit*(size-vec2f(1.0))+0.5)/size;
  return textureSampleLevel(diffuseTexture,physicalAtmosphereSampler,uv,0.0).xyz;
}
@compute @workgroup_size(8,8) fn physicalSkyBuild(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(skyOutput);
  if(any(id.xy>=size)) { return; }
  let uv=(vec2f(id.xy)+0.5)/vec2f(size); let azimuth=(uv.x-0.5)*2.0*ATMOSPHERE_PI;
  let v=uv.y*2.0-1.0; let elevation=sign(v)*v*v*0.5*ATMOSPHERE_PI;
  let ray=vec3f(cos(elevation)*cos(azimuth),sin(elevation),cos(elevation)*sin(azimuth));
  physicalShadowedAir=true;
  textureStore(skyOutput,id.xy,vec4f(physicalEvaluateSky(ray),1.0));
}
@compute @workgroup_size(4,4,4) fn physicalAerialBuild(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(aerialRadianceOutput);
  if(any(id>=size)) { return; }
  let uv=(vec3f(id)+0.5)/vec3f(size); let f=physicalFrame(); let ndc=(uv.xy*2.0-1.0)*vec2f(1.0,-1.0);
  let ray=normalize(f.forward.xyz+f.right.xyz*ndc.x*f.viewport.x*f.viewport.y+f.up.xyz*ndc.y*f.viewport.y);
  let distance=ATMOSPHERE_AERIAL_DISTANCE*uv.z*uv.z*uv.z;
  physicalShadowedAir=true;
  let transport=physicalIntegrateAtmosphere(f.camera.xyz,ray,distance*0.001,12u);
  textureStore(aerialRadianceOutput,id,vec4f(transport.radiance,1.0));
  textureStore(aerialTransmissionOutput,id,vec4f(transport.transmission,1.0));
}
`;

/** Clouds consume the completed clear-air product. The full-sphere product
 * supplies reflection/irradiance; the frustum product resolves background detail. */
export const physicalCloudComputeWGSL = /* wgsl */ `
@group(0) @binding(0) var<uniform> atmosphereFrame:PhysicalAtmosphereUniform;
@group(0) @binding(1) var<storage,read> compiledAtmosphereTable:array<vec4f>;
@group(0) @binding(2) var clearSkyTexture:texture_2d<f32>;
@group(0) @binding(3) var cloudSkyOutput:texture_storage_2d<rgba16float,write>;
@group(0) @binding(4) var cloudViewOutput:texture_storage_2d<rgba16float,write>;
@group(0) @binding(5) var diffuseTexture:texture_2d<f32>;
@group(0) @binding(6) var physicalAtmosphereSampler:sampler;
@group(0) @binding(18) var cloudShadowOutput:texture_storage_2d<rgba8unorm,write>;
${cloudAtmosphereSource}
fn physicalFrame()->PhysicalAtmosphereFrame {return atmosphereFrame.frame;}
fn physicalCloudForms()->vec4f {return atmosphereFrame.cloudForms;}
fn physicalCloudLayers()->vec4f {return atmosphereFrame.cloudLayers;}
fn physicalGrowth(index:u32,lobe:u32)->PhysicalCloudGrowth {return atmosphereFrame.cloudGrowth[index*6u+lobe];}
fn physicalFormation(index:u32)->PhysicalCloudFormation {return atmosphereFrame.formations[index];}

fn physicalDiffuseRadiance(point:vec3f,sun:vec3f)->vec3f {
  let c=compiledAtmosphereTable[0];let radius=length(point);
  let height=clamp(radius-c.x,0.0,c.y);let cosine=clamp(dot(point/max(radius,0.001),sun),-1.0,1.0);
  let unit=vec2f(0.5+0.5*sign(cosine)*sqrt(abs(cosine)),pow(height/c.y,0.25));
  let size=vec2f(textureDimensions(diffuseTexture));
  return textureSampleLevel(diffuseTexture,physicalAtmosphereSampler,(unit*(size-vec2f(1.0))+0.5)/size,0.0).xyz;
}
fn physicalCloudClearSky(ray:vec3f)->vec3f {
  let elevation=asin(clamp(ray.y,-1.0,1.0))/(0.5*ATMOSPHERE_PI);
  let uv=vec2f(atan2(ray.z,ray.x)/(2.0*ATMOSPHERE_PI)+0.5,0.5+0.5*sign(elevation)*sqrt(abs(elevation)));
  return textureSampleLevel(clearSkyTexture,physicalAtmosphereSampler,uv,0.0).xyz;
}
@compute @workgroup_size(8,8) fn physicalCloudSkyBuild(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(cloudSkyOutput);if(any(id.xy>=size)){return;}
  let uv=(vec2f(id.xy)+0.5)/vec2f(size);let azimuth=(uv.x-0.5)*2.0*ATMOSPHERE_PI;
  let v=uv.y*2.0-1.0;let elevation=sign(v)*v*v*0.5*ATMOSPHERE_PI;
  let ray=vec3f(cos(elevation)*cos(azimuth),sin(elevation),cos(elevation)*sin(azimuth));
  physicalShadowedAir=true;
  textureStore(cloudSkyOutput,id.xy,physicalCloudRadiance(atmosphereFrame.frame.camera.xyz,ray,physicalCloudClearSky(ray),physicalCloudQuadraturePhase(id.xy),false,2.0*ATMOSPHERE_PI/f32(size.x)));
}
@compute @workgroup_size(8,8) fn physicalCloudViewBuild(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(cloudViewOutput);if(any(id.xy>=size)){return;}
  let uv=(vec2f(id.xy)+0.5)/vec2f(size);let ndc=(uv*2.0-1.0)*vec2f(1.0,-1.0);let f=atmosphereFrame.frame;
  let ray=normalize(f.forward.xyz+f.right.xyz*ndc.x*f.viewport.x*f.viewport.y+f.up.xyz*ndc.y*f.viewport.y);
  physicalShadowedAir=true;
  textureStore(cloudViewOutput,id.xy,physicalCloudRadiance(f.camera.xyz,ray,physicalCloudClearSky(ray),physicalCloudQuadraturePhase(id.xy),true,2.0*f.viewport.y/f32(size.y)));
}
@compute @workgroup_size(8,8) fn physicalCloudShadowBuild(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(cloudShadowOutput);
  if(any(id.xy>=size)) {return;}
  let f=atmosphereFrame.frame;
  let center=floor(f.camera.xz/125.0)*125.0;
  let uv=(vec2f(id.xy)+0.5)/vec2f(size);
  let origin=f.planetCenter.y+compiledAtmosphereTable[0].x*1000.0+20.0;
  let world=vec3f(center.x+(uv.x-0.5)*32000.0,origin,center.y+(uv.y-0.5)*32000.0);
  let ray=normalize(select(f.sun.xyz,f.moon.xyz,f.skyCycle.x>0.5));
  if(f.cloud.x<=0.001||ray.y<=0.0) {
    textureStore(cloudShadowOutput,id.xy,vec4f(0.0,0.0,0.0,1.0));
    return;
  }
  let entry=physicalCloudLayerRange(world,ray,PHYSICAL_CLOUD_BASE,physicalCloudTop()).x;
  let depth=physicalCloudSolarDepth(world+ray*entry,ray);
  let shadow=(1.0-exp(-depth))*
    (1.0-smoothstep(25000.0,50000.0,entry));
  textureStore(cloudShadowOutput,id.xy,vec4f(shadow,shadow,shadow,1.0));
}
`;

/** One light-column field serves cloud self-shadowing and terrain shadows. */
export const physicalCloudLightComputeWGSL = /* wgsl */ `
override PHYSICAL_CLOUD_LIGHT_STEPS:u32=12u;
@group(0) @binding(0) var<uniform> atmosphereFrame:PhysicalAtmosphereUniform;
@group(0) @binding(1) var<storage,read> compiledAtmosphereTable:array<vec4f>;
@group(0) @binding(5) var diffuseTexture:texture_2d<f32>;
@group(0) @binding(23) var ambientOutput:texture_storage_3d<rgba16float,write>;
@group(0) @binding(6) var physicalAtmosphereSampler:sampler;
@group(0) @binding(20) var lightOutput:texture_storage_3d<rgba16float,write>;
${atmosphereSource}
fn physicalFrame()->PhysicalAtmosphereFrame {return atmosphereFrame.frame;}
fn physicalCloudForms()->vec4f {return atmosphereFrame.cloudForms;}
fn physicalCloudLayers()->vec4f {return atmosphereFrame.cloudLayers;}
fn physicalGrowth(index:u32,lobe:u32)->PhysicalCloudGrowth {return atmosphereFrame.cloudGrowth[index*6u+lobe];}
fn physicalFormation(index:u32)->PhysicalCloudFormation {return atmosphereFrame.formations[index];}

fn physicalDiffuseRadiance(point:vec3f,sun:vec3f)->vec3f {
  let c=compiledAtmosphereTable[0];let radius=length(point);
  let height=clamp(radius-c.x,0.0,c.y);let cosine=clamp(dot(point/max(radius,0.001),sun),-1.0,1.0);
  let unit=vec2f(0.5+0.5*sign(cosine)*sqrt(abs(cosine)),pow(height/c.y,0.25));
  let size=vec2f(textureDimensions(diffuseTexture));
  return textureSampleLevel(diffuseTexture,physicalAtmosphereSampler,(unit*(size-vec2f(1.0))+0.5)/size,0.0).xyz;
}
@compute @workgroup_size(4,4,4) fn main(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(lightOutput);
  if(any(id>=size)) {return;}
  let f=atmosphereFrame.frame;
  let center=floor(f.camera.xz/250.0)*250.0;
  let uv=(vec3f(id)+0.5)/vec3f(size);
  let base=f.planetCenter.y+compiledAtmosphereTable[0].x*1000.0;
  let horizontal=center+physicalCloudLightWorld(uv.xy);
  let height=PHYSICAL_CLOUD_BASE+uv.z*(physicalCloudTop()-PHYSICAL_CLOUD_BASE);
  var world=vec3f(horizontal.x,base+height,horizontal.y);
  world.y-=physicalCloudPosition(world).y-height;
  let ray=normalize(select(f.sun.xyz,f.moon.xyz,f.skyCycle.x>0.5));
  if(f.cloud.x<=0.001) {
    textureStore(lightOutput,id,vec4f(0.0));
    textureStore(ambientOutput,id,vec4f(0.0));
    return;
  }
  let edge=select(PHYSICAL_CLOUD_BASE,physicalCloudTop(),ray.y>=0.0);
  let path=min(16000.0,abs(edge-physicalCloudPosition(world).y)/max(0.025,abs(ray.y)));
  var depth=0.0;
  for(var i=0u;i<PHYSICAL_CLOUD_LIGHT_STEPS;i++) {
    let a=f32(i)/f32(PHYSICAL_CLOUD_LIGHT_STEPS);let b=f32(i+1u)/f32(PHYSICAL_CLOUD_LIGHT_STEPS);
    let start=path*a*a;let end=path*b*b;
    depth+=physicalCloudDensityFiltered(world+ray*(0.5*(start+end)),end-start)*(end-start);
  }
  let optical=depth*PHYSICAL_CLOUD_EXTINCTION;
  let planetPoint=physicalPlanetPoint(world);
  let solar=physicalSunTransmissionAt(planetPoint,ray);
  let visibility=physicalCloudDiffuseVisibility(world);
  let ambient=physicalDiffuseRadiance(planetPoint,ray)*visibility;
  textureStore(lightOutput,id,vec4f(optical,solar));
  textureStore(ambientOutput,id,vec4f(ambient,visibility));
}
`;

/** Publish the reconstructed view without applying a second blur. */
export const physicalCloudResolveWGSL = /* wgsl */ `
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var destination:texture_storage_2d<rgba16float,write>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(destination);
  if(any(id.xy>=size)) {return;}
  textureStore(destination,id.xy,textureLoad(source,vec2i(id.xy),0));
}
`;

/** Composition/ground dependent; neither camera nor runtime sunlight invalidates it. */
export const physicalMultipleScatteringComputeWGSL = `${atmosphereSource}\n${multipleSource}`;

/** A bounded procedural realization of the reference field, not an imported asset. */
export const PHYSICAL_CLOUD_NOISE_SIZE = [66, 66, 66] as const;
export const physicalCloudNoiseComputeWGSL = /* wgsl */ `
@group(0) @binding(0) var noiseOutput:texture_storage_3d<rgba8unorm,write>;
${cloudNoiseSource}
@compute @workgroup_size(4,4,4) fn main(@builtin(global_invocation_id) id:vec3u) {
  let size=textureDimensions(noiseOutput);if(any(id>=size)){return;}
  let point=(vec3f(id)-0.5)/8.0;
  let value=physicalCloudNoiseReference3(point);
  let cellular=physicalCloudWorleyReference3(point);
  textureStore(noiseOutput,id,vec4f(value,cellular,0.0,1.0));
}
`;

export const physicalCloudTemporalComputeWGSL = `${physicalCloudComputeWGSL}\n${cloudReprojectionSource}`;
