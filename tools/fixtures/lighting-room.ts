import type { EvaluatedScene, RenderSurface, Vec3 } from "@wrela/model";
import { normalize } from "@wrela/model";
import { evaluateEnvironment } from "@wrela/runtime";
import { box } from "./indirect-scenes";

/** A furnished scale test, using identical plain PBR materials in both renderer
 * revisions so concurrent procedural-material edits cannot confound lighting. */
export function lightingRoomFixture(night = false): EvaluatedScene {
  const surfaces: RenderSurface[] = [];
  const add = (id: string, min: Vec3, max: Vec3, color: Vec3, roughness = 0.85, metallic = 0) => {
    const s = box(id, min, max, color);
    s.material.roughness = roughness;
    s.material.metallic = metallic;
    surfaces.push(s);
    return s;
  };
  const plaster: Vec3 = [0.65, 0.62, 0.55],
    wood: Vec3 = [0.26, 0.13, 0.06],
    metal: Vec3 = [0.14, 0.16, 0.17];
  add("floor", [-3, -0.2, -3], [3, 0, 3], [0.28, 0.25, 0.21], 0.65);
  add("ceiling", [-3, 3, -3], [3, 3.18, 3], plaster);
  add("left-wall", [-3.18, 0, -3], [-3, 3, 3], plaster);
  add("right-wall", [3, 0, -3], [3.18, 3, 3], [0.19, 0.3, 0.28]);
  add("back-wall", [-3, 0, -3.18], [3, 3, -3], plaster);
  add("front-left", [-3, 0, 3], [-1.8, 3, 3.18], plaster);
  add("front-right", [0.5, 0, 3], [3, 3, 3.18], plaster);
  add("window-sill", [-1.8, 0, 3], [0.5, 1.05, 3.18], plaster);
  add("window-header", [-1.8, 2.5, 3], [0.5, 3, 3.18], plaster);
  add("window-mullion", [-0.7, 1.05, 2.97], [-0.63, 2.5, 3.2], metal, 0.4, 0.8);
  add("rug", [-1.8, 0.015, -2.6], [1.7, 0.035, 0.8], [0.13, 0.2, 0.24]);
  add("table", [-1.35, 0.72, -1.6], [1.25, 0.83, -0.25], wood, 0.45);
  for (const x of [-1.18, 1.08])
    for (const z of [-1.43, -0.42])
      add(`table-leg-${x}-${z}`, [x, 0, z], [x + 0.07, 0.72, z + 0.07], metal, 0.32, 0.8);
  add("sofa-seat", [-1.5, 0.25, -2.75], [1.5, 0.52, -2.03], [0.27, 0.31, 0.3]);
  add("sofa-back", [-1.5, 0.48, -2.92], [1.5, 1.08, -2.7], [0.27, 0.31, 0.3]);
  for (const x of [-1.5, 1.3])
    add(`sofa-arm-${x}`, [x, 0.45, -2.75], [x + 0.2, 0.78, -2.02], [0.27, 0.31, 0.3]);
  add("book-a", [-0.75, 0.83, -1.05], [-0.1, 0.91, -0.65], [0.38, 0.09, 0.04]);
  add("book-b", [-0.68, 0.91, -1.1], [-0.03, 0.99, -0.7], [0.52, 0.44, 0.29]);
  add("metal-object", [0.45, 0.83, -1.22], [0.85, 1.23, -0.82], [0.5, 0.55, 0.58], 0.18, 1);
  add("shelf-left", [-2.84, 0, -2.5], [-2.68, 2.35, -0.7], wood);
  add("shelf-right", [-2.14, 0, -2.5], [-1.98, 2.35, -0.7], wood);
  for (let i = 0; i < 4; i++) {
    const y = 0.25 + i * 0.58;
    add(`shelf-${i}`, [-2.84, y, -2.5], [-1.98, y + 0.06, -0.7], wood, 0.55);
    add(`shelf-item-${i}`, [-2.65, y + 0.06, -2.05], [-2.24, y + 0.38, -1.72], [0.13 + i * 0.08, 0.22, 0.24]);
  }
  const emitter = add("pendant", [-0.75, 2.65, -1.3], [0.65, 2.72, -0.95], [0.85, 0.8, 0.65]);
  emitter.material.emission = { color: [1, 0.57, 0.24], intensity: night ? 2 : 0.5 };
  const environment = evaluateEnvironment();
  environment.cloudCover = 0;
  environment.fogDensity = 0;
  environment.sunDirection = normalize(night ? [0, -1, 0.2] : [-0.3, 0.45, 1]);
  environment.sunIntensity = night ? 0 : 2.5;
  environment.exposure = 1.1;
  environment.pointLights = [
    { position: [-0.05, 2.5, -1.05], color: [1, 0.65, 0.32], intensity: night ? 4 : 1, range: 7 },
    { position: [2.65, 0.75, -2.5], color: [0.22, 0.4, 1], intensity: night ? 2 : 0.4, range: 4 },
  ];
  return {
    surfaces,
    environment,
    camera: { position: [2.45, 1.6, 2.15], target: [-0.25, 1.05, -1.25], fov: 62 },
    time: 0,
    mode: "beauty",
    grid: false,
  };
}
