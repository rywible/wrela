import {
  compileIndirectGeometry,
  indirectReferencePFM,
  renderIndirectReference,
  sampleIndirectField,
  traceIndirectRay,
} from "@wrela/compiler";
import type { EvaluatedScene, GpuFrameTiming, Vec3 } from "@wrela/model";

import { cameraRay, WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectAlpineFixture, indirectBoxFixture } from "./indirect-scenes";

function base64(bytes: Uint8Array): string {
  let value = "";
  for (let i = 0; i < bytes.length; i += 8192) value += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(value);
}
async function imageURL(blob: Blob) {
  return `data:image/png;base64,${base64(new Uint8Array(await blob.arrayBuffer()))}`;
}
function referencePNG(width: number, height: number, data: Float32Array): string {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw Error("Reference image context unavailable");
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    for (let c = 0; c < 3; c++) {
      const x = Math.max(0, data[i * 3 + c]);
      pixels[i * 4 + c] = Math.round(
        255 * Math.min(1, (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)) ** (1 / 2.2),
      );
    }
    pixels[i * 4 + 3] = 255;
  }
  context.putImageData(new ImageData(pixels, width, height), 0, 0);
  return canvas.toDataURL("image/png");
}
export async function createGILookdevFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing GI canvas");
  const errors: string[] = [],
    cache = new IndirectLightingCache();
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    resolutionScale: 1,
    antialiasing: "spatial",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const setSize = (width: number, height: number) => {
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
  };
  const assertComplete = () => {
    if (errors.length || !renderer.completeness.complete)
      throw Error(`Incomplete GI frame: ${errors.join("; ")}`);
  };
  async function visibilityCost(scene: EvaluatedScene) {
    const field = scene.indirectLighting;
    if (!field) throw Error("Cannot measure missing indirect field");
    const unguarded = { ...field, key: `${field.key}/unguarded-control`, visibility: undefined };
    const bvh = {
      ...field,
      key: `${field.key}/full-bvh-control`,
      visibility: field.visibility ? { ...field.visibility, cells: undefined } : undefined,
    };
    const bvhTimings: GpuFrameTiming[] = [];
    const guardedTimings: GpuFrameTiming[] = [],
      unguardedTimings: GpuFrameTiming[] = [];
    setSize(640, 480);
    scene.mode = "indirect-lighting";
    // ABCCBA keeps geometry, probes, viewport, shader and lighting unchanged. Uploads
    // and two settling frames are excluded; only the numerical visibility query varies.
    let compiledImage: Float32Array | undefined;
    let maximumCompiledBvhError = 0;
    for (const condition of ["compiled", "bvh", "unguarded", "unguarded", "bvh", "compiled"]) {
      scene.indirectLighting = condition === "compiled" ? field : condition === "bvh" ? bvh : unguarded;
      for (let i = 0; i < 2; i++) {
        renderer.render(scene);
        await renderer.flushGpuTimings();
      }
      renderer.drainGpuTimings();
      for (let i = 0; i < 8; i++) {
        renderer.render(scene);
        await renderer.flushGpuTimings();
        (condition === "compiled"
          ? guardedTimings
          : condition === "bvh"
            ? bvhTimings
            : unguardedTimings
        ).push(...renderer.drainGpuTimings());
      }
      assertComplete();
      if (condition === "compiled") compiledImage = await linearImage(renderer);
      if (condition === "bvh" && compiledImage) {
        const image = await linearImage(renderer);
        for (let i = 0; i < image.length; i++)
          maximumCompiledBvhError = Math.max(maximumCompiledBvhError, Math.abs(image[i] - compiledImage[i]));
      }
    }
    if (maximumCompiledBvhError > 0.002)
      throw Error(`Compiled visibility changed lighting: ${maximumCompiledBvhError}`);
    scene.indirectLighting = field;
    const summarize = (timings: GpuFrameTiming[]) => {
      const sceneMs = timings.map((t) => t.sceneMs).sort((a, b) => a - b);
      return {
        samples: sceneMs.length,
        sceneMedianMs: sceneMs.length ? sceneMs[Math.floor(sceneMs.length / 2)] : null,
        sceneP95Ms: sceneMs.length ? sceneMs[Math.floor((sceneMs.length - 1) * 0.95)] : null,
        timings,
      };
    };
    const fullBvh = summarize(bvhTimings);
    const guarded = summarize(guardedTimings),
      unguardedResult = summarize(unguardedTimings);
    return {
      viewport: [640, 480],
      mode: "indirect-lighting",
      order: "ABCCBA",
      maximumCompiledBvhError,
      fullBvh,
      savedSceneMedianMs:
        fullBvh.sceneMedianMs !== null && guarded.sceneMedianMs !== null
          ? fullBvh.sceneMedianMs - guarded.sceneMedianMs
          : null,
      guarded,
      unguarded: unguardedResult,
      addedSceneMedianMs:
        guarded.sceneMedianMs !== null && unguardedResult.sceneMedianMs !== null
          ? guarded.sceneMedianMs - unguardedResult.sceneMedianMs
          : null,
      scope:
        "Short paired GPU scene-pass observations on this fixture; excludes field build/upload. Not a cross-hardware benchmark.",
    };
  }
  async function capture(name: "box" | "alpine") {
    const fixture = name === "box" ? indirectBoxFixture() : indirectAlpineFixture();
    const scene: EvaluatedScene = {
      surfaces: fixture.surfaces,
      camera: fixture.camera,
      environment: evaluateEnvironment(),
      time: 0,
      mode: "beauty",
      grid: false,
    };
    scene.environment.sunDirection = fixture.lighting.sunDirection;
    scene.environment.sunColor = fixture.lighting.sunRadiance;
    scene.environment.sunIntensity = 1;
    scene.environment.ambient = 0.6;
    scene.environment.fogDensity = 0;
    scene.environment.exposure = 1;
    const start = performance.now();
    cache.update(scene, {
      surfaceCache: false,
      dimensions: name === "box" ? [12, 8, 12] : [16, 6, 16],
      lighting: fixture.lighting,
      bounces: 1,
      samples: 192,
      skySamples: 8,
    });
    const field = await cache.waitReady();
    const buildMs = performance.now() - start;
    setSize(640, 480);
    renderer.render(scene);
    const off = await imageURL(await renderer.capture());
    assertComplete();
    scene.indirectLighting = field;
    renderer.render(scene);
    const on = await imageURL(await renderer.capture());
    assertComplete();
    // Compare an isolated diffuse-indirect pass. It excludes tone mapping, GGX,
    // analytic atmosphere, exposure and direct light from the numerical test.
    const width = 96,
      height = 72;
    setSize(width, height);
    scene.mode = "indirect-lighting";
    renderer.render(scene);
    const gpu = await linearImage(renderer);
    assertComplete();
    const geometry = compileIndirectGeometry(fixture.surfaces);
    const referenceStart = performance.now();
    const reference = renderIndirectReference(geometry, fixture.camera, fixture.lighting, {
      width,
      height,
      samples: 256,
      skySamples: 16,
      seed: 17,
      mode: "indirect",
      jitter: false,
    });
    const referenceMs = performance.now() - referenceStart;
    const cpu = new Float32Array(width * height * 3),
      gpuRGB = new Float32Array(width * height * 3),
      mask = new Uint8Array(width * height);
    let squared = 0,
      absolute = 0,
      maximum = 0,
      parity = 0,
      samples = 0,
      referenceSum = 0;
    for (let y = 0; y < height; y++)
      for (let x = 0; x < width; x++) {
        const pixel = y * width + x,
          ray = cameraRay(
            fixture.camera,
            (2 * (x + 0.5)) / width - 1,
            1 - (2 * (y + 0.5)) / height,
            width / height,
          );
        const hit = traceIndirectRay(geometry, ray.origin, ray.direction);
        if (!hit) continue;
        mask[pixel] = 1;
        const cached = sampleIndirectField(field, hit.position, hit.normal);
        for (let c = 0; c < 3; c++) {
          cpu[pixel * 3 + c] = hit.albedo[c] * cached[c];
          gpuRGB[pixel * 3 + c] = gpu[pixel * 4 + c];
          const difference = Math.abs(gpuRGB[pixel * 3 + c] - reference.data[pixel * 3 + c]);
          squared += difference * difference;
          absolute += difference;
          maximum = Math.max(maximum, difference);
          parity = Math.max(parity, Math.abs(gpuRGB[pixel * 3 + c] - cpu[pixel * 3 + c]));
          referenceSum += reference.data[pixel * 3 + c];
          samples++;
        }
      }
    if (!samples || !Number.isFinite(squared)) throw Error("GI comparison produced no finite surfaces");
    if (parity > 0.002) throw Error(`CPU/GPU indirect interpolation disagrees by ${parity} linear radiance`);
    await renderer.flushGpuTimings();
    const measurements = renderer.measurements,
      gpuTimings = renderer.drainGpuTimings(),
      completeness = renderer.completeness;
    const origin: Vec3 = [16, 0, -32];
    const rebase = (v: Vec3) => v.map((x, i) => x - origin[i]) as Vec3;
    const rebased: EvaluatedScene = {
      ...scene,
      origin,
      camera: {
        ...scene.camera,
        position: rebase(scene.camera.position),
        target: rebase(scene.camera.target),
      },
      surfaces: scene.surfaces.map((s) => {
        const matrix = s.matrix.slice();
        for (let i = 0; i < 3; i++) matrix[12 + i] -= origin[i];
        return { ...s, matrix };
      }),
    };
    renderer.render(rebased);
    const rebasedGpu = await linearImage(renderer);
    assertComplete();
    let rebaseMaximum = 0;
    for (let pixel = 0; pixel < mask.length; pixel++)
      if (mask[pixel])
        for (let c = 0; c < 3; c++)
          rebaseMaximum = Math.max(rebaseMaximum, Math.abs(rebasedGpu[pixel * 4 + c] - gpu[pixel * 4 + c]));
    if (rebaseMaximum > 0.002)
      throw Error(`Rebased indirect lighting changed by ${rebaseMaximum} linear radiance`);
    const visibilityPerformance = await visibilityCost(scene);
    return {
      name,
      off,
      on,
      cacheImage: referencePNG(width, height, gpuRGB),
      referenceImage: referencePNG(width, height, reference.data),
      referencePFM: base64(indirectReferencePFM(reference)),
      cachePFM: base64(indirectReferencePFM({ width, height, data: gpuRGB })),
      metadata: {
        source: "constant-sky-and-directional-sun",
        bounces: 1,
        buildMs,
        referenceMs,
        width,
        height,
        referenceSamples: 256,
        referenceSkySamples: 16,
        field: {
          key: field.key,
          bytes: cache.byteLength,
          dimensions: field.dimensions,
          report: field.report,
        },
        comparison: {
          rms: Math.sqrt(squared / samples),
          meanAbsolute: absolute / samples,
          maximum,
          referenceMean: referenceSum / samples,
          cpuGpuMaximum: parity,
          rebaseMaximum,
          rebaseOrigin: origin,
          channelSamples: samples,
        },
        visibilityPerformance,
        measurements,
        gpuTimings,
        completeness,
        limits:
          "Low-order SH and coarse probes approximate the irradiance field; bounded receiver-to-probe BVH queries reject occluded interpolation. Error is measured, not certified. Reference shares the query mesh and constant lighting source but samples independently.",
      },
    };
  }
  async function check() {
    const fixture = indirectBoxFixture({ ceiling: true, front: true, occluder: false });
    const scene: EvaluatedScene = {
      surfaces: fixture.surfaces,
      camera: { position: [0, 0.8, 0.5], target: [0, 0.6, -0.6], fov: 60 },
      environment: evaluateEnvironment(),
      time: 0,
      mode: "indirect-lighting",
      grid: false,
    };
    cache.update(scene, {
      surfaceCache: false,
      dimensions: [12, 8, 12],
      lighting: fixture.lighting,
      samples: 128,
      skySamples: 8,
    });
    scene.indirectLighting = await cache.waitReady();
    setSize(96, 72);
    renderer.render(scene);
    const dark = await linearImage(renderer);
    assertComplete();
    let max = 0,
      mean = 0;
    for (let i = 0; i < dark.length; i++)
      if (i % 4 !== 3) {
        max = Math.max(max, dark[i]);
        mean += dark[i] / ((dark.length / 4) * 3);
      }
    // A fully closed opaque enclosure has zero source visibility. This is a
    // targeted behavioral gate, not a general interpolation error certificate.
    if (max > 0.0001) throw Error(`Closed-box indirect lighting leaked ${max} linear radiance`);
    return { closedBox: { maximum: max, mean, expected: 0 }, errors };
  }
  return {
    capture,
    check,
    dispose() {
      cache.dispose();
      renderer.dispose();
    },
  };
}
