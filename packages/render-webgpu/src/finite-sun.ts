import type { RenderSurface } from "@wrela/model";

export const MAX_FINITE_SUN_SPHERES = 16;
export interface FiniteSunSphere {
  center: readonly [number, number, number];
  radius: number;
}
/** Exact semantic sphere only, never an interior/exterior proxy. The strict
 * transform test may reject rounded rotations; conventional shadows recover.
 * Current RenderMaterial is fully opaque; any future alpha path must reject. */
export function extractFiniteSunSphere(surface: RenderSurface): FiniteSunSphere | null {
  const product = surface.selectedRenderProduct;
  if (
    surface.castsShadow === false ||
    product?.kind !== "analytic-quadric" ||
    surface.skin ||
    surface.deformation ||
    surface.wind ||
    surface.water
  )
    return null;
  if (
    surface.drawRange &&
    (surface.drawRange.start !== 0 || surface.drawRange.count !== surface.mesh.indices.length)
  )
    return null;
  const shape = product.primitive,
    m = surface.matrix;
  if (
    ![...shape.center, ...shape.rotation, ...shape.radii].every(Number.isFinite) ||
    !(shape.radii[0] > 0) ||
    shape.radii[0] !== shape.radii[1] ||
    shape.radii[0] !== shape.radii[2]
  )
    return null;
  if (m.length !== 16 || !m.every(Number.isFinite) || m[3] !== 0 || m[7] !== 0 || m[11] !== 0 || m[15] !== 1)
    return null;
  const axes = [0, 4, 8].map((offset) => [m[offset], m[offset + 1], m[offset + 2]]);
  const dot = (a: number[], b: number[]) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const squared = axes.map((axis) => dot(axis, axis));
  if (
    !(squared[0] > 0) ||
    squared[0] !== squared[1] ||
    squared[0] !== squared[2] ||
    dot(axes[0], axes[1]) !== 0 ||
    dot(axes[0], axes[2]) !== 0 ||
    dot(axes[1], axes[2]) !== 0
  )
    return null;
  const c = shape.center;
  const center: [number, number, number] = [
    m[0] * c[0] + m[4] * c[1] + m[8] * c[2] + m[12],
    m[1] * c[0] + m[5] * c[1] + m[9] * c[2] + m[13],
    m[2] * c[0] + m[6] * c[1] + m[10] * c[2] + m[14],
  ];
  const radius = shape.radii[0] * Math.sqrt(squared[0]);
  return center.every(Number.isFinite) && Number.isFinite(radius) ? { center, radius } : null;
}
/** Caller must establish that every relevant blocker is an actual, rigid,
 * opaque sphere. A proxy, alpha surface, deformation, or missing blocker must
 * retain conventional shadowing. Header stores count; remaining vec4s spheres. */
export function packFiniteSunSpheres(spheres: readonly FiniteSunSphere[]): Float32Array {
  if (spheres.length > MAX_FINITE_SUN_SPHERES) throw new RangeError("Finite sun blocker budget exceeded");
  const data = new Float32Array((1 + MAX_FINITE_SUN_SPHERES) * 4);
  data[0] = spheres.length;
  spheres.forEach((sphere, i) => {
    if (
      !sphere.center.every((value) => Number.isFinite(value) && Math.abs(value) < 1e12) ||
      !Number.isFinite(sphere.radius) ||
      sphere.radius <= 0 ||
      sphere.radius > 1e12
    )
      throw new RangeError("Invalid finite sun sphere");
    data.set([...sphere.center, sphere.radius], (i + 1) * 4);
  });
  return data;
}
/** x = visible fraction, y = diffuse integral / sun solid angle,
 * z = valid (zero means ordinary-shadow fallback). No specular factorization. */
