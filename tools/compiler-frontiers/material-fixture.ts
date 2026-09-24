import { type EvaluatedScene, type GpuFrameTiming, identityMatrix, materialSchema } from "@wrela/model";

import { cameraBasis, type RendererOptions, WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, gridMesh, renderMaterial } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { errors } from "./probes";

export async function materialFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Canvas missing");
  let renderer: WebGPURenderer | undefined;
  const failures: string[] = [];
  const source = materialSchema.parse({
    id: "compiled-weave",
    name: "Woven winter cloth",
    schemaVersion: 1,
    dependencies: [],
    kind: "material",
    pattern: "weave",
    color: [0.025, 0.08, 0.14],
    secondary: [0.85, 0.63, 0.3],
    roughness: 0.85,
    metallic: 0,
    normalStrength: 0,
    scale: 20,
    domain: "world",
  });
  const scene: EvaluatedScene = {
    surfaces: [
      {
        id: "cloth",
        source: "cloth",
        mesh: gridMesh(100, 1),
        matrix: identityMatrix(),
        material: renderMaterial(source),
      },
    ],
    camera: { position: [1, 4, 6], target: [0, 0, 0], fov: 50 },
    time: 0,
    mode: "beauty",
    grid: false,
    environment: evaluateEnvironment(),
  };
  scene.environment.fogDensity = 0;
  const references = new Map<number, Float32Array>();
  const compiled = new Map<number, Float32Array>();
  function pose(index: number) {
    const frame = index % 8,
      angle = frame * 0.07;
    scene.camera = {
      position: [Math.sin(angle) * 5, index < 8 ? 4 : index < 16 ? 1.2 : 7, Math.cos(angle) * 5],
      target: [0.2 * frame, 0, 0],
      fov: 50,
    };
    scene.surfaces[0].material.scale = index < 8 ? 8 : index < 16 ? 32 : 80;
    scene.time = index / 60;
  }
  const crop = (pixels: Float32Array) => {
    const values: number[] = [];
    for (let y = Math.floor(canvas.height * 0.3); y < canvas.height * 0.9; y++)
      for (let x = Math.floor(canvas.width * 0.15); x < canvas.width * 0.85; x++)
        for (let c = 0; c < 3; c++) values.push(pixels[(y * canvas.width + x) * 4 + c]);
    return values;
  };
  return {
    async mode(mode: RendererOptions["wovenIntegration"]) {
      renderer?.dispose();
      renderer = await WebGPURenderer.create(canvas, {
        quality: "balanced",
        pixelRatio: 1,
        antialiasing: "spatial",
        wovenIntegration: mode,
        onDiagnostic: (d) => {
          if (d.severity === "error") failures.push(d.message);
        },
      });
    },
    async image(index: number, kind: "reference" | "compiled" | "compare") {
      if (!renderer) throw Error("Renderer missing");
      pose(index);
      renderer.render(scene);
      const pixels = await linearImage(renderer);
      if (kind === "reference") {
        references.set(index, pixels);
        return;
      }
      if (kind === "compiled") compiled.set(index, pixels);
      const reference = references.get(index);
      if (!reference) throw Error("Reference missing");
      const imageError = errors(crop(reference), crop(pixels));
      let temporalError: ReturnType<typeof errors> | undefined;
      if (kind === "compiled" && index % 8 > 0) {
        const oldReference = references.get(index - 1),
          oldCompiled = compiled.get(index - 1);
        if (oldReference && oldCompiled)
          temporalError = errors(
            crop(reference.map((v, i) => v - oldReference[i])),
            crop(pixels.map((v, i) => v - oldCompiled[i])),
          );
      }
      return { index, imageError, temporalError };
    },
    async measure() {
      if (!renderer) throw Error("Renderer missing");
      const timing: GpuFrameTiming[] = [];
      for (let frame = 0; frame < 64; frame++) {
        pose(8 + (frame % 8));
        renderer.render(scene);
        if (frame % 4 === 3) {
          await renderer.flushGpuTimings();
          const samples = renderer.drainGpuTimings();
          if (frame >= 16) timing.push(...samples);
        }
      }
      if (
        timing.length !== 48 ||
        renderer.measurements.gpuTimingDroppedFrames !== 0 ||
        !renderer.completeness.complete ||
        failures.length
      )
        throw Error(
          "Incomplete material timing run: " +
            JSON.stringify({ count: timing.length, measurements: renderer.measurements, failures }),
        );
      return { timing, measurements: renderer.measurements, failures };
    },
    async rebase() {
      if (!renderer) throw Error("Renderer missing");
      pose(8);
      scene.mode = "albedo";
      scene.origin = [0, 0, 0];
      renderer.render(scene);
      const a = await linearImage(renderer);
      scene.origin = [1000000000, 0, -1000000000];
      renderer.render(scene);
      const b = await linearImage(renderer);
      scene.origin = [0, 0, 0];
      scene.mode = "beauty";
      return errors(crop(a), crop(b));
    },
    async perspectiveCheck() {
      if (!renderer) throw Error("Renderer missing");
      const results = [];
      scene.mode = "albedo";
      for (const index of [0, 8, 16]) {
        pose(index);
        const device = (renderer as unknown as { device: GPUDevice }).device;
        device.pushErrorScope("validation");
        renderer.render(scene);
        const image = await linearImage(renderer);
        const gpuError = await device.popErrorScope();
        if (gpuError) throw Error(gpuError.message);
        const basis = cameraBasis(scene.camera),
          tangent = Math.tan((scene.camera.fov * Math.PI) / 360),
          aspect = canvas.width / canvas.height;
        const actual: number[] = [],
          reference: number[] = [],
          coarse: number[] = [];
        for (let sample = 0; sample < 96; sample++) {
          const x = Math.floor(canvas.width * (0.2 + (0.6 * ((sample * 17) % 97)) / 97)),
            y = Math.floor(canvas.height * (0.35 + (0.5 * ((sample * 37) % 97)) / 97));
          const integrate = (count: number) => {
            let result = 0;
            for (let sy = 0; sy < count; sy++)
              for (let sx = 0; sx < count; sx++) {
                const px = ((2 * (x + (sx + 0.5) / count)) / canvas.width - 1) * aspect * tangent,
                  py = (1 - (2 * (y + (sy + 0.5) / count)) / canvas.height) * tangent;
                const dx = basis.forward[0] + basis.right[0] * px + basis.up[0] * py;
                const dy = basis.forward[1] + basis.right[1] * px + basis.up[1] * py;
                const dz = basis.forward[2] + basis.right[2] * px + basis.up[2] * py;
                const t = -scene.camera.position[1] / dy;
                const u =
                  (scene.camera.position[0] + t * dx) * scene.surfaces[0].material.scale * 2 * Math.PI;
                const v =
                  (scene.camera.position[2] + t * dz) * scene.surfaces[0].material.scale * 2 * Math.PI;
                result += (0.5 + 0.5 * Math.cos(u)) * (0.5 + 0.5 * Math.cos(v));
              }
            return result / count ** 2;
          };
          const fine = integrate(128),
            low = integrate(64);
          for (let c = 0; c < 3; c++) {
            actual.push(image[(y * canvas.width + x) * 4 + c]);
            reference.push(source.color[c] + (source.secondary[c] - source.color[c]) * fine);
            coarse.push(source.color[c] + (source.secondary[c] - source.color[c]) * low);
          }
        }
        results.push({
          index,
          pixels: 96,
          error: errors(reference, actual),
          convergence: errors(reference, coarse),
        });
      }
      scene.mode = "beauty";
      return results;
    },
    dispose() {
      renderer?.dispose();
    },
  };
}
