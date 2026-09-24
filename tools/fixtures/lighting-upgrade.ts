import { referenceProject } from "@wrela/examples";
import type { EvaluatedScene, Vec3 } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { EnvironmentReflectionGpu } from "@wrela/render-webgpu/environment-reflection";
import { BrowserSceneHost, evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectBoxFixture } from "./indirect-scenes";

async function readBuffer(device: GPUDevice, source: GPUBuffer, count: number) {
  const output = device.createBuffer({
    size: count * 4,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
  });
  const read = device.createBuffer({
    size: count * 4,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  try {
    const pipeline = device.createComputePipeline({
      layout: "auto",
      compute: {
        module: device.createShaderModule({
          code: `@group(0) @binding(0) var<storage,read> src:array<f32>;@group(0) @binding(1) var<storage,read_write> dst:array<f32>;@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3u){if(id.x<arrayLength(&dst)){dst[id.x]=src[id.x];}}`,
        }),
        entryPoint: "main",
      },
    });
    const encoder = device.createCommandEncoder(),
      pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(
      0,
      device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
          { binding: 0, resource: { buffer: source } },
          { binding: 1, resource: { buffer: output } },
        ],
      }),
    );
    pass.dispatchWorkgroups(Math.ceil(count / 64));
    pass.end();
    encoder.copyBufferToBuffer(output, 0, read, 0, count * 4);
    device.queue.submit([encoder.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    return new Float32Array(read.getMappedRange().slice(0));
  } finally {
    read.destroy();
    output.destroy();
  }
}
async function reflectionCheck(device: GPUDevice) {
  const sky = device.createTexture({
    size: [16, 8],
    format: "rgba16float",
    usage: GPUTextureUsage.STORAGE_BINDING | GPUTextureUsage.TEXTURE_BINDING,
  });
  const sampler = device.createSampler({
    minFilter: "linear",
    magFilter: "linear",
    mipmapFilter: "linear",
    addressModeU: "repeat",
  });
  const reflection = new EnvironmentReflectionGpu(device, sky.createView(), sampler);
  try {
    const fill = device.createComputePipeline({
      layout: "auto",
      compute: {
        module: device.createShaderModule({
          code: `@group(0) @binding(0) var dst:texture_storage_2d<rgba16float,write>;@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3u){textureStore(dst,id.xy,vec4f(1.0,0.5,0.25,1.0));}`,
        }),
        entryPoint: "main",
      },
    });
    const encoder = device.createCommandEncoder();
    let pass = encoder.beginComputePass();
    pass.setPipeline(fill);
    pass.setBindGroup(
      0,
      device.createBindGroup({
        layout: fill.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: sky.createView() }],
      }),
    );
    pass.dispatchWorkgroups(2, 1);
    pass.end();
    pass = encoder.beginComputePass();
    reflection.encode(pass);
    pass.end();
    device.queue.submit([encoder.finish()]);
    const lut = await linearImage({ device, sceneColor: reflection.lut } as unknown as WebGPURenderer);
    let maximumError = 0;
    const probes = [];
    // Independent uniform-hemisphere integration of the shared correlated-Smith BRDF.
    for (const x of [8, 28, 56])
      for (const y of [24, 40, 60]) {
        const nv = (x + 0.5) / 64,
          rough = (y + 0.5) / 64,
          a2 = rough ** 4,
          vx = Math.sqrt(1 - nv * nv);
        let A = 0,
          B = 0;
        for (let i = 0; i < 65536; i++) {
          const nl = (i + 0.5) / 65536,
            angle = i * 2.399963229728653,
            r = Math.sqrt(1 - nl * nl),
            lx = r * Math.cos(angle),
            ly = r * Math.sin(angle);
          const length = Math.hypot(vx + lx, ly, nv + nl),
            nh = (nv + nl) / length,
            vh = (vx * (vx + lx) + nv * (nv + nl)) / length;
          const q = 1 - (1 - a2) * nh * nh;
          const value =
            (a2 * nl) /
            (2 *
              Math.PI *
              q *
              q *
              (nl * Math.sqrt(a2 + (1 - a2) * nv * nv) + nv * Math.sqrt(a2 + (1 - a2) * nl * nl)));
          const fc = (1 - vh) ** 5;
          A += (value * (1 - fc) * 2 * Math.PI) / 65536;
          B += (value * fc * 2 * Math.PI) / 65536;
        }
        const actual = [lut[(y * 64 + x) * 4], lut[(y * 64 + x) * 4 + 1]];
        const error = Math.max(Math.abs(actual[0] - A), Math.abs(actual[1] - B));
        maximumError = Math.max(maximumError, error);
        probes.push({ nv, rough, actual, reference: [A, B], error });
      }
    // Probe every mip through a single texture-load shader, avoiding half-float CPU conversion.
    const result = device.createBuffer({
      size: 8 * 16,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
    });
    try {
      const pipeline = device.createComputePipeline({
        layout: "auto",
        compute: {
          module: device.createShaderModule({
            code: `@group(0) @binding(0) var src:texture_2d<f32>;@group(0) @binding(1) var<storage,read_write> dst:array<vec4f>;@compute @workgroup_size(8) fn main(@builtin(global_invocation_id) id:vec3u){dst[id.x]=textureLoad(src,vec2i(0),i32(id.x));}`,
          }),
          entryPoint: "main",
        },
      });
      const e = device.createCommandEncoder(),
        p = e.beginComputePass();
      p.setPipeline(pipeline);
      p.setBindGroup(
        0,
        device.createBindGroup({
          layout: pipeline.getBindGroupLayout(0),
          entries: [
            { binding: 0, resource: reflection.view },
            { binding: 1, resource: { buffer: result } },
          ],
        }),
      );
      p.dispatchWorkgroups(1);
      p.end();
      device.queue.submit([e.finish()]);
      const values = await readBuffer(device, result, 32);
      let constantError = 0;
      for (let i = 0; i < 8; i++)
        for (let c = 0; c < 3; c++)
          constantError = Math.max(constantError, Math.abs(values[i * 4 + c] - [1, 0.5, 0.25][c]));
      if (maximumError > 0.015 || constantError > 0.001)
        throw Error(`Environment integration error ${maximumError}; constant ${constantError}`);
      return { maximumError, constantError, probes };
    } finally {
      result.destroy();
    }
  } finally {
    sky.destroy();
    reflection.destroy();
  }
}
export async function createLightingUpgradeFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing lighting canvas");
  canvas.style.width = "640px";
  canvas.style.height = "480px";
  const errors: string[] = [];
  const cache = new IndirectLightingCache();
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const device = (renderer as unknown as { device: GPUDevice }).device;
  const check = () => {
    if (errors.length || !renderer.completeness.complete)
      throw Error(
        `Lighting frame failed: ${errors.join(";")} ${JSON.stringify(renderer.completeness.rejected)}`,
      );
  };
  const capture = async (scene: EvaluatedScene) => {
    for (let i = 0; i < 5; i++) {
      renderer.render(scene);
      await renderer.flushGpuTimings();
      if (renderer.completeness.complete) break;
    }
    check();
    const pixels = await linearImage(renderer);
    if (!pixels.every(Number.isFinite)) throw Error("Nonfinite lighting output");
    const blob = await renderer.capture();
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let binary = "";
    for (let i = 0; i < bytes.length; i += 8192)
      binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
    return { image: btoa(binary), pixels, measurements: { ...renderer.measurements } };
  };
  async function pairedTimings(setCondition: (enabled: boolean) => void, scene: EvaluatedScene) {
    const enabled: number[] = [],
      control: number[] = [];
    for (const condition of [true, false, false, true]) {
      for (let i = 0; i < 18; i++) {
        setCondition(condition);
        renderer.render(scene);
        await renderer.flushGpuTimings();
        const timings = renderer.drainGpuTimings();
        if (i >= 2) for (const timing of timings) (condition ? enabled : control).push(timing.gpuMs);
        check();
      }
    }
    const summary = (values: number[]) => {
      values.sort((a, b) => a - b);
      return {
        samples: values.length,
        medianMs: values[Math.floor(values.length / 2)],
        p95Ms: values[Math.floor((values.length - 1) * 0.95)],
      };
    };
    return {
      order: "ABBA",
      enabled: summary(enabled),
      control: summary(control),
      scope:
        "Short whole-frame GPU observations, fixed scene at 640×480; build and upload settling excluded.",
    };
  }
  return {
    async check() {
      return reflectionCheck(device);
    },
    async capture(name: "physical-gi" | "lantern" | "winter") {
      if (name === "winter") {
        const project = referenceProject(),
          host = new BrowserSceneHost(project, {
            indirectLighting: {
              dimensions: [12, 6, 12],
              cameraVolume: { radius: 24, halfHeight: 6, snap: 8 },
              samples: 96,
              skySamples: 8,
              maxTriangles: 180000,
            },
          });
        try {
          await host.prepare(project.entry, "neutral-stage", "interactive");
          const camera = { position: [8, 4.2, 11] as Vec3, target: [0.8, 1, 0.5] as Vec3, fov: 48 };
          host.updateView(camera);
          await host.world?.prepare();
          let scene = host.extract(camera);
          scene.grid = false;
          await host.waitForIndirectLighting();
          scene = host.extract(camera);
          scene.grid = false;
          const result = await capture(scene);
          const staticReceivers =
            renderer.realizedScene?.surfaces.filter((s) => s.staticIndirectReceiver).length ?? 0;
          const field = scene.indirectLighting;
          scene.indirectLighting = undefined;
          const fallback = await capture(scene);
          const performance = await pairedTimings((enabled) => {
            scene.indirectLighting = enabled ? field : undefined;
          }, scene);
          const compiledVisibility = await (async () => {
            if (!field?.visibility?.cells) return;
            const cells = field.visibility.cells.slice();
            const count = field.dimensions.reduce((n, d) => n * (d - 1), 1);
            let regions = 0,
              bvhCells = 0,
              emptyCells = 0;
            for (let i = 0; i < count; i++) {
              if (cells[i * 4 + 2]) regions++;
              if (cells[i * 4 + 1] < 0) bvhCells++;
              if (cells[i * 4 + 1] === 0) emptyCells++;
              cells[i * 4 + 2] = 0;
              cells[i * 4 + 3] = 0;
            }
            const control = {
              ...field,
              key: `${field.key}/coarse-cell-control`,
              visibility: { ...field.visibility, cells },
            };
            scene.indirectLighting = control;
            const coarse = await capture(scene);
            let maximumError = 0;
            for (let i = 0; i < result.pixels.length; i++)
              maximumError = Math.max(maximumError, Math.abs(result.pixels[i] - coarse.pixels[i]));
            if (maximumError > 0.002)
              throw Error(`Compiled receiver regions changed lighting: ${maximumError}`);
            let leaves = 0,
              fallbackLeaves = 0,
              candidates = 0,
              maximumCandidates = 0;
            for (let i = 0; i < count; i++) {
              const root = field.visibility.cells[i * 4 + 2];
              const stack = root > 0 ? [root] : [];
              while (stack.length) {
                const r = (stack.pop() ?? 0) * 4;
                const next = field.visibility.cells[r + 2],
                  n = field.visibility.cells[r + 3];
                if (next > 0) for (let child = 0; child < 8; child++) stack.push(next + child);
                else {
                  leaves++;
                  if (n < 0) fallbackLeaves++;
                  else {
                    candidates += n;
                    maximumCandidates = Math.max(n, maximumCandidates);
                  }
                }
              }
            }
            const unguarded = {
              ...field,
              key: `${field.key}/no-visibility-diagnostic`,
              visibility: undefined,
            };
            return {
              regions,
              bvhCells,
              emptyCells,
              cells: count,
              bytes: cells.byteLength,
              maximumError,
              staticReceivers,
              leaves,
              fallbackLeaves,
              candidates,
              maximumCandidates,
              performance: await pairedTimings((enabled) => {
                scene.indirectLighting = enabled ? field : control;
              }, scene),
              diagnosticQueryCost: await pairedTimings((enabled) => {
                scene.indirectLighting = enabled ? field : unguarded;
              }, scene),
            };
          })();
          return {
            ...result,
            pixels: undefined,
            unshadowed: fallback.image,
            performance,
            compiledVisibility,
            report: host.indirectLightingReport,
            volume: field && { origin: field.origin, spacing: field.spacing, dimensions: field.dimensions },
          };
        } finally {
          host.dispose();
        }
      }
      const f = indirectBoxFixture();
      const environment = evaluateEnvironment();
      environment.cloudCover = 0;
      environment.fogDensity = 0;
      environment.ambient = 1;
      environment.exposure = 1;
      const scene: EvaluatedScene = {
        surfaces: f.surfaces,
        camera: f.camera,
        environment,
        time: 0,
        mode: "beauty",
        grid: false,
      };
      if (name === "physical-gi") {
        cache.update(scene, { dimensions: [8, 6, 8], samples: 128, skySamples: 8 });
        scene.indirectLighting = await cache.waitReady();
        const result = await capture(scene);
        const gpu = renderer as unknown as {
          indirectLighting: { buffer: GPUBuffer };
          atmosphereGpu: { irradianceBuffer: GPUBuffer };
        };
        const data = await readBuffer(
          device,
          gpu.indirectLighting.buffer,
          16 + scene.indirectLighting.data.length + (scene.indirectLighting.reflections?.data.length ?? 0),
        );
        if (!data.every(Number.isFinite)) throw Error("Nonfinite relit GI");
        // An intensity edit reuses the compiled field and updates its GPU coefficients.
        const field = scene.indirectLighting;
        environment.sunIntensity *= 0.5;
        if (cache.update(scene, { dimensions: [8, 6, 8], samples: 128, skySamples: 8 }) !== field)
          throw Error("Light intensity rebuilt static geometry");
        await capture(scene);
        const changed = await readBuffer(device, gpu.indirectLighting.buffer, data.length);
        const delta = Math.max(...changed.map((v, i) => Math.abs(v - data[i])));
        if (delta < 1e-5) throw Error("GI did not relight after intensity edit");
        return { ...result, pixels: undefined, relightMaximumChange: delta, report: field.report };
      }
      environment.sunIntensity = 0;
      environment.ambient = 0;
      environment.pointLights = [
        { position: [-0.65, 1.35, 0.4], color: [1, 0.63, 0.25], intensity: 8, range: 6 },
      ];
      renderer.render(scene);
      const firstPasses = renderer.measurements.localShadowPasses;
      const shadowed = await capture(scene);
      renderer.render(scene);
      await renderer.flushGpuTimings();
      if (renderer.measurements.localShadowPasses !== 0) throw Error("Static local shadow was redrawn");
      environment.pointLights[0].shadows = false;
      const unshadowed = await capture(scene);
      let shadowDelta = 0;
      for (let i = 0; i < shadowed.pixels.length; i++)
        if (i % 4 !== 3) shadowDelta += Math.max(0, unshadowed.pixels[i] - shadowed.pixels[i]);
      if (shadowDelta < 0.1) throw Error("Local shadow failed to darken any receiver");
      environment.pointLights[0].shadows = true;
      scene.surfaces[scene.surfaces.length - 1].matrix[12] += 0.3;
      renderer.render(scene);
      await renderer.flushGpuTimings();
      if (!renderer.measurements.localShadowPasses) throw Error("Moving caster failed to invalidate shadow");
      const movedPasses = renderer.measurements.localShadowPasses;
      const performance = await pairedTimings((cached) => {
        if (!cached)
          (renderer as unknown as { pointShadows: { invalidate(): void } }).pointShadows.invalidate();
      }, scene);
      return {
        ...shadowed,
        pixels: undefined,
        unshadowed: unshadowed.image,
        shadowDelta,
        firstPasses,
        movedPasses,
        performance,
      };
    },
    dispose() {
      cache.dispose();
      renderer.dispose();
    },
  };
}
