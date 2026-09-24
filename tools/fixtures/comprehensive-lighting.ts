import { referenceProject } from "@wrela/examples";
import { type EvaluatedScene, type GpuFrameTiming, normalize, type Vec3 } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import {
  BrowserSceneHost,
  evaluateEnvironment,
  RadianceLightingCache,
  SkyVisibilityCache,
} from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectAlpineFixture, indirectBoxFixture, quad } from "./indirect-scenes";
import { lightingRoomFixture } from "./lighting-room";

async function pixels(renderer: WebGPURenderer) {
  const r = renderer as unknown as {
    device: GPUDevice;
    sceneColor?: GPUTexture;
    targets?: { sceneColor?: GPUTexture };
  };
  return linearImage({
    device: r.device,
    sceneColor: r.sceneColor ?? r.targets?.sceneColor,
  } as unknown as WebGPURenderer);
}
async function png(renderer: WebGPURenderer) {
  const bytes = new Uint8Array(await (await renderer.capture()).arrayBuffer());
  let binary = "";
  for (let i = 0; i < bytes.length; i += 8192) binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(binary);
}
export async function comprehensiveLightingFixture(
  Previous: typeof WebGPURenderer,
  previousRuntime?: {
    RadianceLightingCache: typeof RadianceLightingCache;
    SkyVisibilityCache: typeof SkyVisibilityCache;
  },
  ablation?: "indirect" | "reflection" | "shadow" | "direct",
) {
  const errors: string[] = [],
    renderers: WebGPURenderer[] = [];
  for (const [index, Type] of [Previous, WebGPURenderer].entries()) {
    const canvas = document.createElement("canvas");
    canvas.style.position = "fixed";
    canvas.style.inset = "0";
    canvas.style.width = "960px";
    canvas.style.height = "720px";
    document.body.append(canvas);
    renderers.push(
      await Type.create(canvas, {
        quality: "balanced",
        pixelRatio: 1,
        antialiasing: "spatial",
        lightingAblation: index === 1 ? ablation : undefined,
        onDiagnostic: (d) => {
          if (d.severity === "error") errors.push(d.message);
        },
      }),
    );
  }
  const render = async (r: WebGPURenderer, s: EvaluatedScene, frames = 8) => {
    for (let i = 0; i < frames; i++) {
      r.render(s);
      await r.flushGpuTimings();
    }
    if (errors.length || !r.completeness.complete)
      throw Error(JSON.stringify({ errors, completeness: r.completeness }));
  };
  return {
    async capture(kind: string) {
      const sky = new SkyVisibilityCache(),
        radiance = new RadianceLightingCache();
      const previousSky = previousRuntime ? new previousRuntime.SkyVisibilityCache() : undefined;
      const previousRadiance = previousRuntime ? new previousRuntime.RadianceLightingCache() : undefined;
      let host: BrowserSceneHost | undefined;
      let scene: EvaluatedScene;
      if (kind === "winter") {
        host = new BrowserSceneHost(referenceProject());
        await host.prepare(referenceProject().entry, "neutral-stage", "interactive");
        const camera = { position: [8, 4.2, 11] as Vec3, target: [0.8, 1, 0.5] as Vec3, fov: 48 };
        host.updateView(camera);
        await host.world?.prepare();
        scene = host.extract(camera, "beauty");
      } else if (kind === "furnished" || kind === "furnished-night") {
        scene = lightingRoomFixture(kind === "furnished-night");
      } else {
        const enclosed = ["sealed", "interior", "emissive", "eight-lights", "cave", "windowed"].includes(
          kind,
        );
        const f =
          kind === "outdoor" || kind === "night" || kind === "dusk"
            ? indirectAlpineFixture()
            : indirectBoxFixture({
                ceiling: enclosed || kind === "entrance",
                front: enclosed,
                occluder: !["sealed", "cave"].includes(kind),
              });
        scene = { ...f, environment: evaluateEnvironment(), mode: "beauty", grid: false, time: 0 };
        scene.environment.sunDirection = f.lighting.sunDirection;
        if (enclosed) scene.camera = { position: [0, 1.2, 0.85], target: [0, 0.8, -0.7], fov: 78 };
        if (kind === "windowed") {
          scene.surfaces = scene.surfaces.filter((s) => s.id !== "front-wall");
          const panel = (id: string, x: number, y: number, X: number, Y: number) =>
            quad(
              id,
              [
                [x, y, 1],
                [X, y, 1],
                [X, Y, 1],
                [x, Y, 1],
              ],
              [0.7, 0.7, 0.7],
            );
          scene.surfaces.push(
            panel("window-left", -1, 0, -0.4, 2),
            panel("window-right", 0.4, 0, 1, 2),
            panel("window-bottom", -0.4, 0, 0.4, 0.7),
            panel("window-top", -0.4, 1.5, 0.4, 2),
          );
          scene.environment.sunDirection = normalize([0.1, 0.2, 1]);
        }
        if (kind === "dusk") scene.environment.sunDirection = normalize([1, 0.025, 0.1]);
        if (kind === "cave") {
          for (const s of scene.surfaces) for (const a of [0, 5, 10]) s.matrix[a] *= 15;
          scene.camera = { position: [0, 15, 10], target: [0, 8, -14], fov: 75 };
        }
        if (kind === "night") {
          scene.environment.sunDirection = [0, -1, 0];
          scene.environment.sunIntensity = 0;
          scene.environment.exposure = 2;
        }
        if (["interior", "night", "eight-lights"].includes(kind)) {
          const count = kind === "eight-lights" ? 8 : 2;
          scene.environment.pointLights = Array.from({ length: count }, (_, i) => ({
            position: [Math.cos(i * 2.4) * 0.65, 1.45, Math.sin(i * 2.4) * 0.6] as Vec3,
            color: (i % 2 ? [0.2, 0.45, 1] : [1, 0.38, 0.08]) as Vec3,
            intensity: count > 2 ? 0.8 : 3,
            range: 6,
          }));
        }
        if (kind === "emissive")
          scene.surfaces.find((s) => s.id === "ceiling")!.material.emission = {
            color: [1, 0.25, 0.04],
            intensity: 2,
          };
        const object = scene.surfaces.find((s) => s.id === "neutral-block" || s.id === "warm-rock");
        if (object) {
          object.material.metallic = 0.8;
          object.material.roughness = 0.35;
        }
        if (kind === "moving" && object) {
          object.lightingMobility = "dynamic";
          object.matrix[12] = -0.45;
        }
      }
      scene.grid = false;
      const originalSurfaces = scene.surfaces;
      try {
        if (host) {
          await host.waitForSkyVisibility();
          await host.waitForRadianceLighting();
          scene = host.extract(scene.camera, "beauty");
        } else {
          sky.apply(scene);
          await sky.waitReady();
          sky.apply(scene);
          radiance.apply(scene);
          await radiance.waitReady();
          radiance.apply(scene);
        }
        for (
          let frame = 0;
          frame < 240 && (host?.radianceLightingReport ?? radiance.report)?.deferredReceivers;
          frame++
        ) {
          await new Promise<void>((r) => setTimeout(r, 0));
          if (host) host.applyIndirectLighting(scene);
          else {
            radiance.strip(scene);
            sky.apply(scene);
            radiance.apply(scene);
          }
        }
        if ((host?.radianceLightingReport ?? radiance.report)?.deferredReceivers)
          throw Error("Receiver admission did not settle");
        const lit = scene;
        // Same authored scene and full-resident sky occlusion. The old renderer
        // consumes original carriers, so timings include the new refinement cost.
        const control = { ...scene, surfaces: originalSurfaces, radianceLighting: undefined };
        const controlSky = previousSky ?? sky;
        controlSky.apply(control);
        await controlSky.waitReady();
        controlSky.apply(control);
        if (previousRadiance) {
          previousRadiance.apply(control);
          await previousRadiance.waitReady();
          previousRadiance.apply(control);
          for (let frame = 0; frame < 240 && previousRadiance.report?.deferredReceivers; frame++) {
            await new Promise<void>((r) => setTimeout(r, 0));
            previousRadiance.strip(control);
            controlSky.apply(control);
            previousRadiance.apply(control);
          }
        }
        const images: Record<string, string> = {},
          timings: Record<string, GpuFrameTiming[]> = { before: [], after: [] },
          means: Record<string, number> = {};
        const preparationMs: number[] = [];
        for (const [i, s] of [control, lit].entries()) {
          const r = renderers[i],
            started = performance.now();
          await render(r, s, 2);
          if (r.waitForPipelineCompilation) await r.waitForPipelineCompilation();
          else {
            // Frozen historical renderers predate the public preparation barrier.
            const entries = (
              r as unknown as {
                specializedMain: { entries: Map<string, { started: boolean; value?: unknown }> };
              }
            ).specializedMain.entries;
            while ([...entries.values()].some((e) => e.started && !e.value)) {
              if (errors.length || performance.now() - started > 30000)
                throw Error("Lighting pipeline preparation failed");
              await new Promise<void>((resolve) => setTimeout(resolve, 10));
            }
          }
          preparationMs.push(performance.now() - started);
          await render(r, s, 12);
        }
        for (const name of ["before", "after", "after", "before"] as const) {
          const r = renderers[name === "before" ? 0 : 1],
            s = name === "before" ? control : lit;
          await render(r, s, 12);
          r.drainGpuTimings();
          for (let i = 0; i < 8; i++) {
            for (let j = 0; j < 4; j++) r.render(s);
            await r.flushGpuTimings();
            timings[name].push(...r.drainGpuTimings());
          }
          const data = await pixels(r);
          let sum = 0;
          for (let i = 0; i < data.length; i++)
            if (i % 4 !== 3) {
              if (!Number.isFinite(data[i])) throw Error("Nonfinite radiance output");
              sum += data[i];
            }
          means[name] = sum / (data.length * 0.75);
          images[name] = await png(r);
        }
        if (kind === "sealed" && means.after > 0.005)
          throw Error("Closed room leaks excessive exterior light");
        if (kind === "cave" && means.after > 0.01)
          throw Error("Large closed cave receives excessive exterior light");
        if (!previousRuntime && kind === "emissive" && means.after < means.before * 10)
          throw Error("Emitted light did not illuminate the room");
        const builds = host?.radianceLightingBuilds ?? radiance.builds;
        const originalColors = scene.environment.pointLights?.map((l) => l.color);
        if (scene.environment.pointLights?.length) scene.environment.pointLights[0].color = [0.05, 1, 0.1];
        scene.environment.sunIntensity *= 0.7;
        if (host) host.applyIndirectLighting(scene);
        else {
          radiance.strip(scene);
          sky.apply(scene);
          radiance.apply(scene);
        }
        if ((host?.radianceLightingBuilds ?? radiance.builds) !== builds)
          throw Error("Radiometric edit recompiled transport");
        if (kind === "interior") {
          await render(renderers[1], scene);
          images.relit = await png(renderers[1]);
          scene.mode = "indirect-lighting";
          await render(renderers[1], scene);
          const indirect = await pixels(renderers[1]);
          if (ablation !== "indirect" && !indirect.some((v, i) => i % 4 !== 3 && v > 0.01))
            throw Error("Default indirect diagnostic is empty");
          images.indirect = await png(renderers[1]);
          scene.mode = "indirect-cache";
          await render(renderers[1], scene);
          images.coverage = await png(renderers[1]);
          scene.mode = "beauty";
        }
        if (kind === "moving") {
          const object = scene.surfaces.find((s) => s.id === "neutral-block")!;
          object.matrix[12] = 0.45;
          radiance.strip(scene);
          sky.apply(scene);
          radiance.apply(scene);
          if (radiance.builds !== builds) throw Error("Moving receiver recompiled static transport");
          await render(renderers[1], scene);
          images.moved = await png(renderers[1]);
        }
        let traversal: unknown;
        if (kind === "winter" && host) {
          const saved = scene.camera,
            originalKey = scene.radianceLighting?.key;
          const initialBuilds = host.radianceLightingBuilds;
          scene.camera = {
            ...saved,
            position: [saved.position[0] + 32, saved.position[1], saved.position[2]],
            target: [saved.target[0] + 32, saved.target[1], saved.target[2]],
          };
          const start = performance.now();
          host.applyIndirectLighting(scene);
          await host.waitForRadianceLighting();
          host.applyIndirectLighting(scene);
          const neighboringBuild = { ...host.radianceLightingReport, excluded: undefined };
          const outwardMs = performance.now() - start,
            beforeReturnBuilds = host.radianceLightingBuilds;
          scene.camera = saved;
          const returnStart = performance.now();
          host.applyIndirectLighting(scene);
          traversal = {
            initialBuilds,
            neighboringBuild,
            outwardMs,
            returnMs: performance.now() - returnStart,
            returnedWithoutBuild: host.radianceLightingBuilds === beforeReturnBuilds,
            restoredField: scene.radianceLighting?.key === originalKey,
            residentGeometryOnly: true,
          };
          if (
            host.radianceLightingBuilds !== beforeReturnBuilds ||
            scene.radianceLighting?.key !== originalKey
          )
            throw Error("Returning to a resident lighting region rebuilt transport");
        }
        return {
          images,
          report: {
            kind,
            means,
            timings,
            radiance: host?.radianceLightingReport ?? radiance.report,
            sky: host?.skyVisibilityReport ?? sky.report,
            adapter: renderers[1].measurements.adapter,
            errors,
            originalColors,
            traversal,
            comparison: previousRuntime ? "previous-automatic-lighting" : "sky-and-direct",
            preparationMs,
            previousRadiance: previousRadiance?.report,
            ablation,
          },
        };
      } finally {
        host?.dispose();
        radiance.dispose();
        sky.dispose();
        previousSky?.dispose();
        previousRadiance?.dispose();
      }
    },
    dispose() {
      for (const r of renderers) r.dispose();
    },
  };
}
