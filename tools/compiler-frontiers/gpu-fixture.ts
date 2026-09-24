import { phaseSinc } from "@wrela/compiler";
import brdf from "@wrela/render-webgpu/brdf.wgsl" with { type: "text" };
import { type BranchGpuInput, branchShader } from "./branch-gpu";
import { compileFourier, correlatedExpression, emitCorrelatedWGSL } from "./fourier";
import { errors, lightFixture, random } from "./probes";

const declarations = `
@group(0) @binding(0) var<storage,read> input:array<vec4f>;
@group(0) @binding(1) var<storage,read> auxiliary:array<vec4f>;
@group(0) @binding(2) var<storage,read> lights:array<vec4f>;
@group(0) @binding(3) var<storage,read_write> output:array<vec4f>;
fn scalar(i:u32)->f32 {return auxiliary[i/4u][i%4u];}
`;

/** Isolated arithmetic/lookup probes, not scene frame-time predictions. */
export async function gpuFrontiers(branch?: BranchGpuInput) {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter?.features.has("timestamp-query")) throw Error("Hardware GPU timestamp queries required");
  const device = await adapter.requestDevice({ requiredFeatures: ["timestamp-query"] });
  const failures: string[] = [];
  device.addEventListener("uncapturederror", (event) => failures.push(event.error.message));
  const layout = device.createBindGroupLayout({
    entries: [0, 1, 2, 3].map((binding) => ({
      binding,
      visibility: GPUShaderStage.COMPUTE,
      buffer: { type: binding === 3 ? ("storage" as const) : ("read-only-storage" as const) },
    })),
  });
  const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [layout] });
  const query = device.createQuerySet({ type: "timestamp", count: 2 });
  const resolved = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
  });
  const queryRead = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  type Kernel = { code: string; count: number };
  async function run(
    label: string,
    sources: Record<string, string | Kernel[]>,
    inputs: number[][],
    count: number,
    repeats: number,
    expected?: number[],
  ) {
    const owned: GPUBuffer[] = [];
    const buffer = (size: number, usage: GPUBufferUsageFlags) => {
      const result = device.createBuffer({ size, usage });
      owned.push(result);
      return result;
    };
    const buffers = inputs.map((array) => {
      const b = buffer(Math.max(16, array.length * 4), GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST);
      device.queue.writeBuffer(b, 0, new Float32Array(array));
      return b;
    });
    const target = buffer(count * 16, GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC);
    const read = buffer(count * 16, GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST);
    const group = device.createBindGroup({
      layout,
      entries: [...buffers, target].map((b, binding) => ({ binding, resource: { buffer: b } })),
    });
    const pipelines: Record<string, { pipeline: GPUComputePipeline; count: number }[]> = {};
    for (const [mode, source] of Object.entries(sources)) {
      pipelines[mode] = [];
      for (const { code, count: kernelCount } of typeof source === "string"
        ? [{ code: source, count }]
        : source) {
        const module = device.createShaderModule({ code: declarations + code });
        const info = await module.getCompilationInfo();
        const errors = info.messages.filter((m) => m.type === "error");
        if (errors.length) throw Error(errors.map((m) => m.message).join("\n"));
        pipelines[mode].push({
          pipeline: await device.createComputePipelineAsync({
            layout: pipelineLayout,
            compute: { module, entryPoint: "main" },
          }),
          count: kernelCount,
        });
      }
    }
    const dispatch = (mode: string, timed: boolean, iterations: number) => {
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginComputePass(
        timed
          ? { timestampWrites: { querySet: query, beginningOfPassWriteIndex: 0, endOfPassWriteIndex: 1 } }
          : {},
      );
      pass.setBindGroup(0, group);
      for (let i = 0; i < iterations; i++)
        for (const kernel of pipelines[mode]) {
          pass.setPipeline(kernel.pipeline);
          pass.dispatchWorkgroups(Math.ceil(kernel.count / 64));
        }
      pass.end();
      if (timed) {
        encoder.resolveQuerySet(query, 0, 2, resolved, 0);
        encoder.copyBufferToBuffer(resolved, 0, queryRead, 0, 16);
      }
      device.queue.submit([encoder.finish()]);
    };
    const timings: { trial: number; mode: string; ms: number }[] = [];
    const values: Record<string, number[]> = {};
    const modes = Object.keys(sources);
    try {
      for (const mode of modes) dispatch(mode, false, 8);
      await device.queue.onSubmittedWorkDone();
      for (let trial = 0; trial < 8; trial++)
        for (const mode of trial % 2 ? [...modes].reverse() : modes) {
          dispatch(mode, true, repeats);
          await queryRead.mapAsync(GPUMapMode.READ);
          const stamps = new BigUint64Array(queryRead.getMappedRange());
          const ms = Number(stamps[1] - stamps[0]) / 1e6 / repeats;
          queryRead.unmap();
          if (!(ms > 0) || !Number.isFinite(ms)) throw Error("Invalid GPU timestamp");
          timings.push({ trial, mode, ms });
        }
      for (const mode of modes) {
        dispatch(mode, false, 1);
        const encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(target, 0, read, 0, count * 16);
        device.queue.submit([encoder.finish()]);
        await read.mapAsync(GPUMapMode.READ);
        values[mode] = Array.from(new Float32Array(read.getMappedRange())).filter((_, i) => i % 4 < 3);
        read.unmap();
        if (values[mode].some((v) => !Number.isFinite(v))) throw Error(`${label}/${mode}: nonfinite result`);
      }
      const cpuReference = expected ? errors(expected, values[modes[0]]) : undefined;
      if (cpuReference && cpuReference.maximum > 1e-5)
        throw Error("GPU generated integral disagrees with CPU reference");
      return {
        label,
        count,
        repeats,
        timings,
        comparisons: modes
          .slice(1)
          .map((mode) => ({ mode, reference: modes[0], ...errors(values[modes[0]], values[mode]) })),
        firstValues: Object.fromEntries(modes.map((mode) => [mode, values[mode].slice(0, 12)])),
        cpuReference,
      };
    } finally {
      for (const b of owned) b.destroy();
    }
  }
  try {
    if (branch) {
      const results = [];
      for (const order of ["random", "receiver"]) {
        const indices = Array.from({ length: branch.queries.length / 8 }, (_, i) => i);
        if (order === "receiver")
          indices.sort((a, b) => branch.queries[a * 8 + 3] - branch.queries[b * 8 + 3]);
        const queries = indices.flatMap((i) => branch.queries.slice(i * 8, i * 8 + 8));
        const reference = indices.flatMap((i) => branch.reference.slice(i * 3, i * 3 + 3));
        const result = await run(
          `spatial-branch-visibility/${order}`,
          { full: branchShader(branch, false), compiled: branchShader(branch, true) },
          [queries, branch.auxiliary, branch.geometry],
          queries.length / 8,
          16,
          reference,
        );
        if (result.comparisons.some((c) => c.maximum !== 0) || failures.length)
          throw Error(`Spatial branch GPU mismatch: ${failures.join("; ")}`);
        results.push({ order, ...result });
      }
      return {
        adapter: { vendor: adapter.info.vendor, architecture: adapter.info.architecture },
        results,
        failures,
      };
    }
    const lighting = [];
    for (const kind of ["mixed", "above"] as const) {
      const fixture = lightFixture(kind);
      const lists = fixture.domains.flatMap((_, i) => [...fixture.lists.slice(i * 9, i * 9 + 9), 0, 0, 0]);
      const sources: Record<string, string | Kernel[]> = {};
      for (const mode of ["generic", "compiled"])
        sources[mode] =
          brdf +
          `
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {
  let j=id.x%${fixture.points.length / 8}u;let p=input[j*2u];let n=input[j*2u+1u].xyz;
  let view=normalize(vec3f(0,20,25)-p.xyz);var color=vec3f(0);let base=u32(p.w)*12u;
  let count=${mode === "generic" ? "8u" : "u32(scalar(base))"};
  for(var k=0u;k<count;k++) {
    let index=${mode === "generic" ? "k" : "u32(scalar(base+1u+k))"};
    let offset=lights[index].xyz-p.xyz;let distance=max(dot(offset,offset),0.01);let l=offset*inverseSqrt(distance);
    color+=(vec3f(.75)*max(dot(n,l),0.0)/BRDF_PI+brdfGGX(n,view,l,.35,vec3f(.04)))*100.0/(1.0+distance);
  }
  output[id.x]=vec4f(color,1);
}`;
      const groups = new Map<string, { active: number[]; points: number[] }>();
      for (let region = 0; region < fixture.domains.length; region++) {
        const active = fixture.lists.slice(region * 9 + 1, region * 9 + 1 + fixture.lists[region * 9]);
        const key = active.join(",");
        const group = groups.get(key) ?? { active, points: [] };
        for (let i = 0; i < 64; i++) group.points.push(region * 64 + i);
        groups.set(key, group);
      }
      sources.specialized = [];
      for (const group of groups.values()) {
        const offset = lists.length;
        lists.push(...group.points);
        const additions = group.active
          .map(
            (index) => `{
          let offset=lights[${index}u].xyz-p.xyz;let distance=max(dot(offset,offset),0.01);let l=offset*inverseSqrt(distance);
          color+=(vec3f(.75)*max(dot(n,l),0.0)/BRDF_PI+brdfGGX(n,view,l,.35,vec3f(.04)))*100.0/(1.0+distance);
        }`,
          )
          .join("\n");
        sources.specialized.push({
          count: group.points.length * 16,
          code:
            brdf +
            `
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {
  if(id.x>=${group.points.length * 16}u){return;}
  let j=u32(scalar(${offset}u+id.x%${group.points.length}u));
  let outputIndex=j+(id.x/${group.points.length}u)*${fixture.points.length / 8}u;
  let p=input[j*2u];let n=input[j*2u+1u].xyz;
  let view=normalize(vec3f(0,20,25)-p.xyz);var color=vec3f(0);
  ${additions}
  output[outputIndex]=vec4f(color,1);
}`,
        });
      }
      const result = await run(
        `lights-${kind}`,
        sources,
        [fixture.points, lists, fixture.lights.flatMap((l) => [...l, 0])],
        262144,
        32,
      );
      if (result.comparisons.some((c) => c.maximum > 1e-6))
        throw Error("GPU light exclusion changed radiance");
      lighting.push({ ...result, variants: groups.size });
    }
    const rng = random(492),
      temporalInput: number[] = [];
    for (let i = 0; i < 65536; i++)
      temporalInput.push(
        rng() * Math.PI * 2,
        rng() * Math.PI * 2,
        rng() * 30,
        rng() * 8,
        rng() * 20,
        0,
        0,
        0,
      );
    const expected: number[] = [];
    for (let i = 0; i < temporalInput.length; i += 8) {
      const [p, d, dx, dy, shutter] = temporalInput.slice(i, i + 5).map(Math.fround);
      const average = (k: number, offset: number) =>
        Math.cos(k * p + offset) *
        phaseSinc((k * dx) / 2) *
        phaseSinc((k * dy) / 2) *
        phaseSinc((k * shutter) / 2);
      const value =
        0.3 + 0.07875 * Math.cos(d) + 0.175 * average(1, 0) + 0.27 * average(1, d) + 0.07875 * average(2, d);
      expected.push(value, value, value);
    }
    const sources: Record<string, string> = {};
    for (const mode of ["analytic", "grid4", "grid8"])
      sources[mode] = `
fn sinc(x:f32)->f32 {if(abs(x)<0.0001){return 1.0-x*x/6.0;}return sin(x)/x;}
fn average(p:vec4f,shutter:f32,k:f32,shift:f32)->f32 {return cos(k*p.x+shift)*sinc(k*p.z*.5)*sinc(k*p.w*.5)*sinc(k*shutter*.5);}
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u) {
  let p=input[id.x*2u];let shutter=input[id.x*2u+1u].x;var value=0.0;
  ${
    mode === "analytic"
      ? emitCorrelatedWGSL(compileFourier(correlatedExpression, 2))
      : `
  const N=${mode === "grid4" ? "4" : "8"}u;
  for(var z=0u;z<N;z++){for(var y=0u;y<N;y++){for(var x=0u;x<N;x++){
    let phase=p.x+((f32(x)+.5)/f32(N)-.5)*p.z+((f32(y)+.5)/f32(N)-.5)*p.w+((f32(z)+.5)/f32(N)-.5)*shutter;
    value+=(.6+.35*cos(phase))*(.5+.45*cos(phase+p.y));
  }}}value/=f32(N*N*N);`
  }
  output[id.x]=vec4f(value,value,value,1);
}`;
    const temporal = await run(
      "correlated-filtering",
      sources,
      [temporalInput, [0, 0, 0, 0], [0, 0, 0, 0]],
      65536,
      32,
      expected,
    );
    if (failures.length) throw Error(failures.join("\n"));
    return {
      adapter: {
        vendor: adapter.info.vendor,
        architecture: adapter.info.architecture,
        device: adapter.info.device,
        description: adapter.info.description,
      },
      lighting,
      temporal,
      failures,
      note: "Isolated compute kernels with repeated dispatches. Not frame times, not display FPS. Terrain points are repeated 16 times in lighting to expose throughput.",
    };
  } finally {
    query.destroy();
    resolved.destroy();
    queryRead.destroy();
    device.destroy();
  }
}
