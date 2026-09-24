/** Focused hardware harness kept beside the fixture so source snapshots are self-contained. */
export type Work = (encoder: GPUCommandEncoder, timestamps?: GPUComputePassTimestampWrites) => void;

export async function gpuContext() {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
  if (!adapter || !adapter.features.has("timestamp-query"))
    throw Error("Hardware WebGPU timestamps required");
  const device = await adapter.requestDevice({ requiredFeatures: ["timestamp-query"] });
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  const query = device.createQuerySet({ type: "timestamp", count: 2 });
  const resolved = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
  });
  const timingReadback = device.createBuffer({
    size: 16,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });

  async function measure(work: Work, repetitions: number) {
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
    encoder.resolveQuerySet(query, 0, 2, resolved, 0);
    encoder.copyBufferToBuffer(resolved, 0, timingReadback, 0, 16);
    device.queue.submit([encoder.finish()]);
    await timingReadback.mapAsync(GPUMapMode.READ);
    const values = new BigUint64Array(timingReadback.getMappedRange());
    const result = Number(values[1] - values[0]) / (1e6 * repetitions);
    timingReadback.unmap();
    return result;
  }

  async function benchmark(works: Record<string, Work>, repetitions: number) {
    if (!Number.isInteger(repetitions) || repetitions < 2)
      throw Error("At least two timestamped dispatches required");
    const entries = Object.entries(works),
      times: Record<string, number[]> = Object.fromEntries(entries.map(([name]) => [name, []]));
    for (let warmup = 0; warmup < 3; warmup++)
      for (const [, work] of entries) await measure(work, repetitions);
    for (let iteration = 0; iteration < 9; iteration++)
      for (const [name, work] of iteration % 2 ? [...entries].reverse() : entries)
        times[name].push(await measure(work, repetitions));
    return Object.fromEntries(
      Object.entries(times).map(([name, samples]) => {
        const sorted = [...samples].sort((a, b) => a - b);
        return [name, { median: sorted[4], min: sorted[0], max: sorted[8], samples }];
      }),
    );
  }

  async function values(work: Work, source: GPUBuffer, count: number) {
    const readback = device.createBuffer({
      size: count * 4,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    try {
      const encoder = device.createCommandEncoder();
      work(encoder);
      encoder.copyBufferToBuffer(source, 0, readback, 0, count * 4);
      device.queue.submit([encoder.finish()]);
      await readback.mapAsync(GPUMapMode.READ);
      const data = new Float32Array(readback.getMappedRange().slice(0));
      readback.unmap();
      return data;
    } finally {
      readback.destroy();
    }
  }

  async function pipeline(code: string) {
    const module = device.createShaderModule({ code });
    const errors = (await module.getCompilationInfo()).messages.filter((message) => message.type === "error");
    if (errors.length)
      throw Error(errors.map((message) => `${message.lineNum}: ${message.message}`).join("\n"));
    return device.createComputePipelineAsync({ layout: "auto", compute: { module, entryPoint: "main" } });
  }

  return {
    device,
    errors,
    pipeline,
    benchmark,
    values,
    adapter: {
      vendor: adapter.info.vendor,
      architecture: adapter.info.architecture,
      device: adapter.info.device,
      description: adapter.info.description,
    },
    dispose() {
      timingReadback.destroy();
      resolved.destroy();
      query.destroy();
      device.destroy();
    },
  };
}

export function errorStats(values: Float32Array, reference: Float32Array) {
  let squared = 0,
    maximum = 0,
    bias = 0,
    negative = 0;
  for (let i = 0; i < values.length; i++) {
    const delta = values[i] - reference[i];
    squared += delta * delta;
    maximum = Math.max(maximum, Math.abs(delta));
    bias += delta;
    negative += Number(values[i] < 0);
  }
  return { rms: Math.sqrt(squared / values.length), max: maximum, bias: bias / values.length, negative };
}
