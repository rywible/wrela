import { sampleWaterGrid, sampleWaterSpectrum } from "@wrela/compiler";
import { createWaterLookdev } from "@wrela/examples/water-lookdev";
import { identityMatrix, type RenderSurface, type Vec3, waterSchema } from "@wrela/model";
import { packWaterBody } from "@wrela/render-webgpu/water-body";
import waterBody from "@wrela/render-webgpu/water-body.wgsl" with { type: "text" };
import { WaterBodyRuntime } from "@wrela/runtime/water-body";
import { verifyWaterDepthHierarchy } from "./water-depth-conformance";
import { verifyWaterHistory } from "./water-history";
import { verifyWaterSpectrumMap } from "./water-spectrum-conformance";

/** Exercise the shipped grid and spectrum WGSL against its CPU reference, including rebasing. */
export async function verifyWaterGpu() {
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error("WebGPU unavailable");
  const device = await adapter.requestDevice();
  const errors: string[] = [];
  device.addEventListener("uncapturederror", (event) => errors.push(event.error.message));
  try {
    const module = device.createShaderModule({
      code:
        waterBody.slice(0, waterBody.indexOf("@group(1) @binding(6)")) +
        `
@group(0) @binding(0) var<storage,read> queries:array<vec4f>;
@group(0) @binding(1) var<storage,read_write> answers:array<vec4f>;
@compute @workgroup_size(32) fn verify(@builtin(global_invocation_id) id:vec3u){
  if(id.x>=arrayLength(&queries)){return;}
  let q=queries[id.x];let wave=waterBodyWaves(q.xy,q.z,q.w);
  answers[id.x*2u]=wave.value;
  answers[id.x*2u+1u]=waterBodyGrid(q.xy,1u);
}`,
    });
    const compilation = await module.getCompilationInfo();
    if (compilation.messages.some((message) => message.type === "error"))
      throw new Error(compilation.messages.map((message) => message.message).join("; "));
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "verify" },
    });
    const results = [];
    for (const mode of ["creek", "ocean", "mixed"] as const) {
      const ocean = mode !== "creek";
      const project = createWaterLookdev(ocean ? "ocean" : "creek");
      const water = waterSchema.parse(project.documents.find((document) => document.id === project.entry));
      if (mode === "mixed")
        water.waves = [{ amplitude: 0.08, wavelength: 3.27, direction: 1.13, phase: 0.4, speed: 2.1 }];
      const body = new WaterBodyRuntime(water);
      for (let tick = 0; tick < 20; tick++) body.simulation?.step(1 / 60);
      const state = body.renderState();
      const origin: Vec3 = ocean ? [100000, 19, -200000] : [128, 19, -128];
      const surface: RenderSurface = {
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
      };
      const queries = new Float32Array(64 * 4),
        expected: number[][] = [];
      for (let i = 0; i < 64; i++) {
        const x = ocean ? origin[0] + i * 0.125 : -8 + i * 0.25;
        const z = ocean ? origin[2] - i * 0.5 : -16 + i * 0.5;
        const time = 2.25,
          spacing = i % 3 ? 0.25 : 2;
        queries.set([x - origin[0], z - origin[2], time, spacing], i * 4);
        const wave = sampleWaterSpectrum(state.spectrum, x, z, time, spacing);
        const grid =
          state.domain && state.cells ? sampleWaterGrid(state.domain, state.cells, x, z) : undefined;
        expected.push([
          wave.height,
          wave.dx,
          wave.dz,
          (grid?.[0] ?? water.level) - origin[1],
          grid?.[1] ?? 0,
          grid?.[2] ?? 0,
          grid?.[3] ?? 0,
        ]);
      }
      const buffer = (data: Float32Array) => {
        const b = device.createBuffer({
          size: data.byteLength,
          usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
        });
        device.queue.writeBuffer(b, 0, data as Float32Array<ArrayBuffer>);
        return b;
      };
      const packed = buffer(packWaterBody(surface, origin)),
        input = buffer(queries);
      const output = device.createBuffer({
        size: 64 * 8 * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
      });
      const readback = device.createBuffer({
        size: output.size,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
      });
      try {
        const encoder = device.createCommandEncoder(),
          pass = encoder.beginComputePass();
        pass.setPipeline(pipeline);
        pass.setBindGroup(
          0,
          device.createBindGroup({
            layout: pipeline.getBindGroupLayout(0),
            entries: [
              { binding: 0, resource: { buffer: input } },
              { binding: 1, resource: { buffer: output } },
            ],
          }),
        );
        pass.setBindGroup(
          1,
          device.createBindGroup({
            layout: pipeline.getBindGroupLayout(1),
            entries: [{ binding: 5, resource: { buffer: packed } }],
          }),
        );
        pass.dispatchWorkgroups(2);
        pass.end();
        encoder.copyBufferToBuffer(output, 0, readback, 0, output.size);
        device.queue.submit([encoder.finish()]);
        await readback.mapAsync(GPUMapMode.READ);
        const actual = new Float32Array(readback.getMappedRange());
        let maximumError = 0;
        for (let i = 0; i < expected.length; i++)
          for (let channel = 0; channel < 7; channel++)
            maximumError = Math.max(
              maximumError,
              Math.abs(actual[i * 8 + (channel < 3 ? channel : channel + 1)] - expected[i][channel]),
            );
        if (!Number.isFinite(maximumError) || maximumError > 0.001)
          throw new Error(`CPU/GPU water mismatch: ${maximumError}`);
        const shadingMap = await verifyWaterSpectrumMap(device, surface, origin);
        results.push({ ocean, mode, samples: expected.length, maximumError, origin, shadingMap });
        readback.unmap();
      } finally {
        packed.destroy();
        input.destroy();
        output.destroy();
        readback.destroy();
      }
    }
    const depthHierarchy = await verifyWaterDepthHierarchy(device);
    const history = await verifyWaterHistory(device);
    if (errors.length) throw new Error(errors.join("; "));
    return { spectra: results, depthHierarchy, history };
  } finally {
    device.destroy();
  }
}
