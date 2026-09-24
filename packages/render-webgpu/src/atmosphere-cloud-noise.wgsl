// Analytic periodic field retained for reference comparisons. Eight lattice cells
// per period; the compiled texture samples eight times per lattice cell.
fn physicalCloudHash3(cell:vec3f)->f32 {
  let wrapped=cell-floor(cell/8.0)*8.0;
  var p=fract(wrapped*vec3f(0.1031,0.1030,0.0973));
  p+=dot(p,p.yxz+vec3f(33.33));
  return fract((p.x+p.y)*p.z);
}
fn physicalCloudNoiseReference3(position:vec3f)->f32 {
  let cell=floor(position);let f=fract(position);
  // Quintic interpolation has continuous first and second derivatives.
  let u=f*f*f*(f*(f*6.0-15.0)+10.0);
  let a=mix(physicalCloudHash3(cell),physicalCloudHash3(cell+vec3f(1.0,0.0,0.0)),u.x);
  let b=mix(physicalCloudHash3(cell+vec3f(0.0,1.0,0.0)),physicalCloudHash3(cell+vec3f(1.0,1.0,0.0)),u.x);
  let c=mix(physicalCloudHash3(cell+vec3f(0.0,0.0,1.0)),physicalCloudHash3(cell+vec3f(1.0,0.0,1.0)),u.x);
  let d=mix(physicalCloudHash3(cell+vec3f(0.0,1.0,1.0)),physicalCloudHash3(cell+vec3f(1.0)),u.x);
  return mix(mix(a,b,u.y),mix(c,d,u.y),u.z);
}
fn physicalCloudFeature(cell:vec3i)->vec3f {
  let wrapped=vec3u((cell%vec3i(8)+vec3i(8))%vec3i(8));
  var h=wrapped.x*0x9e3779b9u+wrapped.y*0x85ebca6bu+wrapped.z*0xc2b2ae35u;
  h=(h^(h>>16u))*0x7feb352du;
  let x=f32(h&0xffffu)/65535.0;
  h=(h^(h>>15u))*0x846ca68bu;
  let y=f32(h&0xffffu)/65535.0;
  h=(h^(h>>16u))*0x9e3779b9u;
  return vec3f(x,y,f32(h&0xffffu)/65535.0);
}
fn physicalCloudWorleyReference3(position:vec3f)->f32 {
  let cell=vec3i(floor(position));
  let fraction=fract(position);
  var squared=10.0;
  for(var z=-1;z<=1;z++) {
    for(var y=-1;y<=1;y++) {
      for(var x=-1;x<=1;x++) {
        let offset=vec3i(x,y,z);
        let delta=vec3f(offset)+physicalCloudFeature(cell+offset)-fraction;
        squared=min(squared,dot(delta,delta));
      }
    }
  }
  return 1.0-smoothstep(0.2,1.1,sqrt(squared));
}
