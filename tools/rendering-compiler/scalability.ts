import type { EvaluatedScene, RenderSurface, Vec3 } from "@wrela/model";

import { transformMatrix } from "@wrela/model";

export type SceneWorkload = {
  name: string;
  foliage?: number;
  characters?: number;
  lights?: number;
  particles?: number;
  materialLayers?: boolean;
};
export const SCALABILITY_WORKLOADS: SceneWorkload[] = [
  { name: "baseline" },
  { name: "foliage-2x", foliage: 2 },
  { name: "foliage-4x", foliage: 4 },
  { name: "characters-4x", characters: 4 },
  { name: "characters-8x", characters: 8 },
  { name: "lights-4", lights: 4 },
  { name: "lights-8", lights: 8 },
  { name: "debris-512", particles: 512 },
  { name: "materials-2-layers", materialLayers: true },
  { name: "combined", foliage: 4, characters: 4, lights: 8, particles: 512, materialLayers: true },
];
const debrisMesh = {
  positions: new Float32Array([0, 1, 0, -0.7, -0.5, 0.5, 0.7, -0.5, 0.5, 0, -0.5, -0.7]),
  normals: new Float32Array([0, 1, 0, -0.7, -0.5, 0.5, 0.7, -0.5, 0.5, 0, -0.5, -0.7]),
  indices: new Uint32Array([0, 1, 2, 0, 2, 3, 0, 3, 1, 1, 3, 2]),
  bounds: { min: [-0.7, -0.5, -0.7] as Vec3, max: [0.7, 1, 0.5] as Vec3 },
};
/** Renderer stress packets retain the authored camera, terrain, animation and water.
 * Actor poses are shared snapshots: this intentionally measures rendering, not AI/physics. */
export function applyWorkload(scene: EvaluatedScene, workload: SceneWorkload): EvaluatedScene {
  for (const n of [workload.foliage ?? 1, workload.characters ?? 1])
    if (!Number.isInteger(n) || n < 1 || n > 8) throw Error("Instance multiplier must be in [1,8]");
  if (!Number.isInteger(workload.lights ?? 0) || (workload.lights ?? 0) < 0 || (workload.lights ?? 0) > 8)
    throw Error("Renderer supports at most eight point lights");
  if (
    !Number.isInteger(workload.particles ?? 0) ||
    (workload.particles ?? 0) < 0 ||
    (workload.particles ?? 0) > 4096
  )
    throw Error("Debris count must be in [0,4096]");
  const surfaces: RenderSurface[] = [];
  for (const source of scene.surfaces) {
    const count = source.skin ? (workload.characters ?? 1) : source.wind ? (workload.foliage ?? 1) : 1;
    for (let copy = 0; copy < count; copy++) {
      let surface = source;
      if (copy) {
        const matrix = source.matrix.slice();
        matrix[12] += (copy % 2 ? 1 : -1) * (0.8 + Math.floor(copy / 2) * 1.1);
        matrix[14] -= 0.5 + Math.floor(copy / 2) * 1.0;
        surface = {
          ...source,
          id: `${source.id}/stress-${copy}`,
          instanceId: `${source.instanceId ?? source.id}/stress-${copy}`,
          matrix,
        };
      }
      if (workload.materialLayers && !surface.water)
        surface = {
          ...surface,
          material: {
            ...surface.material,
            layers: [
              {
                color: [0.8, 0.86, 0.9],
                roughness: 0.8,
                metallic: 0,
                coverage: 0.25,
                slopeBias: 0.3,
                noiseScale: 2,
                normalStrength: 0.3,
              },
              {
                color: [0.19, 0.23, 0.16],
                roughness: 0.95,
                metallic: 0,
                coverage: 0.15,
                slopeBias: -0.3,
                noiseScale: 7,
                normalStrength: 0.15,
              },
            ],
          },
        };
      surfaces.push(surface);
    }
  }
  for (let i = 0; i < (workload.particles ?? 0); i++) {
    const phase = i * 2.399963;
    const radius = 1 + (i % 47) * 0.12;
    const position: Vec3 = [
      scene.camera.target[0] + Math.cos(phase + scene.time * 0.15) * radius,
      0.3 + ((i * 0.173 + scene.time * 0.3) % 5),
      scene.camera.target[2] + Math.sin(phase + scene.time * 0.15) * radius,
    ];
    surfaces.push({
      id: `stress-debris-${i}`,
      source: "stress-debris",
      instanceId: `stress-debris-${i}`,
      mesh: debrisMesh,
      matrix: transformMatrix(position, 0.035),
      material: {
        color: [0.8, 0.87, 0.95],
        secondary: [0.8, 0.87, 0.95],
        roughness: 0.8,
        metallic: 0,
        pattern: 0,
        scale: 1,
        normalStrength: 0,
      },
    });
  }
  return {
    ...scene,
    surfaces,
    environment: {
      ...scene.environment,
      ...(workload.lights === undefined
        ? {}
        : {
            pointLights: Array.from({ length: workload.lights }, (_, i) => ({
              position: [Math.cos(i * 2.4) * 5, 2, Math.sin(i * 2.4) * 5] as Vec3,
              color: [1, 0.75 + (i % 2) * 0.2, 0.55] as Vec3,
              intensity: 4,
            })),
          }),
    },
  };
}
export function workloadInventory(scene: EvaluatedScene) {
  return {
    surfaces: scene.surfaces.length,
    foliageSurfaces: scene.surfaces.filter((s) => s.wind).length,
    skinnedSurfaces: scene.surfaces.filter((s) => s.skin).length,
    pointLights: scene.environment.pointLights?.length ?? 0,
    particles: scene.surfaces.filter((s) => s.source === "stress-debris").length,
  };
}
