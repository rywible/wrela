import type { EvaluatedScene } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, IndirectLightingCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectBoxFixture } from "./indirect-scenes";

/** Research fixture: a sealed, nonemissive room must not receive exterior light. */
export async function createLightingAuditFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing canvas");
  canvas.style.width = "640px";
  canvas.style.height = "480px";
  const errors: string[] = [];
  const cache = new IndirectLightingCache();
  const createRenderer = (ablation: "none" | "reflection" | "direct" = "none") =>
    WebGPURenderer.create(canvas, {
      pixelRatio: 1,
      quality: "balanced",
      antialiasing: "spatial",
      lightingAblation: ablation,
      onDiagnostic: (d) => {
        if (d.severity === "error") errors.push(d.message);
      },
    });
  let renderer = await createRenderer();
  const linearCaptures = new Map<string, Float32Array>();
  const capture = async (name: string, scene: EvaluatedScene) => {
    for (let i = 0; i < 8; i++) {
      renderer.render(scene);
      await renderer.flushGpuTimings();
    }
    if (!renderer.completeness.complete || errors.length)
      throw Error(JSON.stringify({ errors, completeness: renderer.completeness }));
    const pixels = await linearImage(renderer);
    linearCaptures.set(name, pixels);
    let maximumRGB = 0,
      meanLuminance = 0;
    for (let i = 0; i < pixels.length; i += 4) {
      if (![pixels[i], pixels[i + 1], pixels[i + 2]].every(Number.isFinite))
        throw Error("Nonfinite radiance");
      maximumRGB = Math.max(maximumRGB, pixels[i], pixels[i + 1], pixels[i + 2]);
      meanLuminance += pixels[i] * 0.2126 + pixels[i + 1] * 0.7152 + pixels[i + 2] * 0.0722;
    }
    // Central metal face, away from wall edges and shadow-map bias seams.
    let metalPatchLuminance = 0;
    for (let y = 310; y < 410; y++) {
      for (let x = 240; x < 400; x++) {
        const i = (y * 640 + x) * 4;
        metalPatchLuminance += pixels[i] * 0.2126 + pixels[i + 1] * 0.7152 + pixels[i + 2] * 0.0722;
      }
    }
    const blob = await renderer.capture();
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let binary = "";
    for (let i = 0; i < bytes.length; i += 8192)
      binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
    return {
      name,
      image: btoa(binary),
      maximumRGB,
      meanLuminance: meanLuminance / (pixels.length / 4),
      centralPatchMeanLuminance: metalPatchLuminance / 16000,
      adapter: renderer.measurements.adapter,
      completeness: renderer.completeness.complete,
      outputResolution: renderer.measurements.outputResolution,
    };
  };
  return {
    async capture() {
      const f = indirectBoxFixture({ ceiling: true, front: true });
      const environment = evaluateEnvironment();
      environment.cloudCover = 0;
      environment.fogDensity = 0;
      environment.ambient = 1;
      environment.exposure = 1;
      environment.pointLights = [];
      const scene: EvaluatedScene = {
        surfaces: f.surfaces,
        camera: { position: [0, 1.05, 0.85], target: [0, 0.75, -0.5], fov: 70 },
        environment,
        time: 0,
        mode: "beauty",
        grid: false,
      };
      const block = scene.surfaces.at(-1);
      if (!block) throw Error("Missing fixture block");
      block.material = { ...block.material, metallic: 1, roughness: 0.22, color: [0.8, 0.8, 0.8] };
      const captures = [await capture("sealed-default", scene)];
      cache.update(scene, { dimensions: [8, 6, 8], samples: 128, skySamples: 8 });
      scene.indirectLighting = await cache.waitReady();
      const report = scene.indirectLighting.report;
      captures.push(await capture("sealed-gi", scene));
      scene.mode = "indirect-lighting";
      captures.push(await capture("sealed-diffuse-only", scene));
      scene.mode = "beauty";
      renderer.dispose();
      renderer = await createRenderer("reflection");
      captures.push(await capture("sealed-gi-no-sky-reflection", scene));
      renderer.dispose();
      renderer = await createRenderer("direct");
      captures.push(await capture("sealed-gi-no-direct", scene));
      renderer.dispose();
      renderer = await createRenderer();
      const open = indirectBoxFixture();
      scene.surfaces = open.surfaces;
      scene.camera = open.camera;
      scene.indirectLighting = undefined;
      captures.push(await capture("open-default", scene));
      cache.update(scene, { dimensions: [8, 6, 8], samples: 128, skySamples: 8 });
      scene.indirectLighting = await cache.waitReady();
      captures.push(await capture("open-gi", scene));
      const openBlock = scene.surfaces.at(-1);
      if (!openBlock) throw Error("Missing open-room block");
      openBlock.material = { ...openBlock.material, metallic: 1, roughness: 0.35, color: [0.8, 0.8, 0.8] };
      cache.update(scene, { dimensions: [8, 6, 8], samples: 128, skySamples: 8 });
      const metalField = await cache.waitReady();
      scene.indirectLighting = {
        ...metalField,
        key: `${metalField.key}/sky-reflection-control`,
        reflections: undefined,
      };
      captures.push(await capture("open-metal-sky", scene));
      scene.indirectLighting = metalField;
      captures.push(await capture("open-metal-local", scene));
      const sealed = captures.find((c) => c.name === "sealed-gi");
      const fallback = captures.find((c) => c.name === "sealed-default");
      const openGI = captures.find((c) => c.name === "open-gi");
      const sealedPixels = linearCaptures.get("sealed-gi");
      const noDirectPixels = linearCaptures.get("sealed-gi-no-direct");
      if (!sealed || !fallback || !openGI || !sealedPixels || !noDirectPixels)
        throw Error("Missing lighting regression capture");
      let maximumDirectLeak = 0;
      for (let i = 0; i < sealedPixels.length; i++)
        if (i % 4 !== 3)
          maximumDirectLeak = Math.max(maximumDirectLeak, Math.abs(sealedPixels[i] - noDirectPixels[i]));
      const checks = {
        reflectionLeakFraction: sealed.centralPatchMeanLuminance / fallback.centralPatchMeanLuminance,
        maximumDirectLeak,
        openRoomLuminance: openGI.centralPatchMeanLuminance,
      };
      if (checks.reflectionLeakFraction > 0.01) errors.push("Sealed metal receives exterior reflection");
      if (maximumDirectLeak > 0.0001) errors.push("Direct sunlight leaks through the sealed enclosure");
      if (checks.openRoomLuminance < 0.05) errors.push("Occlusion also removed valid open-room light");
      return {
        captures,
        report,
        checks,
        errors,
        interpretation:
          "Production lighting with diagnostic ablations and enclosure regression gates. Images use identical exposure. Linear RGB statistics include the full frame. Aerial perspective remains active in beauty views.",
      };
    },
    dispose() {
      cache.dispose();
      renderer.dispose();
    },
  };
}
