import { waterDepthReduceWGSL } from "@wrela/render-webgpu/water-transport";

/** Compare the shipped GPU reduction with exact normalized-UV source footprints. */
export async function verifyWaterDepthHierarchy(device: GPUDevice) {
  const pipeline = await device.createComputePipelineAsync({
    layout: "auto",
    compute: { module: device.createShaderModule({ code: waterDepthReduceWGSL }), entryPoint: "main" },
  });
  const results = [];
  for (const [width, height] of [
    [65, 33],
    [5, 3],
    [1, 17],
  ]) {
    const w = Math.max(1, width >> 1),
      h = Math.max(1, height >> 1);
    const source = device.createTexture({
      size: [width, height],
      format: "r32float",
      usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.TEXTURE_BINDING,
    });
    const output = device.createTexture({
      size: [w, h],
      format: "r32float",
      usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.COPY_SRC,
    });
    const rowBytes = Math.ceil((w * 4) / 256) * 256;
    const readback = device.createBuffer({
      size: rowBytes * h,
      usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });
    try {
      const values = Float32Array.from({ length: width * height }, (_, i) => 0.3 + ((i * 31) % 173) / 300);
      values[values.length - 1] = 0.01;
      device.queue.writeTexture({ texture: source }, values, { bytesPerRow: width * 4 }, [width, height]);
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginComputePass();
      pass.setPipeline(pipeline);
      pass.setBindGroup(
        0,
        device.createBindGroup({
          layout: pipeline.getBindGroupLayout(0),
          entries: [
            { binding: 0, resource: source.createView() },
            { binding: 1, resource: output.createView() },
          ],
        }),
      );
      pass.dispatchWorkgroups(Math.ceil(w / 8), Math.ceil(h / 8));
      pass.end();
      encoder.copyTextureToBuffer({ texture: output }, { buffer: readback, bytesPerRow: rowBytes }, [w, h]);
      device.queue.submit([encoder.finish()]);
      await readback.mapAsync(GPUMapMode.READ);
      const actual = new Float32Array(readback.getMappedRange());
      let maximumError = 0;
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++) {
          let expected = 1;
          for (let sy = Math.floor((y * height) / h); sy < Math.ceil(((y + 1) * height) / h); sy++)
            for (let sx = Math.floor((x * width) / w); sx < Math.ceil(((x + 1) * width) / w); sx++)
              expected = Math.min(expected, values[sy * width + sx]);
          maximumError = Math.max(maximumError, Math.abs(expected - actual[(y * rowBytes) / 4 + x]));
        }
      if (!Number.isFinite(maximumError) || maximumError > 1e-7)
        throw Error(`Non-conservative water depth mip: ${width}×${height}, error ${maximumError}`);
      readback.unmap();
      results.push({ width, height, maximumError });
    } finally {
      source.destroy();
      output.destroy();
      readback.destroy();
    }
  }
  return results;
}
