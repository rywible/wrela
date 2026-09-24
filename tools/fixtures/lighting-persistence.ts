import { referenceProject } from "@wrela/examples";
import type { EvaluatedScene, Vec3 } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost, RadianceLightingCache, SkyVisibilityCache } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { lightingRoomFixture } from "./lighting-room";
import { lightingRouteScene } from "./lighting-route-scene";

export async function lightingPersistenceFixture() {
  const errors: string[] = [];
  window.addEventListener("error", (e) => errors.push(e.message));
  window.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));
  const canvas = document.createElement("canvas");
  canvas.style.cssText = "position:fixed;inset:0;width:960px;height:720px";
  document.body.append(canvas);
  const renderer = await WebGPURenderer.create(canvas, {
    quality: "balanced",
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const hash = async (array: Float32Array) =>
    Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", array.buffer as ArrayBuffer)), (v) =>
      v.toString(16).padStart(2, "0"),
    ).join("");
  return {
    async capture(kind: "furnished" | "winter" | "route", travel = false) {
      let host: BrowserSceneHost | undefined;
      const sky = new SkyVisibilityCache(),
        radiance = new RadianceLightingCache();
      let scene: EvaluatedScene;
      if (kind === "winter") {
        const project = referenceProject();
        host = new BrowserSceneHost(project);
        await host.prepare(project.entry, "neutral-stage", "interactive");
        const camera = { position: [8, 4.2, 11] as Vec3, target: [0.8, 1, 0.5] as Vec3, fov: 48 };
        host.updateView(camera);
        await host.world?.prepare();
        scene = host.extract(camera, "beauty", false);
      } else if (kind === "route") {
        scene = lightingRouteScene(false).scene;
        scene.camera = { position: [2.2, 1.65, 10], target: [2.2, 1.65, 4], fov: 65 };
      } else scene = lightingRoomFixture(true);
      try {
        sky.apply(scene);
        // Isolate radiance readiness from the independently prepared sky cache.
        const started = performance.now();
        radiance.apply(scene);
        const product = await radiance.waitReady();
        const readyMs = performance.now() - started;
        await sky.waitReady();
        const apply = () => {
          radiance.strip(scene);
          sky.apply(scene);
          radiance.apply(scene);
        };
        apply();
        while (radiance.report?.deferredReceivers) {
          await new Promise<void>((r) => setTimeout(r, 0));
          apply();
        }
        await radiance.waitForStorage();
        for (let i = 0; i < 8; i++) {
          renderer.render(scene);
          await renderer.flushGpuTimings();
        }
        await renderer.waitForPipelineCompilation();
        for (let i = 0; i < 12; i++) {
          renderer.render(scene);
          await renderer.flushGpuTimings();
        }
        if (errors.length || !renderer.completeness.complete)
          throw Error(JSON.stringify({ errors, completeness: renderer.completeness }));
        const gpu = renderer as unknown as { device: GPUDevice; targets: { sceneColor: GPUTexture } };
        const image = await linearImage({
          device: gpu.device,
          sceneColor: gpu.targets.sceneColor,
        } as unknown as WebGPURenderer);
        const bytes = new Uint8Array(await (await renderer.capture()).arrayBuffer());
        let binary = "";
        for (let at = 0; at < bytes.length; at += 8192)
          binary += String.fromCharCode(...bytes.subarray(at, at + 8192));
        const initialBuilds = radiance.builds,
          initialStorageHits = radiance.storageHits;
        let traversal: unknown;
        if (travel) {
          const position = [...scene.camera.position] as Vec3,
            target = [...scene.camera.target] as Vec3;
          const beforeBuilds = radiance.builds,
            beforePrefetch = radiance.prefetchBuilds;
          const changes: { distance: number; elapsedMs: number }[] = [];
          const intervals: number[] = [],
            applyTimes: number[] = [];
          let previous = performance.now(),
            distance = 0,
            lastField = scene.radianceLighting?.key,
            incompleteFrames = 0;
          let staleRegionFrames = 0,
            staleRegionMs = 0;
          const started = previous;
          const path: Vec3[] = [
            [2.2, 1.65, 10],
            [2.2, 1.65, -2.3],
            [0.15, 1.65, -2.3],
            [0.15, 1.65, -13.1],
          ];
          const lengths = path.slice(1).map((p, i) => Math.hypot(...p.map((v, a) => v - path[i][a])));
          const totalDistance = kind === "route" ? lengths.reduce((a, b) => a + b) : 22;
          const routePoint = (at: number): Vec3 => {
            for (let i = 0; i < lengths.length; i++) {
              if (at <= lengths[i] || i === lengths.length - 1)
                return path[i].map((v, a) => v + (path[i + 1][a] - v) * Math.min(1, at / lengths[i])) as Vec3;
              at -= lengths[i];
            }
            return path[path.length - 1];
          };
          let peakRetainedBytes = radiance.byteLength;
          while (distance < totalDistance) {
            await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
            const now = performance.now(),
              dt = now - previous;
            previous = now;
            intervals.push(dt);
            distance = Math.min(totalDistance, distance + Math.min(50, dt) * 0.004);
            scene.camera.position =
              kind === "route" ? routePoint(distance) : [position[0] + distance, position[1], position[2]];
            scene.camera.target =
              kind === "route"
                ? routePoint(Math.min(totalDistance, distance + 1))
                : [target[0] + distance, target[1], target[2]];
            if (kind === "route" && distance === totalDistance) scene.camera.target[2] -= 1;
            const applied = performance.now();
            apply();
            applyTimes.push(performance.now() - applied);
            if (radiance.preparingRegion) {
              staleRegionFrames++;
              staleRegionMs += dt;
            }
            peakRetainedBytes = Math.max(peakRetainedBytes, radiance.byteLength);
            if (scene.radianceLighting?.key !== lastField) {
              changes.push({ distance, elapsedMs: performance.now() - started });
              lastField = scene.radianceLighting?.key;
            }
            renderer.render(scene);
            if (!renderer.completeness.complete) incompleteFrames++;
            if (performance.now() - started > 20000) throw Error("Traversal exceeded its wall-time budget");
          }
          const summary = (values: number[]) => {
            const sorted = values.slice().sort((a, b) => a - b);
            return {
              median: sorted[Math.floor(sorted.length * 0.5)],
              p95: sorted[Math.floor(sorted.length * 0.95)],
              max: sorted.at(-1),
            };
          };
          traversal = {
            residentGeometryOnly: true,
            path: kind === "route" ? "exterior-room-cave" : "winter-straight",
            peakRetainedBytes,
            retainedRegions: radiance.retainedRegions,
            distance,
            durationMs: performance.now() - started,
            frames: intervals.length,
            intervalMs: summary(intervals),
            applyMs: summary(applyTimes),
            foregroundBuilds: radiance.builds - beforeBuilds,
            prefetchBuilds: radiance.prefetchBuilds - beforePrefetch,
            prefetchHits: radiance.prefetchHits,
            prefetchWaitMs: radiance.prefetchWaitMs,
            changes,
            incompleteFrames,
            staleRegionFrames,
            staleRegionMs,
            regionStillPreparing: radiance.preparingRegion,
            lightingReadyThroughout: staleRegionFrames === 0 && !radiance.preparingRegion,
            pendingReceiverAdmissions: radiance.report?.deferredReceivers,
          };
          if (errors.length) throw Error(errors.join(";"));
        }
        return {
          png: btoa(binary),
          report: {
            kind,
            traversal,
            readyMs,
            initialBuilds,
            initialStorageHits,
            builds: radiance.builds,
            storageHits: radiance.storageHits,
            storageWrites: radiance.storageWrites,
            storageLoadMs: radiance.storageLoadMs,
            storageWriteMs: radiance.storageWriteMs,
            storageError: radiance.storageError,
            retainedBytes: radiance.byteLength,
            radiance: radiance.report,
            transferHash: await hash(product.field.transfer),
            imageHash: await hash(image),
            buildMs: product.field.report.buildMs,
            activeMs: product.field.report.activeMs,
            maxSliceMs: product.field.report.maxSliceMs,
            errors,
          },
        };
      } finally {
        radiance.dispose();
        sky.dispose();
        host?.dispose();
      }
    },
    dispose() {
      renderer.dispose();
    },
  };
}
