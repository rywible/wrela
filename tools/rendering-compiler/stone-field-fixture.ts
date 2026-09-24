import { BrowserCompiler } from "@wrela/compiler/client";
import { referenceProject } from "@wrela/examples";
import type { EvaluatedScene, GpuFrameTiming } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import { assertCompleteIdentities, compareLinear } from "./manifest";
import { STONE_COMPILE_QUALITY, stoneFieldCamera, stoneFieldScene } from "./stone-field-layout";

/** Read the actual scene-linear HDR target through its existing texture binding, without changing renderer allocation. */
async function linearImage(renderer: WebGPURenderer) {
  const { device, sceneColor } = renderer as unknown as { device: GPUDevice; sceneColor: GPUTexture };
  const size = sceneColor.width * sceneColor.height * 16;
  const output = device.createBuffer({ size, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
  const readback = device.createBuffer({ size, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  try {
    const module = device.createShaderModule({
      code: `
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> pixels: array<vec4f>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id: vec3u) {
 let size = textureDimensions(source);
 if (id.x >= size.x || id.y >= size.y) { return; }
 pixels[id.y * size.x + id.x] = textureLoad(source, vec2i(id.xy), 0);
}`,
    });
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "main" },
    });
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sceneColor.createView() },
        { binding: 1, resource: { buffer: output } },
      ],
    });
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, group);
    pass.dispatchWorkgroups(Math.ceil(sceneColor.width / 8), Math.ceil(sceneColor.height / 8));
    pass.end();
    encoder.copyBufferToBuffer(output, 0, readback, 0, size);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const pixels = new Float32Array(readback.getMappedRange().slice(0));
    readback.unmap();
    // This also rejects nonfinite output in direct captures.
    compareLinear(pixels, pixels);
    return pixels;
  } finally {
    output.destroy();
    readback.destroy();
  }
}

export type StoneVariant = "auto" | "analytic" | "parametric";
/** The production compiler and renderer; only the deterministic workload is synthetic. */
export async function createStoneFieldFixture(count: number) {
  const side = Math.sqrt(count);
  if (!Number.isInteger(side) || side < 2 || side > 64)
    throw Error("Stone grid must be square, at most 4096 instances");
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing canvas");
  const compiler = new BrowserCompiler("/compile-worker.js");
  const host = new BrowserSceneHost(referenceProject(), {
    compile: compiler.compile,
    generateTerrain: compiler.generateTerrain,
  });
  let renderer: WebGPURenderer | undefined;
  const start = performance.now();
  await host.prepare("river-stone", "neutral-stage", STONE_COMPILE_QUALITY);
  const camera = stoneFieldCamera(side);
  host.evaluate(0, camera);
  const base = host.extract(camera);
  const stone = base.surfaces.find((s) => s.source === "river-stone");
  if (!stone?.renderProducts?.some((p) => p.kind === "analytic-quadric"))
    throw Error("Compiler did not supply analytic stone product");
  const scene = stoneFieldScene(base, stone, count);
  const preparationMs = performance.now() - start;
  const ids = scene.surfaces.map((s) => s.id);
  const references = new Map<string, Float32Array>();
  let variant: StoneVariant = "auto";
  return {
    preparation: {
      count,
      side,
      compileQuality: STONE_COMPILE_QUALITY,
      spacingMetres: 2.5,
      camera,
      preparationMs,
      compilerProducts: stone.renderProducts.map((p) => ({
        key: p.key,
        kind: p.kind,
        errors: p.errors,
        byteLength: p.byteLength,
        triangles:
          p.kind === "parametric-mesh"
            ? p.mesh.indices.length / 3
            : p.kind === "direct-mesh"
              ? stone.mesh.indices.length / 3
              : null,
      })),
      primaryTriangles: stone.mesh.indices.length / 3,
    },
    async begin(next: StoneVariant) {
      renderer?.dispose();
      variant = next;
      const start = performance.now();
      renderer = await WebGPURenderer.create(canvas, {
        pixelRatio: 1,
        quality: "balanced",
        renderCompiler: { geometry: variant, maxGeometryErrorPixels: 0.25, visibility: false },
      });
      if (/swiftshader|llvmpipe|software/i.test(renderer.measurements.adapter))
        throw Error("Hardware adapter required");
      renderer.render(scene);
      await renderer.capture();
      assertCompleteIdentities(ids, renderer.completeness);
      const selections = renderer.completeness.realizations ?? [];
      if (selections.length !== count) throw Error("Every stone must have a recorded realization");
      if (
        variant !== "auto" &&
        selections.some((s) => s.kind !== (variant === "analytic" ? "analytic-quadric" : "parametric-mesh"))
      )
        throw Error("Requested control is not valid at the common 0.25 pixel error budget");
      return {
        initializationMs: performance.now() - start,
        adapter: renderer.measurements.adapter,
        measurements: structuredClone(renderer.measurements),
        selections: structuredClone(selections),
      };
    },
    async run(frames: number) {
      if (!renderer || frames < 1 || frames > 64) throw Error("Invalid bounded run");
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      const first = renderer.measurements.frame + 1;
      const cpu: number[] = [],
        gpu: GpuFrameTiming[] = [];
      for (let frame = 0; frame < frames; frame++) {
        await new Promise(requestAnimationFrame);
        const start = performance.now();
        renderer.render(scene);
        cpu.push(performance.now() - start);
        gpu.push(...renderer.drainGpuTimings());
        assertCompleteIdentities(ids, renderer.completeness);
      }
      await renderer.flushGpuTimings();
      gpu.push(...renderer.drainGpuTimings());
      const last = renderer.measurements.frame;
      return {
        variant,
        cpu,
        gpu: gpu
          .filter((s) => s.frame >= first && s.frame <= last)
          .map((s) => ({ ...s, trajectoryFrame: s.frame - first })),
        measurements: structuredClone(renderer.measurements),
        completeness: structuredClone(renderer.completeness),
      };
    },
    async capture(mode: EvaluatedScene["mode"]) {
      if (!renderer) throw Error("Begin variant first");
      renderer.render({ ...scene, mode });
      await renderer.capture();
      assertCompleteIdentities(ids, renderer.completeness);
      const pixels = await linearImage(renderer);
      let comparison: ReturnType<typeof compareLinear> | null = null;
      if (variant === "analytic") references.set(mode, pixels);
      else {
        const reference = references.get(mode);
        if (reference) comparison = compareLinear(reference, pixels);
      }
      return {
        variant,
        mode,
        finite: true,
        comparison,
        completeness: structuredClone(renderer.completeness),
      };
    },
    dispose() {
      renderer?.dispose();
      host.dispose();
      compiler.dispose();
    },
  };
}
export type StoneFieldFixture = Awaited<ReturnType<typeof createStoneFieldFixture>>;
