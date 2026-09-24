@group(1) @binding(3) var thinCoverageTexture:texture_2d_array<f32>;
@group(1) @binding(4) var thinCoverageSampler:sampler;

// UVs are attached to the source surface. The same filtered field is used by
// camera and directional shadow fragments. Thin proxies are excluded from
// conservative opaque camera-visibility occluders.
fn thinCoverageAlpha(uv:vec3f)->f32 {
  return textureSampleGrad(thinCoverageTexture,thinCoverageSampler,uv.xy,i32(round(uv.z)),dpdx(uv.xy),dpdy(uv.xy)).r;
}
fn thinCoverageHash(cell:vec2f)->f32 {
  var h=vec2u(vec2i(cell));
  var n=(h.x*1597334677u)^(h.y*3812015801u);
  n=(n^(n>>16u))*2246822519u;n=(n^(n>>13u))*3266489917u;
  return f32((n^(n>>16u))&16777215u)/16777216.0;
}
fn thinCoverageFootprint(uv:vec3f)->vec4f {
  let size=vec2f(textureDimensions(thinCoverageTexture));
  let p=uv.xy*size;
  let footprint=max(length(dpdx(p)),length(dpdy(p)));
  let logScale=log2(max(footprint,1.0));
  let lo=exp2(floor(logScale));
  return vec4f(p,lo,fract(logScale));
}
fn thinCoverageThresholdFor(footprint:vec4f,salt:vec2f)->f32 {
  let p=footprint.xy;let lo=footprint.z;let t=footprint.w;
  // Include the absolute level: when the footprint exceeds the whole field,
  // both cells may be zero, but the two uniforms must remain independent.
  // The upper hash of level L is exactly the lower hash of level L+1.
  let level=round(log2(lo));
  let levelSalt=vec2f(17473.0,28753.0);
  let a=thinCoverageHash(floor(p/lo)+salt+level*levelSalt);
  let b=thinCoverageHash(floor(p/(lo*2.0))+salt+(level+1.0)*levelSalt);
  let value=mix(a,b,t);
  // CDF correction keeps the blended threshold uniform rather than making
  // intermediate footprint sizes unexpectedly denser or more transparent.
  let small=max(min(t,1.0-t),0.00001);
  let large=1.0-small;
  if(value<small){return value*value/(2.0*small*large);}
  if(value<large){return (value-small*0.5)/large;}
  return 1.0-(1.0-value)*(1.0-value)/(2.0*small*large);
}
fn thinCoverageThreshold(uv:vec3f)->f32 {
  return thinCoverageThresholdFor(thinCoverageFootprint(uv),vec2f(uv.z*1031.0,uv.z*2099.0));
}
// Independent source-anchored bits avoid the nested hardware alpha-to-coverage
// masks which can under-cover many overlapping low-opacity ribbons.
fn thinCoverageSampleMask(uv:vec3f,source:vec3f,samples:u32)->u32 {
  let coverage=thinCoverageAlpha(uv);
  let footprint=thinCoverageFootprint(uv);
  let cell=floor(source*128.0);
  let salt=cell.xy+cell.z*vec2f(211.0,397.0);
  var mask=0u;
  for(var sample=0u;sample<samples;sample++) {
    let offset=vec2f(f32(sample)*1031.0,f32(sample)*2099.0);
    let threshold=thinCoverageThresholdFor(footprint,salt+offset);
    if(coverage>0.0&&(coverage>=1.0||coverage>=threshold)){mask|=1u<<sample;}
  }
  return mask;
}
fn thinCoverageReject(uv:vec3f,pixel:vec2f)->bool {
  let coverage=thinCoverageAlpha(uv);
  let threshold=thinCoverageThreshold(uv);
  return coverage<=0.0 || (coverage<1.0 && coverage<threshold);
}

fn thinCrownNormal(uv:vec3f,world:vec3f,fallback:vec3f)->vec3f {
  let du=dpdx(uv.xy);let dv=dpdy(uv.xy);let dx=dpdx(world);let dy=dpdy(world);
  let determinant=du.x*dv.y-du.y*dv.x;
  if(abs(determinant)<1e-14){return fallback;}
  var local=textureSampleGrad(thinCoverageTexture,thinCoverageSampler,uv.xy,i32(round(uv.z)),du,dv).gba*2.0-1.0;
  let tangent=normalize((dx*dv.y-dy*du.y)/determinant);let bitangent=normalize((dy*du.x-dx*dv.x)/determinant);
  let normal=normalize(cross(tangent,bitangent));
  // Projected shoots carry the front needle hemisphere. On the reverse side,
  // mirror its normal component while retaining along/across orientation.
  local.z*=select(-1.0,1.0,dot(normal,fallback)>=0.0);
  let result=tangent*local.x+bitangent*local.y+normal*local.z;
  if(dot(result,result)<1e-12){return fallback;}return normalize(result);
}
