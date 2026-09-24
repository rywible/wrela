import type { MaterialDefinition } from "./documents";
import { createSurfaceAppearance } from "./surface-appearance";

/** Art-directed linear-albedo starting points for the alpine conifer lookdev specimen.
 * These are restrained procedural presets, not measured spectral coefficients. Geometry
 * carries the needle silhouettes, age variation and bark ridges; materials avoid painting
 * large cloudy patches or circumferential stripes over those structures. */
export function alpineConiferMaterials(): { needles: MaterialDefinition; bark: MaterialDefinition } {
  const foliage = createSurfaceAppearance("foliage");
  foliage.transmission = 0.12;
  foliage.response = { subsurface: 0.05, thickness: 0.0008, scatterColor: [0.48, 0.72, 0.38] };
  const bark = createSurfaceAppearance();
  bark.detail = { kind: "bark", scale: 1, strength: 0.8 };
  return {
    needles: {
      id: "alpine-lookdev-needles",
      name: "Alpine conifer needles",
      schemaVersion: 1,
      dependencies: [],
      kind: "material",
      color: [0.042, 0.073, 0.043],
      secondary: [0.077, 0.122, 0.065],
      roughness: 0.8,
      metallic: 0,
      pattern: "noise",
      scale: 28,
      normalStrength: 0.025,
      domain: "local",
      appearance: foliage,
    },
    bark: {
      id: "alpine-lookdev-bark",
      name: "Alpine conifer bark",
      schemaVersion: 1,
      dependencies: [],
      kind: "material",
      color: [0.048, 0.035, 0.025],
      secondary: [0.14, 0.112, 0.082],
      roughness: 0.96,
      metallic: 0,
      pattern: "noise",
      scale: 8,
      normalStrength: 0,
      appearance: bark,
      domain: "local",
    },
  };
}

/** Paper-birch surface starting points; annual shoots retain their warm vertex tint. */
export function paperBirchMaterials(): { leaves: MaterialDefinition; bark: MaterialDefinition } {
  const source = alpineConiferMaterials();
  source.bark.id = "paper-birch-bark";
  source.bark.name = "Paper birch bark";
  source.bark.color = [0.07, 0.057, 0.041];
  source.bark.secondary = [0.72, 0.71, 0.65];
  if (source.bark.appearance) source.bark.appearance.detail.kind = "birch-bark";
  source.needles.id = "paper-birch-leaves";
  source.needles.name = "Paper birch leaves";
  source.needles.color = [0.047, 0.13, 0.024];
  source.needles.secondary = [0.09, 0.18, 0.044];
  if (source.needles.appearance) source.needles.appearance.transmission = 0.38;
  return { leaves: source.needles, bark: source.bark };
}
