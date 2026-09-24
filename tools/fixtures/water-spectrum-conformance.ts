import { sampleWaterSpectrum } from "@wrela/compiler";
import type { RenderSurface, Vec3 } from "@wrela/model";

import { packWaterBody } from "@wrela/render-webgpu/water-body";
import waterBody from "@wrela/render-webgpu/water-body.wgsl" with { type: "text" };
import { WaterSpectrumGpu, WaterSpectrumPrograms } from "@wrela/render-webgpu/water-spectrum-gpu";

/** Compare geometry to physical CPU queries and shading to its retained-band closure. */
export async function verifyWaterSpectrumMap(device: GPUDevice, surface: RenderSurface, origin: Vec3) {
  const spectrum = surface.waterState?.spectrum;
  if (!spectrum?.tiles) throw Error("Missing periodic spectrum");
  const time = 2.25,
    count = 128;
  const makeBuffer = (data: Float32Array, uniform = false) => {
    const buffer = device.createBuffer({
      size: data.byteLength,
      usage: GPUBufferUsage.COPY_DST | (uniform ? GPUBufferUsage.UNIFORM : GPUBufferUsage.STORAGE),
    });
    device.queue.writeBuffer(buffer, 0, data as Float32Array<ArrayBuffer>);
    return buffer;
  };
  const programs = new WaterSpectrumPrograms(device);
  const body = makeBuffer(packWaterBody(surface, origin));
  const map = new WaterSpectrumGpu(programs, body, spectrum);
  const queries = new Float32Array((count + 2) * 4);
  for (let i = 0; i < count / 2; i++) {
    queries.set([i * 0.173, -i * 0.281, 0, 0], i * 4);
    queries.set([i * 0.173 + spectrum.tiles[0], -i * 0.281, 0, 0], (i + count / 2) * 4);
  }
  // Resolve the entire coarsest tile to test loss of phase and preservation of roughness.
  queries.set([1, -2, 0, spectrum.tiles[0] * 2], count * 4);
  queries.set([12, 23, 0, spectrum.tiles[0] * 2], (count + 1) * 4);
  const input = makeBuffer(queries),
    clock = makeBuffer(new Float32Array([time, 0, 0, 0]), true);
  const output = device.createBuffer({
    size: (count + 2) * 32,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const readback = device.createBuffer({
    size: output.size,
    usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
  });
  try {
    const module = device.createShaderModule({
      code: `
struct Clock {params:vec4f}; @group(0) @binding(2) var<uniform> g:Clock;
${waterBody.slice(0, waterBody.indexOf("// Recover the filtered"))}
@group(0) @binding(0) var<storage,read> queries:array<vec4f>;
@group(0) @binding(1) var<storage,read_write> answers:array<vec4f>;
@compute @workgroup_size(32) fn main(@builtin(global_invocation_id) id:vec3u) {
 if(id.x>=arrayLength(&queries)){return;}let q=queries[id.x];let wave=waterBodyFilteredWaves(q.xy,q.w);
 answers[id.x*2u]=wave.value;answers[id.x*2u+1u]=vec4f(wave.jacobian,0.0);
}`,
    });
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "main" },
    });
    const encoder = device.createCommandEncoder();
    if (map.encode(encoder, time, spectrum.key) !== 16 || map.encode(encoder, time, spectrum.key) !== 0)
      throw Error("Frozen spectrum was synthesized twice");
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(
      0,
      device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: input } },
          { binding: 1, resource: { buffer: output } },
          { binding: 2, resource: { buffer: clock } },
        ],
      }),
    );
    pass.setBindGroup(
      1,
      device.createBindGroup({
        layout: pipeline.getBindGroupLayout(1),
        entries: [
          { binding: 5, resource: { buffer: body } },
          { binding: 6, resource: map.view },
          { binding: 7, resource: programs.sampler },
        ],
      }),
    );
    pass.dispatchWorkgroups(Math.ceil((count + 2) / 32));
    pass.end();
    encoder.copyBufferToBuffer(output, 0, readback, 0, output.size);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const actual = new Float32Array(readback.getMappedRange());
    const periodic = actual.slice();
    if (spectrum.carriers.length > 54 * 8) {
      const authored = { ...spectrum, carriers: spectrum.carriers.slice(0, -54 * 8) };
      for (let i = 0; i < count; i++) {
        const wave = sampleWaterSpectrum(
          authored,
          queries[i * 4] + origin[0],
          queries[i * 4 + 1] + origin[2],
          time,
        );
        periodic[i * 8] -= wave.height;
        periodic[i * 8 + 1] -= wave.dx;
        periodic[i * 8 + 2] -= wave.dz;
        periodic[i * 8 + 4] -= wave.jxx - 1;
        periodic[i * 8 + 5] -= wave.jxz;
        periodic[i * 8 + 6] -= wave.jzz - 1;
      }
    }
    let heightError = 0,
      slopeError = 0,
      jacobianError = 0,
      seamError = 0;
    // Shading retains the large waves and transfers a known fraction of the
    // two fine cascades into statistical roughness. Geometry/queries stay exact.
    const retained = { ...spectrum, carriers: spectrum.carriers.slice() };
    const generatedStart = retained.carriers.length / 8 - 54;
    for (let wave = generatedStart; wave < retained.carriers.length / 8; wave++)
      retained.carriers[wave * 8 + 4] *= [1, 0.7, 0.25][Math.floor((wave - generatedStart) / 18)];
    for (let i = 0; i < count; i++) {
      const expected = sampleWaterSpectrum(
        spectrum,
        queries[i * 4] + origin[0],
        queries[i * 4 + 1] + origin[2],
        time,
      );
      const a = actual.subarray(i * 8, i * 8 + 8);
      const shading = sampleWaterSpectrum(
        retained,
        queries[i * 4] + origin[0],
        queries[i * 4 + 1] + origin[2],
        time,
      );
      heightError = Math.max(heightError, Math.abs(a[0] - expected.height));
      slopeError = Math.max(slopeError, Math.abs(a[1] - shading.dx), Math.abs(a[2] - shading.dz));
      jacobianError = Math.max(
        jacobianError,
        Math.abs(a[4] - expected.jxx),
        Math.abs(a[5] - expected.jxz),
        Math.abs(a[6] - expected.jzz),
      );
      if (i < count / 2)
        for (let c = 0; c < 7; c++)
          seamError = Math.max(seamError, Math.abs(periodic[i * 8 + c] - periodic[(i + count / 2) * 8 + c]));
      if (!a.every(Number.isFinite) || a[3] < 0) throw Error("Invalid filtered water moments");
    }
    const far = actual.subarray(count * 8, count * 8 + 8);
    if (heightError > 0.006 || slopeError > 0.008 || jacobianError > 0.004 || seamError > 0.0003)
      throw Error(
        `Water spectrum map error: ${JSON.stringify({ heightError, slopeError, jacobianError, seamError })}`,
      );
    if (Math.abs(far[1]) + Math.abs(far[2]) > 0.0001 || far[3] <= 0)
      throw Error("Unresolved waves lost their slope variance");
    for (let c = 0; c < 7; c++)
      if (Math.abs(far[c] - actual[(count + 1) * 8 + c]) > 1e-6)
        throw Error("Far field retained spatial phase");
    const result = {
      samples: count,
      origin,
      heightError,
      slopeError,
      jacobianError,
      seamError,
      farVariance: far[3],
    };
    readback.unmap();
    return result;
  } finally {
    map.destroy();
    programs.destroy();
    body.destroy();
    input.destroy();
    clock.destroy();
    output.destroy();
    readback.destroy();
  }
}
