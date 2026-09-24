import { coniferMeshes } from "@wrela/compiler/conifer-mesh";
import { type Camera, type EvaluatedScene, type Project, parseProject, type Vec3 } from "@wrela/model";
import { type RenderQuality, WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost, type IndirectLightingOptions } from "@wrela/runtime";
import { isolateLookdevSubtree } from "./lookdev-subtree";

export type LookdevFrame = {
  id: string;
  /** Isolate a source subtree for close inspection; never used by benchmarks. */
  sourcePrefix?: string;
  camera: Camera;
  time?: number;
  mode?: EvaluatedScene["mode"];
  sunDirection?: Vec3;
  motion?: { definition: string; id: string };
};
export type LookdevStudy = {
  project: Project;
  waterWarmup?: number;
  subject: string;
  stage?: string;
  frames: LookdevFrame[];
  indirectLighting?: IndirectLightingOptions;
  antialiasing?: "spatial" | "msaa" | "temporal";
  quality?: RenderQuality;
  cloudReconstruction?: "temporal" | "full";
  /** Bounded, untruncated conifer reference for source/proxy comparisons. */
  coniferReference?: { branch: string; representation: "filtered" | "triangles" | "legacy-filtered" };
};

/** Visual review uses the exact compiler, runtime and renderer shipped in Studio. */
export async function createLookdevFixture(input: LookdevStudy) {
  // The host solves GI from authored environment state during extraction. A later
  // render-packet-only override cannot update that solve or its cache identity.
  const overriddenSun = input.indirectLighting && input.frames.find((frame) => frame.sunDirection);
  if (overriddenSun)
    throw new Error(
      `Look development frame ${overriddenSun.id} combines indirect lighting with an unsupported sunDirection override; author the sun in the environment source so direct and indirect lighting agree`,
    );
  const project = parseProject(input.project);
  const coniferDoc = project.documents.find((d) => d.id === input.subject);
  const conifer =
    input.coniferReference && coniferDoc?.kind === "vegetation"
      ? coniferMeshes(
          coniferDoc,
          "review",
          false,
          input.coniferReference.representation,
          input.coniferReference.branch,
        )
      : undefined;
  if (input.coniferReference && (!conifer || conifer.truncated))
    throw Error("Bounded conifer reference is missing or truncated");
  const canvas = document.querySelector("canvas");
  if (!canvas) throw new Error("Look development requires a canvas");
  const diagnostics: string[] = [];
  const host = new BrowserSceneHost(project, {
    maxInstalledBytes: 192 * 1024 * 1024,
    indirectLighting: input.indirectLighting,
  });
  if (input.frames[0]) host.updateView(input.frames[0].camera);
  await host.prepare(input.subject, input.stage, "review");
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: input.antialiasing ?? "spatial",
    quality: input.quality,
    cloudReconstruction: input.cloudReconstruction,
    onDiagnostic: (d) => {
      if (d.severity === "error") diagnostics.push(d.message);
    },
  });
  return {
    async inspectWater() {
      const internals = renderer as unknown as {
        device: GPUDevice;
        objects: Map<
          string,
          {
            waterBuffer: GPUBuffer;
            uniform: GPUBuffer;
            waterSpectrum?: { texture: GPUTexture };
            waterState?: unknown;
            waterOrigin?: string;
            spectrumKey?: string;
          }
        >;
        realizedScene: EvaluatedScene;
      };
      const buffers = [];
      for (const [key, o] of internals.objects) {
        const read = internals.device.createBuffer({
          size: o.waterBuffer.size,
          usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
        });
        const encoder = internals.device.createCommandEncoder();
        encoder.copyBufferToBuffer(o.waterBuffer, 0, read, 0, o.waterBuffer.size);
        internals.device.queue.submit([encoder.finish()]);
        await read.mapAsync(GPUMapMode.READ);
        const header = Array.from(new Float32Array(read.getMappedRange()).slice(0, 96));
        read.unmap();
        read.destroy();
        const uniform = internals.device.createBuffer({
          size: o.uniform.size,
          usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
        });
        const cmd = internals.device.createCommandEncoder();
        cmd.copyBufferToBuffer(o.uniform, 0, uniform, 0, o.uniform.size);
        internals.device.queue.submit([cmd.finish()]);
        await uniform.mapAsync(GPUMapMode.READ);
        const object = Array.from(new Float32Array(uniform.getMappedRange()));
        uniform.unmap();
        uniform.destroy();
        let texture: number[] = [];
        if (o.waterSpectrum) {
          const tex = o.waterSpectrum.texture;
          const data = internals.device.createBuffer({
            size: tex.width * tex.height * 8,
            usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
          });
          const c = internals.device.createCommandEncoder();
          c.copyTextureToBuffer({ texture: tex }, { buffer: data, bytesPerRow: tex.width * 8 }, [
            tex.width,
            tex.height,
          ]);
          internals.device.queue.submit([c.finish()]);
          await data.mapAsync(GPUMapMode.READ);
          texture = Array.from(new Uint16Array(data.getMappedRange()).slice(0, 64));
          data.unmap();
          data.destroy();
        }
        buffers.push({ key, origin: o.waterOrigin, spectrumKey: o.spectrumKey, header, object, texture });
      }
      return {
        surfaces: internals.realizedScene.surfaces
          .filter((s) => s.water)
          .map((s) => ({
            id: s.id,
            water: s.water,
            effect: s.waterEffect,
            spectrum: s.waterState?.spectrum,
          })),
        buffers,
      };
    },
    disturbWater(id: string, x: number, z: number, radius = 1.4, strength = 2.5) {
      if (!host.runtime) throw new Error("No water runtime");
      host.runtime.disturbWater(id, x, z, radius, strength);
    },
    async benchmarkWater(frames = 120, omitWater = false) {
      const camera = input.frames[0].camera;
      host.updateView(camera);
      const samples: {
        cpuMs: number;
        fluidMs: number;
        gpuMs: number;
        uploadedBytes: number;
        waterMs?: number;
        spectrumMs?: number;
      }[] = [];
      await host.seek(5);
      renderer.drainGpuTimings();
      for (let frame = 0; frame < frames + 45; frame++) {
        const started = performance.now();
        host.advance(1 / 60, camera);
        const scene = host.extract(camera, "beauty");
        scene.grid = false;
        if (omitWater) scene.surfaces = scene.surfaces.filter((surface) => !surface.water);
        renderer.render(scene);
        const cpuMs = performance.now() - started;
        const fluidMs = [...(host.runtime?.waterBodies.values() ?? [])].reduce(
          (sum, body) => sum + (body.simulation?.lastStepMs ?? 0),
          0,
        );
        await renderer.flushGpuTimings();
        const timings = renderer.drainGpuTimings();
        const gpuMs = timings.at(-1)?.gpuMs;
        if (frame >= 45 && gpuMs !== undefined)
          samples.push({
            cpuMs,
            fluidMs,
            gpuMs,
            uploadedBytes: renderer.measurements.uploadedBytes ?? 0,
            waterMs: timings.at(-1)?.waterMs,
            spectrumMs: timings
              .at(-1)
              ?.intervals?.filter((p) => p.pass === "water-spectrum")
              .reduce((sum, p) => sum + p.endMs - p.startMs, 0),
          });
      }
      return {
        omitWater,
        samples,
        measurements: renderer.measurements,
        resources: host.resourceUsage,
        diagnostics: [...diagnostics],
      };
    },
    async benchmark(
      times: number[],
      camera: Camera = input.frames[0].camera,
      movePerFrame = 0,
      turnPerFrame = 0,
    ) {
      const samples: {
        time: number;
        gpuTimings: ReturnType<typeof renderer.drainGpuTimings>;
        measurements: typeof renderer.measurements;
      }[] = [];
      host.updateView(camera);
      if (host.world) await host.world.prepare();
      for (let index = 0; index < times.length; index++) {
        const angle = turnPerFrame * index;
        const dx = camera.target[0] - camera.position[0];
        const dz = camera.target[2] - camera.position[2];
        const view: Camera = movePerFrame
          ? {
              ...camera,
              position: [camera.position[0] + movePerFrame * index, camera.position[1], camera.position[2]],
              target: [camera.target[0] + movePerFrame * index, camera.target[1], camera.target[2]],
            }
          : turnPerFrame
            ? {
                ...camera,
                target: [
                  camera.position[0] + dx * Math.cos(angle) + dz * Math.sin(angle),
                  camera.target[1],
                  camera.position[2] + dz * Math.cos(angle) - dx * Math.sin(angle),
                ],
              }
            : camera;
        host.updateView(view);
        await host.seek(times[index]);
        const scene = host.extract(view, "beauty");
        scene.grid = false;
        renderer.render(scene);
        await renderer.flushGpuTimings();
        if (index >= 3)
          samples.push({
            time: times[index],
            gpuTimings: renderer.drainGpuTimings(),
            measurements: renderer.measurements,
          });
        else renderer.drainGpuTimings();
      }
      return { samples, diagnostics: [...diagnostics], adapter: renderer.measurements.adapter };
    },
    async frame(index: number) {
      const frameStartedAt = performance.now();
      const frame = input.frames[index];
      if (!frame) throw new RangeError("Unknown look development frame");
      host.updateView(frame.camera);
      if (host.world) await host.world.prepare();
      if (frame.motion) host.playMotion(frame.motion.definition, frame.motion.id, 0);
      const targetTime = frame.time ?? 0;
      if (input.waterWarmup && targetTime > 0) {
        const ticks = Math.min(Math.round(targetTime * 60), Math.round(input.waterWarmup * 60));
        await host.seek(targetTime - ticks / 60);
        for (let i = 0; i < ticks; i++) {
          host.advance(1 / 60, frame.camera);
          const warm = host.extract(frame.camera, frame.mode ?? "beauty");
          warm.grid = false;
          renderer.render(warm);
          if (i % 15 === 14) await renderer.flushGpuTimings();
        }
      }
      await host.seek(targetTime);
      let scene = host.extract(frame.camera, frame.mode ?? "beauty");
      if (input.indirectLighting) {
        await host.waitForIndirectLighting();
        scene = host.extract(frame.camera, frame.mode ?? "beauty");
      }
      scene.grid = false;
      if (frame.sunDirection) scene.environment.sunDirection = frame.sunDirection;
      if (frame.sourcePrefix) scene.surfaces = isolateLookdevSubtree(scene.surfaces, frame.sourcePrefix);
      if (conifer)
        scene.surfaces = scene.surfaces
          .filter((s) => s.source === input.subject)
          .filter(
            (s, i, all) =>
              !s.id.endsWith("-foliage") ||
              i === all.findIndex((candidate) => candidate.id.endsWith("-foliage")),
          )
          .map((s) => ({
            ...s,
            mesh: s.id.endsWith("-foliage") ? conifer.foliage : conifer.trunk,
            details: undefined,
            drawRange: undefined,
          }));
      // Allow bounded uploads to settle. A missing product is a failed capture.
      let captured: Blob | undefined;
      let attempts = 0;
      for (let attempt = 0; attempt < 6; attempt++) {
        attempts = attempt + 1;
        renderer.render(scene);
        captured = await renderer.capture();
        if (renderer.completeness.complete) break;
      }
      if (!captured || !renderer.completeness.complete || diagnostics.length)
        throw new Error(`Incomplete look development frame ${frame.id}: ${diagnostics.join("; ")}`);
      await renderer.flushGpuTimings();
      const gpuTimings = renderer.drainGpuTimings();
      const completeAt = performance.now();
      const image = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(captured);
      });
      return {
        image,
        id: frame.id,
        camera: frame.camera,
        time: frame.time ?? 0,
        mode: frame.mode ?? "beauty",
        motion: frame.motion,
        surfaces: scene.surfaces.length,
        triangles: scene.surfaces.reduce((n, s) => n + (s.drawRange?.count ?? s.mesh.indices.length) / 3, 0),
        complete: renderer.completeness.complete,
        timing: { frameToCompleteMs: completeAt - frameStartedAt, completeAt, uploadAttempts: attempts },
        resources: host.resourceUsage,
        indirectLighting: host.indirectLightingReport ?? null,
        measurements: renderer.measurements,
        gpuTimings,
        diagnostics: [...diagnostics],
        coniferReference: conifer
          ? { ...input.coniferReference, needles: conifer.leafCount, truncated: conifer.truncated }
          : undefined,
      };
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
