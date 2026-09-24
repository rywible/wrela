import {
  createSurfaceAppearance,
  createSurfaceLayer,
  type EvaluatedScene,
  identityMatrix,
  materialSchema,
} from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, gridMesh, renderMaterial } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";

/** Hardware evidence: evaluate every surface family and compare authored changes against a dry control. */
export async function surfaceAppearanceFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Surface review canvas missing");
  const errors: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") errors.push(diagnostic.message);
    },
  });
  const source = materialSchema.parse({
    id: "surface-review",
    name: "Surface review",
    kind: "material",
    schemaVersion: 1,
    dependencies: [],
    color: [0.4, 0.3, 0.2],
    secondary: [0.2, 0.3, 0.4],
    roughness: 0.7,
    metallic: 0,
    pattern: "solid",
    scale: 1,
    normalStrength: 0,
  });
  const scene: EvaluatedScene = {
    surfaces: [
      {
        id: "review",
        source: "review",
        mesh: gridMesh(8, 4),
        matrix: identityMatrix(),
        material: renderMaterial(source),
      },
    ],
    camera: { position: [1, 3, 4], target: [0, 0, 0], fov: 50 },
    environment: evaluateEnvironment(),
    time: 0,
    mode: "beauty",
    grid: false,
  };
  scene.environment.fogDensity = 0;
  async function pixels() {
    renderer.render(scene);
    return linearImage(renderer);
  }
  const baseline = await pixels();
  const rms = (image: Float32Array, reference = baseline) =>
    Math.sqrt(image.reduce((sum, value, index) => sum + (value - reference[index]) ** 2, 0) / image.length);
  return {
    async check() {
      const results: Record<string, number> = {};
      const invariants: Record<string, number> = {};
      for (const family of ["skin", "foliage", "fabric", "metal", "glass"] as const) {
        source.appearance = createSurfaceAppearance(family);
        scene.surfaces[0].material = renderMaterial(source);
        results[family] = rms(await pixels());
      }
      for (const property of ["wetness", "weathering", "dirt", "damage"] as const) {
        source.appearance = createSurfaceAppearance();
        source.appearance[property] = 1;
        scene.surfaces[0].material = renderMaterial(source);
        results[property] = rms(await pixels());
      }
      for (const kind of ["uniform", "noise", "slope", "height", "combined"] as const) {
        source.appearance = createSurfaceAppearance();
        const layer = createSurfaceLayer("coating");
        layer.color = [0.05, 0.6, 0.2];
        layer.mask.kind = kind;
        layer.mask.minimumHeight = -1;
        layer.mask.maximumHeight = 1;
        layer.relief = 0.01;
        source.appearance.layers = [layer];
        scene.surfaces[0].material = renderMaterial(source);
        results[`layer-${kind}`] = rms(await pixels());
      }
      source.appearance = createSurfaceAppearance();
      source.appearance.detail = { kind: "mineral", scale: 1, strength: 0.7 };
      scene.surfaces[0].material = renderMaterial(source);
      const detailOnly = await pixels();
      source.layers = [
        {
          color: [0.05, 0.6, 0.2],
          roughness: source.roughness,
          metallic: 0,
          coverage: 1,
          slopeBias: 0,
          noiseScale: 1,
          normalStrength: 0,
        },
      ];
      scene.surfaces[0].material = renderMaterial(source);
      results["legacy-layer-with-detail"] = rms(await pixels(), detailOnly);
      source.layers = undefined;
      source.appearance = createSurfaceAppearance();
      source.color = [0.05, 0.6, 0.2];
      scene.surfaces[0].material = renderMaterial(source);
      const opaqueControl = await pixels();
      for (const family of ["glass", "skin", "fabric"] as const) {
        source.appearance = createSurfaceAppearance(family);
        const layer = createSurfaceLayer("opaque-paint");
        layer.color = [...source.color];
        layer.roughness = source.roughness;
        layer.mask.kind = "uniform";
        source.appearance.layers = [layer];
        scene.surfaces[0].material = renderMaterial(source);
        invariants[`opaque-coating-${family}`] = rms(await pixels(), opaqueControl);
      }
      return {
        results,
        invariants,
        errors,
        complete: renderer.completeness.complete,
        adapter: renderer.measurements.adapter,
      };
    },
    dispose() {
      renderer.dispose();
    },
  };
}
