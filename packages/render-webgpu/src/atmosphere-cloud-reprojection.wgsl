// Reproject the authored advected field, using extinction-weighted distance and
// spread. Every sample expires after seven reuses; fresh samples are not blended.
struct CloudHistoryFrame { previous:PhysicalAtmosphereUniform, schedule:vec4u };
@group(0) @binding(22) var cloudHistory:texture_2d<f32>;
@group(0) @binding(24) var cloudHistoryMoments:texture_2d<f32>;
@group(0) @binding(25) var cloudMomentsOutput:texture_storage_2d<rgba16float,write>;
@group(0) @binding(26) var<uniform> cloudHistoryFrame:CloudHistoryFrame;
struct CloudRayQueue { counts:array<atomic<u32>,128>, pixels:array<u32> };
@group(0) @binding(27) var<storage,read_write> cloudRayQueue:CloudRayQueue;
fn cloudProject(ray:vec3f,f:PhysicalAtmosphereFrame)->vec2f {
  let forward=dot(ray,f.forward.xyz);
  if(forward<=0.0) {return vec2f(-1.0);}
  return vec2f(dot(ray,f.right.xyz)/(forward*f.viewport.x*f.viewport.y),
    dot(ray,f.up.xyz)/(forward*f.viewport.y))*vec2f(0.5,-0.5)+0.5;
}
fn cloudRay(uv:vec2f,f:PhysicalAtmosphereFrame)->vec3f {
  let ndc=(uv*2.0-1.0)*vec2f(1.0,-1.0);
  return normalize(f.forward.xyz+f.right.xyz*ndc.x*f.viewport.x*f.viewport.y+f.up.xyz*ndc.y*f.viewport.y);
}
fn physicalCloudReuse(id:vec3u)->bool {
  let size=textureDimensions(cloudViewOutput);if(any(id.xy>=size)){return false;}
  let f=atmosphereFrame.frame;let previous=cloudHistoryFrame.previous.frame;
  let uv=(vec2f(id.xy)+0.5)/vec2f(size);let ray=cloudRay(uv,f);
  let phase=(id.x&3u)+4u*(id.y&1u);
  let shift=compiledCloudMotion(vec3f(previous.cloud.y,0.0,previous.cloud.z)*previous.cloud.w,
    vec3f(f.cloud.y,0.0,f.cloud.z)*f.cloud.w);
  var oldUV=cloudProject(ray,previous);
  let margin=vec2f(1.5)/vec2f(size);
  var valid=cloudHistoryFrame.schedule.y!=0u && phase!=cloudHistoryFrame.schedule.x;
  var moments=vec4f(0.0);
  var oldPoint=vec3f(0.0);
  for(var iteration=0u;iteration<3u;iteration++) {
    if(!valid||any(oldUV<margin)||any(oldUV>vec2f(1.0)-margin)) {valid=false;break;}
    let cell=vec2i(oldUV*vec2f(size));
    moments=textureLoad(cloudHistoryMoments,cell,0);
    moments=vec4f(moments.xy*1000.0,moments.zw);
    moments.y+=moments.x*0.001+1.0;
    oldPoint=previous.camera.xyz+cloudRay((vec2f(cell)+0.5)/vec2f(size),previous)*moments.x+shift;
    let distance=max(1.0,dot(oldPoint-f.camera.xyz,ray));
    if(iteration<2u) {oldUV=cloudProject(f.camera.xyz+ray*distance-shift-previous.camera.xyz,previous);}
  }
  if(valid) {
    let distance=max(1.0,dot(oldPoint-f.camera.xyz,ray));
    let cell=vec2i(oldUV*vec2f(size));
    let oldRay=cloudRay((vec2f(cell)+0.5)/vec2f(size),previous);
    let centerPoint=oldPoint-f.camera.xyz;
    let scale=vec2f(size)/(2.0*vec2f(f.viewport.x*f.viewport.y,f.viewport.y));
    // The compiler encloses the projection of the whole retained depth
    // interval. This is a geometric bound, not a radiance/visibility guarantee.
    let uncertainty=compiledCloudProjectionRadius(centerPoint-oldRay*moments.y,centerPoint,
      centerPoint+oldRay*moments.y,f.right.xyz,f.up.xyz,f.forward.xyz,scale);
    let fraction=fract(oldUV*vec2f(size)-0.5);
    // Bilinear resampling adds f*(1-f) to each axis' filter variance. Spend
    // that blur budget explicitly instead of repeatedly softening old clouds.
    let filterVariance=moments.w+dot(fraction,vec2f(1.0)-fraction);
    valid=moments.z<f32(cloudHistoryFrame.schedule.z) && uncertainty<0.35 && filterVariance<0.5;
    let a=textureLoad(cloudHistory,cell+vec2i(-1,0),0);
    let b=textureLoad(cloudHistory,cell+vec2i(1,0),0);
    let c=textureLoad(cloudHistory,cell+vec2i(0,-1),0);
    let d=textureLoad(cloudHistory,cell+vec2i(0,1),0);
    // Disocclusions and thin cloud contours are always traced this frame.
    let minT=min(min(a.w,b.w),min(c.w,d.w));
    let maxT=max(max(a.w,b.w),max(c.w,d.w));
    valid=valid && maxT-minT<0.12;
    if(valid) {
      let color=textureSampleLevel(cloudHistory,physicalAtmosphereSampler,oldUV,0.0);
      textureStore(cloudViewOutput,id.xy,color);
      textureStore(cloudMomentsOutput,id.xy,vec4f(distance*0.001,moments.y*0.001,moments.z+1.0,filterVariance));
      return false;
    }
  }
  return true;
}
var<workgroup> cloudTileCount:atomic<u32>;
var<workgroup> cloudTilePixels:array<u32,64>;
var<workgroup> cloudTileOffset:u32;
@compute @workgroup_size(8,8) fn physicalCloudTemporalBuild(@builtin(global_invocation_id) id:vec3u,
  @builtin(local_invocation_index) lane:u32) {
  if(lane==0u) {atomicStore(&cloudTileCount,0u);}
  workgroupBarrier();
  if(physicalCloudReuse(id)) {
    let local=atomicAdd(&cloudTileCount,1u);
    cloudTilePixels[local]=id.y*textureDimensions(cloudViewOutput).x+id.x;
  }
  workgroupBarrier();
  let count=atomicLoad(&cloudTileCount);
  if(lane==0u) {cloudTileOffset=atomicAdd(&cloudRayQueue.counts[id.y/16u],count)+id.y/16u*textureDimensions(cloudViewOutput).x*16u;}
  workgroupBarrier();
  if(lane<count) {cloudRayQueue.pixels[cloudTileOffset+lane]=cloudTilePixels[lane];}
}
// Compact active rays before expensive transport. Masking three out of four
// lanes in the march would keep paying for an almost-empty SIMD wave.
@compute @workgroup_size(64) fn physicalCloudTraceQueued(@builtin(global_invocation_id) id:vec3u) {
  if(id.x>=atomicLoad(&cloudRayQueue.counts[id.y])) {return;}
  let size=textureDimensions(cloudViewOutput);
  let pixel=cloudRayQueue.pixels[id.y*size.x*16u+id.x];let xy=vec2u(pixel%size.x,pixel/size.x);
  physicalCloudTracePixel(xy);
}
@compute @workgroup_size(8,8) fn physicalCloudFullBuild(@builtin(global_invocation_id) id:vec3u) {
  if(any(id.xy>=textureDimensions(cloudViewOutput))) {return;}
  physicalCloudTracePixel(id.xy);
}
fn physicalCloudTracePixel(xy:vec2u) {
  let size=textureDimensions(cloudViewOutput);
  let uv=(vec2f(xy)+0.5)/vec2f(size);let f=atmosphereFrame.frame;let ray=cloudRay(uv,f);
  physicalShadowedAir=true;
  let color=physicalCloudRadiance(f.camera.xyz,ray,physicalCloudClearSky(ray),
    physicalCloudQuadraturePhase(xy),true,2.0*f.viewport.y/f32(size.y));
  textureStore(cloudViewOutput,xy,color);
  textureStore(cloudMomentsOutput,xy,vec4f(physicalCloudDepthMoments*0.001,0.0,0.0));
}
