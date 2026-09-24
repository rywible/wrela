import { PHYSICAL_CLOUD_NOISE_SIZE, physicalCloudNoiseComputeWGSL } from "@wrela/render-webgpu/atmosphere";
import referenceSource from "@wrela/render-webgpu/atmosphere-cloud-noise.wgsl" with { type: "text" };
import { createCloudKernelComparison } from "./cloud-kernel";
import { errorStats, gpuContext, type Work } from "./cloud-noise-gpu";

/** Isolates the compiled field from cloud lighting and all scene geometry. */
export async function createCloudNoiseFixture() {
  const context = await gpuContext(),
    device = context.device;
  const texture = device.createTexture({
    size: [...PHYSICAL_CLOUD_NOISE_SIZE],
    dimension: "3d",
    format: "rgba8unorm",
    usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
  });
  const view = texture.createView();
  const sampler = device.createSampler({ minFilter: "linear", magFilter: "linear" });
  const count = 4096;
  const output = device.createBuffer({
    size: count * 16,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const build = await context.pipeline(physicalCloudNoiseComputeWGSL);
  const buildGroup = device.createBindGroup({
    layout: build.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: view }],
  });
  const buildWork: Work = (encoder, timestampWrites) => {
    const pass = encoder.beginComputePass({ timestampWrites });
    pass.setPipeline(build);
    pass.setBindGroup(0, buildGroup);
    pass.dispatchWorkgroups(17, 17, 17);
    pass.end();
  };
  const layout = device.createBindGroupLayout({
    entries: [
      {
        binding: 0,
        visibility: GPUShaderStage.COMPUTE,
        texture: { sampleType: "float", viewDimension: "3d" },
      },
      { binding: 1, visibility: GPUShaderStage.COMPUTE, sampler: { type: "filtering" } },
      { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
    ],
  });
  const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [layout] });
  const group = device.createBindGroup({
    layout,
    entries: [
      { binding: 0, resource: view },
      { binding: 1, resource: sampler },
      { binding: 2, resource: { buffer: output } },
    ],
  });
  const works: Record<string, Work> = {};
  for (const mode of ["analytic", "compiled", "periodic", "filtering", "coordinates", "slope"]) {
    const module = device.createShaderModule({
      code: `${referenceSource}
@group(0) @binding(0) var field:texture_3d<f32>;
@group(0) @binding(1) var linearSampler:sampler;
@group(0) @binding(2) var<storage,read_write> result:array<vec4f>;
fn sampleField(point:vec3f)->f32 {
  ${mode === "analytic" ? "return physicalCloudNoiseReference3(point);" : "return textureSampleLevel(field,linearSampler,(fract(point/8.0)*64.0+1.0)/66.0,0.0).x;"}
}
// Full f32 trilinear arithmetic isolates the hardware filtering path.
fn manualField(point:vec3f)->f32 {
  let texel=fract(point/8.0)*64.0+0.5;let base=vec3i(floor(texel));let f=fract(texel);
  let a=mix(textureLoad(field,base,0).x,textureLoad(field,base+vec3i(1,0,0),0).x,f.x);
  let b=mix(textureLoad(field,base+vec3i(0,1,0),0).x,textureLoad(field,base+vec3i(1,1,0),0).x,f.x);
  let c=mix(textureLoad(field,base+vec3i(0,0,1),0).x,textureLoad(field,base+vec3i(1,0,1),0).x,f.x);
  let d=mix(textureLoad(field,base+vec3i(0,1,1),0).x,textureLoad(field,base+vec3i(1,1,1),0).x,f.x);
  return mix(mix(a,b,f.y),mix(c,d,f.y),f.z);
}
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) invocation:vec3u) {
  let id=invocation.x;if(id>=4096u){return;}
  ${
    mode === "slope"
      ? `
  var largest=vec3f(0.0);
  // For trilinear interpolation each partial is a convex combination of
  // adjacent texel differences. Exhausting every pair certifies the maximum.
  for(var index=id;index<66u*66u*66u;index+=4096u) {
    let cell=vec3i(i32(index%66u),i32((index/66u)%66u),i32(index/(66u*66u)));
    let value=textureLoad(field,cell,0).x;
    if(cell.x<65) {largest.x=max(largest.x,abs(textureLoad(field,cell+vec3i(1,0,0),0).x-value)*8.0);}
    if(cell.y<65) {largest.y=max(largest.y,abs(textureLoad(field,cell+vec3i(0,1,0),0).x-value)*8.0);}
    if(cell.z<65) {largest.z=max(largest.z,abs(textureLoad(field,cell+vec3i(0,0,1),0).x-value)*8.0);}
  }
  result[id]=vec4f(largest,1.0);return;
  `
      : ""
  }
  let point=vec3f(f32((id*73u+19u)%4093u),f32((id*179u+217u)%4093u),f32((id*1097u+13u)%4093u))/4093.0*24.0-8.0;
  let shifted=point+vec3f(8.0,-8.0,16.0);
  ${
    mode === "periodic"
      ? `// Binary-grid coordinates and integer periods retain exactly equal f32 phases.
  let exact=vec3f(f32((id*73u+19u)%4096u),f32((id*179u+217u)%4096u),f32((id*1097u+13u)%4096u))/512.0-4.0;
  result[id]=vec4f(sampleField(exact),sampleField(exact+vec3f(8.0,-8.0,16.0)),0.0,0.0);`
      : mode === "filtering"
        ? `result[id]=vec4f(sampleField(point),sampleField(shifted),manualField(point),manualField(shifted));`
        : mode === "coordinates"
          ? `result[id]=vec4f(fract(point/8.0)-fract(shifted/8.0),0.0);`
          : `let value=sampleField(point);let repeated=sampleField(shifted);
  result[id]=vec4f(value,repeated,exp(-value*3.3),1.0);`
  }
}`,
    });
    const info = await module.getCompilationInfo();
    const failures = info.messages.filter((message) => message.type === "error");
    if (failures.length) throw Error(failures.map((message) => message.message).join("\n"));
    const pipeline = await device.createComputePipelineAsync({
      layout: pipelineLayout,
      compute: { module, entryPoint: "main" },
    });
    works[mode] = (encoder, timestampWrites) => {
      const pass = encoder.beginComputePass({ timestampWrites });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, group);
      pass.dispatchWorkgroups(count / 64);
      pass.end();
    };
  }
  const cloudKernel = await createCloudKernelComparison(context, view, sampler);
  return {
    async check() {
      const encoder = device.createCommandEncoder();
      buildWork(encoder);
      device.queue.submit([encoder.finish()]);
      const analytic = await context.values(works.analytic, output, count * 4);
      const compiled = await context.values(works.compiled, output, count * 4);
      const slopes = await context.values(works.slope, output, count * 4);
      const maximumNoisePartial = slopes.reduce(
        (largest, v, i) => (i % 4 === 3 ? largest : Math.max(largest, v)),
        0,
      );
      const failures: string[] = [];
      if (maximumNoisePartial > 2.2) failures.push("Cloud weather basis exceeds certified traversal slope");
      if (![...analytic, ...compiled].every((value) => Number.isFinite(value) && value >= 0))
        failures.push("Invalid cloud field readback");
      const density = errorStats(
        compiled.filter((_, i) => i % 4 === 0),
        analytic.filter((_, i) => i % 4 === 0),
      );
      const homogeneousTransmission = errorStats(
        compiled.filter((_, i) => i % 4 === 2),
        analytic.filter((_, i) => i % 4 === 2),
      );
      const repeated = await context.values(works.periodic, output, count * 4);
      const filtering = await context.values(works.filtering, output, count * 4);
      const coordinates = await context.values(works.coordinates, output, count * 4);
      let periodicity = 0,
        translatedCoordinateDrift = 0,
        manualTranslatedDrift = 0,
        filteringDeviation = 0,
        maximumPhaseDrift = 0,
        changedPhaseAxes = 0;
      for (let i = 0; i < count; i++) {
        const j = i * 4;
        periodicity = Math.max(periodicity, Math.abs(repeated[j] - repeated[j + 1]));
        translatedCoordinateDrift = Math.max(
          translatedCoordinateDrift,
          Math.abs(compiled[j] - compiled[j + 1]),
        );
        manualTranslatedDrift = Math.max(
          manualTranslatedDrift,
          Math.abs(filtering[j + 2] - filtering[j + 3]),
        );
        filteringDeviation = Math.max(
          filteringDeviation,
          Math.abs(filtering[j] - filtering[j + 2]),
          Math.abs(filtering[j + 1] - filtering[j + 3]),
        );
        for (let axis = 0; axis < 3; axis++) {
          const delta = Math.abs(coordinates[j + axis]);
          maximumPhaseDrift = Math.max(maximumPhaseDrift, delta);
          changedPhaseAxes += Number(delta > 0);
        }
      }
      if (density.max > 0.035) failures.push("Cloud density error exceeded 0.035");
      if (periodicity > 0.0001) failures.push("Equal-phase periodicity error exceeded 0.0001");
      const samplingMs = await context.benchmark({ analytic: works.analytic, compiled: works.compiled }, 64);
      const constructionMs = await context.benchmark({ construction: buildWork }, 4);
      const cloudMarch = await cloudKernel.check();
      if (cloudMarch.cases.some((entry) => !entry.finite)) failures.push("Invalid cloud-kernel HDR readback");
      failures.push(...context.errors);
      return {
        maximumNoisePartial,
        passed: failures.length === 0,
        failures,
        adapter: context.adapter,
        samples: count,
        bytes: 66 * 66 * 66 * 4,
        density,
        homogeneousTransmission,
        periodicity,
        coordinateDiagnostics: {
          translatedCoordinateDrift,
          manualTranslatedDrift,
          filteringDeviation,
          maximumPhaseDrift,
          changedPhaseAxes,
          scope:
            "Arbitrary f32 coordinates change phase after adding a period. Exact binary-grid repeat is gated separately; hardware filtering deviation is measured, with no assumed portable precision bound.",
        },
        samplingMs,
        constructionMs,
        cloudMarch,
        scope:
          "Component and production cloud-kernel A/B comparisons. Neither establishes a whole-frame speedup or a transport error certificate.",
      };
    },
    dispose() {
      cloudKernel.dispose();
      texture.destroy();
      output.destroy();
      context.dispose();
    },
  };
}