export const finiteSunWGSL = /* wgsl */ `
const COMPILED_SUN_PI: f32 = 3.141592653589793;
fn compiledSunArea(r: f32) -> f32 { let h=sin(r*0.5); return 4.0*COMPILED_SUN_PI*h*h; }
fn compiledSunSegment(x: f32) -> f32 {
  if(abs(x)>=0.25) { return x-sin(x)*cos(x); }
  let q=x*x;
  return x*q*(2.0/3.0+q*(-2.0/15.0+q*(4.0/315.0+q*(-2.0/2835.0+q*4.0/155925.0))));
}
fn compiledSunAngle(s: f32,d: f32,r: f32) -> f32 {
  return 2.0*asin(sqrt(clamp(sin(s-d)*sin(s-r)/(sin(r)*sin(d)),0.0,1.0)));
}
// Area followed by the vector first moment, in the caller's coordinate frame.
fn compiledSunLens(c1: vec3f,r1: f32,c2: vec3f,r2: f32) -> vec4f {
  let d=atan2(length(cross(c1,c2)),dot(c1,c2));
  if(d>=r1+r2) { return vec4f(0.0); }
  if(d+r1<=r2) { return vec4f(compiledSunArea(r1),COMPILED_SUN_PI*sin(r1)*sin(r1)*c1); }
  if(d+r2<=r1) { return vec4f(compiledSunArea(r2),COMPILED_SUN_PI*sin(r2)*sin(r2)*c2); }
  let s=(r1+r2+d)*0.5; let a=compiledSunAngle(s,d,r1); let b=compiledSunAngle(s,d,r2);
  let e=4.0*atan(sqrt(max(0.0,tan(s*0.5)*tan((s-r1)*0.5)*tan((s-r2)*0.5)*tan((s-d)*0.5))));
  let area=clamp(4.0*a*sin(r1*0.5)*sin(r1*0.5)+4.0*b*sin(r2*0.5)*sin(r2*0.5)-2.0*e,0.0,min(compiledSunArea(r1),compiledSunArea(r2)));
  let moment=sin(r1)*sin(r1)*compiledSunSegment(a)*c1+sin(r2)*sin(r2)*compiledSunSegment(b)*c2;
  return vec4f(area,moment);
}
fn compiledSphereSun(receiver: vec3f,normal: vec3f,sunDirection: vec3f,sunRadius: f32,sphere: vec4f) -> vec4f {
  if(!(sunRadius>=0.0001 && sunRadius<=0.1 && sphere.w>0.0) ||
     !all(abs(receiver)<vec3f(1e12)) || !all(abs(sphere.xyz)<vec3f(1e12)) ||
     !all(abs(normal)<vec3f(1e12)) || !all(abs(sunDirection)<vec3f(1e12)) ||
     !(length(normal)>0.000001 && length(sunDirection)>0.000001)) { return vec4f(0.0); }
  let sun=normalize(sunDirection); let n=normalize(normal);
  if(dot(n,sun)<=sin(sunRadius)+0.000002) { return vec4f(0.0); }
  let delta=sphere.xyz-receiver; let distance=length(delta);
  if(!(distance>sphere.w*1.000002)) { return vec4f(0.0); }
  let blocked=compiledSunLens(sun,sunRadius,delta/distance,asin(clamp(sphere.w/distance,0.0,1.0)));
  let total=compiledSunArea(sunRadius);
  let visibleMoment=COMPILED_SUN_PI*sin(sunRadius)*sin(sunRadius)*sun-blocked.yzw;
  return vec4f(clamp(1.0-blocked.x/total,0.0,1.0),max(0.0,dot(n,visibleMoment)/total),1.0,0.0);
}
`;
/** Optional storage-binding wrapper. Multiple partly overlapping blockers are
 * deliberately rejected; union visibility cannot be obtained by multiplication. */
export function createFiniteSunWGSL(group: number, binding: number): string {
  if (![group, binding].every((x) => Number.isInteger(x) && x >= 0 && x < 32))
    throw new RangeError("Invalid shader binding");
  return `${finiteSunWGSL}
@group(${group}) @binding(${binding}) var<storage,read> compiledSunSpheres: array<vec4f>;
fn compiledFiniteSun(receiver: vec3f,normal: vec3f,sunDirection: vec3f,sunRadius: f32) -> vec4f {
  if(!(sunRadius>=0.0001 && sunRadius<=0.1) || !(length(normal)>0.000001 && length(sunDirection)>0.000001) ||
     !all(abs(receiver)<vec3f(1e12)) || !all(abs(normal)<vec3f(1e12)) || !all(abs(sunDirection)<vec3f(1e12))) { return vec4f(0.0); }
  let sun=normalize(sunDirection); let n=normalize(normal);
  if(dot(n,sun)<=sin(sunRadius)+0.000002) { return vec4f(0.0); }
  if(arrayLength(&compiledSunSpheres)<1u) { return vec4f(0.0); }
  let count=u32(compiledSunSpheres[0].x);
  if(count>16u || arrayLength(&compiledSunSpheres)<count+1u) { return vec4f(0.0); }
  var result=vec4f(1.0,0.5*(1.0+cos(sunRadius))*dot(n,sun),1.0,0.0);
  var overlapping=0u;
  for(var i=0u;i<count;i++) {
    let sphere=compiledSunSpheres[i+1u]; let delta=sphere.xyz-receiver; let distance=length(delta);
    if(!(sphere.w>0.0 && distance>sphere.w*1.000002)) { return vec4f(0.0); }
    let radius=asin(clamp(sphere.w/distance,0.0,1.0));
    let separation=atan2(length(cross(sun,delta/distance)),dot(sun,delta/distance));
    if(separation>radius+sunRadius+0.000002) { continue; }
    overlapping+=1u;
    if(overlapping>1u) { return vec4f(0.0); }
    result=compiledSphereSun(receiver,n,sun,sunRadius,sphere);
  }
  return result;
}
`;
}
