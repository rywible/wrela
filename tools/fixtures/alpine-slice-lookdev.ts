import { type FrameMeasurements, type GpuFrameTiming, parseProject } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import { type AlpineSliceStudy, assessAlpineBudget, matchAlpineGpuTimings } from "../alpine-slice-study";

const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
export async function createAlpineSliceFixture(study: AlpineSliceStudy) {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing alpine review canvas");
  const errors: string[] = [];
  const host = new BrowserSceneHost(parseProject(study.project), {
    maxInstalledBytes: study.contract.installedBytes,
  });
  host.setViewportHeight(canvas.height);
  host.updateView(study.cameras[0].camera);
  const started = performance.now();
  let renderer: WebGPURenderer;
  try {
    await host.prepare(study.subject, undefined, "review");
    renderer = await WebGPURenderer.create(canvas, {
      quality: study.contract.quality,
      antialiasing: "temporal",
      pixelRatio: 1,
      onDiagnostic: (diagnostic) => {
        if (diagnostic.severity === "error") errors.push(diagnostic.message);
      },
    });
  } catch (error) {
    host.dispose();
    throw error;
  }
  const preparationMs = performance.now() - started;
  let disposed = false;
  let progress: {
    cameraIndex: number;
    phase: "idle" | "residency" | "warmup" | "settling" | "measurement" | "timestamps" | "complete";
    completed: number;
    total: number;
    elapsedMs: number;
  } = { cameraIndex: -1, phase: "idle", completed: 0, total: 0, elapsedMs: 0 };
  return {
    progress: () => ({ ...progress }),
    async frame(index: number) {
      const frame = study.cameras[index];
      if (!frame) throw Error("Unknown alpine review camera");
      const entered = performance.now();
      const checkpoint = (phase: typeof progress.phase, completed = 0, total = 0) => {
        if (disposed) throw Error("Alpine fixture disposed during frame measurement");
        progress = { cameraIndex: index, phase, completed, total, elapsedMs: performance.now() - entered };
      };
      checkpoint("residency");
      host.updateView(frame.camera);
      const readiness = await host.world?.prepare();
      if (readiness && !readiness.ready)
        throw Error(`Alpine world residency incomplete: ${readiness.missing.join(", ")}`);
      let peakInstalledBytes = host.resourceUsage.installedBytes;
      const render = () => {
        if (disposed) throw Error("Alpine fixture disposed during frame measurement");
        const start = performance.now();
        const scene = host.extract(frame.camera);
        scene.grid = false;
        const extracted = performance.now();
        renderer.render(scene);
        const totalMs = performance.now() - start;
        peakInstalledBytes = Math.max(peakInstalledBytes, host.resourceUsage.installedBytes);
        return {
          measurement: {
            ...structuredClone(renderer.measurements),
            cpuMs: totalMs,
            gpuMs: null,
          } as FrameMeasurements,
          extractionMs: extracted - start,
          rendererMs: renderer.measurements.cpuMs,
          totalMs,
          simulationTime: scene.time,
        };
      };
      checkpoint("warmup", 0, 120);
      for (let i = 0; i < 120; i++) {
        render();
        await nextFrame();
        checkpoint("warmup", i + 1, 120);
        if (renderer.completeness.complete && i >= 8) break;
      }
      if (!renderer.completeness.complete)
        throw Error(`Incomplete alpine scene: ${JSON.stringify(renderer.completeness.rejected)}`);
      // Discard earlier view results before collecting a frame-tagged sample window.
      checkpoint("timestamps");
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      const observations: ReturnType<typeof render>[] = [];
      const gpuTimings: GpuFrameTiming[] = [];
      for (let i = 0; i < 64; i++) {
        checkpoint(i < 16 ? "settling" : "measurement", i < 16 ? i : i - 16, i < 16 ? 16 : 48);
        // One persistent renderer accumulates the same static view; no readback resets temporal history.
        const observation = render();
        await nextFrame();
        gpuTimings.push(...renderer.drainGpuTimings());
        if (i >= 16) {
          if (
            !renderer.completeness.complete ||
            host.world?.metrics.pending ||
            host.world?.metrics.collisionPending
          )
            throw Error(`Alpine frame ${observation.measurement.frame} became incomplete during measurement`);
          observations.push(observation);
        }
        checkpoint(i < 16 ? "settling" : "measurement", i < 16 ? i + 1 : i - 15, i < 16 ? 16 : 48);
      }
      checkpoint("timestamps", 48, 48);
      await renderer.flushGpuTimings();
      gpuTimings.push(...renderer.drainGpuTimings());
      const measurements = matchAlpineGpuTimings(
        observations.map((observation) => observation.measurement),
        gpuTimings,
      );
      const measuredFrames = new Set(measurements.map((measurement) => measurement.frame));
      if (errors.length) throw Error(errors.join("\n"));
      checkpoint("complete", measurements.length, 48);
      return {
        id: frame.id,
        camera: frame.camera,
        preparationMs,
        viewToReviewMs: performance.now() - entered,
        measurements,
        gpuTimings: gpuTimings.filter((timing) => measuredFrames.has(timing.frame)),
        cpuTimings: observations.map(({ measurement, ...timing }) => ({
          frame: measurement.frame,
          ...timing,
        })),
        budget: assessAlpineBudget(study.profile, measurements, peakInstalledBytes),
        resources: host.resourceUsage,
        peakInstalledBytes,
        diagnostics: [...host.diagnostics, ...renderer.diagnostics],
        completeness: renderer.completeness,
        acceptance: "unreviewed",
      };
    },
    dispose() {
      disposed = true;
      renderer.dispose();
      host.dispose();
    },
  };
}
