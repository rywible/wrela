import brdf from "@wrela/render-webgpu/brdf.wgsl" with { type: "text" };
import waterProgram from "@wrela/render-webgpu/water.wgsl" with { type: "text" };
import { createWaterBudgetWGSL } from "@wrela/render-webgpu/water-budget";
import waterGlints from "@wrela/render-webgpu/water-glints.wgsl" with { type: "text" };

type V3 = [number, number, number];
const normalize = (v: V3): V3 => {
  const length = Math.hypot(...v);
  return v.map((value) => value / length) as V3;
};
const dot = (a: V3, b: V3) => a.reduce((sum, value, index) => sum + value * b[index], 0);
export type WaterReferenceQuery = {
  roughness: number;
  shutter: number;
  time: number;
  direction: number;
  view: V3;
  light: V3;
  quality: 1 | 2 | 3;
  label: string;
  secondSpeed?: number;
  footprint?: number;
  coherent?: boolean;
  compiled?: boolean;
};
export function waterReferenceQueries(): WaterReferenceQuery[] {
  const queries: WaterReferenceQuery[] = [];
  for (const roughness of [0.06, 0.18, 0.5])
    for (const shutter of [1, 2.25])
      for (const direction of [Math.PI / 2, 0.0000001])
        for (let index = 0; index < 4; index++) {
          const angle = (index * Math.PI) / 2 + 0.173,
            view = normalize([0.15 * Math.cos(angle + 0.4), 1, 0.15 * Math.sin(angle + 0.4)]);
          const half = normalize([-0.1 * Math.cos(angle), 1, 0.1 * Math.sin(angle)]);
          const light = normalize(half.map((value, axis) => 2 * dot(view, half) * value - view[axis]) as V3);
          queries.push({
            roughness,
            shutter,
            time: index * 0.133,
            direction,
            view,
            light,
            quality: 2,
            label: `roughness-${roughness}/shutter-${shutter}/${direction < 0.01 ? "degenerate" : "coherent"}/phase-${index}`,
          });
        }
  for (const roughness of [0.06, 0.18, 0.5])
    for (const footprint of [0.005, 0.02, 0.08])
      for (const height of [0.15, 1])
        for (const offset of [0, 0.4]) {
          const view = normalize([0.3, height, 0.15]);
          const half = normalize([-0.1 + offset, 1, 0]);
          const light = normalize(half.map((value, axis) => 2 * dot(view, half) * value - view[axis]) as V3);
          queries.push({
            roughness,
            footprint,
            shutter: 1 / 60,
            time: 0,
            direction: Math.PI / 2,
            view,
            light,
            quality: 2,
            secondSpeed: 0.8,
            coherent: false,
            label: `ordinary/roughness-${roughness}/footprint-${footprint}/height-${height}/offset-${offset}`,
          });
        }
  for (const time of [0.03, 0.09, 0.17, 0.3])
    for (const footprint of [0.002, 0.006, 0.012])
      for (const roughness of [0.06, 0.08, 0.12]) {
        const view = normalize([0.3, 1, 0.15]);
        const slope = [
          0.1 * Math.cos(-2 * Math.PI * time),
          0.1 * Math.cos(-2 * Math.PI * 0.8 * time + Math.PI / 2),
        ];
        const half = normalize([-slope[0], 1, -slope[1]]);
        const light = normalize(half.map((v, i) => 2 * dot(view, half) * v - view[i]) as V3);
        queries.push({
          roughness,
          footprint,
          shutter: 0.001,
          time,
          direction: Math.PI / 2,
          view,
          light,
          quality: 2,
          secondSpeed: 0.8,
          coherent: false,
          compiled: true,
          label: `compiled/time-${time}/footprint-${footprint}/roughness-${roughness}`,
        });
      }
  return queries;
}
/** Independent canonical normal-vector GGX, no phase factorization, pole fit, or source polynomial. */
function referenceResponse(query: WaterReferenceQuery, time: number, x = 0, z = 0): V3 {
  const k = 2 * Math.PI,
    amplitude = 0.1 / k;
  let sx = 0,
    sz = 0;
  for (const [direction, phase, speed] of [
    [0, 0, 1],
    [query.direction, Math.PI / 2, query.secondSpeed ?? 1],
  ]) {
    const coefficient =
      amplitude *
      k *
      Math.cos(k * (x * Math.cos(direction) + z * Math.sin(direction) - speed * time) + phase);
    sx += coefficient * Math.cos(direction);
    sz += coefficient * Math.sin(direction);
  }
  const n = normalize([-sx, 1, -sz]),
    view = normalize([query.view[0] * 10 - x, query.view[1] * 10, query.view[2] * 10 - z]),
    nv = Math.max(0, Math.min(1, dot(n, view))),
    nl = Math.max(0, dot(n, query.light));
  const reflectedFresnel = 0.02037 + 0.97963 * (1 - nv) ** 5;
  const sky: V3 = [0.2, 0.3, 0.4],
    sun: V3 = [1, 0.9, 0.7];
  const result = sky.map((value) => value * reflectedFresnel) as V3;
  if (nv > 0 && nl > 0) {
    const h = normalize(view.map((value, index) => value + query.light[index]) as V3),
      nh = dot(n, h),
      a2 = query.roughness ** 4;
    const denominator = 1 - nh * nh + a2 * nh * nh;
    const distribution = a2 / (Math.PI * denominator * denominator);
    const masking =
      (2 * nv * nl) / (nl * Math.sqrt(a2 + (1 - a2) * nv * nv) + nv * Math.sqrt(a2 + (1 - a2) * nl * nl));
    const fresnel = 0.02037 + 0.97963 * Math.max(0, Math.min(1, 1 - dot(view, h))) ** 5;
    const reflected = (distribution * masking * fresnel) / (4 * nv);
    for (let axis = 0; axis < 3; axis++) result[axis] += reflected * sun[axis] * 3;
  }
  return result;
}
export function integrateWaterReference(query: WaterReferenceQuery, samples: number): V3 {
  if (!Number.isInteger(samples) || samples < 16 || samples > 131072)
    throw Error("Reference sample budget invalid");
  const sum: V3 = [0, 0, 0];
  for (let index = 0; index < samples; index++) {
    const value = referenceResponse(query, query.time + query.shutter * ((index + 0.5) / samples - 0.5));
    for (let axis = 0; axis < 3; axis++) sum[axis] += value[axis] / samples;
  }
  return sum;
}
/** Independent tensor Gauss-Legendre integration of the actual spatial/shutter box. */
function gaussRule(order: number) {
  const rule: { x: number; w: number }[] = [];
  for (let i = 0; i < order; i++) {
    let z = Math.cos((Math.PI * (i + 0.75)) / (order + 0.5)),
      derivative = 0;
    for (let iteration = 0; iteration < 30; iteration++) {
      let a = 1,
        b = 0;
      for (let j = 1; j <= order; j++) {
        const previous = a;
        a = ((2 * j - 1) * z * a - (j - 1) * b) / j;
        b = previous;
      }
      derivative = (order * (z * a - b)) / (z * z - 1);
      const next = z - a / derivative;
      if (Math.abs(next - z) < 1e-15) {
        z = next;
        break;
      }
      z = next;
    }
    rule.push({ x: z * 0.5, w: 1 / ((1 - z * z) * derivative * derivative) });
  }
  return rule;
}
export function integrateWaterBoxReference(
  query: WaterReferenceQuery,
  order: number | readonly [number, number, number],
): V3 {
  const counts = typeof order === "number" ? [order, order, order] : order;
  const [xRule, zRule, tRule] = counts.map(gaussRule),
    sum: V3 = [0, 0, 0],
    footprint = query.footprint ?? 0;
  for (const x of xRule)
    for (const z of zRule)
      for (const t of tRule) {
        const value = referenceResponse(
            query,
            query.time + t.x * query.shutter,
            x.x * footprint,
            z.x * footprint,
          ),
          weight = x.w * z.w * t.w;
        for (let axis = 0; axis < 3; axis++) sum[axis] += value[axis] * weight;
      }
  return sum;
}
/** The phase integrator runs unchanged against controlled constant-sky, Schlick
 * environment transport and unoccluded sun. Production GGX environment LUT
 * accuracy is verified separately by the lighting-upgrade fixture. */
