import {
  type EvaluatedScene,
  identityColor,
  identityMatrix,
  type RenderSurface,
  waterSchema,
} from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, gridMesh } from "@wrela/runtime";
import { WaterBodyRuntime } from "@wrela/runtime/water-body";
import { linearImage } from "../rendering-compiler/fixture";
import { indirectBoxFixture } from "./indirect-scenes";

/** Production spectrum-water shading must obey the same local-light range and
 * shadow policy as solid surfaces. Fixed time and no simulation isolate light. */
export async function createLightingWaterFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing canvas");
  const errors: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    quality: "balanced",
    antialiasing: "spatial",
    onDiagnostic: (d) => {
      if (d.severity === "error") errors.push(d.message);
    },
  });
  const water = waterSchema.parse({
    kind: "water",
    schemaVersion: 1,
    dependencies: [],
    id: "water-light-policy",
    name: "Water light policy",
    level: 0,
    color: [0.06, 0.17, 0.2],
    roughness: 0.3,
    waves: [],
    spectrum: {
      seed: 23,
      windSpeed: 3,
      amplitude: 0.01,
      wavelength: 1.5,
      direction: 1.2,
      spread: 1,
      choppiness: 0,
    },
    optics: { absorption: [0.23, 0.075, 0.05], caustics: 0, foam: 0 },
  });
  const body = new WaterBodyRuntime(water);
  const surface: RenderSurface = {
    id: water.id,
    source: water.id,
    matrix: identityMatrix(),
    mesh: gridMesh(12, 24),
    water,
    waterState: body.renderState(),
    material: {
      color: water.color,
      secondary: water.color,
      roughness: water.roughness,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
  };
  const blocker = indirectBoxFixture().surfaces.at(-1);
  if (!blocker) throw Error("Missing blocker");
  blocker.matrix[0] = 3;
  blocker.matrix[10] = 3;
  blocker.matrix[13] = 0.8;
  blocker.castsShadow = false;
  const environment = evaluateEnvironment();
  environment.sunIntensity = 0;
  environment.ambient = 0;
  environment.fogDensity = 0;
  environment.cloudCover = 0;
  const scene: EvaluatedScene = {
    surfaces: [surface, blocker],
    environment,
    time: 0,
    mode: "beauty",
    grid: false,
    camera: { position: [4, 3, 5], target: [0, 0, 0], fov: 55 },
  };
  const capture = async () => {
    for (let i = 0; i < 8; i++) {
      renderer.render(scene);
      await renderer.flushGpuTimings();
    }
    if (!renderer.completeness.complete || errors.length)
      throw Error(JSON.stringify({ completeness: renderer.completeness, errors }));
    const pixels = await linearImage(renderer);
    if (!pixels.every(Number.isFinite)) throw Error("Nonfinite water lighting");
    return pixels;
  };
  return {
    async check() {
      scene.mode = "identity";
      const material = await capture();
      const waterPixels: number[] = [];
      for (let i = 0; i < material.length; i += 4)
        if (identityColor(surface.id).every((value, c) => Math.abs(value - material[i + c]) < 0.001))
          waterPixels.push(i);
      if (waterPixels.length < 1000) throw Error("Insufficient visible water in local-light check");
      scene.mode = "beauty";
      environment.pointLights = [];
      const dark = await capture();
      environment.pointLights = [
        { position: [-1, 3, -1], color: [1, 0.63, 0.25], intensity: 100, range: 0.1 },
      ];
      const outsideRange = await capture();
      environment.pointLights[0].range = 12;
      const unshadowed = await capture();
      blocker.castsShadow = true;
      const shadowed = await capture();
      let rangeMaximumError = 0,
        litDifference = 0,
        shadowDifference = 0;
      for (const pixel of waterPixels)
        for (let c = 0; c < 3; c++) {
          const i = pixel + c;
          rangeMaximumError = Math.max(rangeMaximumError, Math.abs(dark[i] - outsideRange[i]));
          litDifference += Math.max(0, unshadowed[i] - outsideRange[i]);
          shadowDifference += Math.max(0, unshadowed[i] - shadowed[i]);
        }
      if (rangeMaximumError > 0.0001) errors.push("An out-of-range light illuminates water");
      if (litDifference < 0.1) errors.push("An in-range light does not illuminate water");
      if (shadowDifference < 0.1) errors.push("The local caster does not shadow water");
      return {
        rangeMaximumError,
        litDifference,
        shadowDifference,
        waterPixels: waterPixels.length,
        adapter: renderer.measurements.adapter,
        errors,
      };
    },
    dispose() {
      renderer.dispose();
    },
  };
}
