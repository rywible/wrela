import { integrateOrbit, orbitFixture } from "./coherent-ggx";
import { errorStats, type GpuContext, type Work } from "./gpu-common";

const common = /* wgsl */ `
const PI = 3.141592653589793;
const TAU = 6.283185307179586;
const OA = vec2f(0.095, 0.022);
const OB = vec2f(0.008, 0.115);
@group(0) @binding(0) var<storage, read_write> result: array<f32>;
@group(0) @binding(1) var<uniform> settings: vec4f;
struct Base { v: vec3f, l: vec3f, matrix: vec3f, center: vec2f, delta: f32, a2: f32, fresnel: f32 }
struct Proposal { axis: vec2f, metric: f32, low: f32, high: f32, radius: f32, integral: f32, acceptance: f32, control: f32 }
fn slope(axis: vec2f) -> vec2f { return OA * axis.x + OB * axis.y; }
fn bilinear(x: vec2f, y: vec2f, p: Base) -> f32 {
  return p.matrix.x*x.x*y.x + p.matrix.y*(x.x*y.y+x.y*y.x) + p.matrix.z*x.y*y.y;
}
fn quadratic(s: vec2f, p: Base) -> f32 { let t=s-p.center; return p.delta+bilinear(t,t,p); }
fn numerator(s: vec2f, p: Base) -> f32 {
  let norm2=1.0+dot(s,s); let inv=inverseSqrt(norm2);
  let nv=dot(vec3f(-s.x,1.0,-s.y),p.v)*inv; let nl=dot(vec3f(-s.x,1.0,-s.y),p.l)*inv;
  if (nv<=0.0 || nl<=0.0) { return 0.0; }
  let sv=sqrt(p.a2+(1.0-p.a2)*nv*nv); let sl=sqrt(p.a2+(1.0-p.a2)*nl*nl);
  return p.a2*norm2*norm2*p.fresnel*nl/(2.0*PI*(nl*sv+nv*sl));
}
fn response(s: vec2f, p: Base) -> f32 { let q=quadratic(s,p); return numerator(s,p)/(q*q); }
fn prepare(x: f32, y: f32) -> Base {
  var p: Base;
  p.v=normalize(vec3f(-0.22+0.45*x,0.9,-0.3+0.3*y));
  p.l=normalize(vec3f(0.12+0.4*y,0.9,0.04+0.35*x));
  let h=normalize(p.v+p.l); p.a2=settings.x*settings.x*settings.x*settings.x;
  let beta=1.0-p.a2; let detA=h.y*h.y+p.a2*dot(h.xz,h.xz);
  p.matrix=vec3f(1.0-beta*h.x*h.x,-beta*h.x*h.z,1.0-beta*h.z*h.z);
  p.center=-beta*h.y*h.xz/detA; p.delta=p.a2/detA;
  p.fresnel=0.02037+(1.0-0.02037)*pow(1.0-dot(p.v,h),5.0);
  return p;
}
fn propose(p: Base, refine: bool, control: bool) -> Proposal {
  let determinant=OA.x*OB.y-OA.y*OB.x;
  let x=vec2f(OB.y*p.center.x-OB.x*p.center.y,-OA.y*p.center.x+OA.x*p.center.y)/determinant;
  let radius=length(x); let h=normalize(p.v+p.l); let detA=h.y*h.y+p.a2*dot(h.xz,h.xz);
  var q: Proposal; q.metric=abs(determinant)*sqrt(detA); q.axis=vec2f(1.0,0.0);
  if (radius>0.0) { q.axis=x/radius; }
  let gamma2=p.delta/q.metric;
  q.low=gamma2+(radius-1.0)*(radius-1.0); q.high=gamma2+(radius+1.0)*(radius+1.0); q.radius=radius;
  if (refine) {
    var axis=q.axis; var gradient=0.0; var curvature=0.0;
    for (var j=0u;j<6u;j++) {
      let s=slope(axis); let ds=s-p.center; let tangent=-OA*axis.y+OB*axis.x;
      gradient=2.0*bilinear(tangent,ds,p);
      curvature=2.0*(bilinear(tangent,tangent,p)+bilinear(-s,ds,p));
      if (j==5u || curvature<=0.0) { break; }
      let step=clamp(-gradient/curvature,-0.5,0.5);
      axis=vec2f(axis.x-step*axis.y,axis.y+step*axis.x)*inverseSqrt(1.0+step*step);
    }
    if (curvature>0.0 && abs(gradient)<1e-5*max(curvature,1e-8) && radius>0.55 && radius<1.6) {
      q.axis=axis; q.low=quadratic(slope(axis),p); q.high=q.low+2.0*curvature;
      q.radius=curvature*0.5; q.metric=1.0;
    }
  }
  let mean=(q.low+q.high)*0.5; let product=q.low*q.high;
  q.integral=mean/(q.metric*q.metric*product*sqrt(product)); q.acceptance=mean/q.high; q.control=0.0;
  if (control) { let s=slope(q.axis); let ratio=q.metric*q.low/quadratic(s,p); q.control=numerator(s,p)*ratio*ratio; }
  return q;
}
fn random(state: ptr<function,u32>) -> f32 {
  *state+=0x9e3779b9u; var x=*state; x=(x^(x>>16u))*0x21f0aaadu;
  x=(x^(x>>15u))*0x735a2d97u; x=x^(x>>15u);
  return (f32(x>>9u)+0.5)/8388608.0;
}
fn sampleOrbit(p: Base, q: Proposal, state: ptr<function,u32>) -> f32 {
  let ratio=q.low/q.high; var axis=vec2f(1.0,0.0); var oneMinusCos=0.0; var accepted=false;
  for (var j=0u;j<4u;j++) {
    let t=tan(PI*(random(state)-0.5)); let t2=t*t; let z2=ratio*t2;
    axis=vec2f(1.0-z2,2.0*sqrt(ratio)*t)/(1.0+z2); oneMinusCos=2.0*z2/(1.0+z2);
    if (random(state)<(1.0+z2)/(1.0+t2)) { accepted=true; break; }
  }
  if (!accepted) { let theta=TAU*random(state); axis=vec2f(cos(theta),sin(theta)); oneMinusCos=1.0-axis.x; }
  let actual=vec2f(axis.x*q.axis.x-axis.y*q.axis.y,axis.y*q.axis.x+axis.x*q.axis.y);
  let s=slope(actual); let qp=q.metric*(q.low+2.0*q.radius*oneMinusCos); let weight=qp/quadratic(s,p);
  let missed=pow(1.0-q.acceptance,4.0); let mixture=1.0-missed+missed*q.integral*qp*qp;
  return q.integral*(q.control+(numerator(s,p)*weight*weight-q.control)/mixture);
}
`;

