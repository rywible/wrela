import { type RadianceLightingField, type Vec3, validRadianceSurfaceCache } from "@wrela/model";
import { createAtmosphereWGSL } from "./atmosphere";
import sceneSource from "./scene.wgsl" with { type: "text" };

export const radianceLightingBytes = (field: RadianceLightingField) =>
  (48 + field.positions.length * 76) * 4 +
  field.transfer.byteLength +
  (field.receivers?.byteLength ?? 0) +
  ((field.receiverEmission?.length ?? 0) / 3) * 16 +
  (field.receiverEmission ? field.positions.length * 9 * 4 * 4 * 2 : 0) +
  (field.surfaceDiffuse
    ? field.surfaceDiffuse.positions.length * 29 * 16 + field.surfaceDiffuse.receivers.byteLength
    : 0);
export function packRadianceLighting(field: RadianceLightingField, origin: Vec3 = [0, 0, 0]): Float32Array {
  const count = field.positions.length;
  if (
    count > 512 ||
    !validRadianceSurfaceCache(field.surfaceDiffuse) ||
    (field.emissionScale !== undefined &&
      (!Number.isFinite(Math.fround(field.emissionScale)) || field.emissionScale < 0)) ||
    (field.receiverEmission !== undefined &&
      (field.receiverEmission.length !== ((field.receivers?.length ?? 0) / 8) * 3 ||
        !field.receiverEmission.every(Number.isFinite) ||
        !field.directEmission)) ||
    (field.directEmission !== undefined &&
      (field.directEmission.length !== count * 27 || !field.directEmission.every(Number.isFinite))) ||
    (field.receivers !== undefined &&
      (field.receivers.length % 8 !== 0 ||
        field.receivers.length > 131072 * 8 ||
        !field.receivers.every(Number.isFinite))) ||
    field.enclosed.length !== count ||
    !field.enclosed.every((v) => Number.isInteger(v) && v <= 200000) ||
    field.transfer.length !== count * 9 * 27 * 4 ||
    field.skyVisibility.length !== count * 9 ||
    field.lights.length > 8 ||
    !field.transfer.every(Number.isFinite) ||
    !field.skyVisibility.every(Number.isFinite) ||
    !origin.every(Number.isFinite) ||
    field.positions.some((p) => p.length !== 3 || !p.every(Number.isFinite)) ||
    field.lights.some(
      (l) =>
        !l.position.every(Number.isFinite) ||
        (l.range !== undefined && (!Number.isFinite(l.range) || l.range <= 0)),
    )
  )
    throw Error("Invalid compiled radiance field");
  if (field.receivers)
    for (let i = 0; i < field.receivers.length; i += 8) {
      let total = 0;
      for (let j = 0; j < 4; j++) {
        const id = field.receivers[i + j],
          w = field.receivers[i + 4 + j];
        if (!Number.isInteger(id) || id < 0 || id > count || w < 0 || w > 1 || (w > 0 && id === 0))
          throw Error("Invalid radiance receiver mixture");
        total += w;
      }
      if (Math.abs(total - 1) > 0.00001) throw Error("Invalid radiance receiver normalization");
    }
  const data = new Float32Array(radianceLightingBytes(field) / 4);
  // Negative version distinguishes this compact payload from the diagnostic GI field.
  data.set([count, 12 + count * 19, field.lights.length, -1]);
  data[12] = field.emissionScale ?? 1;
  field.lights.forEach((l, i) => {
    data.set([...l.position.map((v, a) => v - origin[a]), (l.range ?? 0) ** 2], 16 + i * 4);
  });
  field.positions.forEach((p, i) => {
    data.set([...p.map((v, a) => v - origin[a]), field.enclosed[i]], 48 + i * 76);
    for (let k = 0; k < 9; k++) data[48 + i * 76 + (10 + k) * 4 + 3] = field.skyVisibility[i * 9 + k];
  });
  data.set(field.transfer, 48 + count * 76);
  const receiverOffset = 48 + count * 76 + field.transfer.length;
  data.set([receiverOffset / 4, (field.receivers?.length ?? 0) / 8], 4);
  if (field.receivers) data.set(field.receivers, receiverOffset);
  if (field.receiverEmission) {
    const offset = receiverOffset + (field.receivers?.length ?? 0);
    data[6] = offset / 4;
    // CPU records omit padding; only the GPU storage layout needs vec4 alignment.
    for (let i = 0; i < field.receiverEmission.length / 3; i++)
      data.set(field.receiverEmission.subarray(i * 3, i * 3 + 3), offset + i * 4);
    // Shared immutable direct SH, then relit diffuse with this sharp term
    // removed. Static and moving receivers still perform one nine-term gather.
    const direct = offset + (field.receiverEmission.length / 3) * 4;
    data[7] = direct / 4;
    data[8] = direct / 4 + count * 9;
    for (let i = 0; i < count * 9; i++)
      data.set(field.directEmission!.subarray(i * 3, i * 3 + 3), direct + i * 4);
  }
  if (field.surfaceDiffuse) {
    const cache = field.surfaceDiffuse;
    const offset = data.length - cache.receivers.length - cache.positions.length * 116;
    data[9] = offset / 4;
    data[10] = cache.positions.length;
    data[11] = offset / 4 + cache.positions.length * 29;
    data[13] = cache.receivers.length / 12;
    cache.positions.forEach((p, i) => {
      data.set(
        p.map((v, a) => v - origin[a]),
        offset + i * 116,
      );
      data.set(cache.transfer.subarray(i * 108, (i + 1) * 108), offset + i * 116 + 8);
    });
    data.set(cache.receivers, data[11] * 4);
  }
  return data;
}
const basis = /* wgsl */ `
fn radianceSH(n:vec3f,k:u32)->f32 {
 switch(k){case 0u:{return 0.2820947918;}case 1u:{return 0.4886025119*n.y;}case 2u:{return 0.4886025119*n.z;}case 3u:{return 0.4886025119*n.x;}
 case 4u:{return 1.0925484306*n.x*n.y;}case 5u:{return 1.0925484306*n.y*n.z;}case 6u:{return 0.3153915653*(3.0*n.z*n.z-1.0);}
 case 7u:{return 1.0925484306*n.x*n.z;}default:{return 0.5462742153*(n.x*n.x-n.y*n.y);}}
}
`;
export const radianceRelightWGSL = `${sceneSource.slice(0, sceneSource.indexOf("struct Wave"))}
@group(0) @binding(0) var<uniform> g:Globals;
${createAtmosphereWGSL(0, 4)}
@group(0) @binding(22) var<storage,read_write> field:array<vec4f>;
${basis}
struct RelitRadiance {sky:vec3f, local:vec3f};
fn relightInputs(point:vec3f,start:u32,emissionGain:f32)->RelitRadiance {
 let uv=(point.xz-floor(g.camera.xz/125.0)*125.0)/32000.0+0.5;
 var cloud=0.0;
 if(g.cloud.x>0.001&&all(uv>=vec2f(0))&&all(uv<=vec2f(1))){cloud=textureSampleLevel(physicalCloudShadowTexture,physicalAtmosphereSampler,uv,0.0).x;}
 let sun=g.sunlight.xyz*g.sun.w*physicalSunTransmissionAt(physicalPlanetPoint(point),normalize(g.sun.xyz))*(1.0-cloud*0.8);
 let moon=vec3f(0.72,0.82,1.0)*g.moon.w*physicalSunTransmissionAt(physicalPlanetPoint(point),normalize(g.moon.xyz))*(1.0-cloud*0.8);
 var directSky=vec3f(0);var local=field[start+26u].xyz*emissionGain;
 for(var input=0u;input<9u;input++){
  let convolution=select(select(1.0,0.6666666667,input>0u),0.25,input>3u);
  let sky=skyIrradiance[input].xyz*(g.horizon.w/convolution);
  directSky+=field[start+input].xyz*sky;
  // Low-order directional input is used only for diffuse bounce; direct solar
  // visibility remains in the ordinary shadow system.
  local+=field[start+9u+input].xyz*(sky+sun*radianceSH(g.sun.xyz,input)+moon*radianceSH(g.moon.xyz,input));
 }
 for(var i=0u;i<min(u32(g.viewport.z),u32(field[0].z));i++){
  let source=field[4u+i];let light=g.points[i];
  if(distance(source.xyz,light.position.xyz)>0.002||abs(source.w-light.color.w)>0.001){continue;}
  local+=field[start+18u+i].xyz*light.color.xyz*light.position.w;
 }
 return RelitRadiance(directSky,local);
}
@compute @workgroup_size(64) fn relight(@builtin(global_invocation_id) id:vec3u) {
 if(id.y>=9u||field[0].w>=0.0){return;}
 if(id.y==0u){
  for(var i=id.x;i<u32(field[2].z);i+=512u){
   let base=u32(field[2].y)+i*29u;
   let value=relightInputs(field[base].xyz,base+2u,0.0);
   field[base+1u]=vec4f(max(value.sky+value.local,vec3f(0)),0);
  }
 }
 if(id.x>=u32(field[0].x)){return;}
 let base=12u+id.x*19u;
 let start=u32(field[0].y)+(id.x*9u+id.y)*27u;
 let result=relightInputs(field[base].xyz,start,field[3].x);
 let directSky=result.sky;let local=result.local;
 let band=select(select(1.0,0.6666666667,id.y>0u),0.25,id.y>3u);
 field[base+1u+id.y]=vec4f((directSky+local)*band,0);
 if(field[2].x>0.0){
  let direct=field[u32(field[1].w)+id.x*9u+id.y].xyz*field[3].x;
  field[u32(field[2].x)+id.x*9u+id.y]=vec4f((directSky+local-direct)*band,0);
 }
 let visibility=field[base+10u+id.y].w;
 field[base+10u+id.y]=vec4f(local,visibility);
}
`;

