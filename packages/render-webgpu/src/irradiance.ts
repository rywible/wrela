/** Real SH basis, orthonormal over the sphere. Irradiance/pi convolution has
 * band factors 1, 2/3, 1/4. The sun disk is absent from the source sky texture. */
export function irradianceBasis([x, y, z]: readonly number[]) {
  return [
    0.2820947918,
    0.4886025119 * y,
    0.4886025119 * z,
    0.4886025119 * x,
    1.0925484306 * x * y,
    1.0925484306 * y * z,
    0.3153915653 * (3 * z * z - 1),
    1.0925484306 * x * z,
    0.5462742153 * (x * x - y * y),
  ];
}
export const irradianceBasisWGSL = `
fn irradianceBasis(n:vec3f,index:u32)->f32 {
 switch index {
 case 0u: {return 0.2820947918;}
 case 1u: {return 0.4886025119*n.y;}
 case 2u: {return 0.4886025119*n.z;}
 case 3u: {return 0.4886025119*n.x;}
 case 4u: {return 1.0925484306*n.x*n.y;}
 case 5u: {return 1.0925484306*n.y*n.z;}
 case 6u: {return 0.3153915653*(3.0*n.z*n.z-1.0);}
 case 7u: {return 1.0925484306*n.x*n.z;}
 default: {return 0.5462742153*(n.x*n.x-n.y*n.y);}
 }
}`;
export const irradianceBuildWGSL = `
@group(0) @binding(0) var sky:texture_2d<f32>;
@group(0) @binding(1) var linearSampler:sampler;
@group(0) @binding(2) var<storage,read_write> coefficients:array<vec4f>;
var<workgroup> sums:array<vec3f,64>;
${irradianceBasisWGSL}
@compute @workgroup_size(64) fn main(@builtin(local_invocation_index) lane:u32,@builtin(workgroup_id) group:vec3u) {
 var sum=vec3f(0.0);
 for(var i=lane;i<1024u;i+=64u) {
  let y=1.0-2.0*(f32(i)+0.5)/1024.0;
  let azimuth=f32(i)*2.3999632297;
  let radius=sqrt(max(0.0,1.0-y*y));let n=vec3f(radius*cos(azimuth),y,radius*sin(azimuth));
  let elevation=asin(y)/1.57079632679;
  let uv=vec2f(atan2(n.z,n.x)/6.28318530718+0.5,0.5+0.5*sign(elevation)*sqrt(abs(elevation)));
  sum+=textureSampleLevel(sky,linearSampler,uv,0.0).xyz*irradianceBasis(n,group.x);
 }
 sums[lane]=sum;workgroupBarrier();
 for(var stride=32u;stride>0u;stride/=2u) {if(lane<stride){sums[lane]+=sums[lane+stride];}workgroupBarrier();}
 if(lane==0u){let band=select(select(1.0,0.6666666667,group.x>0u),0.25,group.x>3u);coefficients[group.x]=vec4f(sums[0]*(12.5663706144/1024.0)*band,0.0);}
}`;
