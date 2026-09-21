struct PointLight { position:vec4f, color:vec4f };
struct Globals {
  vp: mat4x4f, lightVP: mat4x4f,
  camera: vec4f, sun: vec4f, sunlight: vec4f, sky: vec4f, horizon: vec4f,
  ground: vec4f, wind: vec4f, params: vec4f,
  right: vec4f, up: vec4f, forward: vec4f, viewport: vec4f,
  points:array<PointLight,8>,
};
struct Wave { shape: vec4f, phase: vec4f };
struct MaterialLayer { color:vec4f, properties:vec4f, detail:vec4f, origin:vec4f };
struct Object {
  model: mat4x4f, color: vec4f, secondary: vec4f, material: vec4f, flags: vec4f,
  waves: array<Wave,8>,
  coordinates:vec4f, noiseOrigins:array<vec4f,4>, patternOrigins:vec4f,
  layers:array<MaterialLayer,2>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var shadowMap: texture_depth_2d;
@group(0) @binding(2) var shadowSampler: sampler_comparison;
@group(1) @binding(0) var<uniform> obj: Object;
@group(1) @binding(1) var<storage,read> joints: array<mat4x4f>;
struct Instance { model:mat4x4f, identity:vec4f };
@group(1) @binding(2) var<storage,read> instances:array<Instance>;
struct Vertex {
  @location(0) position: vec3f, @location(1) normal: vec3f,
  @location(2) color: vec3f, @location(3) joints: vec4f, @location(4) weights: vec4f,
};
struct Varying {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f, @location(1) normal: vec3f,
  @location(2) color: vec3f, @location(3) bindColor: vec3f,
  @location(4) @interpolate(flat) identity:vec3f,
  @location(5) local:vec3f,
};
fn palette(t: f32) -> vec3f { return 0.5+0.5*cos(6.283185*(vec3f(0.0,0.33,0.67)+t)); }
fn waterSampleFiltered(xz:vec2f,spacing:f32)->vec3f {
  var height=obj.waves[0].phase.w;
  var slope=vec2f(0.0);
  for(var i=0u;i<u32(obj.flags.w);i++) {
    let w=obj.waves[i]; let k=6.283185/w.shape.y;
    let d=vec2f(cos(w.shape.w),sin(w.shape.w));
    let phase=k*(dot(d,xz)-w.shape.z*g.params.x)+w.phase.x;
    let attenuation=1.0-smoothstep(0.25,0.5,spacing/w.shape.y);
    height += w.shape.x*attenuation*sin(phase);
    slope += w.shape.x*attenuation*k*d*cos(phase);
  }
  return vec3f(height,slope);
}
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
fn deform(v: Vertex, model:mat4x4f) -> Varying {
  var p = vec4f(v.position,1.0);
  var n = vec4f(v.normal,0.0);
  var binding = vec3f(0.28,0.33,0.42);
  if (obj.flags.z > 0.5) {
    let j = vec4u(v.joints);
    let skin = joints[j.x]*v.weights.x+joints[j.y]*v.weights.y+joints[j.z]*v.weights.z+joints[j.w]*v.weights.w;
    p=skin*p; n=skin*n;
    binding=palette(f32(j.x)*0.173)*v.weights.x+palette(f32(j.y)*0.173)*v.weights.y+palette(f32(j.z)*0.173)*v.weights.z+palette(f32(j.w)*0.173)*v.weights.w;
  }
  var world = (model*p).xyz;
  var normal = normalize((model*n).xyz);
  if (obj.material.w > 0.0) {
    let h=max(p.y,0.0);
    let amplitude=min(h*h*0.012,0.6)*clamp(obj.material.w,0.0,2.0);
    let phase=g.params.x*1.4+world.x*0.17+world.z*0.23+g.wind.w;
    let windSpeed=length(g.wind.xz);
    let windDirection=g.wind.xz/max(windSpeed,0.00001);
    let windStrength=min(windSpeed,10.0)/10.0;
    let instanceScale=length(model[0].xyz);
    world += vec3f(windDirection.x,0.0,windDirection.y)*amplitude*windStrength*instanceScale*sin(phase);
  }
  if (obj.flags.x > 0.5) {
    let sample=waterSampleFiltered(world.xz,obj.waves[0].phase.y);
    world.y=sample.x;
    normal=normalize(vec3f(-sample.y,1.0,-sample.z));
  }
  var out:Varying;
  out.world=world; out.normal=normal; out.color=v.color; out.bindColor=binding;
  out.local=v.position;
  out.clip=g.vp*vec4f(world,1.0);
  return out;
}
@vertex fn vertexMain(v:Vertex,@builtin(instance_index) index:u32)->Varying { var out=deform(v,instances[index].model); out.identity=instances[index].identity.xyz; return out; }
@vertex fn shadowMain(v:Vertex,@builtin(instance_index) index:u32)->@builtin(position) vec4f { return g.lightVP*vec4f(deform(v,instances[index].model).world,1.0); }
// A periodic integer lattice avoids the large-coordinate precision loss of sine hashes.
fn hash(p:vec3f)->f32 {
  let q=vec3u((vec3i(p)%vec3i(1024)+vec3i(1024))%vec3i(1024));
  var h=(q.x*1597334677u)^(q.y*3812015801u)^(q.z*2798796415u);
  h=(h^(h>>16u))*2246822519u; h=(h^(h>>13u))*3266489917u;
  return f32((h^(h>>16u))&16777215u)/16777215.0;
}
fn noise(p:vec3f)->f32 {
  let i=floor(p); let f=fract(p); let u=f*f*(3.0-2.0*f);
  return mix(mix(mix(hash(i),hash(i+vec3f(1,0,0)),u.x),mix(hash(i+vec3f(0,1,0)),hash(i+vec3f(1,1,0)),u.x),u.y),mix(mix(hash(i+vec3f(0,0,1)),hash(i+vec3f(1,0,1)),u.x),mix(hash(i+vec3f(0,1,1)),hash(i+vec3f(1,1,1)),u.x),u.y),u.z);
}
fn filteredNoise(p:vec3f,origin:vec3f,footprint:f32)->f32 {
  return mix(0.5,noise(p+origin),1.0-smoothstep(0.25,0.85,footprint));
}
fn skyColor(ray:vec3f)->vec3f {
  let horizon=pow(1.0-abs(ray.y),4.0);
  var sky=mix(g.sky.xyz,g.horizon.xyz,horizon);
  sky=mix(g.ground.xyz*0.45,sky,smoothstep(-0.18,0.08,ray.y));
  let sun=clamp(dot(ray,g.sun.xyz),0.0,1.0);
  sky+=g.sunlight.xyz*g.sun.w*(pow(sun,4000.0)*10.0+pow(sun,24.0)*0.09);
  return sky;
}
fn display(color:vec3f)->vec3f {
  let x=max(color*g.params.y,vec3f(0.0));
  let mapped=clamp((x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14),vec3f(0),vec3f(1));
  return pow(mapped,vec3f(1.0/2.2));
}
fn shadow(world:vec3f,n:vec3f)->f32 {
  let p=g.lightVP*vec4f(world+n*max(0.002,g.ground.w*0.35),1.0);
  let uv=p.xy/p.w*vec2f(0.5,-0.5)+0.5;
  if(any(uv<vec2f(0))||any(uv>vec2f(1))||p.z<0.0||p.z>1.0) { return 1.0; }
  let texel=1.0/vec2f(textureDimensions(shadowMap));
  var result=0.0;
  let bias=max(0.00025,g.ground.w*0.03)*g.sunlight.w;
  for(var y=-1;y<=1;y++) { for(var x=-1;x<=1;x++) { result+=textureSampleCompareLevel(shadowMap,shadowSampler,uv+vec2f(f32(x),f32(y))*texel,p.z/p.w-bias); } }
  return result/9.0;
}
fn shade(v:Varying,front:bool,procedural:bool)->vec4f {
  var n=normalize(v.normal)*select(-1.0,1.0,front);
  if(obj.flags.x>0.5) {n=filteredWaterNormal(v.world.xz);}
  let view=normalize(g.camera.xyz-v.world);
  if(g.params.z>0.5&&g.params.z<1.5) { return vec4f(n*0.5+0.5,1); }
  if(g.params.z>1.5&&g.params.z<2.5) { return vec4f(vec3f(1.0-exp(-distance(g.camera.xyz,v.world)*0.025)),1); }
  if(g.params.z>2.5&&g.params.z<3.5) { return vec4f(obj.color.xyz,1); }
  if(g.params.z>3.5&&g.params.z<4.5) { return vec4f(v.bindColor,1); }
  if(g.params.z>4.5&&g.params.z<5.5) { return vec4f(1); }
  if(g.params.z>5.5&&g.params.z<6.5) { return vec4f(v.identity,1); }
  let coordinates=select(v.local,v.world,obj.coordinates.x>0.5);
  let footprint=max(length(dpdx(coordinates)),length(dpdy(coordinates)));
  let p=coordinates*obj.material.y;
  var grain=0.5;
  if(procedural) {
    let width=footprint*obj.material.y;
    grain=filteredNoise(p*2.0,obj.noiseOrigins[1].xyz,width*2.0)*0.6+filteredNoise(p*7.0,obj.noiseOrigins[2].xyz,width*7.0)*0.3+filteredNoise(p*21.0,obj.noiseOrigins[3].xyz,width*21.0)*0.1;
  }
  var pattern=0.0;
  if(obj.material.x>0.5&&obj.material.x<1.5) {pattern=smoothstep(0.22,0.8,grain);}
  if(obj.material.x>1.5&&obj.material.x<2.5) {pattern=smoothstep(0.0,0.3,sin(p.y*3.0+obj.patternOrigins.x+filteredNoise(p,obj.noiseOrigins[0].xyz,footprint*obj.material.y)*2.0));}
  if(obj.material.x>2.5) {pattern=pow(0.5+0.5*sin(p.x*1.3+p.z*0.4+obj.patternOrigins.y+grain*8.0),3.0);}
  if(obj.material.x>0.5) {pattern=mix(pattern,0.5,smoothstep(0.5,2.0,footprint*obj.material.y));}
  var base=mix(obj.color.xyz,obj.secondary.xyz,pattern)*v.color;
  var rough=clamp(obj.color.w+select(0.0,(grain-0.5)*0.12,obj.material.x>0.5),0.06,1.0);
  var metallic=obj.secondary.w;
  // Screen-space bump derives from continuous procedural detail without texture assets.
  var bump=grain*obj.material.z*0.008;
  for(var i=0u;i<min(u32(obj.coordinates.y),2u);i++) {
    let layer=obj.layers[i];
    let detail=filteredNoise(coordinates*layer.properties.w,layer.origin.xyz,footprint*layer.properties.w);
    let coverage=clamp(layer.properties.y+layer.properties.z*(n.y-0.5)+(detail-0.5)*0.35,0.0,1.0);
    base=mix(base,layer.color.xyz,coverage);
    rough=mix(rough,clamp(layer.color.w,0.06,1.0),coverage);
    metallic=mix(metallic,layer.properties.x,coverage);
    bump+=detail*layer.detail.x*0.008*coverage;
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
  if(g.params.z>8.5) { return vec4f(vec3f(metallic),1); }
  let a2=pow(rough,4.0); let d=a2/max(3.141593*pow(nh*nh*(a2-1.0)+1.0,2.0),0.00001);
  let k=pow(rough+1.0,2.0)/8.0;
  let geometry=nv/(nv*(1.0-k)+k)*nl/(nl*(1.0-k)+k);
  let f0=mix(vec3f(0.04),base,metallic); let fresnel=f0+(1.0-f0)*pow(1.0-vh,5.0);
  let specular=d*geometry*fresnel/max(4.0*nv*nl,0.001);
  let visibility=shadow(v.world,n);
  let diffuse=base*(1.0-metallic)/3.141593;
  let hemisphere=mix(g.ground.xyz,g.sky.xyz,n.y*0.5+0.5)*g.horizon.w;
  var color=(diffuse+specular)*g.sunlight.xyz*g.sun.w*nl*visibility+base*hemisphere;
  color+=skyColor(reflect(-view,n))*f0*(1.0-rough)*0.25;
  if(obj.flags.x>0.5) {
    let fresnelWater=0.02+0.98*pow(1.0-nv,5.0);
    color=mix(base*(0.4+nl*0.25),skyColor(reflect(-view,n)),fresnelWater)*0.85+specular*g.sunlight.xyz*g.sun.w*nl*visibility;
    color+=vec3f(0.09,0.17,0.16)*pow(1.0-nv,2.0);
  }
  for(var i=0u;i<u32(g.viewport.z);i++) {
    let point=g.points[i]; let offset=point.position.xyz-v.world;
    let distanceSquared=max(dot(offset,offset),0.01); let pointDirection=offset*inverseSqrt(distanceSquared);
    let pointNL=max(dot(n,pointDirection),0.0); let pointHalf=normalize(pointDirection+view);
    let pointNH=max(dot(n,pointHalf),0.0);let pointVH=max(dot(view,pointHalf),0.0);
    let pointD=a2/max(3.141593*pow(pointNH*pointNH*(a2-1.0)+1.0,2.0),0.00001);
    let pointG=nv/(nv*(1.0-k)+k)*pointNL/(pointNL*(1.0-k)+k);
    let pointF=f0+(1.0-f0)*pow(1.0-pointVH,5.0);
    let pointSpecular=pointD*pointG*pointF/max(4.0*nv*pointNL,0.001);
    color+=(diffuse+pointSpecular)*point.color.xyz*point.position.w*pointNL/(1.0+distanceSquared);
  }
  if(obj.flags.y>0.5) {color+=vec3f(0.15,0.65,0.5)*pow(1.0-nv,4.0)*0.7;}
  let gridWidth=max(fwidth(v.world.xz),vec2f(0.001));
  if(g.params.w>0.5&&abs(v.world.y)<0.012&&obj.flags.x<0.5) {
    let cell=abs(fract(v.world.xz-0.5)-0.5)/gridWidth;
    let line=1.0-min(min(cell.x,cell.y),1.0);
    color=mix(color,color+vec3f(0.13),line*0.4);
  }
  let fog=1.0-exp(-distance(v.world,g.camera.xyz)*g.sky.w);
  color=mix(color,skyColor(normalize(v.world-g.camera.xyz)),fog);
  return vec4f(color,1.0);
}
@fragment fn fragmentMain(v:Varying,@builtin(front_facing) front:bool)->@location(0) vec4f {return shade(v,front,true);}
@fragment fn fragmentSolid(v:Varying,@builtin(front_facing) front:bool)->@location(0) vec4f {return shade(v,front,false);}
struct SkyVarying { @builtin(position) position:vec4f, @location(0) uv:vec2f };
@vertex fn skyVertex(@builtin(vertex_index) i:u32)->SkyVarying {
  let x=f32((i<<1u)&2u); let y=f32(i&2u);
  var o:SkyVarying; o.position=vec4f(x*2.0-1.0,y*2.0-1.0,0.999999,1.0); o.uv=o.position.xy; return o;
}
@fragment fn skyFragment(v:SkyVarying)->@location(0) vec4f {
  if(g.params.z>4.5) { return vec4f(0,0,0,1); }
  let ray=normalize(g.forward.xyz+g.right.xyz*v.uv.x*g.viewport.x*g.viewport.y+g.up.xyz*v.uv.y*g.viewport.y);
  return vec4f(skyColor(ray),1.0);
}
