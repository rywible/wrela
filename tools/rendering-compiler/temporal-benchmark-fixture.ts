import displaySource from "@wrela/render-webgpu/display.wgsl" with { type: "text" };
import { TemporalResolve } from "@wrela/render-webgpu/temporal";
/** Chained offscreen resolves isolate reconstruction from scene shading and presentation pacing.
 * Dependencies between history frames prevent independent passes from overlapping. */
export async function benchmarkTemporalGpu(width = 1920, height = 1080) {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter?.features.has("timestamp-query")) throw Error("GPU timestamps required");
  const device = await adapter.requestDevice({ requiredFeatures: ["timestamp-query"] });
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
  const texture = (format: GPUTextureFormat) =>
    device.createTexture({
      size: [width, height],
      format,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
  const color = texture("rgba16float"),
    motion = texture("rgba32float"),
    depth = texture("depth32float"),
    target = device.createTexture({
      size: [width, height],
      format: "rgba8unorm",
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.STORAGE_BINDING,
    });
  const vertex = `@vertex fn vertexMain(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {return vec4f(vec2f(f32((i<<1u)&2u),f32(i&2u))*2.0-1.0,0.0,1.0);}`;
  const initModule = device.createShaderModule({
    code: `${vertex}
 struct Result{@location(0) color:vec4f,@location(1) motion:vec4f,@builtin(frag_depth) depth:f32};
 @fragment fn fragmentMain(@builtin(position) p:vec4f)->Result {let v=0.1+f32((u32(p.x)+u32(p.y))%3u)*0.6;return Result(vec4f(v,v*0.8,v*0.6,1.0),vec4f((p.xy+vec2f(0.25,0.1))/vec2f(${width}.0,${height}.0),1.0,2.0),2375.0/2499.95);}`,
  });
  const init = device.createRenderPipeline({
    layout: "auto",
    vertex: { module: initModule, entryPoint: "vertexMain" },
    fragment: {
      module: initModule,
      entryPoint: "fragmentMain",
      targets: [{ format: "rgba16float" }, { format: "rgba32float" }],
    },
    depthStencil: { format: "depth32float", depthWriteEnabled: true, depthCompare: "always" },
  });
  const encoder = device.createCommandEncoder(),
    pass = encoder.beginRenderPass({
      colorAttachments: [color, motion].map((t) => ({
        view: t.createView(),
        loadOp: "clear",
        storeOp: "store",
        clearValue: [0, 0, 0, 0],
      })),
      depthStencilAttachment: {
        view: depth.createView(),
        depthLoadOp: "clear",
        depthStoreOp: "store",
        depthClearValue: 1,
      },
    });
  pass.setPipeline(init);
  pass.draw(3);
  pass.end();
  device.queue.submit([encoder.finish()]);
  const module = device.createShaderModule({ code: displaySource });
  const display = device.createRenderPipeline({
    layout: "auto",
    vertex: { module, entryPoint: "vertexMain" },
    fragment: { module, entryPoint: "fragmentMain", targets: [{ format: "rgba8unorm" }] },
  });
  const frame = device.createBuffer({ size: 576, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const data = new Float32Array(144);
  data[61] = 1;
  data[79] = 2;
  device.queue.writeBuffer(frame, 0, data);
  const global = device.createBindGroup({
      layout: display.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: frame } }],
    }),
    sampler = device.createSampler({ minFilter: "linear", magFilter: "linear" });
  const query = device.createQuerySet({ type: "timestamp", count: 2 }),
    resolve = device.createBuffer({
      size: 16,
      usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
    }),
    read = device.createBuffer({ size: 16, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  const results: { trial: number; mode: string; millisecondsPerResolve: number }[] = [];
  try {
    for (let trial = 0; trial < 4; trial++)
      for (const mode of trial % 2 ? ["compute", "raster", "storage"] : ["storage", "raster", "compute"]) {
        const taa = new TemporalResolve(device, width, height);
        try {
          const warm = device.createCommandEncoder();
          taa.encode(warm, color, motion, depth);
          device.queue.submit([warm.finish()]);
          await device.queue.onSubmittedWorkDone();
          const commands = device.createCommandEncoder(),
            iterations = 32;
          for (let i = 0; i < iterations; i++) {
            const stamps = {
              querySet: query,
              ...(i === 0 ? { beginningOfPassWriteIndex: 0 } : {}),
              ...(i === iterations - 1 ? { endOfPassWriteIndex: 1 } : {}),
            };
            if (mode === "storage")
              taa.encode(
                commands,
                color,
                motion,
                depth,
                i === 0 || i === iterations - 1 ? stamps : undefined,
                target,
                1,
              );
            else if (mode === "raster")
              taa.encodeDisplay(
                commands,
                color,
                motion,
                depth,
                target,
                1,
                i === 0 || i === iterations - 1 ? stamps : undefined,
              );
            else {
              const image = taa.encode(
                commands,
                color,
                motion,
                depth,
                i === 0 ? { querySet: query, beginningOfPassWriteIndex: 0 } : undefined,
              );
              const present = commands.beginRenderPass({
                colorAttachments: [
                  { view: target.createView(), loadOp: "clear", storeOp: "store", clearValue: [0, 0, 0, 0] },
                ],
                ...(i === iterations - 1
                  ? { timestampWrites: { querySet: query, endOfPassWriteIndex: 1 } }
                  : {}),
              });
              present.setPipeline(display);
              present.setBindGroup(0, global);
              present.setBindGroup(
                1,
                device.createBindGroup({
                  layout: display.getBindGroupLayout(1),
                  entries: [
                    { binding: 0, resource: image.createView() },
                    { binding: 1, resource: sampler },
                  ],
                }),
              );
              present.draw(3);
              present.end();
            }
          }
          commands.resolveQuerySet(query, 0, 2, resolve, 0);
          commands.copyBufferToBuffer(resolve, 0, read, 0, 16);
          device.queue.submit([commands.finish()]);
          await read.mapAsync(GPUMapMode.READ);
          const timestamps = new BigUint64Array(read.getMappedRange());
          const millisecondsPerResolve = Number(timestamps[1] - timestamps[0]) / 1e6 / iterations;
          read.unmap();
          results.push({ trial, mode, millisecondsPerResolve });
        } finally {
          taa.destroy();
        }
      }
    if (errors.length) throw Error(errors.join("\n"));
    return { resolution: [width, height], iterations: 32, results, errors };
  } finally {
    for (const t of [color, motion, depth, target]) t.destroy();
    for (const b of [frame, resolve, read]) b.destroy();
    query.destroy();
    device.destroy();
  }
}