function kernel(mode: string, width: number, group: number) {
  const count = Number(mode.replace(/\D/g, ""));
  const warped = mode.startsWith("warp") || mode.startsWith("fixed");
  const rotation = [Math.cos((2 * Math.PI) / count), Math.sin((2 * Math.PI) / count)].map((x) =>
    Math.abs(x) < 1e-14 ? 0 : x,
  );
  const body = warped
    ? `let q=propose(p,true,false); let ratio=q.low/q.high;
    var state=index*391u+137u+u32(settings.y); let shift=${mode.startsWith("fixed") ? "0.5" : "random(&state)"};
    let psi=TAU*shift/${count}.0; var cs=vec2f(cos(psi),sin(psi)); var total=0.0;
    let rotation=vec2f(${rotation[0]},${rotation[1]});
    for (var j=0u;j<${count}u;j++) {
      let denominator=(1.0+cs.x)+ratio*(1.0-cs.x);
      let mapped=vec2f((1.0+cs.x)-ratio*(1.0-cs.x),2.0*sqrt(ratio)*cs.y)/denominator;
      let axis=vec2f(mapped.x*q.axis.x-mapped.y*q.axis.y,mapped.y*q.axis.x+mapped.x*q.axis.y);
      let s=slope(axis); let qp=q.metric*2.0*q.low/denominator; let weight=qp/quadratic(s,p);
      total+=numerator(s,p)*weight*weight*denominator/(1.0+ratio);
      cs=vec2f(cs.x*rotation.x-cs.y*rotation.y,cs.y*rotation.x+cs.x*rotation.y);
    } result[index]=q.integral*total/${count}.0;`
    : mode.startsWith("regular")
      ? `var total=0.0; for (var j=0u;j<${count}u;j++) { let angle=TAU*(f32(j)+0.5)/${count}.0; total+=response(slope(vec2f(cos(angle),sin(angle))),p); } result[index]=total/${count}.0;`
      : `let q=propose(p,${mode.startsWith("pole")},${mode.startsWith("pole")});
      ${count === 0 ? "result[index]=q.integral*q.control;" : `var total=0.0; var state=index*391u+137u+u32(settings.y); for(var j=0u;j<${count}u;j++) { total+=sampleOrbit(p,q,&state); } result[index]=total/${count}.0;`}`;
  return `${common}
  @compute @workgroup_size(${group}) fn main(@builtin(global_invocation_id) id: vec3u) {
    let index=id.x; if (index>=${width * width}u) { return; }
    let x=(f32(index%${width}u)+0.37)/${width}.0; let y=(f32(index/${width}u)+0.61)/${width}.0;
    let p=prepare(x,y); ${body}
  }`;
}

