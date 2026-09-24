@group(0) @binding(0) var correction:texture_2d<f32>;
@group(0) @binding(1) var metadata:texture_2d<f32>;
@group(0) @binding(2) var motion:texture_2d<f32>;
@group(0) @binding(3) var depth:texture_depth_2d;
@vertex fn vertexMain(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let uv=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(uv*2.0-1.0,0,1);
}
@fragment fn fragmentMain(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let at=vec2i(pixel.xy);let m=textureLoad(motion,at,0);if(m.w>=0.0){discard;}
 let d=textureLoad(depth,at,0);let distance=125.0/max(2500.0-d*2499.95,0.00001);
 let size=vec2i(textureDimensions(correction));let q=pixel.xy*0.5-0.5;
 let base=vec2i(floor(q));let f=fract(q);var delta=vec3f(0);var coverage=0.0;
 for(var y=0;y<2;y++){for(var x=0;x<2;x++){
  let p=base+vec2i(x,y);if(any(p<vec2i(0))||any(p>=size)){continue;}
  let priorData=textureLoad(metadata,p,0);
  if(priorData.y != -m.w||abs(priorData.x-distance)>max(0.02,distance*0.02)){continue;}
  let weight=select(1.0-f.x,f.x,x==1)*select(1.0-f.y,f.y,y==1);
  delta+=textureLoad(correction,p,0).rgb*weight;coverage+=weight;
 }}
 // Add only the low-frequency correction, retaining full-resolution detail.
 return vec4f(delta/max(coverage,1.0),0);
}
