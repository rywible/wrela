struct WaterHistoryParameters { full:vec2u, size:vec2u, valid:u32, pad:vec3u };
@group(0) @binding(0) var current:texture_2d<f32>;
@group(0) @binding(1) var motion:texture_2d<f32>;
@group(0) @binding(2) var depth:texture_depth_2d;
@group(0) @binding(3) var history:texture_2d<f32>;
@group(0) @binding(4) var historyMeta:texture_2d<f32>;
@group(0) @binding(5) var nextColor:texture_storage_2d<rgba16float,write>;
@group(0) @binding(6) var nextMeta:texture_storage_2d<rg32float,write>;
@group(0) @binding(7) var correction:texture_storage_2d<rgba16float,write>;
@group(0) @binding(8) var<uniform> params:WaterHistoryParameters;
fn distanceAt(p:vec2i)->f32 {let d=textureLoad(depth,p,0);return 125.0/max(2500.0-d*2499.95,0.00001);}
fn compressedWater(rgb:vec3f)->vec3f {return rgb/(1.0+max(max(rgb.r,rgb.g),rgb.b));}
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) invocation:vec3u){
 let cell=invocation.xy;if(any(cell>=params.size)){return;}
 let first=cell*2u;let last=min(first+2u,params.full);
 var anchor=vec2i(first);var closest=1e20;var found=false;
 for(var y=first.y;y<last.y;y++){for(var x=first.x;x<last.x;x++){
  let p=vec2i(vec2u(x,y));let m=textureLoad(motion,p,0);let d=distanceAt(p);
  if(m.w<0.0&&d<closest){anchor=p;closest=d;found=true;}
 }}
 let at=vec2i(cell);
 if(!found){textureStore(nextColor,at,vec4f(0));textureStore(nextMeta,at,vec4f(0));textureStore(correction,at,vec4f(0));return;}
 let m=textureLoad(motion,anchor,0);let identity=-m.w;
 var center=vec3f(0);var count=0.0;var low=vec3f(1e20);var high=vec3f(0);
 for(var y=first.y;y<last.y;y++){for(var x=first.x;x<last.x;x++){
  let p=vec2i(vec2u(x,y));
  if(textureLoad(motion,p,0).w!=m.w||abs(distanceAt(p)-closest)>max(0.02,closest*0.02)){continue;}
  let c=textureLoad(current,p,0).rgb;center+=c;count+=1.0;low=min(low,c);high=max(high,c);
 }}
 center/=max(count,1.0);var delta=vec3f(0);
 if(params.valid!=0u&&m.z>0.0&&all(m.xy>vec2f(0))&&all(m.xy<vec2f(1))){
  let q=m.xy*vec2f(params.size)-0.5;let base=vec2i(floor(q));let f=fract(q);
  var prior=vec3f(0);var coverage=0.0;
  for(var y=0;y<2;y++){for(var x=0;x<2;x++){
   let p=base+vec2i(x,y);if(any(p<vec2i(0))||any(p>=vec2i(params.size))){continue;}
   let priorData=textureLoad(historyMeta,p,0);
   if(priorData.y!=identity||abs(priorData.x-m.z)>max(0.02,m.z*0.015)){continue;}
   let weight=select(1.0-f.x,f.x,x==1)*select(1.0-f.y,f.y,y==1);
   prior+=textureLoad(history,p,0).rgb*weight;coverage+=weight;
  }}
  if(coverage>0.25){
   prior/=coverage;
   // Foam births, glints and changing refracted objects reject stale radiance.
   let change=length(compressedWater(prior)-compressedWater(center));
   let reactive=1.0-smoothstep(0.015,0.18,change);
   let speed=length(m.xy*vec2f(params.full)-(vec2f(anchor)+0.5));
   let extent=max(vec3f(0.005),center*0.08);
   let bounded=clamp(prior,max(low,center-extent),min(high,center+extent));
   delta=(bounded-center)*(0.55*reactive*min(1.0,coverage)/(1.0+speed*0.1));
  }
 }
 textureStore(nextColor,at,vec4f(max(center+delta,vec3f(0)),1));
 textureStore(nextMeta,at,vec4f(closest,identity,0,0));
 textureStore(correction,at,vec4f(delta,0));
}
