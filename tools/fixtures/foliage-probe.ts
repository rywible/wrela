import type { Vec3 } from "@wrela/model";

import { type FoliageOptics, foliageBudget, foliageDiffuse } from "@wrela/render-webgpu/foliage";
import foliageWGSL from "@wrela/render-webgpu/foliage.wgsl" with { type: "text" };

/** Runs the production leaf closure on hardware, separate from tone mapping and sky fill. */
export async function probeFoliageResponse(device: GPUDevice) {
  const samples: { optics: FoliageOptics; cosine: number; visibility: number }[] = [];
  for (const transmission of [0, 0.5, 1])
    for (const thickness of [0, 0.0005, 0.01])
      for (const cosine of [-1, -0.1, 0, 0.1, 1])
        for (const visibility of [0, 0.3, 1])
          samples.push({
            optics: { albedo: [0.15, 0.75, 1], scatterColor: [0.4, 0.8, 1], transmission, thickness },
            cosine,
            visibility,
          });
  const values = new Float32Array(samples.length * 12);
  for (const [index, sample] of samples.entries()) {
    values.set(
      [
        ...sample.optics.albedo,
        0,
        ...sample.optics.scatterColor,
        0,
        sample.optics.transmission,
        sample.optics.thickness,
        sample.cosine,
        sample.visibility,
      ],
      index * 12,
    );
  }
  const input = device.createBuffer({
    size: values.byteLength,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
  const output = device.createBuffer({
    size: values.byteLength,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const readback = device.createBuffer({
    size: values.byteLength,
    usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
  });
  try {
    device.queue.writeBuffer(input, 0, values);
    const module = device.createShaderModule({
      code: `${foliageWGSL}
struct Sample { base:vec4f, scatter:vec4f, properties:vec4f };
struct Result { reflection:vec4f, transmission:vec4f, response:vec4f };
@group(0) @binding(0) var<storage,read> samples:array<Sample>;
@group(0) @binding(1) var<storage,read_write> results:array<Result>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) invocation:vec3u) {
 let id=invocation.x;if(id>=arrayLength(&samples)){return;}
 let sample=samples[id];let p=sample.properties;
 let budget=foliageBudget(sample.base.xyz,sample.scatter.xyz,p.x,p.y);
 results[id]=Result(vec4f(budget.reflection,0),vec4f(budget.transmission,0),vec4f(foliageDiffuse(budget,p.z,max(p.z,0.0)*p.w,p.w),0));
}`,
    });
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "main" },
    });
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: input } },
        { binding: 1, resource: { buffer: output } },
      ],
    });
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, group);
    pass.dispatchWorkgroups(Math.ceil(samples.length / 64));
    pass.end();
    encoder.copyBufferToBuffer(output, 0, readback, 0, values.byteLength);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const actual = new Float32Array(readback.getMappedRange().slice(0));
    readback.unmap();
    let maximumParityError = 0,
      maximumDiffuseBudget = 0,
      maximumOccludedResponse = 0;
    for (const [index, sample] of samples.entries()) {
      const budget = foliageBudget(sample.optics);
      const response = foliageDiffuse(budget, sample.cosine, sample.visibility);
      const expected: Vec3[] = [budget.reflection, budget.transmission, response];
      for (let block = 0; block < 3; block++)
        for (let axis = 0; axis < 3; axis++) {
          const value = actual[index * 12 + block * 4 + axis];
          if (!Number.isFinite(value)) throw Error("Nonfinite GPU leaf response");
          maximumParityError = Math.max(maximumParityError, Math.abs(value - expected[block][axis]));
        }
      for (let axis = 0; axis < 3; axis++) {
        maximumDiffuseBudget = Math.max(
          maximumDiffuseBudget,
          (actual[index * 12 + axis] + actual[index * 12 + 4 + axis]) / sample.optics.albedo[axis],
        );
        if (sample.visibility === 0)
          maximumOccludedResponse = Math.max(maximumOccludedResponse, actual[index * 12 + 8 + axis]);
      }
    }
    if (maximumParityError > 1e-6 || maximumDiffuseBudget > 0.960001 || maximumOccludedResponse !== 0)
      throw Error(
        `Leaf GPU response failed: ${JSON.stringify({ maximumParityError, maximumDiffuseBudget, maximumOccludedResponse })}`,
      );
    return {
      samples: samples.length,
      maximumParityError,
      maximumDiffuseBudget,
      maximumOccludedResponse,
      scope:
        "Diffuse closure only; excludes separate GGX specular, canopy multiple scattering and shadow-map filtering.",
    };
  } finally {
    input.destroy();
    output.destroy();
    readback.destroy();
  }
}
