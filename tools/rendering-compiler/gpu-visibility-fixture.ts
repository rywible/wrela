import { type Bounds, identityMatrix } from "@wrela/model";

import { INSTANCE_FLOATS } from "@wrela/render-webgpu/batching";
import { GpuVisibility } from "@wrela/render-webgpu/gpu-visibility";

export async function verifyGpuVisibility() {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error("WebGPU adapter unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  const results = [];
  const bounds = (x: number, z: number): Bounds => ({
    min: [x - 0.01, -0.01, z - 0.01],
    max: [x + 0.01, 0.01, z + 0.01],
  });
  for (const samples of [1, 4])
    for (const hole of [false, true]) {
      const visibility = new GpuVisibility(device, 65, 63, samples, 16, 3);
      const boxes = [bounds(0, 0.8), bounds(0.7, 0.8), bounds(0, 0), bounds(NaN, 0.8), bounds(0, 0.2)];
      const records = new Float32Array(boxes.length * INSTANCE_FLOATS);
      boxes.forEach((_, i) => {
        records.set(identityMatrix(), i * INSTANCE_FLOATS);
        records[i * INSTANCE_FLOATS + 16] = i + 100;
        records.set(identityMatrix(), i * INSTANCE_FLOATS + 20);
        records[i * INSTANCE_FLOATS + 32] = i + 200;
      });
      visibility.prepare(identityMatrix(), [
        { bounds: boxes, instances: records, indexCount: 36, firstIndex: 12 },
      ]);
      const pipeline = device.createRenderPipeline({
        layout: "auto",
        vertex: {
          module: device.createShaderModule({
            code: `
@vertex fn main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.2,1.0);
}`,
          }),
          entryPoint: "main",
        },
        depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "less" },
        multisample: { count: samples },
      });
      const encoder = device.createCommandEncoder();
      const pass = visibility.beginDepthPass(encoder);
      pass.setPipeline(pipeline);
      if (hole) {
        pass.setScissorRect(16, 16, 13, 31);
        pass.draw(3);
        pass.setScissorRect(36, 16, 13, 31);
        pass.draw(3);
      } else {
        pass.setScissorRect(16, 16, 33, 31);
        pass.draw(3);
      }
      pass.end();
      visibility.encode(encoder);
      const commands = device.createBuffer({
        size: 20,
        usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
      });
      const survivors = device.createBuffer({
        size: records.byteLength,
        usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
      });
      encoder.copyBufferToBuffer(visibility.indirectBuffer, 0, commands, 0, 20);
      encoder.copyBufferToBuffer(visibility.survivorBuffer, 0, survivors, 0, records.byteLength);
      device.queue.submit([encoder.finish()]);
      await Promise.all([commands.mapAsync(GPUMapMode.READ), survivors.mapAsync(GPUMapMode.READ)]);
      const args = Array.from(new Uint32Array(commands.getMappedRange()));
      const data = new Float32Array(survivors.getMappedRange());
      const identities = Array.from({ length: args[1] }, (_, i) => data[i * INSTANCE_FLOATS + 16]).sort();
      const expected = hole ? [100, 101, 102, 103, 104] : [101, 102, 103, 104];
      if (JSON.stringify(identities) !== JSON.stringify(expected) || args[0] !== 36 || args[2] !== 12)
        throw new Error(
          `Incorrect survivor compaction ${JSON.stringify({ samples, hole, args, identities, expected })}`,
        );
      for (let i = 0; i < args[1]; i++) {
        const original = data[i * INSTANCE_FLOATS + 16] - 100;
        for (let j = 0; j < INSTANCE_FLOATS; j++)
          if (data[i * INSTANCE_FLOATS + j] !== records[original * INSTANCE_FLOATS + j])
            throw Error("Compaction corrupted the current/previous instance record");
      }
      results.push({
        samples,
        hole,
        args,
        identities,
        bytes: visibility.byteLength,
        uploadBytes: visibility.uploadBytes,
      });
      commands.unmap();
      survivors.unmap();
      commands.destroy();
      survivors.destroy();
      visibility.destroy();
    }
  await device.queue.onSubmittedWorkDone();
  if (errors.length) throw new Error(errors.join("\n"));
  const result = { adapter: adapter.info, results, errors };
  device.destroy();
  return result;
}
