// Exact authored relief bands shared with compiler/surface-relief-pattern.ts.
// Geometry carries resolvable depth. This evaluates only its filtered residual.
fn reliefHash(input:u32)->f32 {
  var h=(input^(input>>16u))*0x7feb352du;
  h=(h^(h>>15u))*0x846ca68bu;
  return f32((h^(h>>16u))>>8u)/16777216.0;
}
fn reliefNoise(point:vec3f,seed:u32)->f32 {
  let base=vec3i(floor(point));let f=fract(point);let s=f*f*(3.0-2.0*f);
  var value=0.0;
  for(var corner=0u;corner<8u;corner++) {
    let delta=vec3i(i32(corner&1u),i32((corner>>1u)&1u),i32((corner>>2u)&1u));
    let q=vec3u(base+delta);
    let weight=select(vec3f(1.0)-s,s,delta==vec3i(1));
    value+=reliefHash((q.x*73856093u)^(q.y*19349663u)^(q.z*83492791u)^seed)*weight.x*weight.y*weight.z;
  }
  return value;
}
fn reliefBands(position:vec3f,normal:vec3f)->vec4f {
  let point=position/obj.relief.z;let seed=u32(obj.relief.w)|(u32(obj.reliefDirection.w)<<16u);
  if(obj.relief.x>1.5) {
    let broad=reliefNoise(point,seed);
    let chips=pow(max(0.0,(reliefNoise(point*2.7,seed+37u)-0.43)/0.57),1.5);
    let fine=reliefNoise(point*7.1,seed+101u);
    return vec4f((broad-0.5)*0.28,(chips-0.15)*0.43,(fine-0.5)*0.21,0.3895);
  }
  let axis=normalize(obj.reliefDirection.xyz);
  let u=normalize(cross(axis,select(vec3f(1,0,0),vec3f(0,1,0),abs(axis.y)<0.9)));
  let v=cross(axis,u);let a=dot(point,u);let b=dot(point,v);let along=dot(point,axis);
  let warp=sin(along*0.27+obj.reliefGeometry.w)*0.32+reliefNoise(vec3f(a*0.5,along*0.09,b*0.5),seed)*0.6;
  let wu=pow(abs(dot(normal,u)),4.0);let wv=pow(abs(dot(normal,v)),4.0);let denominator=max(0.00000001,wu+wv);
  let coefficients=array<f32,8>(22880.0,16016.0,8736.0,3640.0,1120.0,240.0,32.0,2.0);
  var bands=vec3f(0.0);
  for(var harmonic=1u;harmonic<=8u;harmonic++) {
    let wave=(cos(f32(harmonic)*((b+warp)*6.28318530718-1.57079632679))*wu+cos(f32(harmonic)*((a+warp)*6.28318530718-1.57079632679))*wv)/denominator;
    let band=select(select(2u,1u,harmonic<=3u),0u,harmonic==1u);
    bands[band]+=0.79*coefficients[harmonic-1u]/65536.0*wave;
  }
  bands.z+=(reliefNoise(vec3f(a*3.7,along*0.7,b*3.7),seed+73u)-0.5)*0.175;
  return vec4f(bands,0.035+0.79*(12870.0/65536.0)*(wu+wv)/denominator+0.0875);
}
// Return filtered signed bump (metres) and estimated unresolved tangent slope variance.
fn authoredReliefResidual(position:vec3f,normal:vec3f,footprint:f32)->vec2f {
  let frequencies=select(vec3f(1.0,3.0,8.0),vec3f(1.0,2.7,7.1),obj.relief.x>1.5);
  let visibility=vec3f(1.0)-smoothstep(vec3f(0.2),vec3f(0.65),frequencies*footprint/obj.relief.z);
  let residual=obj.reliefResidual.xyz;
  let variance=dot(max(obj.reliefSlopeVariance.xyz,vec3f(0.0)),residual*residual*(vec3f(1.0)-visibility*visibility));
  if(all(residual*visibility<vec3f(0.000001))){return vec2f(0.0,variance);}
  let bands=reliefBands(position,normalize(normal));
  let geometry=clamp(bands.w+dot(bands.xyz,obj.reliefGeometry.xyz),0.0,1.0);
  let resolved=clamp(bands.w+dot(bands.xyz,obj.reliefGeometry.xyz+residual*visibility),0.0,1.0);
  return vec2f(-obj.relief.y*(resolved-geometry),variance);
}
