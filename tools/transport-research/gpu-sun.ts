import { errorStats, type GpuContext, type Work } from "./gpu-common";
import { capArea, visibleSun } from "./sun-caps";

const common = /* wgsl */ `
const PI=3.141592653589793;
const SUN=0.00465;
@group(0) @binding(0) var<storage,read_write> result: array<f32>;
@group(0) @binding(1) var<storage,read> rays: array<vec4f>;
fn fullArea(r: f32) -> f32 { let s=sin(r*0.5); return 4.0*PI*s*s; }
fn triangleAngle(semi: f32, distance: f32, radius: f32) -> f32 {
  return 2.0*asin(sqrt(clamp(sin(semi-distance)*sin(semi-radius)/(sin(radius)*sin(distance)),0.0,1.0)));
}
fn segmentMoment(x: f32) -> f32 {
  if(abs(x)>=0.25) { return x-sin(x)*cos(x); }
  let q=x*x;
  return x*q*(2.0/3.0+q*(-2.0/15.0+q*(4.0/315.0+q*(-2.0/2835.0+q*4.0/155925.0))));
}
// Return the blocked area and X/Z first moments in the sun-centered frame.
fn lens(r: f32, d: f32) -> vec3f {
  if (d>=r+SUN) { return vec3f(0.0); }
  if (d+SUN<=r) { return vec3f(fullArea(SUN),0.0,PI*sin(SUN)*sin(SUN)); }
  let semi=(r+SUN+d)*0.5; let a=triangleAngle(semi,d,r); let b=triangleAngle(semi,d,SUN);
  let e=4.0*atan(sqrt(max(0.0,tan(semi*0.5)*tan((semi-r)*0.5)*tan((semi-SUN)*0.5)*tan((semi-d)*0.5))));
  let sr=sin(r); let ss=sin(SUN); let sd=sin(d); let cd=cos(d);
  let area=4.0*a*sin(r*0.5)*sin(r*0.5)+4.0*b*sin(SUN*0.5)*sin(SUN*0.5)-2.0*e;
  let moment=sr*sr*segmentMoment(a);
  let mx=moment*sd; let mz=moment*cd+ss*ss*segmentMoment(b);
  return vec3f(area,mx,mz);
}
fn planar(r: f32, d: f32) -> f32 {
  if (d>=r+SUN) { return 0.0; }
  if (d+SUN<=r) { return PI*SUN*SUN; }
  // Heron's product avoids 1-cos(theta) cancellation but this is planar geometry.
  let delta=sqrt(max(0.0,(r+SUN+d)*(r+SUN-d)*(d+r-SUN)*(d-r+SUN)));
  let a=atan2(delta,d*d+r*r-SUN*SUN); let b=atan2(delta,d*d+SUN*SUN-r*r);
  return r*r*a+SUN*SUN*b-0.5*delta;
}
`;

function kernel(mode: string, width: number) {
  const count = Number(mode.replace(/\D/g, "")) || 0;
  const body =
    mode === "lens"
      ? `if(d+SUN<=r) { for(var j=0u;j<4u;j++) { result[index*4u+j]=0.0; } return; }
      let overlap=lens(r,d); let total=fullArea(SUN);
      result[index*4u]=1.0-overlap.x/total;
      result[index*4u+1u]=-overlap.y/total; result[index*4u+2u]=0.0;
      result[index*4u+3u]=(PI*sin(SUN)*sin(SUN)-overlap.z)/total;`
      : mode === "planar"
        ? `result[index*4u]=1.0-planar(r,d)/(PI*SUN*SUN);
      result[index*4u+1u]=0.0; result[index*4u+2u]=0.0; result[index*4u+3u]=result[index*4u];`
        : `let center=vec3f(sin(d),0.0,cos(d)); let threshold=cos(r); var sum=vec4f(0.0);
      for(var j=0u;j<${count}u;j++) { let ray=rays[j].xyz;
        if(dot(center,ray)<threshold) { sum+=vec4f(1.0,ray); }
      } sum/=${count}.0; for(var j=0u;j<4u;j++) { result[index*4u+j]=sum[j]; }`;
  return `${common}
  @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3u) {
    let index=id.x; if(index>=${width * width}u) { return; }
    let x=(f32(index%${width}u)+0.37)/${width}.0; let y=(f32(index/${width}u)+0.61)/${width}.0;
    let r=0.02+0.18*y; let d=r+(2.0*x-1.0)*1.2*SUN; ${body}
  }`;
}

export async function sunExperiment(ctx: GpuContext) {
  const width = 128,
    count = width * width,
    radius = 0.00465;
  const truth = new Float32Array(count * 4);
  for (let i = 0; i < count; i++) {
    const x = ((i % width) + 0.37) / width,
      y = (Math.floor(i / width) + 0.61) / width;
    const r = 0.02 + 0.18 * y,
      d = r + (2 * x - 1) * 1.2 * radius;
    const value = visibleSun([0, 0, 1], radius, [Math.sin(d), 0, Math.cos(d)], r);
    truth.set([value.fraction, ...value.vector.map((x) => x / capArea(radius))], i * 4);
  }
  const output = ctx.buffer(new Float32Array(count * 4), GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
  const works: Record<string, Work> = {},
    buffers: GPUBuffer[] = [],
    compileMs: Record<string, number> = {};
  for (const mode of ["lens", "planar", "rays9", "rays64", "rays256"]) {
    const steps = Number(mode.replace(/\D/g, "")) || 1,
      samples = new Float32Array(steps * 4);
    for (let i = 0; i < steps; i++) {
      const height = (2 * Math.sin(radius / 2) ** 2 * (i + 0.5)) / steps,
        z = 1 - height;
      const radial = Math.sqrt(height * (2 - height)),
        angle = i * Math.PI * (3 - Math.sqrt(5));
      samples.set([radial * Math.cos(angle), radial * Math.sin(angle), z, 0], i * 4);
    }
    const rays = ctx.buffer(samples);
    buffers.push(rays);
    const start = performance.now(),
      pipeline = await ctx.pipeline(kernel(mode, width));
    compileMs[mode] = performance.now() - start;
    const entries = [{ binding: 0, resource: { buffer: output } }];
    if (mode.startsWith("rays")) entries.push({ binding: 1, resource: { buffer: rays } });
    const bind = ctx.device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries });
    works[mode] = (encoder, stamps) => {
      const pass = encoder.beginComputePass({ timestampWrites: stamps });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, bind);
      pass.dispatchWorkgroups(Math.ceil(count / 64));
      pass.end();
    };
  }
  const errors: Record<string, unknown> = {},
    images: Record<string, number[]> = { reference: Array.from(truth).filter((_, i) => i % 4 === 0) };
  const select = (v: Float32Array, channel: number) =>
    Float32Array.from(v.filter((_, i) => i % 4 === channel));
  for (const [mode, work] of Object.entries(works)) {
    const values = await ctx.values(work, output, count * 4);
    errors[mode] = {
      fraction: errorStats(select(values, 0), select(truth, 0)),
      vectorX: errorStats(select(values, 1), select(truth, 1)),
      vectorZ: errorStats(select(values, 3), select(truth, 3)),
    };
    images[mode] = Array.from(select(values, 0));
  }
  const times = await ctx.benchmark(works, 64);
  for (const buffer of [output, ...buffers]) buffer.destroy();
  return { width, count, sunRadius: radius, compileMs, times, errors, images };
}
