import { compileMaterialLattice } from "@wrela/compiler";
import { MaterialLatticeGpu } from "@wrela/render-webgpu/material-lattice";
import cache from "@wrela/render-webgpu/material-lattice.wgsl" with { type: "text" };
import source from "@wrela/render-webgpu/scene.wgsl" with { type: "text" };

export async function checkMaterialLatticeGpu() {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw Error("WebGPU unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
  const count = 2048,
    positions = new Float32Array(count * 4);
  for (let i = 0; i < count; i++)
    positions.set(
      [((i * 17) % 83) - 41.5 + (i % 3 === 0 ? 1024 : 0), ((i * 31) % 79) - 39.125, ((i * 13) % 89) - 44.25],
      i * 4,
    );
  // Boundary and origin checks, including a large rebased coordinate.
  [
    [-32, -32, -32],
    [31.999, 31.999, 31.999],
    [32, 0, 0],
    [-32.001, 0, 0],
    [0, 0, 0],
    [1024, -1024, 2048],
  ].forEach((p, i) => {
    positions.set(p, i * 4);
  });
  const inputs = device.createBuffer({
    size: positions.byteLength,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
  const output = device.createBuffer({
    size: count * 16,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const read = device.createBuffer({
    size: count * 16,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  const lattice = new MaterialLatticeGpu(device);
  try {
    const product = compileMaterialLattice();
    lattice.update(product, product.corners.byteLength + 16);
    device.queue.writeBuffer(inputs, 0, positions);
    const functions = source.slice(source.indexOf("fn hash("), source.indexOf("fn filteredNoise("));
    const direct = functions
      .slice(functions.indexOf("fn noise("))
      .replace("fn noise(", "fn directNoise(")
      .replace("  if(MATERIAL_CACHE){let cached=cachedMaterialNoise(p);if(cached>=0.0){return cached;}}", "");
    const module = device.createShaderModule({
      code: `${cache}\n${functions}\n${direct}
@group(0) @binding(0) var<storage,read> points:array<vec4f>;
@group(0) @binding(1) var<storage,read_write> results:array<vec4f>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u){if(id.x>=${count}u){return;}let p=points[id.x].xyz;results[id.x]=vec4f(noise(p),directNoise(p),cachedMaterialNoise(p),0.0);}`,
    });
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "main" },
    });
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: inputs } },
        { binding: 1, resource: { buffer: output } },
        { binding: 13, resource: lattice.texture.createView() },
        { binding: 14, resource: { buffer: lattice.bounds } },
      ],
    });
    const encoder = device.createCommandEncoder(),
      pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, group);
    pass.dispatchWorkgroups(count / 64);
    pass.end();
    encoder.copyBufferToBuffer(output, 0, read, 0, count * 16);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const values = new Float32Array(read.getMappedRange());
    let maximumError = 0,
      hits = 0;
    for (let i = 0; i < count; i++) {
      maximumError = Math.max(maximumError, Math.abs(values[i * 4] - values[i * 4 + 1]));
      if (values[i * 4 + 2] >= 0) hits++;
    }
    read.unmap();
    if (!Number.isFinite(maximumError) || maximumError > 0.0000002 || hits < 100 || hits >= count)
      throw Error(`Material lattice mismatch: ${maximumError}, ${hits} hits`);
    if (errors.length) throw Error(errors.join("\n"));
    return { count, hits, fallbacks: count - hits, maximumError, bytes: lattice.bytes, errors };
  } finally {
    lattice.destroy();
    inputs.destroy();
    output.destroy();
    read.destroy();
    device.destroy();
  }
}
