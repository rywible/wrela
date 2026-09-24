import type { EvaluatedScene, GpuFrameTiming, Vec3 } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectBoxFixture } from "./indirect-scenes";

/** Hold scene/transport constant across frozen old and current renderer modules.
 * Guards against an apparent cache win caused by slowing the uncached kernel. */
export async function surfaceLightingKernelControl(previous: typeof WebGPURenderer) {
  const errors: string[] = [],
    renderers: WebGPURenderer[] = [];
  const fixture = indirectBoxFixture();
  const scene: EvaluatedScene = {
    surfaces: fixture.surfaces,
    camera: fixture.camera,
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  scene.environment.cloudCover = 0;
  scene.environment.fogDensity = 0;
  const cache = new IndirectLightingCache();
  cache.update(scene, { surfaceCache: false, dimensions: [8, 6, 8] as Vec3, samples: 64, skySamples: 4 });
  const field = await cache.waitReady();
  try {
    for (const type of [previous, WebGPURenderer]) {
      const canvas = document.createElement("canvas");
      canvas.style.width = "640px";
      canvas.style.height = "480px";
      document.body.append(canvas);
      renderers.push(
        await type.create(canvas, {
          pixelRatio: 1,
          resolutionScale: 1,
          quality: "balanced",
          antialiasing: "spatial",
          onDiagnostic: (d) => {
            if (d.severity === "error") errors.push(d.message);
          },
        }),
      );
    }
    const results = [];
    for (const gi of [false, true]) {
      scene.indirectLighting = gi ? field : undefined;
      const timings: GpuFrameTiming[][] = [[], []];
      const images: Float32Array[] = [];
      for (const index of [0, 1, 1, 0]) {
        const renderer = renderers[index];
        for (let frame = 0; frame < 5; frame++) {
          renderer.render(scene);
          await renderer.flushGpuTimings();
        }
        renderer.drainGpuTimings();
        for (let frame = 0; frame < 16; frame++) {
          renderer.render(scene);
          await renderer.flushGpuTimings();
          timings[index].push(...renderer.drainGpuTimings());
        }
        if (!renderer.completeness.complete) throw Error("Incomplete renderer kernel control");
        images[index] = await linearImage(renderer);
      }
      let maximum = 0;
      for (let i = 0; i < images[0].length; i++)
        maximum = Math.max(maximum, Math.abs(images[0][i] - images[1][i]));
      if (maximum > 0.0001) errors.push("Uncached renderer changed the control image");
      results.push({ gi, maximum, timings });
    }
    return {
      adapter: renderers[1].measurements.adapter,
      viewport: [640, 480],
      order: "ABBA",
      results,
      errors,
    };
  } finally {
    cache.dispose();
    for (const renderer of renderers) renderer.dispose();
  }
}