/** Compiled IDs replace spatial searches and BVH queries during shading. */
const diffuseGather = Array.from(
  { length: 9 },
  (_, k) => `radiance+=indirectField[diffuseBase+${k}u].xyz*radianceSH(n,${k}u);`,
).join("\n");
const reflectionGather = Array.from(
  { length: 9 },
  (_, k) =>
    `radiance+=indirectField[base+${10 + k}u]*radianceSH(ray,${k}u)*bands.${k === 0 ? "x" : k < 4 ? "y" : "z"};`,
).join("\n");
export const radianceLightingWGSL = `${basis}
fn automaticRadianceReady()->bool {return obj.radianceSettings.x>0.0&&indirectField[0].w< -0.5;}
fn automaticEnclosed(ids:vec2f)->f32 {
 // Only the compiler's whole-triangle certificate may suppress direct light.
 // A moving object's center or an analytic fallback is not such a proof.
 if(!automaticRadianceReady()||obj.radianceSettings.y<0.5||obj.radianceSettings.y>1.5){return 0.0;}
 return fract(ids.x)*2.0*obj.radianceSettings.x;
}
// Interpolation premultiplies by vertex coverage. Undo that exactly once
// before blending with fallback, or partially admitted triangles darken twice.
fn automaticDiffuseResolve(value:vec4f)->vec4f {
 if(value.w<=0.0){return vec4f(0);}
 let coverage=clamp(value.w/max(obj.radianceSettings.x,0.000001),0.0,1.0);
 return vec4f(value.xyz/max(coverage,0.000001),clamp(value.w,0.0,1.0));
}
fn radiancePairWeight(ids:vec2f)->f32 {
 let packed=fract(ids.y);
 return select(0.5,clamp((packed-0.125)*4.0,0.0,1.0),packed>=0.125);
}
struct RadianceMixture { samples:vec4f, weights:vec4f };
fn radianceMixture(ids:vec2f)->RadianceMixture {
 if(!RADIANCE_PAIR_ONLY&&obj.radianceSettings.y>1.5){return RadianceMixture(obj.radianceProbes,obj.radianceWeights);}
 let id=floor(ids.x);
 if(!RADIANCE_PAIR_ONLY&&id>=1024.0) {
  let index=u32(id)-1024u;
  if(index>=u32(indirectField[1].y)){return RadianceMixture(vec4f(0),vec4f(0));}
  let base=u32(indirectField[1].x)+index*2u;
  return RadianceMixture(indirectField[base],indirectField[base+1u]);
 }
 let w=radiancePairWeight(ids);
 return RadianceMixture(vec4f(floor(ids),0,0),vec4f(w,1.0-w,0,0));
}
fn automaticDiffuse(ids:vec2f,n:vec3f)->vec4f {
 if(!automaticRadianceReady()||LIGHTING_ABLATION==1u){return vec4f(0);}
 if(!RADIANCE_PAIR_ONLY&&obj.radianceSettings.y<1.5&&ids.x>=1024.0&&ids.y>=1.0&&ids.y<=indirectField[3].y){
  let record=u32(indirectField[2].w)+(u32(ids.y)-1u)*3u;
  let samples=indirectField[record];let weights=indirectField[record+1u];
  var value=indirectField[record+2u].xyz*indirectField[3].x;
  for(var j=0u;j<3u;j++){
   value+=indirectField[u32(indirectField[2].y)+u32(samples[j])*29u+1u].xyz*weights[j];
  }
  return vec4f(value,obj.radianceSettings.x);
 }
 var value=vec3f(0);var weight=0.0;
 let mixture=radianceMixture(ids);
 var emissionBase=0u;
 if(!RADIANCE_PAIR_ONLY&&obj.radianceSettings.y<1.5&&ids.x>=1024.0&&indirectField[1].z>0.0){
  let index=u32(floor(ids.x))-1024u;
  if(index<u32(indirectField[1].y)){emissionBase=u32(indirectField[1].z)+index;}
 }
 for(var j=0u;j<select(4u,2u,RADIANCE_PAIR_ONLY);j++){
  let w=mixture.weights[j];let sample=mixture.samples[j];
  if(w<=0.0||sample<0.5||sample>indirectField[0].x){continue;}
  var diffuseBase=13u+(u32(sample)-1u)*19u;
  if(emissionBase>0u){diffuseBase=u32(indirectField[2].x)+(u32(sample)-1u)*9u;}
  var radiance=vec3f(0);
  ${diffuseGather}
  value+=max(radiance,vec3f(0))*w;weight+=w;
 }
 if(weight==0.0){return vec4f(0);}
 var resolved=value/weight;
 if(emissionBase>0u){resolved+=indirectField[emissionBase].xyz*indirectField[3].x;}
 return vec4f(resolved,obj.radianceSettings.x);
}
fn automaticLocalReflection(ids:vec2f,ray:vec3f,rough:f32)->vec4f {
 let bands=environmentReflectionBands(rough);var value=vec4f(0);var weight=0.0;
 let mixture=radianceMixture(ids);
 for(var j=0u;j<select(4u,2u,RADIANCE_PAIR_ONLY);j++){
  let w=mixture.weights[j];let sample=mixture.samples[j];
  if(w<=0.0||sample<0.5||sample>indirectField[0].x){continue;}
  let base=12u+(u32(sample)-1u)*19u;var radiance=vec4f(0);
  ${reflectionGather}
  value+=vec4f(max(radiance.xyz,vec3f(0)),clamp(radiance.w,0.0,1.0))*w;weight+=w;
 }
 if(weight==0.0){return vec4f(0,0,0,-1);}
 value/=weight;
 return value;
}
fn automaticReflection(ids:vec2f,ray:vec3f,rough:f32,context:vec4f,occlusion:f32)->vec3f {
 if(LIGHTING_ABLATION==3u){return vec3f(0);}
 if(!automaticRadianceReady()){return compiledReflectionRadiance(context,ray,rough)*occlusion;}
 let value=automaticLocalReflection(ids,ray,rough);
 if(value.w<0.0){return compiledReflectionRadiance(context,ray,rough)*occlusion;}
 var sky=vec3f(0);if(value.w>0.000001||obj.radianceSettings.x<1.0){sky=physicalSkyRoughReflection(ray,rough);}
 let resolved=value.xyz+sky*value.w;
 if(obj.radianceSettings.x>=1.0){return resolved;}
 return mix((context.xyz+sky*context.w)*occlusion,resolved,obj.radianceSettings.x);
}
`;
