import { compileDocument, withVegetationCrowns } from "@wrela/compiler";
import { type EvaluatedScene, parseProject, type Vec3 } from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { crownViewRanges } from "@wrela/render-webgpu/vegetation-selection";
import { BrowserSceneHost } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import type { LookdevStudy } from "./lookdev";

const SIZE = 32;
function downsample(input: Float32Array, scale: number) {
  const size = SIZE * scale,
    out = new Float32Array(SIZE * SIZE);
  for (let y = 0; y < size; y++)
    for (let x = 0; x < size; x++)
      out[Math.floor(y / scale) * SIZE + Math.floor(x / scale)] +=
        input[(y * size + x) * 4] / (scale * scale);
  return out;
}
function difference(reference: Float32Array, candidate: Float32Array) {
  let minX = SIZE,
    minY = SIZE,
    maxX = 0,
    maxY = 0;
  for (let i = 0; i < reference.length; i++)
    if (reference[i] > 0.0001) {
      minX = Math.min(minX, i % SIZE);
      minY = Math.min(minY, Math.floor(i / SIZE));
      maxX = Math.max(maxX, i % SIZE);
      maxY = Math.max(maxY, Math.floor(i / SIZE));
    }
  minX = Math.max(0, minX - 1);
  minY = Math.max(0, minY - 1);
  maxX = Math.min(SIZE - 1, maxX + 1);
  maxY = Math.min(SIZE - 1, maxY + 1);
  let a = 0,
    b = 0,
    squared = 0,
    n = 0;
  for (let y = minY; y <= maxY; y++)
    for (let x = minX; x <= maxX; x++) {
      const i = y * SIZE + x;
      a += reference[i];
      b += candidate[i];
      squared += (reference[i] - candidate[i]) ** 2;
      n++;
    }
  const referenceCoverage = a / Math.max(1, n),
    coverageError = (b - a) / Math.max(1, n);
  return {
    roi: [minX, minY, maxX, maxY],
    referenceCoverage,
    candidateCoverage: b / Math.max(1, n),
    relativeAreaError: a ? Math.abs(b - a) / a : Math.abs(b - a),
    coverageError,
    rmse: Math.sqrt(squared / Math.max(1, n)),
    passesCoverage:
      Math.sqrt(squared / Math.max(1, n)) <= 0.03 &&
      (referenceCoverage >= 0.1 ? Math.abs(b - a) / a <= 0.02 : Math.abs(coverageError) <= 0.002),
  };
}
export async function createVegetationFrontier(study: LookdevStudy, options: { depthGrid?: 0 | 16 } = {}) {
  const candidateCanvas = document.querySelector("canvas");
  if (!candidateCanvas) throw Error("Missing canvas");
  const canvas: HTMLCanvasElement = candidateCanvas;
  const host = new BrowserSceneHost(parseProject(study.project), {
    maxInstalledBytes: 192 * 1024 * 1024,
    compile: async (document, quality) => {
      const artifact = compileDocument(document, quality);
      return artifact?.kind === "vegetation" ? withVegetationCrowns(artifact, options) : artifact;
    },
  });
  await host.prepare(study.subject, study.stage, "review");
  const errors: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    resolutionScale: 1,
    antialiasing: "spatial",
    maxGpuBytes: 256 * 1024 * 1024,
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  async function frame(
    pixels: number,
    azimuth: number,
    elevation: number,
    choice: "source" | "crown",
    scale: number,
  ) {
    const original = host.extract(study.frames[0].camera, "silhouette");
    const surfaces = original.surfaces.filter((surface) => surface.id.endsWith("-foliage"));
    if (surfaces.length !== 1) throw Error("Crown study requires one foliage surface");
    const source = surfaces[0],
      bounds = source.mesh.bounds;
    const target = bounds.min.map((v, i) => (v + bounds.max[i]) * 0.5) as Vec3;
    const radius = Math.hypot(...bounds.max.map((v, i) => (v - bounds.min[i]) * 0.5));
    const distance = radius + (radius * SIZE) / (pixels * Math.tan((21 * Math.PI) / 180));
    const direction: Vec3 = [
      Math.cos(azimuth) * Math.cos(elevation),
      Math.sin(elevation),
      Math.sin(azimuth) * Math.cos(elevation),
    ];
    const camera = { position: target.map((v, i) => v + direction[i] * distance) as Vec3, target, fov: 42 };
    const scene: EvaluatedScene = {
      ...original,
      camera,
      time: 0,
      grid: false,
      surfaces,
      environment: { ...original.environment, wind: [0, 0, 0] },
    };
    if (choice === "crown") {
      const detail = source.details?.find((detail) => detail.vegetation);
      if (!detail) throw Error("Missing candidate crown");
      scene.surfaces = [
        { ...source, mesh: detail.mesh, ...crownViewRanges(scene, source, detail), details: undefined },
      ];
    } else scene.surfaces = surfaces.map((surface) => ({ ...surface, details: undefined }));
    canvas.style.width = `${SIZE * scale}px`;
    canvas.style.height = `${SIZE * scale}px`;
    for (let attempt = 0; attempt < 64; attempt++) {
      renderer.render(scene);
      await renderer.flushGpuTimings();
      renderer.drainGpuTimings();
      if (renderer.completeness.complete) break;
      if (renderer.completeness.rejected.length) throw Error(JSON.stringify(renderer.completeness));
    }
    if (!renderer.completeness.complete || errors.length)
      throw Error(errors.join("\n") || "Incomplete crown capture");
    if (renderer.measurements.renderResolution?.[0] !== SIZE * scale)
      throw Error("Reference resolution was reduced");
    return downsample(await linearImage(renderer), scale);
  }
  return {
    async compare(pixels: number, azimuth: number, elevation: number) {
      const a = await frame(pixels, azimuth, elevation, "source", 32),
        reference = await frame(pixels, azimuth, elevation, "source", 64),
        candidate = await frame(pixels, azimuth, elevation, "crown", 64);
      return {
        pixels,
        azimuth,
        elevation,
        referenceConvergence: difference(reference, a),
        candidate: difference(reference, candidate),
        reference: Array.from(reference),
        coverage: Array.from(candidate),
        adapter: renderer.measurements.adapter,
      };
    },
    dispose() {
      renderer.dispose();
      host.dispose();
    },
  };
}
