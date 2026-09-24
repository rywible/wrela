import { BrowserCompiler } from "@wrela/compiler/client";
import { referenceProject } from "@wrela/examples";
import type { Camera, Diagnostic } from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";

type Profile = "low" | "balanced" | "high";
const target = window as unknown as {
  ready?: boolean;
  failure?: string;
  fixture?: Awaited<ReturnType<typeof setup>>;
};
async function setup() {
  const startup = performance.now(),
    project = referenceProject();
  const compiler = new BrowserCompiler("/compile-worker.js");
  const host = new BrowserSceneHost(project, {
    compile: compiler.compile,
    generateTerrain: compiler.generateTerrain,
  });
  await host.prepare(project.entry, "neutral-stage", "interactive");
  const canvas = document.querySelector("canvas");
  if (!canvas) throw new Error("Performance canvas is missing");
  const profile = (new URL(location.href).searchParams.get("profile") ?? "balanced") as Profile;
  const renderer = await WebGPURenderer.create(canvas, { pixelRatio: 1, quality: profile });
  const camera: Camera = { position: [18, 11, 23], target: [0, 1, 0], fov: 50 };
  renderer.render(host.evaluate(0, camera));
  let tick = 0;
  return {
    startupMs: performance.now() - startup,
    profile,
    async prepareDensity() {
      const world = project.documents.find((document) => document.kind === "world");
      const actor = world?.instances.find((instance) => instance.definition === "polar-bunny");
      if (!world || !actor) throw new Error("Density scenario has no actor");
      for (let index = 1; index <= 12; index++)
        world.instances.push({
          ...structuredClone(actor),
          id: `density-${index}`,
          position: [(index % 4) * 3 - 6, 0, Math.floor(index / 4) * 3 - 6],
        });
      await host.setProject(project);
      await host.prepare(project.entry, "neutral-stage", "interactive");
      tick = 0;
      camera.position = [18, 11, 23];
      camera.target = [0, 1, 0];
    },
    async run(frames: number, travel: boolean) {
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      const firstFrame = renderer.measurements.frame + 1;
      const cpu: number[] = [],
        pacing: number[] = [];
      const gpu: ReturnType<WebGPURenderer["drainGpuTimings"]> = [];
      const residency: {
        resident: number;
        cacheBytes: number;
        liveBytes: number;
        cpuArtifactsBytes: number;
        gpuBytes: number;
        pending: boolean;
      }[] = [];
      const incomplete: { frame: number; rejected: unknown[]; uploading: string[] }[] = [];
      let previous: number | undefined;
      for (let index = 0; index < frames; index++) {
        await new Promise(requestAnimationFrame);
        const now = performance.now();
        if (previous !== undefined) pacing.push(now - previous);
        previous = now;
        if (travel) {
          camera.position[0] += 0.6;
          camera.target[0] += 0.6;
          camera.position[2] += 0.15;
          camera.target[2] += 0.15;
        }
        const start = performance.now();
        renderer.render(host.evaluate(++tick / 60, camera));
        cpu.push(performance.now() - start);
        gpu.push(...renderer.drainGpuTimings());
        if (!renderer.completeness.complete)
          incomplete.push({
            frame: renderer.measurements.frame,
            rejected: structuredClone(renderer.completeness.rejected),
            uploading: [...renderer.completeness.uploading],
          });
        if (host.world)
          residency.push({
            ...host.world.metrics,
            cpuArtifactsBytes: host.world.metrics.liveBytes + host.resourceUsage.liveBytes,
            gpuBytes: renderer.measurements.gpuBytes,
          });
      }
      await renderer.flushGpuTimings();
      gpu.push(...renderer.drainGpuTimings());
      const lastFrame = renderer.measurements.frame;
      const timing = gpu.filter((sample) => sample.frame >= firstFrame && sample.frame <= lastFrame);
      if (new Set(timing.map((sample) => sample.frame)).size !== timing.length)
        throw new Error("Duplicate GPU timing frame");
      if (host.world) {
        const ready = await host.world.prepare();
        if (!ready.ready) throw new Error(`Streaming incomplete: ${ready.missing.join(", ")}`);
      }
      renderer.render(host.evaluate(tick / 60, camera));
      await renderer.capture();
      return {
        cpu,
        gpu: timing.map((sample) => sample.gpuMs),
        gpuFrames: timing,
        pacing,
        residency,
        frameRange: { firstFrame, lastFrame },
        incomplete,
        completeness: renderer.completeness,
        measurements: renderer.measurements,
        runtime: { tick: host.runtime?.clock.tick, blockedReason: host.runtime?.blockedReason },
        diagnostics: [...renderer.diagnostics, ...host.diagnostics] as Diagnostic[],
      };
    },
  };
}
void setup()
  .then((fixture) => {
    target.fixture = fixture;
    target.ready = true;
  })
  .catch((error) => {
    target.failure = String(error);
  });
