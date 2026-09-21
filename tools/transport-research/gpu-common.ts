export type Work = (encoder: GPUCommandEncoder, stamps?: GPUComputePassTimestampWrites) => void;
export async function gpuContext() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
  if (!adapter || !adapter.features.has("timestamp-query"))
    throw new Error("Hardware WebGPU timestamps required");
  const device = await adapter.requestDevice({ requiredFeatures: ["timestamp-query"] });
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  const query = device.createQuerySet({ type: "timestamp", count: 2 });
  const resolve = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
  });
  const read = device.createBuffer({ size: 16, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  async function measure(work: Work, repetitions = 64) {
    const encoder = device.createCommandEncoder();
    for (let i = 0; i < repetitions; i++)
      work(
        encoder,
        i === 0
          ? { querySet: query, beginningOfPassWriteIndex: 0 }
          : i === repetitions - 1
            ? { querySet: query, endOfPassWriteIndex: 1 }
            : undefined,
      );
    encoder.resolveQuerySet(query, 0, 2, resolve, 0);
    encoder.copyBufferToBuffer(resolve, 0, read, 0, 16);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const values = new BigUint64Array(read.getMappedRange());
    const result = Number(values[1] - values[0]) / (1e6 * repetitions);
    read.unmap();
    return result;
  }
  async function benchmark(works: Record<string, Work>, repetitions = 64) {
    const entries = Object.entries(works),
      times: Record<string, number[]> = Object.fromEntries(entries.map(([name]) => [name, []]));
    for (let i = 0; i < 3; i++) for (const [, work] of entries) await measure(work, repetitions);
    for (let i = 0; i < 9; i++)
      for (const [name, work] of i % 2 ? [...entries].reverse() : entries)
        times[name].push(await measure(work, repetitions));
    return Object.fromEntries(
      Object.entries(times).map(([name, samples]) => {
        const sorted = [...samples].sort((a, b) => a - b);
        return [name, { median: sorted[4], min: sorted[0], max: sorted[8], samples }];
      }),
    );
  }
  async function values(work: Work, source: GPUBuffer, count: number) {
    const target = device.createBuffer({
      size: count * 4,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const encoder = device.createCommandEncoder();
    work(encoder);
    encoder.copyBufferToBuffer(source, 0, target, 0, count * 4);
    device.queue.submit([encoder.finish()]);
    await target.mapAsync(GPUMapMode.READ);
    const data = new Float32Array(target.getMappedRange().slice(0));
    target.unmap();
    target.destroy();
    return data;
  }
  function buffer(data: Float32Array | Uint32Array, usage = GPUBufferUsage.STORAGE) {
    const result = device.createBuffer({
      size: Math.max(16, data.byteLength),
      usage: usage | GPUBufferUsage.COPY_DST,
    });
    device.queue.writeBuffer(result, 0, data as Float32Array<ArrayBuffer>);
    return result;
  }
  async function pipeline(code: string) {
    const module = device.createShaderModule({ code });
    const info = await module.getCompilationInfo();
    if (info.messages.some((m) => m.type === "error"))
      throw new Error(info.messages.map((m) => `${m.lineNum}: ${m.message}`).join("\n"));
    return device.createComputePipelineAsync({ layout: "auto", compute: { module, entryPoint: "main" } });
  }
  return {
    device,
    adapter: {
      vendor: adapter.info.vendor,
      architecture: adapter.info.architecture,
      device: adapter.info.device,
      description: adapter.info.description,
    },
    errors,
    buffer,
    pipeline,
    values,
    benchmark,
  };
}
export type GpuContext = Awaited<ReturnType<typeof gpuContext>>;
export const errorStats = (values: Float32Array, truth: Float32Array) => {
  let sum = 0,
    max = 0,
    negative = 0,
    bias = 0;
  for (let i = 0; i < values.length; i++) {
    const d = values[i] - truth[i];
    sum += d * d;
    bias += d;
    max = Math.max(max, Math.abs(d));
    negative += Number(values[i] < 0);
  }
  return { rms: Math.sqrt(sum / values.length), max, bias: bias / values.length, negative };
};
