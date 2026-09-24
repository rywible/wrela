import { BrowserCompiler } from "@wrela/compiler/client";
import { referenceProject } from "@wrela/examples";
import { type EvaluatedScene, type GpuFrameTiming, identityMatrix, transformMatrix } from "@wrela/model";
import { type RendererOptions, WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import {
  type AcceptanceScenario,
  type AcceptanceVariant,
  assertCompleteIdentities,
  compareLinear,
  PRIMITIVE_CAMERAS,
  trajectoryFrame,
  VARIANTS,
} from "./manifest";
import { applyWorkload, type SceneWorkload, workloadInventory } from "./scalability";
import { validateWaterReference } from "./water-reference";
import { WINTER_ACCEPTANCE_RECIPE, winterAcceptanceProject } from "./winter-scene";

/** Read the actual scene-linear HDR target through its existing texture binding, without changing renderer allocation. */
export async function linearImage(renderer: WebGPURenderer) {
  const { device, sceneColor } = renderer as unknown as { device: GPUDevice; sceneColor: GPUTexture };
  const size = sceneColor.width * sceneColor.height * 16;
  const output = device.createBuffer({ size, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
  const readback = device.createBuffer({ size, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
  try {
    const module = device.createShaderModule({
      code: `
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> pixels: array<vec4f>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id: vec3u) {
 let size = textureDimensions(source);
 if (id.x >= size.x || id.y >= size.y) { return; }
 pixels[id.y * size.x + id.x] = textureLoad(source, vec2i(id.xy), 0);
}`,
    });
    const pipeline = await device.createComputePipelineAsync({
      layout: "auto",
      compute: { module, entryPoint: "main" },
    });
    const group = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: sceneColor.createView() },
        { binding: 1, resource: { buffer: output } },
      ],
    });
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, group);
    pass.dispatchWorkgroups(Math.ceil(sceneColor.width / 8), Math.ceil(sceneColor.height / 8));
    pass.end();
    encoder.copyBufferToBuffer(output, 0, readback, 0, size);
    device.queue.submit([encoder.finish()]);
    await readback.mapAsync(GPUMapMode.READ);
    const pixels = new Float32Array(readback.getMappedRange().slice(0));
    readback.unmap();
    // This also rejects nonfinite output in direct captures.
    compareLinear(pixels, pixels);
    return pixels;
  } finally {
    output.destroy();
    readback.destroy();
  }
}
function clonePacket(scene: EvaluatedScene): EvaluatedScene {
  return {
    ...scene,
    camera: structuredClone(scene.camera),
    environment: structuredClone(scene.environment),
    surfaces: scene.surfaces.map((surface) => ({
      ...surface,
      matrix: surface.matrix.slice(),
      material: structuredClone(surface.material),
      ...(surface.skin ? { skin: { ...surface.skin, matrices: surface.skin.matrices.slice() } } : {}),
    })),
  };
}
export async function createAcceptanceFixture() {
  const errors: string[] = [];
  window.addEventListener("error", (event) => errors.push(event.message));
  window.addEventListener("unhandledrejection", (event) => errors.push(String(event.reason)));
  const compiler = new BrowserCompiler("/compile-worker.js");
  let project = referenceProject();
  let scenarioName: AcceptanceScenario = "winter-valley";
  let host: BrowserSceneHost | undefined;
  let renderer: WebGPURenderer | undefined;
  let scenes: EvaluatedScene[] = [];
  let baseScenes: EvaluatedScene[] = [];
  let materialReference: Float32Array | undefined;
  let variant: AcceptanceVariant = "production";
  const references = new Map<string, Float32Array>();
  const previousImages = new Map<string, Float32Array>();
  const previousReferences = new Map<string, Float32Array>();
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Acceptance canvas is missing");
  const profile = new URL(location.href).searchParams.get("profile") === "low" ? "low" : "balanced";
  return {
    errors,
    async prepare(scenario: AcceptanceScenario, frames = 91) {
      host?.dispose();
      scenarioName = scenario;
      project = scenario === "winter-valley" ? winterAcceptanceProject() : referenceProject();
      if (scenario === "lighting" || scenario === "visibility") {
        const sphere = project.documents.find((document) => document.id === "river-stone");
        if (!sphere || sphere.kind !== "object") throw Error("Sphere control source missing");
        sphere.field.nodes = [
          {
            ...sphere.field.nodes[0],
            kind: "sphere",
            position: [0, 1, 0],
            size: [1, 1, 1],
            radius: 1,
            rotation: [0, 0, 0],
          },
        ];
        sphere.field.bounds = { min: [-1.1, -0.1, -1.1], max: [1.1, 2.1, 1.1] };
      }
      references.clear();
      previousImages.clear();
      previousReferences.clear();
      host = new BrowserSceneHost(project, {
        materialCache: true,
        compile: compiler.compile,
        generateTerrain: compiler.generateTerrain,
      });
      const start = performance.now();
      await host.prepare(
        scenario === "winter-valley"
          ? project.entry
          : scenario === "primitive" || scenario === "lighting" || scenario === "visibility"
            ? "river-stone"
            : "coastal-waves",
        "neutral-stage",
        "interactive",
      );
      scenes = [];
      for (let frame = 0; frame < frames; frame++) {
        const sample = trajectoryFrame(frame, frames);
        if (scenario === "primitive")
          sample.camera = structuredClone(
            PRIMITIVE_CAMERAS[
              Math.min(PRIMITIVE_CAMERAS.length - 1, Math.floor((frame * PRIMITIVE_CAMERAS.length) / frames))
            ].camera,
          );
        if (scenario === "water")
          sample.camera = { position: [8 - (frame / frames) * 4, 4, 9], target: [0, -1.3, 0], fov: 45 };
        host.evaluate(sample.time, sample.camera);
        if (host.world) {
          const readiness = await host.world.prepare();
          if (!readiness.ready)
            throw Error(`Acceptance streaming incomplete: ${readiness.missing.join(", ")}`);
        }
        const scene = clonePacket(host.extract(sample.camera));
        scene.environment.sunDirection = sample.sunDirection;
        scene.time = sample.time;
        if (scenario === "water") {
          const roughness = [0.06, 0.1, 0.18, 0.5][Math.min(3, Math.floor((frame * 4) / frames))];
          scene.surfaces = scene.surfaces.map((surface) =>
            surface.water
              ? {
                  ...surface,
                  material: { ...surface.material, roughness },
                  water: { ...surface.water, roughness },
                }
              : surface,
          );
        }
        if (scenario === "lighting" || scenario === "visibility") {
          const sphere = scene.surfaces.find((surface) => surface.source === "river-stone");
          if (!sphere) throw Error("Compiled sphere control missing");
          const ground = {
            ...sphere,
            id: "acceptance-ground",
            source: "acceptance-ground",
            renderProducts: undefined,
            selectedRenderProduct: undefined,
            matrix: identityMatrix(),
            material: {
              ...sphere.material,
              color: [0.5, 0.5, 0.5] as [number, number, number],
              secondary: [0.5, 0.5, 0.5] as [number, number, number],
              pattern: 0,
              normalStrength: 0,
            },
            mesh: {
              positions: new Float32Array([-100, 0, -100, 100, 0, -100, 100, 0, 100, -100, 0, 100]),
              normals: new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0]),
              indices: new Uint32Array([0, 2, 1, 0, 3, 2]),
              bounds: {
                min: [-100, 0, -100] as [number, number, number],
                max: [100, 0, 100] as [number, number, number],
              },
            },
          };
          scene.surfaces = [ground, { ...sphere, matrix: identityMatrix() }];
          scene.camera = { position: [5, 3, 7], target: [0, 0.5, 0], fov: 50 };
          scene.environment.sunDirection = [0.4 + (frame / frames) * 0.3, 0.15 + (frame / frames) * 0.4, 0.7];
          if (scenario === "visibility") {
            scene.camera = { position: [(0.1 * frame) / frames, 2, 10], target: [0, 2, 0], fov: 50 };
            scene.environment.sunDirection = [0, 0.1, 1];
            scene.surfaces[1] = { ...sphere, matrix: transformMatrix([0, 1, 3]) };
            scene.surfaces.push(
              ...Array.from({ length: 24 }, (_, index) => ({
                ...sphere,
                id: `hidden-sphere-${index}`,
                instanceId: `hidden-sphere-${index}`,
                matrix: transformMatrix(
                  [(index % 4) * 0.1 - 0.15, 1.9, -1 - Math.floor(index / 4) * 0.2],
                  0.1,
                ),
              })),
            );
          }
        }
        scenes.push(scene);
      }
      baseScenes = scenes;
      return {
        preparationMs: performance.now() - start,
        frames,
        scenario,
        identities: scenes.map((scene) => scene.surfaces.map((surface) => surface.id)),
        source: {
          project: project.id,
          entry: project.entry,
          recipe: scenario === "winter-valley" ? WINTER_ACCEPTANCE_RECIPE : "reference-project",
        },
        diagnostics: host.diagnostics,
      };
    },
    freezeFrame(frame = 0) {
      if (!baseScenes[frame]) throw Error("Unknown frame");
      scenes = baseScenes.map(() => baseScenes[frame]);
    },
    configureWorkload(workload: SceneWorkload) {
      scenes = baseScenes.map((scene) => applyWorkload(scene, workload));
      return workloadInventory(scenes[0]);
    },
    async compareMaterial(frame: number) {
      if (!renderer) throw Error("Prepare renderer first");
      renderer.render(scenes[frame]);
      await renderer.capture();
      const image = await linearImage(renderer);
      if (!materialReference) {
        materialReference = image;
        return null;
      }
      const result = compareLinear(materialReference, image);
      materialReference = undefined;
      return result;
    },
    async beginVariant(
      requested: AcceptanceVariant,
      renderCompilerOverrides: RendererOptions["renderCompiler"] = {},
      resolution?: [number, number],
      antialiasing?: RendererOptions["antialiasing"],
      resolutionScale?: number,
      extraOptions: Pick<RendererOptions, "temporalResolve" | "materialCache"> = {},
    ) {
      renderer?.dispose();
      variant = requested;
      canvas.style.width = resolution ? `${resolution[0]}px` : "100%";
      canvas.style.height = resolution ? `${resolution[1]}px` : "100%";
      const options: RendererOptions = {
        ...extraOptions,
        pixelRatio: 1,
        quality: profile,
        antialiasing,
        resolutionScale,
        renderCompiler: { ...VARIANTS[variant], shutterSeconds: 1 / 60, ...renderCompilerOverrides },
      };
      const start = performance.now();
      renderer = await WebGPURenderer.create(canvas, options);
      if (/swiftshader|llvmpipe|software/i.test(renderer.measurements.adapter))
        throw Error("Hardware adapter required");
      renderer.render(scenes[0]);
      await renderer.capture();
      return { initializationMs: performance.now() - start, adapter: renderer.measurements.adapter, options };
    },
    async capture(frame: number, mode: EvaluatedScene["mode"] = "beauty") {
      if (!renderer || !scenes[frame]) throw Error("Prepare fixture and variant first");
      const scene = { ...scenes[frame], mode };
      renderer.render(scene);
      await renderer.capture();
      assertCompleteIdentities(
        scene.surfaces.map((surface) => surface.id),
        renderer.completeness,
      );
      const pixels = await linearImage(renderer),
        key = `${frame}/${mode}`;
      let comparison: ReturnType<typeof compareLinear> | null = null;
      let temporal: ReturnType<typeof compareLinear> | null = null;
      if (variant === "production") references.set(key, pixels);
      else {
        const reference = references.get(key);
        if (!reference) throw Error(`Missing matched production capture ${key}`);
        comparison =
          variant === "water-reference" ? compareLinear(pixels, reference) : compareLinear(reference, pixels);
        if (scenarioName === "visibility" && mode === "identity" && comparison.maximum !== 0)
          throw Error(`Opaque visibility changed GPU identity pixels: ${comparison.maximum}`);
        const previousKey = `${variant}/${mode}`,
          previous = previousImages.get(previousKey),
          previousReference = previousReferences.get(previousKey);
        if (previous && previousReference && mode === "beauty") {
          const directDelta = new Float32Array(pixels.length),
            candidateDelta = new Float32Array(pixels.length);
          for (let index = 0; index < pixels.length; index++) {
            directDelta[index] = reference[index] - previousReference[index];
            candidateDelta[index] = pixels[index] - previous[index];
          }
          temporal =
            variant === "water-reference"
              ? compareLinear(candidateDelta, directDelta)
              : compareLinear(directDelta, candidateDelta);
        }
        previousImages.set(previousKey, pixels);
        previousReferences.set(previousKey, reference);
      }
      return {
        frame,
        mode,
        variant,
        camera: scene.camera,
        time: scene.time,
        sun: scene.environment.sunDirection,
        comparison,
        temporal,
        completeness: structuredClone(renderer.completeness),
        measurements: structuredClone(renderer.measurements),
      };
    },
    async run(warmup = false, frameLimit = scenes.length, serialGpu = false, paced = true, burst = 1) {
      if (!renderer) throw Error("Prepare variant first");
      if (!Number.isInteger(burst) || burst < 1 || burst > 8) throw Error("Invalid GPU burst size");
      const active = renderer;
      await active.flushGpuTimings();
      active.drainGpuTimings();
      const cpu: number[] = [],
        pacing: number[] = [],
        gpu: GpuFrameTiming[] = [];
      const elapsedStart = performance.now();
      const firstFrame = active.measurements.frame + 1;
      let previous: number | undefined;
      const count = Math.min(scenes.length, frameLimit);
      for (let frame = 0; frame < count; frame++) {
        if (paced && frame % burst === 0) await new Promise(requestAnimationFrame);
        const now = performance.now();
        if (previous !== undefined) pacing.push(now - previous);
        previous = now;
        const start = performance.now();
        active.render(scenes[frame]);
        cpu.push(performance.now() - start);
        // The expensive dense-water control can fill the asynchronous query ring.
        // Both matched conditions may explicitly wait for each frame so every
        // timestamp is attributed. GPU timestamps and CPU render submission above
        // exclude this wait and its readback, so this is not a pacing measurement.
        if ((serialGpu || !paced) && ((frame + 1) % burst === 0 || frame === count - 1))
          await active.flushGpuTimings();
        gpu.push(...active.drainGpuTimings());
        if (warmup && active.needsRender) await active.capture();
        if (!warmup)
          assertCompleteIdentities(
            scenes[frame].surfaces.map((surface) => surface.id),
            active.completeness,
          );
      }
      await active.flushGpuTimings();
      gpu.push(...active.drainGpuTimings());
      const lastFrame = active.measurements.frame;
      const samples = gpu.filter((sample) => sample.frame >= firstFrame && sample.frame <= lastFrame);
      if (new Set(samples.map((sample) => sample.frame)).size !== samples.length)
        throw Error("Duplicate GPU timing sample");
      return {
        cpu,
        pacing,
        gpu: samples.map((sample) => ({ ...sample, trajectoryFrame: sample.frame - firstFrame })),
        frameRange: { firstFrame, lastFrame },
        paced,
        burst,
        elapsedMs: performance.now() - elapsedStart,
        measurements: structuredClone(active.measurements),
        diagnostics: active.diagnostics,
      };
    },
    async runLive(frames = 91) {
      if (!host || !renderer) throw Error("Prepare fixture and renderer first");
      const active = renderer,
        liveHost = host;
      const simulation: number[] = [],
        extraction: number[] = [],
        submission: number[] = [],
        cpu: number[] = [],
        pacing: number[] = [];
      const gpu: GpuFrameTiming[] = [];
      let firstFrame = 0,
        previous: number | undefined;
      for (let index = -45; index < frames; index++) {
        await new Promise(requestAnimationFrame);
        if (index === 0) {
          await active.flushGpuTimings();
          active.drainGpuTimings();
          firstFrame = active.measurements.frame + 1;
          previous = undefined;
        }
        const start = performance.now(),
          camera = trajectoryFrame(Math.max(0, index), frames).camera;
        liveHost.advance(1 / 60, camera);
        const simulated = performance.now(),
          scene = liveHost.extract(camera),
          extracted = performance.now();
        active.render(scene);
        if (index >= 0) {
          const end = performance.now();
          simulation.push(simulated - start);
          extraction.push(extracted - simulated);
          submission.push(end - extracted);
          cpu.push(end - start);
          if (previous !== undefined) pacing.push(start - previous);
          previous = start;
          assertCompleteIdentities(
            scene.surfaces.map((s) => s.id),
            active.completeness,
          );
          gpu.push(...active.drainGpuTimings());
        }
      }
      await active.flushGpuTimings();
      gpu.push(...active.drainGpuTimings());
      return {
        simulation,
        extraction,
        submission,
        cpu,
        pacing,
        gpu: gpu.filter((s) => s.frame >= firstFrame),
        measurements: structuredClone(active.measurements),
        diagnostics: active.diagnostics,
      };
    },
    async waterReference() {
      if (!renderer) throw Error("Create the production renderer before validating water");
      return validateWaterReference((renderer as unknown as { device: GPUDevice }).device);
    },
    dispose() {
      renderer?.dispose();
      host?.dispose();
      compiler.dispose();
    },
  };
}
export type AcceptanceFixture = Awaited<ReturnType<typeof createAcceptanceFixture>>;
