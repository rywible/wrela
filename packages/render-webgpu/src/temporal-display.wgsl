// Resolve and display in one raster pass. The HDR history remains scene-linear.
fn colorAt(pixel:vec2i,offset:vec2i)->vec3f {return textureLoad(current,clamp(pixel+offset,vec2i(0),vec2i(params.size)-1),0).rgb;}
fn motionAt(pixel:vec2i,offset:vec2i)->vec4f {return textureLoad(motion,clamp(pixel+offset,vec2i(0),vec2i(params.size)-1),0);}
@vertex fn vertexMain(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
 let p=vec2f(f32((index<<1u)&2u),f32(index&2u));return vec4f(p*2.0-1.0,0.0,1.0);
}
struct Presented { @location(0) color:vec4f, @location(1) metadata:vec2f, @location(2) display:vec4f };
@fragment fn fragmentMain(@builtin(position) position:vec4f)->Presented {
 let r=resolvePixel(vec2i(position.xy));
 return Presented(r.color,r.metadata.xy,displayResolved(r.color));
}
