import {
  alpineConiferMaterials,
  createSurfaceAppearance,
  type EvaluatedScene,
  identityColor,
  identityMatrix,
  materialSchema,
  normalize,
  type RenderSurface,
  type Vec3,
} from "@wrela/model";

import { WebGPURenderer } from "@wrela/render-webgpu";
import { evaluateEnvironment, renderMaterial } from "@wrela/runtime";
import { linearImage } from "../rendering-compiler/fixture";
import { probeFoliageResponse } from "./foliage-probe";
import { needleShootMesh, thinLeafMesh } from "./foliage-specimens";

export type FoliageLighting = "front" | "back" | "grazing";
const directions: Record<FoliageLighting, Vec3> = {
  front: normalize([0.2, 0.35, 1]),
  back: normalize([-0.2, 0.35, -1]),
  grazing: normalize([1, 0.2, 0.02]),
};
const imageURL = (blob: Blob) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
export async function createFoliageLookdevFixture() {
  const canvas = document.querySelector("canvas");
  if (!canvas) throw Error("Missing foliage review canvas");
  const errors: string[] = [];
  const renderer = await WebGPURenderer.create(canvas, {
    pixelRatio: 1,
    antialiasing: "spatial",
    onDiagnostic: (diagnostic) => {
      if (diagnostic.severity === "error") errors.push(diagnostic.message);
    },
  });
  const appearance = createSurfaceAppearance("foliage");
  appearance.transmission = 0.55;
  appearance.response = { thickness: 0.00035, scatterColor: [0.55, 0.95, 0.35] };
  const leaf = materialSchema.parse({
    id: "thin-leaf",
    name: "Thin broadleaf",
    kind: "material",
    schemaVersion: 1,
    dependencies: [],
    color: [0.075, 0.26, 0.025],
    secondary: [0.075, 0.26, 0.025],
    pattern: "solid",
    scale: 1,
    normalStrength: 0,
    roughness: 0.7,
    metallic: 0,
    appearance,
  });
  const leafMesh = thinLeafMesh();
  const surfaces: RenderSurface[] = [-0.32, 0].map((x, index) => {
    const matrix = identityMatrix();
    matrix[12] = x;
    const material = structuredClone(leaf);
    if (index === 1 && material.appearance?.response) material.appearance.response.thickness = 0.004;
    return {
      id: index ? "thick-leaf" : "thin-leaf",
      source: leaf.id,
      matrix,
      mesh: leafMesh,
      material: renderMaterial(material),
    };
  });
  const needles = identityMatrix();
  needles[12] = 0.33;
  needles[13] = 0.18;
  surfaces.push({
    id: "needle-shoot",
    source: "alpine-pine",
    matrix: needles,
    mesh: needleShootMesh(),
    material: renderMaterial(alpineConiferMaterials().needles),
  });
  const scene: EvaluatedScene = {
    surfaces,
    camera: { position: [0, 0.18, 1.65], target: [0, 0.18, 0], fov: 38 },
    environment: evaluateEnvironment(),
    mode: "beauty",
    time: 0,
    grid: false,
  };
  scene.environment.fogDensity = 0;
  scene.environment.ambient = 0.45;
  scene.environment.sunIntensity = 3;
  scene.environment.pointLights = [];
  const assertComplete = () => {
    if (!renderer.completeness.complete || errors.length)
      throw Error(`Incomplete foliage frame: ${errors.join("; ")}`);
  };
  async function meanLeaf() {
    renderer.render(scene);
    const pixels = await linearImage(renderer);
    assertComplete();
    scene.mode = "identity";
    renderer.render(scene);
    const mask = await linearImage(renderer);
    scene.mode = "beauty";
    const expected = identityColor("isolated-leaf"),
      sum: Vec3 = [0, 0, 0];
    let count = 0;
    for (let pixel = 0; pixel < pixels.length; pixel += 4) {
      if (expected.every((value, axis) => Math.abs(value - mask[pixel + axis]) < 0.002)) {
        for (let axis = 0; axis < 3; axis++) sum[axis] += pixels[pixel + axis];
        count++;
      }
    }
    if (count < 20) throw Error("Isolated leaf did not cover enough pixels for the occlusion probe");
    return { mean: sum.map((value) => value / count) as Vec3, pixels: count };
  }
  return {
    async capture(light: FoliageLighting, distance = 1.65, reverse = false) {
      scene.surfaces = surfaces;
      scene.mode = "beauty";
      scene.camera.position = [0, 0.18, reverse ? -distance : distance];
      scene.environment.sunDirection = directions[light];
      scene.environment.ambient = 0.45;
      renderer.render(scene);
      const image = await imageURL(await renderer.capture());
      assertComplete();
      return {
        image,
        light,
        distance,
        reverse,
        complete: renderer.completeness.complete,
        adapter: renderer.measurements.adapter,
        errors: [...errors],
        specimens: [
          { id: "thin-leaf", thickness: 0.00035 },
          { id: "thick-leaf", thickness: 0.004 },
          { id: "needle-shoot", geometry: "one shoot extracted from production conifer compiler" },
        ],
      };
    },
    async check() {
      const device = (renderer as unknown as { device: GPUDevice }).device;
      const closure = await probeFoliageResponse(device);
      const isolated: RenderSurface = {
        id: "isolated-leaf",
        source: leaf.id,
        matrix: identityMatrix(),
        mesh: thinLeafMesh(true),
        material: renderMaterial(leaf),
      };
      const blockerMatrix = identityMatrix();
      blockerMatrix[13] = 0.215;
      blockerMatrix[14] = -0.12;
      const blocker: RenderSurface = {
        id: "external-blocker",
        source: "external-blocker",
        matrix: blockerMatrix,
        mesh: {
          positions: new Float32Array([-0.25, -0.27, 0, 0.25, -0.27, 0, 0.25, 0.27, 0, -0.25, 0.27, 0]),
          normals: new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1]),
          indices: new Uint32Array([0, 1, 2, 0, 2, 3]),
          bounds: { min: [-0.25, -0.27, 0], max: [0.25, 0.27, 0] },
        },
        material: {
          color: [0.02, 0.02, 0.02],
          secondary: [0.02, 0.02, 0.02],
          roughness: 1,
          metallic: 0,
          pattern: 0,
          scale: 1,
          normalStrength: 0,
        },
      };
      scene.camera.position = [0, 0.18, 1.25];
      scene.environment.sunDirection = normalize([0, 0.3, -1]);
      scene.environment.ambient = 0;
      const sample = async (transmission: number, blocked: boolean) => {
        const source = structuredClone(leaf);
        if (source.appearance) source.appearance.transmission = transmission;
        isolated.material = renderMaterial(source);
        scene.surfaces = blocked ? [isolated, blocker] : [isolated];
        return meanLeaf();
      };
      const unblocked = await sample(0.8, false),
        opaque = await sample(0, false);
      const blocked = await sample(0.8, true),
        blockedOpaque = await sample(0, true);
      const transmitted = unblocked.mean.map((value, axis) => value - opaque.mean[axis]);
      const occluded = blocked.mean.map((value, axis) => Math.abs(value - blockedOpaque.mean[axis]));
      const backlightSignal = Math.max(...transmitted),
        shadowLeak = Math.max(...occluded);
      if (backlightSignal < 0.0001)
        throw Error(`Backlit thin leaf has no transmitted direct signal: ${backlightSignal}`);
      if (shadowLeak > backlightSignal * 0.02 + 0.00001)
        throw Error(`External blocker leaks transmission: ${shadowLeak} / ${backlightSignal}`);
      assertComplete();
      return {
        closure,
        backlightSignal,
        shadowLeak,
        isolatedPixels: unblocked.pixels,
        complete: renderer.completeness.complete,
        errors: [...errors],
        adapter: renderer.measurements.adapter,
        scope:
          "Single isolated leaf with opaque external blocker; excludes canopy multiple scattering, spectral fitting and complete BSDF energy conservation.",
      };
    },
    dispose() {
      renderer.dispose();
    },
  };
}
