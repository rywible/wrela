import { expect, test } from "bun:test";
import { compileSurface } from "@wrela/compiler/surface";
import { referenceProject } from "@wrela/examples";
import { type EvaluatedScene, identityMatrix, type RenderSurface } from "@wrela/model";
import { projectedGeometryError, selectRenderProducts } from "@wrela/render-webgpu/realization";
import { STONE_COMPILE_QUALITY, stoneFieldScene } from "./stone-field-layout";

test("all stone workload scales have a genuinely admissible cheapest parametric control", () => {
  const doc = referenceProject().documents.find((d) => d.id === "river-stone");
  if (doc?.kind !== "object") throw Error("Stone missing");
  const compiled = compileSurface(doc, STONE_COMPILE_QUALITY);
  const stone: RenderSurface = {
    id: "stone",
    source: doc.id,
    mesh: compiled.mesh,
    renderProducts: compiled.renderProducts,
    matrix: identityMatrix(),
    material: {
      color: [0.5, 0.5, 0.5],
      secondary: [0.5, 0.5, 0.5],
      roughness: 0.4,
      metallic: 0,
      pattern: 0,
      scale: 1,
      normalStrength: 0,
    },
  };
  const base: EvaluatedScene = {
    surfaces: [stone],
    camera: { position: [0, 1, 0], target: [0, 0, 0], fov: 45 },
    environment: {
      sunDirection: [0, 1, 0],
      sunColor: [1, 1, 1],
      sunIntensity: 1,
      ambient: 1,
      skyColor: [1, 1, 1],
      horizonColor: [1, 1, 1],
      groundColor: [1, 1, 1],
      fogDensity: 0,
      wind: [0, 0, 0],
      exposure: 1,
    },
    time: 0,
    mode: "beauty",
    grid: false,
  };
  for (const [count, height] of [
    [64, 180],
    [256, 360],
    [4096, 1080],
  ]) {
    const scene = stoneFieldScene(base, stone, count);
    const selected = selectRenderProducts(scene, height, {
      geometry: "parametric",
      maxGeometryErrorPixels: 0.25,
    });
    expect(selected.decisions.length).toBe(count);
    expect(selected.decisions.every((d) => d.kind === "parametric-mesh")).toBe(true);
    const maxima = selected.scene.surfaces.map((s) => {
      const p = s.selectedRenderProduct;
      if (!p || p.kind !== "parametric-mesh") throw Error("Invalid choice");
      const bound = p.errors.find((e) => e.kind !== "unknown" && e.metric === "silhouette");
      if (!bound || bound.kind === "unknown") throw Error("Missing bound");
      const error = projectedGeometryError(scene, s, bound.maximum, height);
      const alternatives =
        s.renderProducts?.filter(
          (candidate) =>
            candidate.kind === "parametric-mesh" &&
            candidate.errors.some(
              (e) =>
                e.kind !== "unknown" &&
                e.metric === "silhouette" &&
                projectedGeometryError(scene, s, e.maximum, height) <= 0.25,
            ),
        ) ?? [];
      expect(p.byteLength).toBe(Math.min(...alternatives.map((p) => p.byteLength)));
      return error;
    });
    expect(Math.max(...maxima)).toBeLessThanOrEqual(0.25);
    console.log(
      JSON.stringify({
        count,
        height,
        maximumProjectedError: Math.max(...maxima),
        selectedKey: selected.decisions[0].key,
        trianglesPerStone: selected.scene.surfaces[0].mesh.indices.length / 3,
      }),
    );
  }
});
