import coverageWGSL from "@wrela/render-webgpu/thin-coverage.wgsl" with { type: "text" };
import { thinCoverageThresholdReference } from "@wrela/render-webgpu/thin-coverage-sampling";

/** Test the production quantile directly when both spatial hash cells collapse
 * to zero. This avoids constructing millions of subpixel proxy triangles. */
export async function probeCoarseThinCoverage(device: GPUDevice) {
  const samples = 65536,
    cases = 9,
    bytes = samples * cases * 8;
  const output = device.createBuffer({
    size: bytes,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const readback = device.createBuffer({
    size: bytes,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  try {
    const module = device.createShaderModule({
      code: `${coverageWGSL}
@group(0) @binding(0) var<storage,read_write> results:array<vec2f>;
@compute @workgroup_size(64) fn coarse(@builtin(global_invocation_id) invocation:vec3u) {
  let id=invocation.x;if(id>=589824u){return;}
  let caseIndex=id/65536u;let sample=id%65536u;
  let levels=array<f32,3>(256.0,1024.0,65536.0);
  let lo=levels[caseIndex/3u];let blend=0.25+0.25*f32(caseIndex%3u);
  let salt=vec2f(f32(sample%256u)*67.0,f32(sample/256u)*103.0);
  let threshold=thinCoverageThresholdFor(vec4f(0.5,0.5,lo,blend),salt);
  let left=thinCoverageThresholdFor(vec4f(0.5,0.5,lo,1.0),salt);
  let right=thinCoverageThresholdFor(vec4f(0.5,0.5,lo*2.0,0.0),salt);
  results[id]=vec2f(threshold,abs(left-right));
}`,
    });
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "coarse" },
    });
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: output } }],
    });
    const encoder = device.createCommandEncoder(),
      pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, group);
    pass.dispatchWorkgroups((samples * cases) / 64);
    pass.end();
    encoder.copyBufferToBuffer(output, 0, readback, 0, bytes);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const values = new Float32Array(readback.getMappedRange()),
      results = [];
    let maximumParityError = 0,
      maximumBoundaryError = 0,
      maximumCoverageBias = 0;
    for (let index = 0; index < cases; index++) {
      const footprint = [256, 1024, 65536][Math.floor(index / 3)],
        blend = 0.25 + 0.25 * (index % 3);
      const coverage = [0.01, 0.1, 0.5, 0.9],
        counts = coverage.map(() => 0);
      for (let sample = 0; sample < samples; sample++) {
        const value = values[(index * samples + sample) * 2];
        if (!Number.isFinite(value) || value < 0 || value > 1)
          throw Error("Invalid coarse coverage quantile");
        coverage.forEach((alpha, i) => {
          if (value < alpha) counts[i]++;
        });
        maximumBoundaryError = Math.max(maximumBoundaryError, values[(index * samples + sample) * 2 + 1]);
        if (sample % 257 === 0) {
          const expected = thinCoverageThresholdReference([0.5, 0.5], footprint, blend, [
            (sample % 256) * 67,
            Math.floor(sample / 256) * 103,
          ]);
          maximumParityError = Math.max(maximumParityError, Math.abs(expected - value));
        }
      }
      const observations = coverage.map((expected, i) => ({
        expected,
        measured: counts[i] / samples,
        bias: counts[i] / samples - expected,
      }));
      maximumCoverageBias = Math.max(
        maximumCoverageBias,
        ...observations.map((value) => Math.abs(value.bias)),
      );
      results.push({ footprint, blend, samples, observations });
    }
    readback.unmap();
    if (maximumParityError > 1e-5 || maximumBoundaryError > 1e-6 || maximumCoverageBias > 0.006)
      throw Error(
        `Coarse thin coverage failed: ${JSON.stringify({ maximumParityError, maximumBoundaryError, maximumCoverageBias })}`,
      );
    return {
      results,
      maximumParityError,
      maximumBoundaryError,
      maximumCoverageBias,
      scope:
        "Production hash/scale-CDF on GPU across 65536 deterministic source salts per case. Footprints exceed the entire coverage field; verifies mean coverage and adjacent-level endpoint continuity, not subpixel proxy rasterization or temporal convergence.",
    };
  } finally {
    output.destroy();
    readback.destroy();
  }
}