export async function validateWaterReference(device: GPUDevice) {
  const queries = waterReferenceQueries(),
    data = new Float32Array(queries.length * 16);
  for (const [index, q] of queries.entries())
    data.set(
      [
        ...q.view,
        q.roughness,
        ...q.light,
        q.shutter,
        q.time,
        q.direction,
        q.quality,
        q.secondSpeed ?? 1,
        q.footprint ?? 0,
        q.coherent === false ? 0 : 1,
        q.compiled ? 1 : 0,
        0,
      ],
      index * 16,
    );
  const code = `${brdf}
struct Point {position:vec4f,color:vec4f};
struct Global {camera:vec4f,sun:vec4f,sunlight:vec4f,viewport:vec4f,params:vec4f,points:array<Point,8>};
struct Wave {shape:vec4f,phase:vec4f};
struct Object {flags:vec4f,waves:array<Wave,8>,glintWaves:array<vec4f,2>,glintTime:vec4f,glintInverse:vec4f,localLighting:vec4f};
var<private> g:Global;var<private> obj:Object;
fn skyColor(direction:vec3f)->vec3f{return vec3f(0.2,0.3,0.4);}
fn physicalSkyWithoutSun(world:vec3f,ray:vec3f)->vec3f{return vec3f(0.2,0.3,0.4);}
fn physicalSky(world:vec3f,direction:vec3f)->vec3f{return vec3f(0.2,0.3,0.4);}
fn shadow(world:vec3f,normal:vec3f)->f32{return 1.0;}
fn waterGlintVisibility(world:vec3f,dx:vec3f,dy:vec3f)->f32{return 1.0;}
fn physicalSunTransmittance(world:vec3f)->vec3f{return vec3f(1.0);}
fn physicalCachedSunTransmittance(world:vec3f)->vec3f{return vec3f(1.0);}
fn physicalSkyRoughReflection(ray:vec3f,rough:f32)->vec3f{return vec3f(0.2,0.3,0.4);}
fn environmentSpecularWeight(nv:f32,rough:f32,f0:vec3f)->vec3f{return f0+(vec3f(1.0)-f0)*pow(1.0-nv,5.0);}
fn localLightAttenuation(distanceSquared:f32,rangeSquared:f32)->f32{return 1.0/(1.0+distanceSquared);}
fn localLightVisibility(index:u32,world:vec3f,n:vec3f)->f32{return 1.0;}
fn waterSampleFiltered(xz:vec2f,spacing:f32)->vec3f{return vec3f(0.0);}
fn waterTransportWithSky(world:vec3f,normal:vec3f,view:vec3f,base:vec3f,rough:f32,integratedSky:vec3f)->vec3f{return integratedSky;}
${createWaterBudgetWGSL()}
${waterProgram}
${waterGlints}
struct Query {viewRough:vec4f,lightShutter:vec4f,source:vec4f,unused:vec4f};
@group(0) @binding(0) var<storage,read> queries:array<Query>;
@group(0) @binding(1) var<storage,read_write> output:array<vec4f>;
@compute @workgroup_size(32) fn main(@builtin(global_invocation_id) id:vec3u) {
 if(id.x>=arrayLength(&queries)){return;}let q=queries[id.x];if(WATER_COHERENT!=(q.unused.y>0.5)){return;}
 g.camera=vec4f(q.viewRough.xyz*10.0,1.0);g.sun=vec4f(q.lightShutter.xyz,3.0);g.sunlight=vec4f(1.0,0.9,0.7,0.0);g.params=vec4f(q.source.x,0.0,0.0,0.0);
 obj.flags=vec4f(0.0,0.0,0.0,2.0);
 obj.waves[0].shape=vec4f(0.1/BRDF_TAU,1.0,1.0,0.0);obj.waves[0].phase=vec4f(0.0,0.0,q.source.z,0.0);
 obj.waves[1].shape=vec4f(0.1/BRDF_TAU,1.0,q.source.w,q.source.y);obj.waves[1].phase=vec4f(BRDF_PI*0.5,q.lightShutter.w,0.0,0.0);
 if(q.unused.z>0.5){
  obj.glintWaves[0]=vec4f(0.1,0.0,BRDF_TAU,0.0);
  obj.glintWaves[1]=vec4f(0.1*cos(q.source.y),0.1*sin(q.source.y),BRDF_TAU*cos(q.source.y),BRDF_TAU*sin(q.source.y));
  obj.glintTime=vec4f(-BRDF_TAU,-BRDF_TAU*q.source.w,1.0,0.0);
  obj.glintInverse=vec4f(10.0,-10.0*cos(q.source.y)/sin(q.source.y),0.0,10.0/sin(q.source.y));
 }
 let world=vec3f(0.0);let base=vec3f(0.08,0.25,0.3);let value=integratedWaterResponse(world,vec3f(q.unused.x,0.0,0.0),vec3f(0.0,0.0,q.unused.x),base,q.viewRough.w,0.0);
 let box=prepareWaterPhaseBox(world,vec3f(0.0),vec3f(0.0));let orbit=waterConditionalOrbit(box,2u,vec3f(0.0));
 let selected=waterConditionalResponse(world,orbit,abs(box.dt[0]),base,q.viewRough.w,0.0,u32(q.source.z));
 let compiled=compiledWaterResponse(world,vec3f(q.unused.x,0.0,0.0),vec3f(0.0,0.0,q.unused.x),base,q.viewRough.w,0.0);
 output[id.x]=vec4f(value,select(select(-1.0,f32(selected.path),WATER_COHERENT),4.0,compiled.valid));
}`;
  const input = device.createBuffer({
      size: data.byteLength,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    }),
    output = device.createBuffer({
      size: queries.length * 16,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
    }),
    read = device.createBuffer({
      size: queries.length * 16,
      usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });
  try {
    const module = device.createShaderModule({ code }),
      info = await module.getCompilationInfo();
    const failures = info.messages.filter((message) => message.type === "error");
    if (failures.length) throw Error(failures.map((message) => message.message).join("\n"));
    const pipelines = await Promise.all(
      [1, 0].map((coherent) =>
        device.createComputePipelineAsync({
          layout: "auto",
          compute: { module, entryPoint: "main", constants: { WATER_SURFACE: 1, WATER_COHERENT: coherent } },
        }),
      ),
    );
    device.queue.writeBuffer(input, 0, data);
    const encoder = device.createCommandEncoder();
    for (const pipeline of pipelines) {
      const group = device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: input } },
          { binding: 1, resource: { buffer: output } },
        ],
      });
      const pass = encoder.beginComputePass();
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      pass.dispatchWorkgroups(Math.ceil(queries.length / 32));
      pass.end();
    }
    encoder.copyBufferToBuffer(output, 0, read, 0, queries.length * 16);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const values = new Float32Array(read.getMappedRange().slice(0));
    read.unmap();
    const cases = queries.map((query, index) => {
      let reference =
          query.coherent === false
            ? integrateWaterBoxReference(query, 32)
            : integrateWaterReference(query, 16384),
        converged =
          query.coherent === false
            ? integrateWaterBoxReference(query, 64)
            : integrateWaterReference(query, 32768);
      let referenceOrders = query.coherent === false ? [32, 64] : [16384, 32768];
      if (
        query.coherent === false &&
        Math.hypot(...reference.map((value, axis) => value - converged[axis])) /
          Math.max(0.01, Math.hypot(...converged)) >
          1e-6
      ) {
        reference = integrateWaterBoxReference(query, 96);
        converged = integrateWaterBoxReference(query, 128);
        referenceOrders = [96, 128];
      }
      const actual = Array.from(values.slice(index * 4, index * 4 + 3));
      if (actual.some((value) => !Number.isFinite(value)))
        throw Error(`Nonfinite production water output: ${query.label}`);
      const error = Math.hypot(...actual.map((value, axis) => value - converged[axis])),
        energy = Math.hypot(...converged);
      return {
        query,
        referenceOrders,
        specialization: query.coherent === false ? "ordinary" : "coherent",
        actual,
        reference: converged,
        path: values[index * 4 + 3],
        relativeError: error / Math.max(energy, 0.01),
        maximumError: Math.max(...actual.map((value, axis) => Math.abs(value - converged[axis]))),
        referenceConvergence:
          Math.hypot(...reference.map((value, axis) => value - converged[axis])) / Math.max(energy, 0.01),
      };
    });
    return {
      source: "production brdf.wgsl and water.wgsl",
      reference:
        "independent CPU double normal-vector GGX and authored two-wave source; coherent midpoint temporal16384/32768, ordinary tensor Gauss-Legendre spatial/shutter32/64 peraxis, escalating96/128 untilconverged",
      domain:
        "constant sky, unoccluded directional light; coherent zerofootprint finite shutter, ordinary unequal speed1/.8 spatial.005/.02/.08 and shutter1/60; amplitude0.1/(2pi), wavelength1",
      cases,
      maximumRelativeError: Math.max(...cases.map((sample) => sample.relativeError)),
      maximumReferenceConvergence: Math.max(...cases.map((sample) => sample.referenceConvergence)),
    };
  } finally {
    input.destroy();
    output.destroy();
    read.destroy();
  }
}
