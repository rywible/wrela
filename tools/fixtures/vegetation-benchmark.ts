import { type Camera, type GpuFrameTiming, parseProject } from "@wrela/model";

import { type RendererOptions, WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import type { LookdevStudy } from "./lookdev";

export async function createVegetationBenchmark(
  study: LookdevStudy,
  frameIndex: number,
  options: RendererOptions = {},
  workload: { vegetation?: boolean } = {},
) {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing canvas");
  const frame = study.frames[frameIndex];
  if (!frame) throw Error("Missing benchmark camera");
  const errors: string[] = [];
  const host = new BrowserSceneHost(parseProject(study.project), { maxInstalledBytes: 192 * 1024 * 1024 });
  host.updateView(frame.camera);
  await host.prepare(study.subject, study.stage, "review");
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: study.antialiasing,
    ...options,
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const vegetationIds = new Set(
    study.project.documents
      .filter((document) => document.kind === "vegetation")
      .map((document) => document.id),
  );
  let vegetationEnabled = workload.vegetation !== false;
  const extract = (camera: Camera, diagnostic: boolean) => {
    const scene = host.extract(camera, diagnostic ? "albedo" : "beauty");
    if (!vegetationEnabled)
      scene.surfaces = scene.surfaces.filter((surface) => !vegetationIds.has(surface.source));
    scene.grid = false;
    return scene;
  };
  let tick = 0;
  let lastDiagnostic = false;
  const cameraAt = (moving: boolean): Camera => {
    if (!moving) return frame.camera;
    const angle = Math.sin((tick / 120) * 0.12) * 0.16;
    const [x, y, z] = frame.camera.position;
    const [tx, , tz] = frame.camera.target;
    return {
      ...frame.camera,
      position: [
        tx + (x - tx) * Math.cos(angle) - (z - tz) * Math.sin(angle),
        y,
        tz + (x - tx) * Math.sin(angle) + (z - tz) * Math.cos(angle),
      ],
    };
  };
  return {
    async reset(vegetation = true) {
      vegetationEnabled = vegetation;
      tick = 0;
      await host.seek(0);
    },
    async batch(count: number, moving = false, paced = false, diagnostic = false, queueDepth = 4) {
      if (!Number.isInteger(count) || count < 1 || count > 120 || ![1, 2, 4, 8].includes(queueDepth))
        throw Error("Invalid bounded benchmark batch");
      lastDiagnostic = diagnostic;
      const gpu: GpuFrameTiming[] = [];
      const frames: {
        frame: number;
        prepareMs: number;
        renderMs: number;
        complete: boolean;
        gpuBytes: number;
        triangles: number;
        draws: number;
        uploadedBytes: number;
      }[] = [];
      const started = performance.now();
      for (let i = 0; i < count; i++) {
        if (paced) await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
        const start = performance.now();
        const camera = cameraAt(moving);
        host.advance(1 / 120, camera);
        tick++;
        const scene = extract(camera, diagnostic);
        scene.grid = false;
        const prepared = performance.now();
        renderer.render(scene);
        const m = renderer.measurements;
        frames.push({
          frame: m.frame,
          prepareMs: prepared - start,
          renderMs: m.cpuMs,
          complete: renderer.completeness.complete,
          gpuBytes: m.gpuBytes,
          triangles: m.triangles,
          draws: m.drawCalls,
          uploadedBytes: m.uploadedBytes ?? 0,
        });
        if ((i + 1) % queueDepth === 0 || i === count - 1) {
          await renderer.flushGpuTimings();
          gpu.push(...renderer.drainGpuTimings());
        }
      }
      if (errors.length) throw Error(errors.join("\n"));
      return {
        frames,
        gpu,
        elapsedMs: performance.now() - started,
        measurements: renderer.measurements,
        errors: [...errors],
        dropped: renderer.measurements.gpuTimingDroppedFrames ?? 0,
      };
    },
    async capture() {
      // Fixed pose/camera allows exact material-kernel comparisons across different run speeds.
      host.updateView(frame.camera);
      await host.seek(frame.time ?? 0);
      const scene = extract(frame.camera, lastDiagnostic);
      scene.grid = false;
      renderer.render(scene);
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      const blob = await renderer.capture();
      return await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
