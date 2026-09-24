import { WebGPURenderer } from "@wrela/render-webgpu";
import { RadianceLightingCache, SkyVisibilityCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { LIGHTING_ROUTE_VIEWS, lightingRouteScene } from "./lighting-route-scene";

const comparisonImages = new Map<string, { previous: boolean; pixels: Float32Array }>();

export async function lightingRouteFixture(
  Renderer: typeof WebGPURenderer = WebGPURenderer,
  Runtime: typeof RadianceLightingCache = RadianceLightingCache,
  options: {
    width?: number;
    height?: number;
    temporal?: boolean;
    indirect?: boolean;
    noReflection?: boolean;
    steady?: boolean;
  } = {},
) {
  const errors: string[] = [];
  window.addEventListener("error", (e) => errors.push(e.message));
  window.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));
  const canvas = document.createElement("canvas");
  const width = options.width ?? 960,
    height = options.height ?? 720;
  canvas.style.cssText = `position:fixed;inset:0;width:${width}px;height:${height}px`;
  document.body.append(canvas);
  const renderer = await Renderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    antialiasing: options.temporal ? "temporal" : "spatial",
    lightingAblation: options.noReflection ? "reflection" : "none",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const route = lightingRouteScene();
  const radiance = new Runtime(null),
    sky = new SkyVisibilityCache();
  const shots = new Map<string, Float32Array>();
  const summary = (values: number[]) => {
    const sorted = values.slice().sort((a, b) => a - b);
    return {
      median: sorted[Math.floor(sorted.length * 0.5)],
      p95: sorted[Math.floor(sorted.length * 0.95)],
      max: sorted.at(-1),
    };
  };
  return {
    async capture(
      view: string,
      night = false,
      door: "open" | "closed" | "fixed" = "open",
      emissionMultiplier = 1,
    ) {
      const scene = route.scene;
      scene.mode = options.indirect ? "indirect-lighting" : "beauty";
      radiance.strip(scene);
      route.setDoor(door === "open" ? 1.5 : 0, door !== "fixed");
      scene.camera = structuredClone(LIGHTING_ROUTE_VIEWS[view]);
      scene.environment.sunIntensity = night ? 0 : 2.5;
      scene.environment.sunDirection = night ? [0, -1, 0.2] : [-0.263117, 0.394676, 0.877058];
      const lights = scene.environment.pointLights,
        emitter = scene.surfaces.find((s) => s.id === "pendant")?.material.emission;
      if (!lights || !emitter) throw Error("Missing route lights");
      lights[0].intensity = night ? 4 : 1;
      lights[1].intensity = night ? 2 : 0.4;
      emitter.intensity = (night ? 2 : 0.5) * emissionMultiplier;
      const preparationStarted = performance.now();
      sky.apply(scene);
      radiance.apply(scene);
      await Promise.all([sky.waitReady(), radiance.waitReady()]);
      radiance.strip(scene);
      sky.apply(scene);
      radiance.apply(scene);
      while (radiance.report?.deferredReceivers) {
        await new Promise<void>((r) => setTimeout(r, 0));
        radiance.strip(scene);
        sky.apply(scene);
        radiance.apply(scene);
      }
      const readinessMs = performance.now() - preparationStarted;
      const warmFrames = options.steady ? 120 : 12,
        measuredFrames = options.steady ? 120 : 24;
      for (let i = 0; i < warmFrames; i++) {
        renderer.render(scene);
        await renderer.flushGpuTimings();
      }
      await renderer.waitForPipelineCompilation();
      renderer.drainGpuTimings();
      const cpu: number[] = [],
        gpu: number[] = [],
        shadow: number[] = [],
        sceneTimes: number[] = [];
      for (let i = 0; i < measuredFrames; i++) {
        const started = performance.now();
        renderer.render(scene);
        cpu.push(performance.now() - started);
        await renderer.flushGpuTimings();
        const timings = renderer.drainGpuTimings();
        gpu.push(...timings.map((t) => t.gpuMs));
        shadow.push(...timings.map((t) => t.shadowMs));
        sceneTimes.push(...timings.map((t) => t.sceneMs));
      }
      if (errors.length || !renderer.completeness.complete)
        throw Error(JSON.stringify({ errors, completeness: renderer.completeness }));
      const internal = renderer as unknown as { device: GPUDevice; targets: { sceneColor: GPUTexture } };
      const pixels = await linearImage({
        device: internal.device,
        sceneColor: internal.targets.sceneColor,
      } as unknown as WebGPURenderer);
      const shot = `${options.indirect ? "indirect-" : ""}${options.noReflection ? "no-reflection-" : ""}${night ? "night" : "day"}-${view}-${door}${emissionMultiplier === 1 ? "" : `-emission-${emissionMultiplier}`}`;
      let imageDifference: unknown;
      const previous = Renderer !== WebGPURenderer || Runtime !== RadianceLightingCache;
      const counterpart = comparisonImages.get(shot);
      if (counterpart && counterpart.previous !== previous) {
        const previousImage = previous ? pixels : counterpart.pixels;
        const currentImage = previous ? counterpart.pixels : pixels;
        let error = 0,
          signal = 0,
          max = 0,
          changed = 0;
        for (let i = 0; i < pixels.length; i++)
          if (i % 4 !== 3) {
            const difference = Math.abs(currentImage[i] - previousImage[i]);
            error += difference;
            max = Math.max(max, difference);
            signal += Math.abs(previousImage[i]);
            if (difference > 0) changed++;
          }
        imageDifference = { normalizedL1: error / Math.max(1e-9, signal), max, changedChannels: changed };
        comparisonImages.delete(shot);
      } else comparisonImages.set(shot, { previous, pixels });
      shots.set(shot, pixels);
      const closedShot = shots.get(
        `${options.indirect ? "indirect-" : ""}${options.noReflection ? "no-reflection-" : ""}${night ? "night" : "day"}-${view}-closed`,
      );
      let mobilityDifference: unknown;
      if (door === "fixed" && closedShot) {
        let error = 0,
          signal = 0,
          actual = 0;
        // The image is identical geometry. Only admission to indirect transport
        // changes. This is a static-transport control, not path-traced truth.
        for (let i = 0; i < pixels.length; i += 4)
          for (let c = 0; c < 3; c++) {
            error += Math.abs(pixels[i + c] - closedShot[i + c]);
            signal += pixels[i + c];
            actual += closedShot[i + c];
          }
        mobilityDifference = {
          normalizedL1: error / Math.max(1e-9, signal),
          fixedMean: signal / (pixels.length * 0.75),
          dynamicMean: actual / (pixels.length * 0.75),
        };
      }
      const png = new Uint8Array(await (await renderer.capture()).arrayBuffer());
      const measurements = { ...renderer.measurements };
      let binary = "";
      for (let i = 0; i < png.length; i += 8192) binary += String.fromCharCode(...png.subarray(i, i + 8192));
      scene.mode = "identity";
      for (let i = 0; i < 4; i++) {
        renderer.render(scene);
        await renderer.flushGpuTimings();
      }
      const identities = await linearImage({
        device: internal.device,
        sceneColor: internal.targets.sceneColor,
      } as unknown as WebGPURenderer);
      const identityHash = Array.from(
        new Uint8Array(await crypto.subtle.digest("SHA-256", identities.buffer as ArrayBuffer)),
        (v) => v.toString(16).padStart(2, "0"),
      ).join("");
      const identityBytes = new Uint8Array(await (await renderer.capture()).arrayBuffer());
      let identityBinary = "";
      for (let i = 0; i < identityBytes.length; i += 8192)
        identityBinary += String.fromCharCode(...identityBytes.subarray(i, i + 8192));
      scene.mode = options.indirect ? "indirect-lighting" : "beauty";
      return {
        png: btoa(binary),
        identityPng: btoa(identityBinary),
        report: {
          shot,
          warmFrames,
          measuredFrames,
          viewport: { width, height, antialiasing: options.temporal ? "temporal" : "spatial" },
          identityHash,
          imageDifference,
          cpuMs: summary(cpu),
          gpuMs: summary(gpu),
          shadowMs: summary(shadow),
          sceneMs: summary(sceneTimes),
          mobilityDifference,
          radiance: radiance.report,
          radianceBuilds: radiance.builds,
          emissionScale: scene.radianceLighting?.emissionScale ?? 1,
          readinessMs,
          compiledEmissionReceivers: (scene.radianceLighting?.receiverEmission?.length ?? 0) / 3,
          measurements,
          errors,
        },
      };
    },
    dispose() {
      radiance.dispose();
      sky.dispose();
      renderer.dispose();
      canvas.remove();
    },
  };
}
