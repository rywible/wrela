import { TemporalResolve } from "@wrela/render-webgpu/temporal";
/** Synthetic HDR histories exercise the production GPU resolve, independently of motion generation. */
export async function checkTemporalGpu(mode: "compute" | "raster" | "storage" = "compute") {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw Error("WebGPU unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
  const size = 9,
    count = size * size,
    center = 4 * size + 4;
  const color = device.createTexture({
    size: [size, size],
    format: "rgba32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  const motion = device.createTexture({
    size: [size, size],
    format: "rgba32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });
  const depth = device.createTexture({
    size: [size, size],
    format: "depth32float",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.RENDER_ATTACHMENT,
  });
  const values = new Float32Array(count * 4),
    vectors = new Float32Array(count * 4);
  const output = device.createBuffer({
    size: count * 16,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const read = device.createBuffer({
    size: count * 16,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  const pipeline = await device.createComputePipelineAsync({
    layout: "auto",
    compute: {
      module: device.createShaderModule({
        code: `
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var<storage,read_write> pixels:array<vec4f>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u){if(any(id.xy>=vec2u(9))){return;}pixels[id.y*9u+id.x]=textureLoad(source,vec2i(id.xy),0);}`,
      }),
      entryPoint: "main",
    },
  });
  const target = device.createTexture({
    size: [size, size],
    format: "rgba8unorm",
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.STORAGE_BINDING,
  });
  const results: { name: string; actual: number; expected: number }[] = [];
  try {
    for (const [name, expected] of [
      ["stable", 0.12],
      ["reactive-water", 0.45],
      ["identity-change", 1],
      ["depth-disocclusion", 1],
      ["reset", 1],
      ["outside-screen", 1],
      ["neighborhood-clamp", 1],
    ] as const) {
      const resolve = new TemporalResolve(device, size, size);
      try {
        for (let frame = 0; frame < 2; frame++) {
          for (let i = 0; i < count; i++) {
            const c = frame === 0 ? (name === "neighborhood-clamp" ? 5 : 0) : i % 2 === 0 ? 1 : 0;
            values.set([c, c, c, 1], i * 4);
            vectors.set(
              [
                ((i % size) + 0.5) / size,
                (Math.floor(i / size) + 0.5) / size,
                1,
                name === "reactive-water" ? -2 : 2,
              ],
              i * 4,
            );
          }
          if (frame === 1) {
            if (name === "identity-change") for (let i = 0; i < count; i++) vectors[i * 4 + 3] = 3;
            if (name === "depth-disocclusion") vectors[center * 4 + 2] = 20;
            if (name === "reset") resolve.reset();
            if (name === "outside-screen") vectors[center * 4] = -1;
          }
          device.queue.writeTexture({ texture: color }, values, { bytesPerRow: size * 16 }, [size, size]);
          device.queue.writeTexture({ texture: motion }, vectors, { bytesPerRow: size * 16 }, [size, size]);
          const encoder = device.createCommandEncoder();
          encoder
            .beginRenderPass({
              colorAttachments: [],
              depthStencilAttachment: {
                view: depth.createView(),
                depthClearValue: (2500 - 125) / 2499.95,
                depthLoadOp: "clear",
                depthStoreOp: "store",
              },
            })
            .end();
          const resolved =
            mode === "raster"
              ? resolve.encodeDisplay(encoder, color, motion, depth, target, 1)
              : resolve.encode(
                  encoder,
                  color,
                  motion,
                  depth,
                  undefined,
                  mode === "storage" ? target : undefined,
                );
          const pass = encoder.beginComputePass();
          pass.setPipeline(pipeline);
          pass.setBindGroup(
            0,
            device.createBindGroup({
              layout: pipeline.getBindGroupLayout(0),
              entries: [
                { binding: 0, resource: resolved.createView() },
                { binding: 1, resource: { buffer: output } },
              ],
            }),
          );
          pass.dispatchWorkgroups(2, 2);
          pass.end();
          encoder.copyBufferToBuffer(output, 0, read, 0, count * 16);
          device.queue.submit([encoder.finish()]);
          await read.mapAsync(GPUMapMode.READ);
          const pixels = new Float32Array(read.getMappedRange().slice(0));
          read.unmap();
          if (!pixels.every(Number.isFinite)) throw Error(`${name}: nonfinite history`);
          if (frame === 1) {
            const actual = pixels[center * 4];
            results.push({ name, actual, expected });
            if (Math.abs(actual - expected) > 0.002)
              throw Error(`${name}: ${actual}, expected ${expected}; ${errors.join("; ")}`);
          }
        }
      } finally {
        resolve.destroy();
      }
    }
    if (errors.length) throw Error(errors.join("\n"));
    return { results, errors };
  } finally {
    color.destroy();
    motion.destroy();
    depth.destroy();
    output.destroy();
    read.destroy();
    target.destroy();
    device.destroy();
  }
}
