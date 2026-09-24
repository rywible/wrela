override LOCAL_SKY_VISIBILITY:bool=false;
override COMPILED_REFLECTION_CACHE:bool=false;
override RADIANCE_PAIR_ONLY:bool=false;
override WATER_BODY:bool=false;
override THIN_GLASS_TRANSPARENCY:bool=false;
const LIGHTING_ABLATION:u32=0u;
override THIN_COVERAGE_MSAA:bool=false;
struct PointLight { position:vec4f, color:vec4f };
struct Globals {
  vp: mat4x4f, lightVP: mat4x4f,
  camera: vec4f, sun: vec4f, sunlight: vec4f, sky: vec4f, horizon: vec4f,
  ground: vec4f, wind: vec4f, params: vec4f,
  right: vec4f, up: vec4f, forward: vec4f, viewport: vec4f,
  points:array<PointLight,8>,
  renderCompiler:vec4f, planetCenter:vec4f, inverseVP:mat4x4f, previousVP:mat4x4f, previousParams:vec4f, cloud:vec4f,
  moon:vec4f, skyCycle:vec4f, cloudShape:vec4f, frontOrigin:vec4f, frontDirection:vec4f,
  cloudViewRight:vec4f, cloudViewUp:vec4f, cloudViewForward:vec4f, cloudViewViewport:vec4f,
  cloudForms:vec4f, cloudLayers:vec4f, formations:array<PhysicalCloudFormation,8>,
  cloudGrowth:array<PhysicalCloudGrowth,48>,
};
struct Wave { shape: vec4f, phase: vec4f };
struct MaterialLayer { color:vec4f, properties:vec4f, detail:vec4f, origin:vec4f };
struct SurfaceLayer { color:vec4f, properties:vec4f, mask:vec4f, height:vec4f, origin:vec4f, reserved:vec4f };
struct Object {
  model: mat4x4f, color: vec4f, secondary: vec4f, material: vec4f, flags: vec4f,
  waves: array<Wave,8>,
  coordinates:vec4f, noiseOrigins:array<vec4f,4>, patternOrigins:vec4f,
  layers:array<MaterialLayer,2>,
  creature:vec4f, creatureScatter:vec4f, creatureFiber:vec4f, creatureCoat:vec4f,
  creatureOrigin:vec4f, creatureX:vec4f, creatureY:vec4f, creatureZ:vec4f,
  glintWaves:array<vec4f,2>, glintTime:vec4f, glintInverse:vec4f,
  surface:vec4f, surfaceHistory:vec4f, surfaceDirt:vec4f, surfaceDamage:vec4f, surfaceOrigin:vec4f, surfaceDetail:vec4f, surfaceDetailOrigin:vec4f,
  surfaceLayers:array<SurfaceLayer,4>,
  relief:vec4f, reliefDirection:vec4f, reliefGeometry:vec4f, reliefResidual:vec4f, reliefSlopeVariance:vec4f,
  waterField:vec4f,
  localLighting:vec4f,
  radianceProbes:vec4f, radianceSettings:vec4f, emission:vec4f, radianceWeights:vec4f,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var shadowMap: texture_depth_2d;
@group(0) @binding(2) var shadowSampler: sampler_comparison;
@group(1) @binding(0) var<uniform> obj: Object;
@group(1) @binding(1) var<storage,read> joints: array<mat4x4f>;
struct Instance { model:mat4x4f, identity:vec4f, previousModel:mat4x4f, shootLocal:mat4x4f, shootAnchor:vec4f, shootMotion:vec4f, shootSelection:vec4f };
@group(1) @binding(2) var<storage,read> instances:array<Instance>;
struct Vertex {
  @location(0) position: vec3f, @location(1) normal: vec3f,
  @location(2) color: vec3f, @location(3) joints: vec4f, @location(4) weights: vec4f,
  @location(5) sourcePosition:vec3f,
  @location(6) previousPosition:vec3f,
  @location(7) wind:vec4f,
  @location(8) thinUV:vec2f,
  @location(9) reliefPosition:vec3f,
  @location(10) reliefNormal:vec3f,
};
struct Varying {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f, @location(1) normal: vec3f,
  @location(2) color: vec4f, @location(3) bindColor: vec4f,
  @location(4) @interpolate(flat) identity:vec3f,
  @location(5) local:vec3f,
  @location(6) fiberTangent:vec4f,
  @location(7) previousClip:vec4f,
  @location(8) @interpolate(flat) history:vec4f,
  @location(9) thinUV:vec4f,
  @location(10) localNormal:vec4f,
  @location(11) backRadiance:vec4f,
  @location(12) restWorld:vec3f,
  @location(13) skyExposure:vec4f,
  @location(14) @interpolate(flat) indirectProof:vec4f,
};
fn palette(t: f32) -> vec3f { return 0.5+0.5*cos(6.283185*(vec3f(0.0,0.33,0.67)+t)); }
fn waterSampleAt(xz:vec2f,spacing:f32,time:f32)->vec3f {
  var height=obj.waves[0].phase.w;
  var slope=vec2f(0.0);
  for(var i=0u;i<u32(obj.flags.w);i++) {
    let w=obj.waves[i]; let k=6.283185/w.shape.y;
    let d=vec2f(cos(w.shape.w),sin(w.shape.w));
    let phase=k*(dot(d,xz)-w.shape.z*time)+w.phase.x;
    let attenuation=1.0-smoothstep(0.25,0.5,spacing/w.shape.y);
    height += w.shape.x*attenuation*sin(phase);
    slope += w.shape.x*attenuation*k*d*cos(phase);
  }
  return vec3f(height,slope);
}
fn waterSampleFiltered(xz:vec2f,spacing:f32)->vec3f {return waterSampleAt(xz,spacing,g.params.x);}
fn waterSample(xz:vec2f)->vec3f { return waterSampleFiltered(xz,0.0); }
// Suppress wave-normal frequencies smaller than a pixel to prevent distant specular moire.
// Geometry and physical samples remain the unfiltered analytic surface.
fn filteredWaterNormal(xz:vec2f)->vec3f {
  var slope=vec2f(0.0);
  for(var i=0u;i<u32(obj.flags.w);i++) {
    let w=obj.waves[i]; let k=6.283185/w.shape.y;
    let d=vec2f(cos(w.shape.w),sin(w.shape.w));
    let phase=k*(dot(d,xz)-w.shape.z*g.params.x)+w.phase.x;
    let footprint=length(vec2f(dpdx(phase),dpdy(phase)));
    let attenuation=1.0-smoothstep(0.2,1.2,footprint);
    slope += w.shape.x*k*d*cos(phase)*attenuation;
  }
  return normalize(vec3f(-slope.x,1.0,-slope.y));
}
fn transformedNormal(model:mat4x4f,n:vec3f)->vec3f {
  let a=model[0].xyz;let b=model[1].xyz;let c=model[2].xyz;
  let determinant=dot(a,cross(b,c));
  return normalize((cross(b,c)*n.x+cross(c,a)*n.y+cross(a,b)*n.z)*select(-1.0,1.0,determinant>=0.0));
}
fn deform(v: Vertex, model:mat4x4f,time:f32,windPhase:f32,previous:bool,record:Instance) -> Varying {
  var p = vec4f(select(v.position,v.previousPosition,previous),1.0);
  var n = vec4f(v.normal,0.0);
  var fiberTangent = vec4f(obj.creatureFiber.xyz,0.0);
  var binding = vec3f(0.28,0.33,0.42);
  if (obj.flags.z > 0.5) {
    let j = vec4u(v.joints)+vec4u(select(0u,64u,previous));
    let skin = joints[j.x]*v.weights.x+joints[j.y]*v.weights.y+joints[j.z]*v.weights.z+joints[j.w]*v.weights.w;
    p=skin*p; n=skin*n; fiberTangent=skin*fiberTangent;
    binding=palette(f32(j.x)*0.173)*v.weights.x+palette(f32(j.y)*0.173)*v.weights.y+palette(f32(j.z)*0.173)*v.weights.z+palette(f32(j.w)*0.173)*v.weights.w;
  }
  var world = (model*p).xyz;
  var normal = transformedNormal(model,n.xyz);
  let restWorld=world;
  let isShoot=record.shootSelection.x>3.5;
  let treeLocal=select(p.xyz,(record.shootLocal*p).xyz,isShoot);
  var motion=v.wind;
  if(isShoot) {
    let anchored=clamp(treeLocal.y/0.08,0.0,1.0);
    motion=vec4f(record.shootAnchor.w,min(0.25,pow(max(0.0,distance(treeLocal,record.shootAnchor.xyz)-record.shootMotion.w),1.4)*0.055)*record.shootMotion.x*anchored,
      record.shootMotion.z,min(0.035,length(p.xyz)*0.12)*record.shootMotion.y*anchored);
  }
  if (obj.material.w > 0.0) {
    let h=max(treeLocal.y,0.0);
    let amplitude=min(h*h*0.012,0.6)*clamp(obj.material.w,0.0,2.0);
    let phase=time*1.4+world.x*0.17+world.z*0.23+windPhase;
    let windSpeed=length(g.wind.xz);
    let windDirection=g.wind.xz/max(windSpeed,0.00001);
    let windStrength=min(windSpeed,10.0)/10.0;
    let instanceScale=length(model[0].xyz);
    world += vec3f(windDirection.x,0.0,windDirection.y)*amplitude*windStrength*instanceScale*sin(phase);
    let branchPhase=time*2.1+windPhase+world.x*0.17+world.z*0.23+motion.x;
    let leafPhase=time*7.3+motion.z;
    let flutterDirection=normalize(vec3f(-windDirection.y,0.35,windDirection.x));
    world += windStrength*instanceScale*clamp(obj.material.w,0.0,2.0)*(
      vec3f(windDirection.x,0.0,windDirection.y)*sin(branchPhase)*motion.y+
      flutterDirection*sin(leafPhase)*motion.w);
  }
  if (obj.flags.x > 0.5) {
    if(WATER_BODY) {
      if(obj.waterField.y>0.5){
        let effect=waterEffectPoint(v.position,v.color,time);world=effect.position;normal=effect.normal;
      }else{
      let spacing=select(max(waterBody[0].z,waterBody[0].w),v.color.r,waterBody[1].x<1.0);
      let fluid=waterBodyGrid(world.xz,select(1u,2u,previous));
      let depth=select(max(0.0,fluid.x-waterBodyGrid(world.xz,0u).x),1000.0,waterBody[1].x<1.0);
      var wave=WaterBodyWave(vec4f(0.0),vec2f(0.0),vec3f(1.0,0.0,1.0),vec3f(0.0));
      if(depth>0.003){wave=waterBodyWaves(world.xz,time,spacing);}
      normal=waterBodyBaseNormal(world.xz,previous);
      world.y=fluid.x+wave.value.x*min(1.0,depth/0.5);
      let lateral=wave.lateral;
      world+=vec3f(lateral.x,0.0,lateral.y);
      }
    } else {
    let sample=waterSampleAt(world.xz,obj.waves[0].phase.y,time);
    world.y=sample.x;
    normal=normalize(vec3f(-sample.y,1.0,-sample.z));
    }
  }
  var out:Varying;
  out.indirectProof=v.weights;
  out.skyExposure=select(vec4f(0,0,0,1),v.weights,obj.localLighting.w>=2.0);
  out.history.y=select(1.0,record.shootSelection.y,isShoot);
  out.world=world; out.restWorld=restWorld; out.normal=normal; out.color=vec4f(v.color,0); out.bindColor=vec4f(binding,0);
  out.local=select(select(v.sourcePosition,v.joints.xyz,obj.coordinates.x>1.5),v.reliefPosition,obj.relief.x>0.5);
  if(isShoot){out.local=treeLocal;}
  out.thinUV=vec4f(v.thinUV,select(v.joints.w,0.0,obj.flags.z>0.5),0);out.localNormal=vec4f(v.reliefNormal,abs(dot(normal,(model*vec4f(v.reliefNormal,0.0)).xyz)));
  out.fiberTangent=vec4f((model*fiberTangent).xyz,0);
  out.clip=g.vp*vec4f(world,1.0);
  return out;
}
@vertex fn vertexMain(v:Vertex,@builtin(instance_index) index:u32)->Varying {
 let record=instances[index];
 // Pass ownership is compiler-known. Reject shadow-only instances before
 // deformation, previous-pose evaluation and radiance gathers.
 if(record.shootSelection.x>3.5&&(u32(record.shootSelection.x)&1u)==0u){var rejected:Varying;rejected.clip=vec4f(2,2,2,1);return rejected;}
 var out=deform(v,record.model,g.params.x,g.wind.w,false,record);
  if(automaticRadianceReady()) {
    let ids=select(obj.radianceProbes,v.wind,obj.radianceSettings.y>0.5&&obj.radianceSettings.y<1.5);
    out.indirectProof=ids;
    out.history.z=automaticEnclosed(ids.xy);out.history.w=automaticEnclosed(ids.zw);
    let view=normalize(g.camera.xyz-out.world);
    let determinant=dot(cross(record.model[0].xyz,record.model[1].xyz),record.model[2].xyz);
    let frontSide=determinant*dot(out.normal,view)>=0.0;
    // A certified geometric plane has one camera-facing side for the entire
    // triangle. Smooth/deformed carriers still evaluate both sides.
    let singleSide=obj.radianceSettings.w>0.5&&obj.radianceSettings.y>0.5&&obj.radianceSettings.y<1.5;
    if((!singleSide||frontSide)&&(g.params.z<0.5||g.params.z>4.5)){out.bindColor=automaticDiffuse(ids.xy,out.normal);}
    if(!singleSide||!frontSide){out.backRadiance=automaticDiffuse(ids.zw,-out.normal);}
    if(obj.radianceSettings.z>0.5) {
      let reflectionFront=obj.radianceSettings.w<0.5||frontSide;
      let ray=reflect(-view,out.normal);
      let exposure=select(v.weights.w,clamp(length(v.weights.xyz),0.0,1.0),reflectionFront);
      var value=vec3f(0);
      if(obj.radianceSettings.z>1.5){value=automaticReflection(select(ids.zw,ids.xy,reflectionFront),ray,obj.color.w,vec4f(0,0,0,1),exposure);}
      else {
        let local=automaticLocalReflection(select(ids.zw,ids.xy,reflectionFront),ray,obj.color.w);
        value=local.xyz*obj.radianceSettings.x;
        out.localNormal.w=select(exposure,mix(exposure,local.w,obj.radianceSettings.x),local.w>=0.0);
        if(LIGHTING_ABLATION==3u){value=vec3f(0);out.localNormal.w=0.0;}
      }
      out.color.w=value.x;out.fiberTangent.w=value.y;out.thinUV.w=value.z;
    }
  }
 let prior=deform(v,record.previousModel,g.previousParams.x,g.previousParams.y,true,record);
 out.identity=record.identity.xyz;out.history.x=record.identity.w;
 out.previousClip=g.previousVP*vec4f(prior.world,1.0);
 return out;
}
// Reproject the surface through the previous camera without another carrier solve.
// Compact radiance history rejects changed depth/radiance; sheets remain reactive.
@vertex fn vertexWaterBody(v:Vertex,@builtin(instance_index) index:u32)->Varying {
 let record=instances[index];var out=deform(v,record.model,g.params.x,g.wind.w,false,record);
 out.identity=record.identity.xyz;out.history.x=record.identity.w*select(1.0,0.0,obj.waterField.y>0.5);
 out.previousClip=g.previousVP*vec4f(out.world,1.0);return out;
}
struct ShadowVarying { @builtin(position) clip:vec4f, @location(0) uv:vec3f };
@vertex fn shadowMain(v:Vertex,@builtin(instance_index) index:u32)->ShadowVarying {
 let record=instances[index];
 if(record.shootSelection.x>3.5&&(u32(record.shootSelection.x)&2u)==0u){return ShadowVarying(vec4f(2,2,2,1),vec3f(0));}
 let clip=g.lightVP*vec4f(deform(v,record.model,g.params.x,g.wind.w,false,record).world,1.0);
 return ShadowVarying(clip,vec3f(v.thinUV,select(v.joints.w,0.0,obj.flags.z>0.5)));
}
@fragment fn shadowFragment(v:ShadowVarying) {if(obj.coordinates.w>0.5&&thinCoverageReject(v.uv,v.clip.xy)){discard;}}
@fragment fn visibilityFragment(v:Varying) {if(obj.coordinates.w>0.5&&thinCoverageReject(v.thinUV.xyz,v.clip.xy)){discard;}}
@fragment fn thinDepthFragment(v:Varying)->@builtin(sample_mask) u32 {
  return thinCoverageSampleMask(v.thinUV.xyz,v.local,select(1u,4u,THIN_COVERAGE_MSAA));
}
// A periodic integer lattice avoids the large-coordinate precision loss of sine hashes.
fn hash(p:vec3f)->f32 {
  let q=vec3u((vec3i(p)%vec3i(1024)+vec3i(1024))%vec3i(1024));
  var h=(q.x*1597334677u)^(q.y*3812015801u)^(q.z*2798796415u);
  h=(h^(h>>16u))*2246822519u; h=(h^(h>>13u))*3266489917u;
  return f32((h^(h>>16u))&16777215u)/16777215.0;
}
fn noise(p:vec3f)->f32 {
  if(MATERIAL_CACHE){let cached=cachedMaterialNoise(p);if(cached>=0.0){return cached;}}
  let i=floor(p); let f=fract(p); let u=f*f*(3.0-2.0*f);
  return mix(mix(mix(hash(i),hash(i+vec3f(1,0,0)),u.x),mix(hash(i+vec3f(0,1,0)),hash(i+vec3f(1,1,0)),u.x),u.y),mix(mix(hash(i+vec3f(0,0,1)),hash(i+vec3f(1,0,1)),u.x),mix(hash(i+vec3f(0,1,1)),hash(i+vec3f(1,1,1)),u.x),u.y),u.z);
}
fn filteredNoise(p:vec3f,origin:vec3f,footprint:f32)->f32 {
  if(footprint>=0.85) {return 0.5;}
  return mix(0.5,noise(p+origin),1.0-smoothstep(0.25,0.85,footprint));
}
fn skyColor(ray:vec3f)->vec3f { return physicalSky(g.camera.xyz,ray); }
fn display(color:vec3f)->vec3f {
  let x=max(color*g.params.y,vec3f(0.0));
  let mapped=clamp((x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14),vec3f(0),vec3f(1));
  return pow(mapped,vec3f(1.0/2.2));
}
fn shadow(world:vec3f,n:vec3f)->f32 {
  if(LIGHTING_ABLATION==4u){return 1.0;}
  let p=g.lightVP*vec4f(world+n*max(0.0005,g.ground.w*0.15),1.0);
  let uv=p.xy/p.w*vec2f(0.5,-0.5)+0.5;
  if(any(uv<vec2f(0))||any(uv>vec2f(1))||p.z<0.0||p.z>1.0) { return 1.0; }
  let texel=1.0/vec2f(textureDimensions(shadowMap));
  var result=0.0;
  let bias=max(0.00005,g.ground.w*0.01)*g.sunlight.w;
  for(var y=-1;y<=1;y++) { for(var x=-1;x<=1;x++) { result+=textureSampleCompareLevel(shadowMap,shadowSampler,uv+vec2f(f32(x),f32(y))*texel,p.z/p.w-bias); } }
  return result/9.0;
}
// Certify a constant PCF result over the whole water footprint, including any
// unit normal's receiver offset. Mixed texels or large regions retain correlated integration.
fn waterGlintVisibility(world:vec3f,dx:vec3f,dy:vec3f)->f32 {
  let p=g.lightVP*vec4f(world,1.0);
  if(abs(p.w-1.0)>0.00001||g.lightVP[0].w!=0.0||g.lightVP[1].w!=0.0||g.lightVP[2].w!=0.0){return -1.0;}
  let offset=max(0.0005,g.ground.w*0.15);
  let normalBound=offset*vec3f(length(vec3f(g.lightVP[0].x,g.lightVP[1].x,g.lightVP[2].x)),length(vec3f(g.lightVP[0].y,g.lightVP[1].y,g.lightVP[2].y)),length(vec3f(g.lightVP[0].z,g.lightVP[1].z,g.lightVP[2].z)));
  let extent=0.5*(abs((g.lightVP*vec4f(dx,0.0)).xyz)+abs((g.lightVP*vec4f(dy,0.0)).xyz))+normalBound+vec3f(0.000002);
  let lo=p.xyz-extent;let hi=p.xyz+extent;
  if(any(lo.xy>vec2f(1.0))||any(hi.xy<vec2f(-1.0))||lo.z>1.0||hi.z<0.0){return 1.0;}
  if(any(lo.xy<vec2f(-1.0))||any(hi.xy>vec2f(1.0))||lo.z<0.0||hi.z>1.0){return -1.0;}
  let size=vec2i(textureDimensions(shadowMap));
  let minimum=clamp(vec2i(floor((vec2f(lo.x,-hi.y)*0.5+0.5)*vec2f(size)-0.5))-vec2i(1),vec2i(0),size-1);
  let maximum=clamp(vec2i(floor((vec2f(hi.x,-lo.y)*0.5+0.5)*vec2f(size)-0.5))+vec2i(2),vec2i(0),size-1);
  if(any(maximum-minimum>vec2i(7))){return -1.0;}
  let bias=max(0.00005,g.ground.w*0.01)*g.sunlight.w;
  var lit=true;var dark=true;
  for(var y=minimum.y;y<=maximum.y;y++){for(var x=minimum.x;x<=maximum.x;x++){
    let depth=textureLoad(shadowMap,vec2i(x,y),0);
    lit=lit&&(depth>hi.z-bias);dark=dark&&(depth<lo.z-bias);
    if(!lit&&!dark){return -1.0;}
  }}
  if(lit){return 1.0;}if(dark){return 0.0;}return -1.0;
}
fn shade(v:Varying,front:bool,procedural:bool)->vec4f {
  receiverTriangleProof=v.indirectProof;
  let clay=g.params.z>9.5&&g.params.z<10.5;
  var n=normalize(v.normal)*select(-1.0,1.0,front);
  if(WATER_SURFACE) {let slope=authoredWaterSlope(v.world.xz,g.params.x);n=normalize(vec3f(-slope.x,1.0,-slope.y));}
  // Derivatives are evaluated uniformly before diagnostic exits and path selection.
  let waterDx=dpdx(v.world);let waterDy=dpdy(v.world);
  let restDx=dpdx(v.restWorld);let restDy=dpdy(v.restWorld);
  if(obj.material.w>0.0 && !WATER_SURFACE) {
    // Rotate the smooth rest normal by the triangle's actual wind deformation.
    // This includes spatial variation of branch weights and leaf flutter.
    let before=cross(restDx,restDy);let after=cross(waterDx,waterDy);
    if(dot(before,before)>1e-18 && dot(after,after)>1e-18) {
      let a=normalize(before);let b=normalize(after);let axis=cross(a,b);let cosine=dot(a,b);
      if(cosine> -0.9999) {n=normalize(n+cross(axis,n)+cross(axis,cross(axis,n))/max(1.0+cosine,0.0001));}
      else {n=select(-b,b,dot(a,n)>=0.0);}
    }
  }
  if(obj.coordinates.w>1.5){n=thinCrownNormal(v.thinUV.xyz,v.world,n);}
  let view=normalize(g.camera.xyz-v.world);
  if(g.params.z>0.5&&g.params.z<1.5) { return vec4f(n*0.5+0.5,1); }
  if(g.params.z>1.5&&g.params.z<2.5) { return vec4f(vec3f(1.0-exp(-distance(g.camera.xyz,v.world)*0.025)),1); }
  if(g.params.z>2.5&&g.params.z<3.5) { return vec4f(obj.color.xyz,1); }
  if(g.params.z>3.5&&g.params.z<4.5) { return vec4f(v.bindColor.xyz,1); }
  if(g.params.z>4.5&&g.params.z<5.5) { return vec4f(1); }
  if(g.params.z>5.5&&g.params.z<6.5) { return vec4f(v.identity,1); }
  if(g.params.z>10.5&&g.params.z<11.5) {return vec4f(vec3f(clamp(obj.creature.w/0.02,0.0,1.0)),1);}
  if(g.params.z>11.5&&g.params.z<12.5) {return vec4f(creatureTangent(n,v.fiberTangent.xyz)*0.5+0.5,1);}
  if(g.params.z>12.5&&g.params.z<13.5) {return vec4f(select(vec3f(0.12),v.color.xyz,obj.coordinates.z>0.5),1);}
  let coordinates=select(creatureCoordinates(v.local),v.world,obj.coordinates.x>0.5&&obj.coordinates.x<1.5);
  let footprint=max(length(dpdx(coordinates)),length(dpdy(coordinates)));
  let p=coordinates*obj.material.y;
  var grain=0.5;
  if(procedural && obj.material.x<3.5 && (obj.material.x>0.5 || obj.material.z>0.0)) {
    let width=footprint*obj.material.y;
    grain=filteredNoise(p*2.0,obj.noiseOrigins[1].xyz,width*2.0)*0.6+filteredNoise(p*7.0,obj.noiseOrigins[2].xyz,width*7.0)*0.3+filteredNoise(p*21.0,obj.noiseOrigins[3].xyz,width*21.0)*0.1;
  }
  var pattern=0.0;
  if(obj.material.x>0.5&&obj.material.x<1.5) {pattern=smoothstep(0.22,0.8,grain);}
  if(obj.material.x>1.5&&obj.material.x<2.5) {pattern=smoothstep(0.0,0.3,sin(p.y*3.0+obj.patternOrigins.x+filteredNoise(p,obj.noiseOrigins[0].xyz,footprint*obj.material.y)*2.0));}
  if(obj.material.x>2.5&&obj.material.x<3.5) {pattern=pow(0.5+0.5*sin(p.x*1.3+p.z*0.4+obj.patternOrigins.y+grain*8.0),3.0);}
  if(obj.material.x>0.5&&obj.material.x<3.5) {pattern=mix(pattern,0.5,smoothstep(0.5,2.0,footprint*obj.material.y));}
  if(obj.material.x>3.5) {pattern=authoredWovenCoverage(p);}
  var base=mix(obj.color.xyz,obj.secondary.xyz,pattern)*select(v.color.xyz,vec3f(1.0),WATER_SURFACE);
  var rough=clamp(obj.color.w+select(0.0,(grain-0.5)*0.12,obj.material.x>0.5),0.06,1.0);
  var metallic=obj.secondary.w;
  // Screen-space bump derives from continuous procedural detail without texture assets.
  var bump=grain*obj.material.z*0.008;
  if(obj.surface.x>0.5 && obj.surfaceDetail.x>0.5) {
    let detail=authoredSurfaceDetail(coordinates,footprint);
    base=mix(obj.color.xyz,obj.secondary.xyz,detail.x)*v.color.xyz;
    bump+=detail.y;
  }
  for(var i=0u;i<min(u32(obj.coordinates.y),2u);i++) {
    let layer=obj.layers[i];
    let detail=filteredNoise(coordinates*layer.properties.w,layer.origin.xyz,footprint*layer.properties.w);
    let coverage=clamp(layer.properties.y+layer.properties.z*(n.y-0.5)+(detail-0.5)*0.35,0.0,1.0);
    base=mix(base,layer.color.xyz,coverage);
    rough=mix(rough,clamp(layer.color.w,0.06,1.0),coverage);
    metallic=mix(metallic,layer.properties.x,coverage);
    bump+=detail*layer.detail.x*0.008*coverage;
  }
  var substrate=1.0;
  if(obj.surface.x>0.5) {
    for(var i=0u;i<min(u32(obj.surfaceDirt.w),4u);i++) {
      let layer=obj.surfaceLayers[i];
      let detail=filteredNoise(coordinates*layer.mask.x,layer.origin.xyz,footprint*layer.mask.x);
      let coverage=authoredSurfaceMask(layer,coordinates,n,detail,footprint);
      substrate*=1.0-coverage;
      base=mix(base,layer.color.xyz,coverage);
      rough=mix(rough,layer.color.w,coverage);
      metallic=mix(metallic,layer.properties.x,coverage);
      bump+=detail*layer.properties.z*coverage;
    }
    let history=authoredSurfaceHistory(coordinates,n,footprint);
    substrate*=(1.0-history.y)*(1.0-history.z);
    let weather=obj.surfaceHistory.x*(0.3+history.x*0.7);
    base=mix(base,base*0.75+vec3f(0.15),weather);
    rough=mix(rough,0.95,weather*0.6);
    base=mix(base,obj.surfaceDirt.xyz,history.y);
    rough=mix(rough,0.95,history.y);metallic*=1.0-history.y;
    base=mix(base,obj.surfaceDamage.xyz,history.z);
    rough=mix(rough,0.8,history.z);
    bump-=history.z*0.003;
    base*=1.0-obj.surface.y*0.3;
    rough=mix(rough,0.08,obj.surface.y);
  }
  if(obj.waterField.w>0.5){
    let wetness=waterBodyGrid(v.world.xz,3u).x;
    base*=1.0-wetness*0.22;rough=mix(rough,0.22,wetness*0.8);
  }
  if(clay) {base=vec3f(0.5);rough=0.7;metallic=0.0;bump=0.0;}
  if(obj.relief.x>0.5&&!clay) {
    let reliefFootprint=max(length(dpdx(v.local)),length(dpdy(v.local)));
    let residual=authoredReliefResidual(v.local,v.localNormal.xyz,reliefFootprint);
    bump+=residual.x*v.localNormal.w;
    rough=clamp(pow(pow(rough,4.0)+0.5*residual.y,0.25),rough,1.0);
  }
  let dp1=dpdx(v.world); let dp2=dpdy(v.world);
  let r1=cross(dp2,n); let r2=cross(n,dp1);
  let det=dot(dp1,r1);
  n=normalize(n-(r1*dpdx(bump)+r2*dpdy(bump))*sign(det)/max(abs(det),0.00001));
  let light=g.sun.xyz;
  let halfVector=normalize(light+view);
  let nl=max(dot(n,light),0.0); let nv=max(dot(n,view),0.001);
  let nh=max(dot(n,halfVector),0.0); let vh=max(dot(view,halfVector),0.0);
  if(g.params.z>6.5&&g.params.z<7.5) { return vec4f(base,1); }
  if(g.params.z>7.5&&g.params.z<8.5) { return vec4f(vec3f(rough),1); }
  if(g.params.z>8.5&&g.params.z<9.5) { return vec4f(vec3f(metallic),1); }
  if(g.params.z>13.5&&g.params.z<14.5) {return vec4f(n*0.5+0.5,1);}
  if(g.params.z>15.5&&g.params.z<16.5) {return vec4f(select(vec3f(1,0,1),vec3f(0,1,0),(select(v.backRadiance.w,v.bindColor.w,front)>0.0||compiledSurfaceWeights(v.world,n).valid||compiledTriangleProof(v.world,n).w>=0.0)),1.0);}
  if(g.params.z>14.5&&g.params.z<15.5) {let cached=automaticDiffuseResolve(select(v.backRadiance,v.bindColor,front));return vec4f(base*(1.0-metallic)*mix(compiledIndirectDiffuse(v.world,n).xyz,cached.xyz,cached.w),1.0);}
  let f0=mix(vec3f(0.04),base,metallic);
  let tangent=creatureSafeDirection(v.fiberTangent.xyz,(obj.model*vec4f(obj.creatureFiber.xyz,0.0)).xyz);
  let thinFoliage=obj.surface.x>2.5&&obj.surface.x<3.5&&obj.creature.x>0.5&&obj.creature.x<1.5;
  if(LIGHTING_ABLATION==8u&&thinFoliage){return vec4f(base,1.0);}
  let shadowNormal=select(n,select(-n,n,dot(n,light)>=0.0),thinFoliage);
  let celestialVisibility=1.0-select(v.history.w,v.history.z,front);
  var visibility=0.0;
  if(celestialVisibility>0.0){visibility=shadow(v.world,shadowNormal)*celestialVisibility;}
  let diffuse=base*(1.0-metallic)/3.141593;
  // A bounded ground-bounce closure follows incident sky energy. A constant
  // authored-color fill kept terrain daylight-bright under twilight exposure.
  let localBounce=max(skyIrradiance[0].xyz*0.2820947918,vec3f(0.0))*g.ground.xyz*0.6;
  let reflectionDirection=reflect(-view,n);
  let localLight=compiledIndirectSample(v.world,n,reflectionDirection,vec2f(rough,select(-1.0,obj.creatureCoat.y,obj.creatureCoat.x>0.0)),true);
  if(visibility>0.0){visibility*=compiledContactVisibility(v.world,shadowNormal,light,localLight.skyVisibility);}
  let indirect=localLight.diffuse;
  let automatic=automaticDiffuseResolve(select(v.backRadiance,v.bindColor,front));
  var skyNormal=n;var skyExposure=1.0;
  if(automatic.w<1.0&&LOCAL_SKY_VISIBILITY&&obj.localLighting.w>=2.0&&!WATER_SURFACE) {
    let magnitude=length(v.skyExposure.xyz);
    let frontExposure=clamp(magnitude/max(length(v.normal),0.0001),0.0,1.0);
    let blend=clamp(obj.localLighting.w-2.0,0.0,1.0);
    skyExposure=mix(1.0,select(v.skyExposure.w,frontExposure,front),blend);
    if(front&&magnitude>0.0001){skyNormal=normalize(n+blend*(v.skyExposure.xyz/magnitude-normalize(v.normal)));}
  }
  var hemisphere=automatic.xyz;
  if(automatic.w<1.0){hemisphere=mix(mix((physicalDiffuseSky(skyNormal)+localBounce)*g.horizon.w*skyExposure,indirect.xyz,indirect.w),automatic.xyz,automatic.w);}
  let environmentReflectance=environmentSpecularWeight(nv,rough,f0);
  var diffuseAmbient=base*(1.0-metallic)*hemisphere*(vec3f(1.0)-min(environmentReflectance,vec3f(1.0)));
  if(thinFoliage&&!clay) {
    let backIndirect=compiledIndirectDiffuse(v.world,-n);
    var backSky=mix((physicalDiffuseSky(-n)+localBounce)*g.horizon.w,backIndirect.xyz,backIndirect.w);
    let opposite=automaticDiffuseResolve(select(v.bindColor,v.backRadiance,front));backSky=mix(backSky,opposite.xyz,opposite.w);
    // The automatic static cache excludes leaves; preserve authored canopy
    // attenuation. The legacy explicit GI diagnostic retains its own closure.
    let frontLocal=hemisphere*mix(v.history.y,1.0,indirect.w);
    let backLocal=backSky*mix(v.history.y,1.0,backIndirect.w);
    diffuseAmbient=authoredFoliageAmbient(base,metallic,frontLocal,backLocal,substrate);
  }
  var diffuseIrradiance=nl*visibility;
  if(g.renderCompiler.x>0.0 && !WATER_SURFACE) {
    let finiteSun=compiledFiniteSun(v.world,n,light,g.renderCompiler.x);
    if(finiteSun.z>0.5) { diffuseIrradiance=finiteSun.y*celestialVisibility; }
  }
  var color=authoredSurfaceDirect(base,n,view,light,tangent,rough,metallic,diffuseIrradiance,visibility,substrate)*g.sunlight.xyz*g.sun.w*physicalCachedSunTransmittance(v.world)+diffuseAmbient;
  // Visibility affects environment light only. Preserve direct lights and an
  // explicitly supplied GI field, which already accounts for local visibility.
  var reflectionOcclusion=1.0;
  if(LOCAL_SKY_VISIBILITY){reflectionOcclusion=mix(clamp(pow(nv+skyExposure,exp2(-16.0*rough-1.0))-1.0+skyExposure,0.0,1.0),1.0,indirect.w);}
  let radianceIds=select(v.indirectProof.zw,v.indirectProof.xy,front);
  var reflectedSky=vec3f(v.color.w,v.fiberTangent.w,v.thinUV.w);
  if(!COMPILED_REFLECTION_CACHE&&(clay||(!front&&obj.radianceSettings.w<0.5)||obj.radianceSettings.z<0.5||!automaticRadianceReady())){reflectedSky=automaticReflection(radianceIds,reflectionDirection,rough,localLight.reflection,reflectionOcclusion);}
  else if(obj.radianceSettings.z<1.5 && v.localNormal.w>0.000001){reflectedSky+=physicalSkyRoughReflection(reflectionDirection,rough)*v.localNormal.w;}
  var reflectedBase=reflectedSky*environmentReflectance;
  if(obj.creatureCoat.x>0.0 && !WATER_SURFACE && !clay) {
    let coatF=0.04+0.96*pow(1.0-clamp(nv,0.0,1.0),5.0);
    let coatSky=automaticReflection(radianceIds,reflectionDirection,obj.creatureCoat.y,localLight.coat,reflectionOcclusion);
    let coatWeight=clamp(obj.creatureCoat.x*coatF*substrate,0.0,1.0);
    color-=diffuseAmbient*coatWeight;reflectedBase*=1.0-coatWeight;
    color+=coatSky*obj.creatureCoat.x*environmentSpecularWeight(nv,obj.creatureCoat.y,vec3f(0.04))*substrate;
  }
  color+=reflectedBase;
  if(!WATER_SURFACE||clay) {
    for(var i=0u;i<u32(g.viewport.z);i++) {
      if((u32(obj.localLighting.x)&(1u<<i))==0u){continue;}
      let point=g.points[i];let offset=point.position.xyz-v.world;
      let distanceSquared=max(dot(offset,offset),0.01);let pointDirection=offset*inverseSqrt(distanceSquared);
      let attenuation=localLightAttenuation(distanceSquared,point.color.w);if(attenuation==0.0){continue;}
      let pointNL=max(dot(n,pointDirection),0.0);
      let pointVisibility=localLightVisibility(i,v.world,select(n,select(-n,n,dot(n,pointDirection)>=0.0),thinFoliage));
      color+=authoredSurfaceDirect(base,n,view,pointDirection,tangent,rough,metallic,pointNL*pointVisibility,pointVisibility,substrate)*point.color.xyz*point.position.w*attenuation;
    }
  } else {
    if(obj.waves[1].phase.z==2.0) {color=waterDirectMaterialResponse(v.world,n,view,base,rough,metallic);}
    else {color=integratedWaterResponse(v.world,waterDx,waterDy,base,rough,metallic);}
    // River vertex color carries authored bank foam coverage, independent of deep-water tint.
    let foamBreakup=smoothstep(0.48,0.78,filteredNoise(v.local*2.1+vec3f(g.params.x*0.08,0.0,0.0),vec3f(0.0),max(length(waterDx),length(waterDy))*2.1));
    let foam=clamp((v.color.r-1.0)*0.5,0.0,1.0)*foamBreakup;
    let foamLight=vec3f(0.8)*(hemisphere+g.sunlight.xyz*g.sun.w*physicalCachedSunTransmittance(v.world)*nl*visibility/3.141593);
    color=mix(color,foamLight,foam);
  }
  if(g.moon.w>0.0001) {
    let moon=normalize(g.moon.xyz);
    let moonNL=max(dot(n,moon),0.0);
    if(moonNL>0.0) {
      let moonVisibility=select(1.0,shadow(v.world,n),g.skyCycle.x>0.5)*celestialVisibility;
      let moonCloud=1.0-physicalCloudShadowCached(v.world,moon)*0.8;
      let moonTransport=physicalSunTransmissionAt(physicalPlanetPoint(v.world),moon);
      color+=authoredSurfaceDirect(base,n,view,moon,tangent,rough,metallic,
        moonNL*moonVisibility,moonVisibility,substrate)*
        vec3f(0.72,0.82,1.0)*g.moon.w*moonTransport*moonCloud;
    }
  }
  var surfaceOpacity=1.0;
  if(obj.surface.x>0.5&&!WATER_SURFACE&&!clay) {
    let wetF=0.02+0.98*pow(1.0-clamp(nv,0.0,1.0),5.0);
    color=color*(1.0-obj.surface.y*wetF)+reflectedSky*obj.surface.y*wetF;
    if(obj.surface.x>5.5) {
      if(THIN_GLASS_TRANSPARENCY) {
        let ior=obj.surfaceHistory.w;
        let f0=pow((ior-1.0)/(ior+1.0),2.0);
        let fresnel=f0+(1.0-f0)*pow(1.0-clamp(nv,0.0,1.0),5.0);
        let transmission=clamp(obj.surfaceHistory.z*substrate,0.0,1.0);
        surfaceOpacity=1.0-transmission*(1.0-fresnel);
        // Premultiplied thin-surface reflection over the actual scene behind it.
        // No bent rays, volumetric absorption or mutually intersecting transparency is implied.
        color=reflectedSky*fresnel+color*(1.0-transmission)*(1.0-fresnel);
      } else {color=mix(color,authoredGlass(base,n,view,v.world,rough,reflectedSky),substrate);}
    }
  }
  if(!clay){color+=obj.emission.xyz*obj.emission.w;}
  if(obj.flags.y>0.5&&!clay) {color+=vec3f(0.15,0.65,0.5)*pow(1.0-nv,4.0)*0.7;}
  let gridWidth=max(fwidth(v.world.xz),vec2f(0.001));
  if(g.params.w>0.5&&abs(v.world.y)<0.012&&!WATER_SURFACE) {
    let cell=abs(fract(v.world.xz-0.5)-0.5)/gridWidth;
    let line=1.0-min(min(cell.x,cell.y),1.0);
    color=mix(color,color+vec3f(0.13),line*0.4);
  }
  color=physicalAerialPerspective(color/max(surfaceOpacity,0.0001),g.camera.xyz,v.world)*surfaceOpacity;
  return vec4f(color,surfaceOpacity);
}
struct SceneOutput { @location(0) color:vec4f, @location(1) motion:vec4f };
fn temporalMotion(previous:vec4f,identity:vec3f,valid:f32,reactive:bool)->vec4f {
 let id=dot(round(identity*255.0),vec3f(65536.0,256.0,1.0))+2.0;
 if(valid<0.5||previous.w<=0.0){return vec4f(0.0);}
 return vec4f(previous.xy/previous.w*vec2f(0.5,-0.5)+0.5,previous.w,select(id,-id,reactive));
}
@fragment fn fragmentMain(v:Varying,@builtin(front_facing) front:bool)->SceneOutput {
  return SceneOutput(shade(v,front,true),temporalMotion(v.previousClip,v.identity,v.history.x,WATER_SURFACE));
}
@fragment fn fragmentSolid(v:Varying,@builtin(front_facing) front:bool)->SceneOutput {
  return SceneOutput(shade(v,front,false),temporalMotion(v.previousClip,v.identity,v.history.x,WATER_SURFACE));
}
// Dedicated realization keeps the old quadrature/material closure out of the
// register footprint of compiled spectrum water.
@fragment fn fragmentWaterBody(v:Varying,@builtin(front_facing) front:bool)->SceneOutput {
  let dx=dpdx(v.world);let dy=dpdy(v.world);
  if(obj.waterField.y<0.5&&waterBodyWet(v.world.xz)<0.003){discard;}
  let n=normalize(v.normal);
  let mode=g.params.z;
  var color=vec3f(0.0);
  if(mode>0.5&&mode<1.5){color=n*0.5+0.5;}
  else if(mode>1.5&&mode<2.5){color=vec3f(1.0-exp(-distance(g.camera.xyz,v.world)*0.025));}
  else if(mode>2.5&&mode<3.5){color=obj.color.xyz;}
  else if(mode>4.5&&mode<5.5){color=vec3f(1.0);}
  else if(mode>5.5&&mode<6.5){color=v.identity;}
  else {
    color=waterBodyResponse(v.world,select(v.restWorld.xz,v.world.xz,obj.waterField.y>0.5),n,dx,dy,obj.color.xyz,clamp(obj.color.w,0.06,1.0));
    if(obj.flags.y>0.5){color+=vec3f(0.15,0.65,0.5)*pow(1.0-abs(dot(n,normalize(g.camera.xyz-v.world))),4.0)*0.7;}
    color=physicalAerialPerspective(color,g.camera.xyz,v.world);
  }
  return SceneOutput(vec4f(color,1.0),temporalMotion(v.previousClip,v.identity,v.history.x,true));
}
struct SkyVarying { @builtin(position) position:vec4f, @location(0) uv:vec2f };
// Keep the prepassed closure free of sample-mask output so early depth tests
// can reject occluded fragments before expensive material evaluation.
@fragment fn fragmentThinMain(v:Varying,@builtin(front_facing) front:bool)->SceneOutput {
  return SceneOutput(shade(v,front,true),temporalMotion(v.previousClip,v.identity,v.history.x,false));
}
@fragment fn fragmentThinSolid(v:Varying,@builtin(front_facing) front:bool)->SceneOutput {
  return SceneOutput(shade(v,front,false),temporalMotion(v.previousClip,v.identity,v.history.x,false));
}
struct ThinSceneOutput { @location(0) color:vec4f, @location(1) motion:vec4f, @builtin(sample_mask) coverage:u32 };
fn thinOutputMask(v:Varying)->u32 {
  return thinCoverageSampleMask(v.thinUV.xyz,v.local,select(1u,4u,THIN_COVERAGE_MSAA));
}
@fragment fn fragmentThinDirectMain(v:Varying,@builtin(front_facing) front:bool)->ThinSceneOutput {
  return ThinSceneOutput(shade(v,front,true),temporalMotion(v.previousClip,v.identity,v.history.x,false),thinOutputMask(v));
}
@fragment fn fragmentThinDirectSolid(v:Varying,@builtin(front_facing) front:bool)->ThinSceneOutput {
  return ThinSceneOutput(shade(v,front,false),temporalMotion(v.previousClip,v.identity,v.history.x,false),thinOutputMask(v));
}
@vertex fn skyVertex(@builtin(vertex_index) i:u32)->SkyVarying {
  let x=f32((i<<1u)&2u); let y=f32(i&2u);
  var o:SkyVarying; o.position=vec4f(x*2.0-1.0,y*2.0-1.0,0.999999,1.0); o.uv=o.position.xy; return o;
}
@fragment fn skyFragment(v:SkyVarying)->SceneOutput {
  if(LIGHTING_ABLATION==7u){return SceneOutput(vec4f(0,0,0,1),vec4f(0));}
  if(g.params.z>4.5&&!(g.params.z>9.5&&g.params.z<10.5)) { return SceneOutput(vec4f(0,0,0,1),vec4f(0)); }
  let ray=normalize(g.forward.xyz+g.right.xyz*(v.uv.x-g.previousParams.z)*g.viewport.x*g.viewport.y+g.up.xyz*(v.uv.y-g.previousParams.w)*g.viewport.y);
  let previous=g.previousVP*vec4f(ray,0.0);
  return SceneOutput(vec4f(skyColor(ray),1.0),vec4f(previous.xy/max(previous.w,0.00001)*vec2f(0.5,-0.5)+0.5,2500.0,1.0));
}
