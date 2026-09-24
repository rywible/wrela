import { compileWaterSpectrum } from "@wrela/compiler";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import { identityMatrix, type RenderSurface, waterSchema } from "@wrela/model";
import { packWaterBody } from "@wrela/render-webgpu/water-body";
import { WaterSpectrumGpu, WaterSpectrumPrograms } from "@wrela/render-webgpu/water-spectrum-gpu";
import { WaterBodyRuntime } from "@wrela/runtime/water-body";
export async function benchmarkWaterSynthesis() {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter?.features.has("timestamp-query")) throw Error("GPU timing unavailable");
  const device = await adapter.requestDevice({ requiredFeatures: ["timestamp-query"] });
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (e) => errors.push(e.error.message));
  const results = [];
  try {
    for (const mode of ["ocean", "creek"] as const) {
      const project = createWaterLookdev(mode),
        water = waterSchema.parse(project.documents.find((d) => d.id === project.entry));
      const runtime = new WaterBodyRuntime(water),
        state = runtime.renderState();
      const surface = {
        id: water.id,
        source: water.id,
        water,
        waterState: state,
        matrix: identityMatrix(),
        material: {
          color: water.color,
          secondary: water.color,
          roughness: water.roughness,
          metallic: 0,
          pattern: 0,
          scale: 1,
          normalStrength: 0,
        },
        mesh: state.domain?.surface ?? {
          positions: new Float32Array(),
          normals: new Float32Array(),
          indices: new Uint32Array(),
          bounds: { min: [0, 0, 0], max: [0, 0, 0] },
        },
      } as RenderSurface;
      const packed = packWaterBody(surface);
      const buffer = device.createBuffer({
        size: packed.byteLength,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
      });
      device.queue.writeBuffer(buffer, 0, packed as Float32Array<ArrayBuffer>);
      const query = device.createQuerySet({ type: "timestamp", count: 32 });
      const resolved = device.createBuffer({
        size: 256,
        usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
      });
      const read = device.createBuffer({
        size: 256,
        usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
      });
      const versions = (["direct", "recurrence"] as const).map((method) => {
        const programs = new WaterSpectrumPrograms(device, method);
        return {
          method,
          programs,
          gpu: new WaterSpectrumGpu(programs, buffer, compileWaterSpectrum(water)),
          samples: [] as number[],
        };
      });
      let time = 0;
      for (const run of [0, 1, 1, 0]) {
        const item = versions[run];
        for (let round = 0; round < 18; round++) {
          const encoder = device.createCommandEncoder();
          for (let i = 0; i < 16; i++) {
            time += 1 / 60;
            item.gpu.encode(encoder, time, "stable", {
              querySet: query,
              beginningOfPassWriteIndex: i * 2,
              endOfPassWriteIndex: i * 2 + 1,
            });
          }
          encoder.resolveQuerySet(query, 0, 32, resolved, 0);
          encoder.copyBufferToBuffer(resolved, 0, read, 0, 256);
          device.queue.submit([encoder.finish()]);
          await read.mapAsync(GPUMapMode.READ);
          const times = new BigUint64Array(read.getMappedRange());
          let duration = 0;
          for (let i = 0; i < 16; i++) {
            if (times[i * 2 + 1] < times[i * 2]) throw Error("Invalid GPU timestamp interval");
            duration += Number(times[i * 2 + 1] - times[i * 2]);
          }
          if (round >= 3) item.samples.push(duration / 1e6 / 16);
          read.unmap();
        }
      }
      results.push({
        mode,
        allocation: versions[0].gpu.allocation,
        methods: versions.map((v) => {
          const s = [...v.samples].sort((a, b) => a - b);
          return {
            method: v.method,
            p50: s[Math.floor(s.length * 0.5)],
            p95: s[Math.floor(s.length * 0.95)],
            samples: v.samples,
          };
        }),
      });
      for (const item of versions) {
        item.gpu.destroy();
        item.programs.destroy();
      }
      buffer.destroy();
      query.destroy();
      resolved.destroy();
      read.destroy();
    }
  } finally {
    device.destroy();
  }
  if (errors.length) throw Error(errors.join("\n"));
  return {
    adapter: {
      vendor: adapter.info.vendor,
      architecture: adapter.info.architecture,
      description: adapter.info.description,
    },
    methodology:
      "ABBA, 16 dispatches per timestamp interval; 3 warmup + 15 measured intervals per leg. Measures synthesis compute passes only; foam and mip reduction are excluded. Identical carrier source and output layout.",
    results,
    errors,
  };
}
