// Shared single-scattering GGX. All water and ordinary materials use the same
// height-correlated Smith model. The positive denominator avoids 1-(1-a²)nh².
const BRDF_PI:f32=3.141592653589793;
const BRDF_TAU:f32=6.283185307179586;
fn brdfGGX(n:vec3f,view:vec3f,light:vec3f,rough:f32,f0:vec3f)->vec3f {
  let nv=dot(n,view);let nl=dot(n,light);let sum=view+light;
  if(nv<=0.0||nl<=0.0||dot(sum,sum)<1e-12) {return vec3f(0.0);}
  let h=normalize(sum);let nh=dot(n,h);let tangent=cross(n,h);
  let a2=rough*rough*rough*rough;
  let q=dot(tangent,tangent)+a2*nh*nh;
  let sv=sqrt(a2+(1.0-a2)*nv*nv);let sl=sqrt(a2+(1.0-a2)*nl*nl);
  let fresnel=f0+(vec3f(1.0)-f0)*pow(clamp(1.0-dot(view,h),0.0,1.0),5.0);
  return a2*fresnel*nl/(2.0*BRDF_PI*q*q*(nl*sv+nv*sl));
}
struct PhaseGGX {
  view:vec3f,light:vec3f,matrix:vec3f,center:vec2f,delta:f32,a2:f32,fresnel:vec3f,
};
fn preparePhaseGGX(view:vec3f,light:vec3f,rough:f32,f0:vec3f)->PhaseGGX {
  var p:PhaseGGX;p.view=view;p.light=light;p.a2=rough*rough*rough*rough;
  let h=normalize(view+light);let beta=1.0-p.a2;
  let detA=h.y*h.y+p.a2*dot(h.xz,h.xz);
  p.matrix=vec3f(1.0-beta*h.x*h.x,-beta*h.x*h.z,1.0-beta*h.z*h.z);
  p.center=-beta*h.y*h.xz/detA;p.delta=p.a2/detA;
  p.fresnel=f0+(vec3f(1.0)-f0)*pow(clamp(1.0-dot(view,h),0.0,1.0),5.0);
  return p;
}
fn phaseBilinear(a:vec2f,b:vec2f,p:PhaseGGX)->f32 {
  return p.matrix.x*a.x*b.x+p.matrix.y*(a.x*b.y+a.y*b.x)+p.matrix.z*a.y*b.y;
}
fn phaseQuadratic(s:vec2f,p:PhaseGGX)->f32 {
  let d=s-p.center;return p.delta+phaseBilinear(d,d,p);
}
fn phaseNumerator(s:vec2f,p:PhaseGGX)->vec3f {
  let norm2=1.0+dot(s,s);let n=vec3f(-s.x,1.0,-s.y)*inverseSqrt(norm2);
  let nv=dot(n,p.view);let nl=dot(n,p.light);
  if(nv<=0.0||nl<=0.0) {return vec3f(0.0);}
  let sv=sqrt(p.a2+(1.0-p.a2)*nv*nv);let sl=sqrt(p.a2+(1.0-p.a2)*nl*nl);
  return p.a2*norm2*norm2*p.fresnel*nl/(2.0*BRDF_PI*(nl*sv+nv*sl));
}
fn phaseGGXResponse(s:vec2f,p:PhaseGGX)->vec3f {
  let q=phaseQuadratic(s,p);return phaseNumerator(s,p)/(q*q);
}
struct PhaseEllipse { mean:vec2f,a:vec2f,b:vec2f };
struct PhaseWarp {
  axis:vec2f,metric:f32,low:f32,high:f32,integral:f32,condition:f32,valid:u32,
};
fn phaseEllipseSlope(orbit:PhaseEllipse,axis:vec2f)->vec2f {
  return orbit.mean+orbit.a*axis.x+orbit.b*axis.y;
}
fn preparePhaseWarp(orbit:PhaseEllipse,p:PhaseGGX)->PhaseWarp {
  var q:PhaseWarp;q.valid=0u;
  let determinant=orbit.a.x*orbit.b.y-orbit.a.y*orbit.b.x;
  let scale=length(orbit.a)*length(orbit.b);
  if(scale<1e-12||abs(determinant)<=1e-6*scale) {return q;}
  let h=normalize(p.view+p.light);let detA=h.y*h.y+p.a2*dot(h.xz,h.xz);
  q.metric=abs(determinant)*sqrt(detA);
  let offset=p.center-orbit.mean;
  let x=vec2f(orbit.b.y*offset.x-orbit.b.x*offset.y,-orbit.a.y*offset.x+orbit.a.x*offset.y)/determinant;
  let radius=length(x);q.axis=vec2f(1.0,0.0);if(radius>0.0) {q.axis=x/radius;}
  let gamma2=p.delta/q.metric;
  q.low=gamma2+(radius-1.0)*(radius-1.0);q.high=gamma2+(radius+1.0)*(radius+1.0);
  let trace=phaseBilinear(orbit.a,orbit.a,p)+phaseBilinear(orbit.b,orbit.b,p);
  q.condition=(trace+sqrt(max(0.0,trace*trace-4.0*q.metric*q.metric)))/(2.0*q.metric);
  if(q.condition>8.0) {return q;}
  var axis=q.axis;var gradient=0.0;var curvature=0.0;
  for(var j=0u;j<6u;j++) {
    let s=phaseEllipseSlope(orbit,axis);let ds=s-p.center;let tangent=-orbit.a*axis.y+orbit.b*axis.x;
    gradient=2.0*phaseBilinear(tangent,ds,p);
    curvature=2.0*(phaseBilinear(tangent,tangent,p)+phaseBilinear(orbit.mean-s,ds,p));
    if(j==5u||curvature<=0.0) {break;}
    let step=clamp(-gradient/curvature,-0.5,0.5);
    axis=vec2f(axis.x-step*axis.y,axis.y+step*axis.x)*inverseSqrt(1.0+step*step);
  }
  if(curvature>0.0&&abs(gradient)<1e-5*max(curvature,1e-8)&&radius>0.55&&radius<1.6) {
    q.axis=axis;q.low=phaseQuadratic(phaseEllipseSlope(orbit,axis),p);q.high=q.low+2.0*curvature;q.metric=1.0;
  }
  let product=q.low*q.high;
  if(q.low<=0.0||product<1e-30) {return q;}
  q.integral=(q.low+q.high)*0.5/(q.metric*q.metric*product*sqrt(product));q.valid=1u;
  return q;
}
fn phaseWarpResponse(orbit:PhaseEllipse,p:PhaseGGX,q:PhaseWarp,nodes:u32,shift:f32)->vec3f {
  let ratio=q.low/q.high;var result=vec3f(0.0);
  for(var i=0u;i<nodes;i++) {
    let angle=BRDF_TAU*(f32(i)+shift)/f32(nodes);let cs=vec2f(cos(angle),sin(angle));
    let denominator=1.0+cs.x+ratio*(1.0-cs.x);
    let mapped=vec2f(1.0+cs.x-ratio*(1.0-cs.x),2.0*sqrt(ratio)*cs.y)/denominator;
    let axis=vec2f(mapped.x*q.axis.x-mapped.y*q.axis.y,mapped.y*q.axis.x+mapped.x*q.axis.y);
    let s=phaseEllipseSlope(orbit,axis);let qp=q.metric*2.0*q.low/denominator;let weight=qp/phaseQuadratic(s,p);
    result+=phaseNumerator(s,p)*weight*weight*denominator/(1.0+ratio);
  }
  return q.integral*result/f32(nodes);
}
// A finite phase tail uses the same smooth coordinate as a complete orbit.
// Inverse-map its endpoints, unwrap once, then retain the actual interval weight.
fn phaseWarpInterval(phaseStart:f32,width:f32,q:PhaseWarp)->vec2f {
  let relative=phaseStart-atan2(q.axis.y,q.axis.x);
  let start=atan2(sin(relative),cos(relative));let end=start+width;
  let root=sqrt(q.low/q.high);
  let psiStart=2.0*atan2(sin(start*0.5),root*cos(start*0.5));
  var psiEnd=2.0*atan2(sin(end*0.5),root*cos(end*0.5));
  var span=psiEnd-psiStart;span-=floor(span/BRDF_TAU)*BRDF_TAU;
  if(width>=BRDF_TAU) {span=BRDF_TAU;}
  return vec2f(psiStart,span);
}
fn phaseWarpIntervalResponse(orbit:PhaseEllipse,p:PhaseGGX,q:PhaseWarp,start:f32,width:f32,nodes:u32)->vec3f {
  if(width<=0.0) {return vec3f(0.0);}
  let interval=phaseWarpInterval(start,width,q);let ratio=q.low/q.high;var result=vec3f(0.0);
  for(var i=0u;i<nodes;i++) {
    let angle=interval.x+interval.y*(f32(i)+0.5)/f32(nodes);let cs=vec2f(cos(angle),sin(angle));
    let denominator=1.0+cs.x+ratio*(1.0-cs.x);
    let mapped=vec2f(1.0+cs.x-ratio*(1.0-cs.x),2.0*sqrt(ratio)*cs.y)/denominator;
    let axis=vec2f(mapped.x*q.axis.x-mapped.y*q.axis.y,mapped.y*q.axis.x+mapped.x*q.axis.y);
    let s=phaseEllipseSlope(orbit,axis);let qp=q.metric*2.0*q.low/denominator;let weight=qp/phaseQuadratic(s,p);
    result+=phaseNumerator(s,p)*weight*weight*denominator/(1.0+ratio);
  }
  return q.integral*result/f32(nodes)*interval.y/width;
}