export async function coherentExperiment(ctx: GpuContext, roughness: number) {
  const width = 128,
    count = width * width;
  const output = ctx.buffer(new Float32Array(count), GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
  const settings = ctx.buffer(new Float32Array([roughness, 0, 0, 0]), GPUBufferUsage.UNIFORM);
  const works: Record<string, Work> = {};
  const compileMs: Record<string, number> = {};
  const modes = [
    "regular8",
    "regular64",
    "regular256",
    "balanced8",
    "pole0",
    "pole1",
    "pole4",
    "pole8",
    "warp2",
    "warp4",
    "warp8",
    "fixed2",
    "fixed4",
    "fixed8",
    "regular4096",
  ];
  for (const mode of modes)
    for (const group of mode === "pole1" ? [64, 128, 256] : [64]) {
      const start = performance.now(),
        pipeline = await ctx.pipeline(kernel(mode, width, group));
      const name = group === 64 ? mode : `${mode}wg${group}`;
      compileMs[name] = performance.now() - start;
      const bind = ctx.device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: output } },
          { binding: 1, resource: { buffer: settings } },
        ],
      });
      works[name] = (encoder, stamps) => {
        const pass = encoder.beginComputePass({ timestampWrites: stamps });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bind);
        pass.dispatchWorkgroups(Math.ceil(count / group));
        pass.end();
      };
    }
  const truth = await ctx.values(works.regular4096, output, count);
  const cpuAgreement = [];
  for (let i = 0; i < count; i += 997) {
    const x = ((i % width) + 0.37) / width,
      y = (Math.floor(i / width) + 0.61) / width;
    const { orbit, lighting } = orbitFixture(x, y, roughness),
      cpu = integrateOrbit(orbit, lighting, 32768);
    cpuAgreement.push({ index: i, gpu: truth[i], cpu, absoluteError: Math.abs(truth[i] - cpu) });
  }
  let peakIndex = 0;
  truth.forEach((v, i) => {
    if (v > truth[peakIndex]) peakIndex = i;
  });
  const peak = orbitFixture(
    ((peakIndex % width) + 0.37) / width,
    (Math.floor(peakIndex / width) + 0.61) / width,
    roughness,
  );
  const peakCpu = integrateOrbit(peak.orbit, peak.lighting, 65536);
  cpuAgreement.push({
    index: peakIndex,
    gpu: truth[peakIndex],
    cpu: peakCpu,
    absoluteError: Math.abs(truth[peakIndex] - peakCpu),
  });
  const errors: Record<string, ReturnType<typeof errorStats>> = {};
  const images: Record<string, number[]> = { reference: Array.from(truth) };
  for (const [name, work] of Object.entries(works)) {
    if (name === "regular4096") continue;
    const values = await ctx.values(work, output, count);
    errors[name] = errorStats(values, truth);
    images[name] = Array.from(values);
  }
  const { regular4096: _, ...timed } = works;
  const times = await ctx.benchmark(timed, 64);
  // Repeat seeds: a single noisy image does not establish expected quality.
  const replicates: Record<string, number[]> = {
    pole1: [],
    pole4: [],
    pole8: [],
    warp2: [],
    warp4: [],
    warp8: [],
  };
  for (let seed = 1; seed <= 32; seed++) {
    ctx.device.queue.writeBuffer(settings, 0, new Float32Array([roughness, seed * 7919, 0, 0]));
    for (const name of Object.keys(replicates)) {
      const values = await ctx.values(works[name], output, count);
      replicates[name].push(errorStats(values, truth).rms);
    }
  }
  const result = {
    roughness,
    width,
    count,
    referenceSamples: 4096,
    referencePeak: truth[peakIndex],
    cpuAgreement,
    compileMs,
    times,
    errors,
    replicates,
    images,
  };
  output.destroy();
  settings.destroy();
  return result;
}
