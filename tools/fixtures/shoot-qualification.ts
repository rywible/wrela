import { compileShootInstances } from "@wrela/compiler/shoot-instances";
import { shapedPineLookdevDefinition } from "@wrela/examples";
import {
  type EvaluatedScene,
  identityMatrix,
  type MeshData,
  parseProject,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";
import { WebGPURenderer } from "@wrela/render-webgpu";
import { BrowserSceneHost } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { vegetationStudies } from "./vegetation-study";

const SIZE = 160;
export function coverageDifference(reference: Float32Array, candidate: Float32Array) {
  let a = 0,
    b = 0,
    error = 0;
  for (let i = 0; i < reference.length; i++) {
    a += reference[i];
    b += candidate[i];
    error += Math.abs(reference[i] - candidate[i]);
  }
  return {
    relativeAreaBias: Math.abs(b - a) / Math.max(a, 1e-8),
    normalizedSpatialL1: error / Math.max(a, 1e-8),
    referenceArea: a,
    candidateArea: b,
  };
}
export async function createShootQualification() {
  const candidateCanvas = document.querySelector("canvas");
  if (!candidateCanvas) throw Error("Missing canvas");
  const canvas: HTMLCanvasElement = candidateCanvas;
  const study = vegetationStudies({ architecture: true })[0],
    host = new BrowserSceneHost(parseProject(study.project));
  await host.prepare(study.subject, study.stage, "review");
  const base = host.extract(study.frames[0].camera, "silhouette");
  const candidateFoliage = base.surfaces.find((s) => s.id.endsWith("-foliage"));
  if (!candidateFoliage) throw Error("Missing foliage surface");
  const foliage: RenderSurface = candidateFoliage;
  const templates = compileShootInstances(shapedPineLookdevDefinition());
  if (!templates) throw Error("Missing shoot products");
  const errors: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    resolutionScale: 1,
    antialiasing: "spatial",
    finiteSun: false,
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  async function render(mesh: MeshData | undefined, scene: EvaluatedScene, scale: number) {
    canvas.style.width = `${SIZE * scale}px`;
    canvas.style.height = `${SIZE * scale}px`;
    const input = {
      ...scene,
      surfaces: mesh
        ? [{ ...foliage, id: "shoot", mesh, matrix: identityMatrix(), wind: 0, details: undefined }]
        : [],
    };
    for (let i = 0; i < 64; i++) {
      renderer.render(input);
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      if (renderer.completeness.complete) break;
    }
    if (!renderer.completeness.complete || errors.length)
      throw Error(errors.join("\n") || "Incomplete qualification frame");
    if (renderer.measurements.renderResolution?.[0] !== SIZE * scale)
      throw Error("Scaled reference resolution");
    return linearImage(renderer);
  }
  const reduce = (image: Float32Array, scale: number) => {
    const out = new Float32Array(SIZE * SIZE);
    for (let y = 0; y < SIZE * scale; y++)
      for (let x = 0; x < SIZE * scale; x++)
        out[Math.floor(y / scale) * SIZE + Math.floor(x / scale)] +=
          image[(y * SIZE * scale + x) * 4] / (scale * scale);
    return out;
  };
  return {
    async compare(pixels: number, angle: number, elevation: number, variant = 0) {
      const product = templates[variant];
      if (!product?.mesh.shoots || !product.details?.[0]) throw Error("Missing shoot reference");
      const bounds = product.mesh.shoots.templateBounds;
      const source = { ...product.mesh, shoots: undefined, bounds },
        proxy = { ...product.details[0].mesh, shoots: undefined, bounds };
      const target = bounds.min.map((v, a) => (v + bounds.max[a]) / 2) as Vec3,
        radius = Math.hypot(...bounds.max.map((v, a) => (v - bounds.min[a]) / 2));
      const distance = radius + (radius * SIZE) / (pixels * Math.tan((Math.PI * 21) / 180));
      const direction: Vec3 = [
        Math.cos(angle) * Math.cos(elevation),
        Math.sin(elevation),
        Math.sin(angle) * Math.cos(elevation),
      ];
      const camera = { position: target.map((v, a) => v + direction[a] * distance) as Vec3, target, fov: 42 };
      const scene: EvaluatedScene = {
        ...base,
        camera,
        mode: "silhouette",
        grid: false,
        time: 0,
        environment: { ...base.environment, wind: [0, 0, 0], fogDensity: 0 },
      };
      const coarse = reduce(await render(source, scene, 4), 4),
        reference = reduce(await render(source, scene, 8), 8),
        candidate = reduce(await render(proxy, scene, 8), 8);
      const convergence = coverageDifference(reference, coarse),
        coverage = coverageDifference(reference, candidate);
      scene.mode = "beauty";
      const background = await render(undefined, scene, 4),
        a = await render(source, scene, 4),
        b = await render(proxy, scene, 4);
      let referenceSignal = 0,
        candidateSignal = 0,
        l1 = 0;
      for (let i = 0; i < a.length; i++)
        if (i % 4 !== 3) {
          referenceSignal += Math.abs(a[i] - background[i]);
          candidateSignal += Math.abs(b[i] - background[i]);
          l1 += Math.abs(a[i] - b[i]);
        }
      const radiance = {
        relativeForegroundEnergy:
          Math.abs(candidateSignal - referenceSignal) / Math.max(referenceSignal, 1e-8),
        normalizedL1: l1 / Math.max(referenceSignal, 1e-8),
      };
      return {
        pixels,
        angle,
        elevation,
        variant,
        convergence,
        coverage,
        radiance,
        adapter: renderer.measurements.adapter,
        passes:
          convergence.relativeAreaBias <= 0.005 &&
          convergence.normalizedSpatialL1 <= 0.005 &&
          coverage.relativeAreaBias <= 0.02 &&
          coverage.normalizedSpatialL1 <= 0.02 &&
          radiance.relativeForegroundEnergy <= 0.05,
      };
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
