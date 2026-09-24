import type { EvaluatedScene, GpuFrameTiming, Vec3 } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { indirectLightingBytes } from "@wrela/render-webgpu/indirect";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectBoxFixture } from "./indirect-scenes";

function comparison(a: Float32Array, b: Float32Array) {
  const errors: number[] = [];
  let sum = 0,
    square = 0,
    signal = 0;
  for (let i = 0; i < a.length; i++)
    if (i % 4 !== 3) {
      const e = Math.abs(a[i] - b[i]);
      if (!Number.isFinite(e)) throw Error("Nonfinite lighting result");
      errors.push(e);
      sum += e;
      square += e * e;
      signal += Math.abs(a[i]);
    }
  errors.sort((a, b) => a - b);
  return {
    meanAbsolute: sum / errors.length,
    rms: Math.sqrt(square / errors.length),
    maximum: errors.at(-1) ?? 0,
    p99: errors[Math.floor(errors.length * 0.99)],
    relativeMean: sum / Math.max(signal, 1e-12),
  };
}
function summarize(t: GpuFrameTiming[]) {
  const metric = (key: "sceneMs" | "gpuMs" | "indirectMs") => {
    const values = t.map((t) => t[key] ?? 0).sort((a, b) => a - b);
    return {
      median: values[Math.floor(values.length / 2)],
      p95: values[Math.floor((values.length - 1) * 0.95)],
    };
  };
  return {
    samples: t.length,
    scene: metric("sceneMs"),
    frame: metric("gpuMs"),
    indirect: metric("indirectMs"),
    timings: t,
  };
}
async function imageURL(blob: Blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let s = "";
  for (let i = 0; i < bytes.length; i += 8192) s += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return `data:image/png;base64,${btoa(s)}`;
}
export async function createSurfaceLightingFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing canvas");
  const errors: string[] = [],
    cache = new IndirectLightingCache();
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    resolutionScale: 1,
    antialiasing: "spatial",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const assertComplete = () => {
    if (errors.length || !renderer.completeness.complete)
      throw Error(JSON.stringify({ errors, completeness: renderer.completeness }));
  };
  const render = async (scene: EvaluatedScene, frames = 3) => {
    for (let i = 0; i < frames; i++) {
      renderer.render(scene);
      await renderer.flushGpuTimings();
    }
    assertComplete();
    return linearImage(renderer);
  };
  return {
    async capture(kind: "matte" | "metal" | "sealed", width = 640, height = 480) {
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      const fixture = indirectBoxFixture(
        kind === "sealed" ? { ceiling: true, front: true, occluder: false } : {},
      );
      // The shared historical fixture exposes the back faces of its room walls.
      // Author this room with inward geometric normals so its interior is a
      // valid static receiver and probe back-face rejection models the enclosure.
      for (const surface of fixture.surfaces)
        if (surface.id.includes("wall") || surface.id === "ceiling") {
          const indices = surface.mesh.indices.slice();
          for (let i = 0; i < indices.length; i += 3)
            [indices[i + 1], indices[i + 2]] = [indices[i + 2], indices[i + 1]];
          surface.mesh = { ...surface.mesh, indices, normals: surface.mesh.normals.map((v) => -v) };
        }
      if (kind === "metal")
        for (const s of fixture.surfaces) {
          s.material.metallic = 0.9;
          s.material.roughness = 0.28;
        }
      const scene: EvaluatedScene = {
        surfaces: fixture.surfaces,
        camera: fixture.camera,
        environment: evaluateEnvironment(),
        time: 0,
        mode: "beauty",
        grid: false,
      };
      scene.environment.sunDirection = fixture.lighting.sunDirection;
      scene.environment.sunIntensity = 2;
      scene.environment.ambient = 0.6;
      scene.environment.fogDensity = 0;
      scene.environment.cloudCover = 0;
      if (kind === "sealed") scene.camera = { position: [0, 1, 0.65], target: [0, 0.8, -0.6], fov: 65 };
      const options = {
        dimensions: [8, 6, 8] as Vec3,
        samples: 128,
        skySamples: 8,
        physicalSky: true,
      };
      const start = performance.now();
      cache.update(scene, options);
      const field = await cache.waitReady();
      const buildMs = performance.now() - start;
      if (!field.surfaceCache?.report.admitted) throw Error("No admitted surface cache tiles");
      const baseline = { ...field, key: `${field.key}/uncached-control`, surfaceCache: undefined };
      const cached = { ...field, key: `${field.key}/cached-control` };
      const timings: { baseline: GpuFrameTiming[]; cached: GpuFrameTiming[] } = { baseline: [], cached: [] };
      const images: Record<string, string> = {};
      await render(scene);
      images.off = await imageURL(await renderer.capture());
      let before: Float32Array | undefined, after: Float32Array | undefined;
      // Matched ABBA, warm-up/upload excluded, fixed lighting and view.
      for (const condition of ["baseline", "cached", "cached", "baseline"] as const) {
        scene.indirectLighting = condition === "baseline" ? baseline : cached;
        await render(scene, 4);
        renderer.drainGpuTimings();
        for (let i = 0; i < 12; i++) {
          renderer.render(scene);
          await renderer.flushGpuTimings();
          timings[condition].push(...renderer.drainGpuTimings());
        }
        assertComplete();
        const pixels = await linearImage(renderer);
        if (condition === "baseline") before = pixels;
        else after = pixels;
        if (!images[condition]) images[condition] = await imageURL(await renderer.capture());
      }
      const relightTimings: { baseline: GpuFrameTiming[]; cached: GpuFrameTiming[] } = {
        baseline: [],
        cached: [],
      };
      for (const condition of ["baseline", "cached", "cached", "baseline"] as const) {
        scene.indirectLighting = condition === "baseline" ? baseline : cached;
        await render(scene, 3);
        renderer.drainGpuTimings();
        for (let i = 0; i < 12; i++) {
          scene.environment.sunIntensity = 1 + (i % 2) * 0.5;
          renderer.render(scene);
          await renderer.flushGpuTimings();
          relightTimings[condition].push(...renderer.drainGpuTimings());
        }
      }
      scene.environment.sunIntensity = 2;
      if (!relightTimings.cached.some((s) => (s.indirectMs ?? 0) > 0))
        errors.push("Missing cache relight GPU timings");
      scene.indirectLighting = cached;
      scene.mode = "indirect-cache";
      const mask = await render(scene);
      let green = 0,
        magenta = 0;
      for (let i = 0; i < mask.length; i += 4) {
        if (mask[i] < 0.001 && mask[i + 1] > 0.999 && mask[i + 2] < 0.001) green++;
        if (mask[i] > 0.999 && mask[i + 1] < 0.001 && mask[i + 2] > 0.999) magenta++;
      }
      const coverage = {
        cachedPixels: green,
        fallbackPixels: magenta,
        fraction: green / Math.max(1, green + magenta),
      };
      if (green < 5000) errors.push("Surface cache did not cover enough visible pixels");
      images.coverage = await imageURL(await renderer.capture());
      scene.mode = "beauty";
      if (!before || !after) throw Error("Missing paired lighting capture");
      const beauty = comparison(before, after);
      // Initial prototype quality gates, scene-linear before exposure/tonemapping.
      if (beauty.meanAbsolute > 0.002 || beauty.p99 > 0.01 || beauty.maximum > 0.05)
        errors.push(`${kind}: beauty interpolation error exceeds prototype gate`);
      const variants: Record<string, ReturnType<typeof comparison>> = {};
      for (const mode of ["indirect-lighting", "beauty"] as const) {
        scene.mode = mode;
        scene.indirectLighting = baseline;
        const a = await render(scene);
        scene.indirectLighting = cached;
        const b = await render(scene);
        variants[mode] = comparison(a, b);
        if (kind === "sealed" && mode === "indirect-lighting") {
          let max = 0;
          for (let i = 0; i < b.length; i++) if (i % 4 !== 3) max = Math.max(max, b[i]);
          if (max > 0.0001) errors.push(`Closed-box cache leaked ${max}`);
        }
      }
      scene.camera = { ...scene.camera, position: [0.3, 1.25, kind === "sealed" ? 0.65 : 3.3] };
      scene.indirectLighting = baseline;
      const movedA = await render(scene);
      scene.indirectLighting = cached;
      const movedB = await render(scene);
      variants.movedCamera = comparison(movedA, movedB);
      scene.environment.sunIntensity *= 0.25;
      scene.environment.ambient *= 0.5;
      const reused = cache.update(scene, options) === field;
      scene.indirectLighting = baseline;
      const dimA = await render(scene);
      scene.indirectLighting = cached;
      const dimB = await render(scene);
      variants.relit = comparison(dimA, dimB);
      if (!reused) errors.push("Intensity relight rebuilt the surface cache");
      scene.mode = "indirect-lighting";
      const constant = {
        ...field,
        key: `${field.key}/constant`,
        transfer: undefined,
        reflections: field.reflections ? { data: field.reflections.data } : undefined,
      };
      scene.indirectLighting = { ...constant, key: `${constant.key}/baseline`, surfaceCache: undefined };
      const constantBaseline = await render(scene);
      scene.indirectLighting = constant;
      variants.constantSource = comparison(constantBaseline, await render(scene));
      scene.indirectLighting = cached;
      const local = await render(scene);
      scene.indirectLighting = undefined;
      await render(scene);
      scene.indirectLighting = cached;
      variants.restoredAfterDisable = comparison(local, await render(scene));
      if (variants.restoredAfterDisable.maximum > 0.0001)
        errors.push("Surface lighting was not restored after disabling the field");
      const origin: Vec3 = [16, 0, -32];
      const relative = (v: Vec3) => v.map((v, a) => v - origin[a]) as Vec3;
      const rebased: EvaluatedScene = {
        ...scene,
        origin,
        camera: {
          ...scene.camera,
          position: relative(scene.camera.position),
          target: relative(scene.camera.target),
        },
        surfaces: scene.surfaces.map((s) => {
          const matrix = s.matrix.slice();
          for (let a = 0; a < 3; a++) matrix[12 + a] -= origin[a];
          return { ...s, matrix };
        }),
      };
      const rebasedCached = await render(rebased);
      variants.rebased = comparison(local, rebasedCached);
      const rebasedBaseline = await render({ ...rebased, indirectLighting: baseline });
      variants.rebasedAgainstBaseline = comparison(rebasedBaseline, rebasedCached);
      const localBaseline = await render({ ...scene, indirectLighting: baseline });
      const originalRebase = comparison(localBaseline, rebasedBaseline);
      if (variants.rebased.maximum > originalRebase.maximum + 0.0001)
        errors.push("Surface cache adds origin-rebase error");
      for (const [name, v] of Object.entries(variants))
        if (name !== "rebased" && (v.meanAbsolute > 0.002 || v.p99 > 0.01 || v.maximum > 0.05))
          errors.push(`${kind}/${name}: interpolation error exceeds prototype gate`);
      return {
        images,
        report: {
          kind,
          viewport: [width, height],
          adapter: renderer.measurements.adapter,
          buildMs,
          field: field.report,
          surface: field.surfaceCache.report,
          bytes: {
            baseline: indirectLightingBytes(baseline),
            cached: indirectLightingBytes(cached),
            added: field.surfaceCache.data.byteLength,
          },
          beauty,
          coverage,
          originalRebase,
          variants,
          relightReused: reused,
          relighting: {
            order: "ABBA; alternating source intensity every frame",
            baseline: summarize(relightTimings.baseline),
            cached: summarize(relightTimings.cached),
          },
          performance: {
            order: "ABBA",
            baseline: summarize(timings.baseline),
            cached: summarize(timings.cached),
          },
          errors: [...errors],
        },
      };
    },
    dispose() {
      cache.dispose();
      renderer.dispose();
    },
  };
}
