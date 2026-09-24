@group(0) @binding(5) var output:texture_storage_2d<rgba16float,write>;
@group(0) @binding(6) var outputMeta:texture_storage_2d<rg32float,write>;
var<workgroup> tileColor:array<vec3f,100>;
var<workgroup> tileMotion:array<vec4f,100>;
fn tileIndex(pixel:vec2i,offset:vec2i)->i32 {return (pixel.y%8+1+offset.y)*10+pixel.x%8+1+offset.x;}
fn colorAt(pixel:vec2i,offset:vec2i)->vec3f {return tileColor[tileIndex(pixel,offset)];}
fn motionAt(pixel:vec2i,offset:vec2i)->vec4f {return tileMotion[tileIndex(pixel,offset)];}
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u,@builtin(local_invocation_index) lane:u32,@builtin(workgroup_id) group:vec3u) {
 // Each 8x8 group loads a shared 10x10 neighborhood once. Partial edge groups
 // still participate in the barrier before inactive output lanes return.
 for(var i=lane;i<100u;i+=64u){
  let p=clamp(vec2i(group.xy*8u)+vec2i(vec2u(i%10u,i/10u))-vec2i(1),vec2i(0),vec2i(params.size)-1);
  tileColor[i]=textureLoad(current,p,0).rgb;tileMotion[i]=textureLoad(motion,p,0);
 }
 workgroupBarrier();
 if(any(id.xy>=params.size)){return;}
 let r=resolvePixel(vec2i(id.xy));
 textureStore(output,vec2i(id.xy),r.color);
 textureStore(outputMeta,vec2i(id.xy),r.metadata);
}
