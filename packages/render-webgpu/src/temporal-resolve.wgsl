struct Parameters { size:vec2u, valid:u32, exposure:f32 };
@group(0) @binding(0) var current:texture_2d<f32>;
@group(0) @binding(1) var motion:texture_2d<f32>;
@group(0) @binding(2) var depth:texture_depth_2d;
@group(0) @binding(3) var history:texture_2d<f32>;
@group(0) @binding(4) var historyMeta:texture_2d<f32>;
@group(0) @binding(8) var<uniform> params:Parameters;
fn compressed(rgb:vec3f)->vec3f {return rgb/(1.0+max(max(rgb.r,rgb.g),rgb.b));}
struct Resolved { color:vec4f, metadata:vec4f };
fn resolvePixel(pixel:vec2i)->Resolved {

 let center=colorAt(pixel,vec2i(0));let m=motionAt(pixel,vec2i(0));let identity=abs(m.w);
 let d=textureLoad(depth,pixel,0);let linearDepth=125.0/max(2500.0-d*2499.95,0.00001);
 var color=center;
 if(params.valid!=0u && identity>0.0 && m.z>0.0 && all(m.xy>vec2f(0.0)) && all(m.xy<vec2f(1.0))) {
  let samplePosition=m.xy*vec2f(params.size)-0.5;let base=vec2i(floor(samplePosition));let f=fract(samplePosition);
  // Reject each bilinear tap independently: silhouettes never borrow a neighbor's identity or depth.
  var prior=vec3f(0.0);var coverage=0.0;
  for(var y=0;y<2;y++){for(var x=0;x<2;x++){
   let p=base+vec2i(x,y);
   if(any(p<vec2i(0))||any(p>=vec2i(params.size))){continue;}
   let previousData=textureLoad(historyMeta,p,0);
   if(previousData.y!=identity||abs(previousData.x-m.z)>max(0.015,m.z*0.01)){continue;}
   let weight=select(1.0-f.x,f.x,x==1)*select(1.0-f.y,f.y,y==1);
   prior+=textureLoad(history,p,0).rgb*weight;coverage+=weight;
  }}
  if(coverage>0.1){
   prior/=coverage;
   var low=center;var high=center;
   for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
    let offset=vec2i(x,y);
    if(abs(motionAt(pixel,offset).w)!=identity){continue;}
    let c=colorAt(pixel,offset);low=min(low,c);high=max(high,c);
   }}
   let bounded=clamp(prior,low,high);
   let change=length(compressed(prior)-compressed(bounded));
   let speed=length(m.xy*vec2f(params.size)-(vec2f(pixel)+0.5));
   let stability=1.0-smoothstep(0.02,0.3,change);
   let weight=select(0.88,0.55,m.w<0.0)*stability*min(coverage,1.0)/(1.0+speed*0.025);
   color=mix(center,bounded,weight);
  }
 }
 return Resolved(vec4f(max(color,vec3f(0.0)),1.0),vec4f(linearDepth,identity,0.0,0.0));
}

fn displayResolved(color:vec4f)->vec4f {
 // Match the separate rgba16float history store/read before tone mapping.
 let rg=unpack2x16float(pack2x16float(color.rg));let ba=unpack2x16float(pack2x16float(color.ba));
 let x=max(vec3f(rg,ba.x)*params.exposure,vec3f(0));
 return vec4f(pow(clamp((x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14),vec3f(0),vec3f(1)),vec3f(1.0/2.2)),1.0);
}
