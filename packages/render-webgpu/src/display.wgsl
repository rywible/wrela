struct Frame { values:array<vec4f,36> };
@group(0) @binding(0) var<uniform> frame:Frame;
@group(1) @binding(0) var sceneColor:texture_2d<f32>;
@group(1) @binding(1) var sceneSampler:sampler;
struct Screen { @builtin(position) position:vec4f, @location(0) uv:vec2f };
@vertex fn vertexMain(@builtin(vertex_index) index:u32)->Screen {
  let p=vec2f(f32((index<<1u)&2u),f32(index&2u));
  var out:Screen; out.position=vec4f(p*2.0-1.0,0,1); out.uv=vec2f(p.x,1.0-p.y); return out;
}
fn display(color:vec3f)->vec3f {
  let x=max(color*frame.values[15].y,vec3f(0));
  return pow(clamp((x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14),vec3f(0),vec3f(1)),vec3f(1.0/2.2));
}
fn luminance(color:vec3f)->f32 {return dot(color,vec3f(0.2126,0.7152,0.0722));}
@fragment fn fragmentMain(screen:Screen)->@location(0) vec4f {
  // Diagnostic captures preserve their exact encoding and full resolution, without AA or display transforms.
  let mode=frame.values[15].z;
  if(mode>0.5&&!(mode>9.5&&mode<10.5)) { return textureLoad(sceneColor,vec2i(screen.position.xy),0); }
  let texel=1.0/vec2f(textureDimensions(sceneColor));
  var color=display(textureSampleLevel(sceneColor,sceneSampler,screen.uv,0.0).xyz);
  // Spatial control and canonical captures use the edge filter. Temporal and
  // MSAA already reconstruct edges; do not blur their output a second time.
  if(frame.values[19].w<2.0) {
    let left=display(textureSampleLevel(sceneColor,sceneSampler,screen.uv-vec2f(texel.x,0),0.0).xyz);
    let right=display(textureSampleLevel(sceneColor,sceneSampler,screen.uv+vec2f(texel.x,0),0.0).xyz);
    let up=display(textureSampleLevel(sceneColor,sceneSampler,screen.uv-vec2f(0,texel.y),0.0).xyz);
    let down=display(textureSampleLevel(sceneColor,sceneSampler,screen.uv+vec2f(0,texel.y),0.0).xyz);
    let minimum=min(luminance(color),min(min(luminance(left),luminance(right)),min(luminance(up),luminance(down))));
    let maximum=max(luminance(color),max(max(luminance(left),luminance(right)),max(luminance(up),luminance(down))));
    if(maximum-minimum>max(0.04,maximum*0.15)) {
      let horizontal=abs(luminance(left)-luminance(right));
      let vertical=abs(luminance(up)-luminance(down));
      color=mix(color,select((left+right)*0.5,(up+down)*0.5,horizontal>vertical),0.35);
    }
  }
  return vec4f(color,1);
}
