import {
  type RenderMaterial,
  type SurfaceAppearance,
  type SurfaceMask,
  surfaceResponseDefaults,
  type Vec3,
} from "@wrela/model";

/** Existing explicit creature responses take precedence over general-purpose presets. */
export function applySurfaceAppearance(material: RenderMaterial): RenderMaterial {
  const appearance = material.appearance;
  if (!appearance) return material;
  const family = appearance.family;
  const authoredResponse = { ...surfaceResponseDefaults(family), ...appearance.response };
  const response =
    family === "skin"
      ? { family: "skin" as const, ...authoredResponse, transmission: appearance.transmission }
      : family === "foliage"
        ? {
            family: "skin" as const,
            ...authoredResponse,
            transmission: appearance.transmission,
          }
        : family === "fabric"
          ? {
              family: (authoredResponse.anisotropy ? "fiber" : "cloth") as "fiber" | "cloth",
              ...authoredResponse,
            }
          : appearance.response
            ? { family: "hard" as const, ...authoredResponse }
            : undefined;
  return {
    ...material,
    metallic: family === "metal" ? 1 : family === "glass" ? 0 : material.metallic,
    creature: material.creature ?? response,
  };
}
const smooth = (low: number, high: number, value: number) => {
  const t = Math.max(0, Math.min(1, (value - low) / Math.max(1e-6, high - low)));
  return t * t * (3 - 2 * t);
};
function noiseMask(mask: SurfaceMask, noise: number, footprint: number) {
  const low = mask.threshold - mask.softness;
  const high = mask.threshold + mask.softness;
  const integral = (x: number) => {
    const t = Math.max(0, Math.min(1, (x - low) / (high - low)));
    return (high - low) * (t ** 3 - 0.5 * t ** 4) + Math.max(x - high, 0);
  };
  const near = smooth(low, high, noise);
  const mean = integral(1) - integral(0);
  return near + (mean - near) * smooth(0.25, 0.85, footprint * mask.scale);
}
/** CPU audit evaluator shares the shader's mask contract; noise is supplied by the caller. */
export function evaluateSurfaceMask(
  mask: SurfaceMask,
  position: Vec3,
  normal: Vec3,
  noise = 0.5,
  footprint = 0,
): number {
  let value = 1;
  if (mask.kind === "noise") value = noiseMask(mask, noise, footprint);
  if (mask.kind === "slope")
    value = smooth(mask.threshold - mask.softness, mask.threshold + mask.softness, Math.max(0, normal[1]));
  if (mask.kind === "height")
    value =
      smooth(mask.minimumHeight - mask.softness, mask.minimumHeight, position[1]) *
      (1 - smooth(mask.maximumHeight, mask.maximumHeight + mask.softness, position[1]));
  if (mask.kind === "combined") {
    const noiseCoverage = noiseMask(mask, noise, footprint);
    const slopeThreshold = mask.slopeThreshold ?? 0.5;
    const slopeSoftness = mask.slopeSoftness ?? 0.15;
    const slope = smooth(
      slopeThreshold - slopeSoftness,
      slopeThreshold + slopeSoftness,
      Math.max(0, normal[1]),
    );
    const height =
      smooth(mask.minimumHeight - mask.softness, mask.minimumHeight, position[1]) *
      (1 - smooth(mask.maximumHeight, mask.maximumHeight + mask.softness, position[1]));
    value =
      noiseCoverage *
      (1 - (1 - slope) * (mask.slopeInfluence ?? 1)) *
      (1 - (1 - height) * (mask.heightInfluence ?? 1));
  }
  return mask.invert ? 1 - value : value;
}
export type SurfaceReviewPreset = {
  id: string;
  name: string;
  distance: number;
  sunDirection: Vec3;
  sunColor: Vec3;
  intensity: number;
  ambient: number;
};
/** Explicit lighting/distance combinations for comparable beauty and diagnostic captures. */
export const SURFACE_REVIEW_PRESETS: readonly SurfaceReviewPreset[] = [
  {
    id: "neutral",
    name: "Neutral daylight",
    distance: 3,
    sunDirection: [0.5, 0.8, 0.3],
    sunColor: [1, 0.96, 0.9],
    intensity: 2,
    ambient: 0.7,
  },
  {
    id: "grazing",
    name: "Grazing detail",
    distance: 1.5,
    sunDirection: [0.98, 0.08, 0.15],
    sunColor: [1, 0.9, 0.75],
    intensity: 3,
    ambient: 0.2,
  },
  {
    id: "backlit",
    name: "Low-sun transmission",
    distance: 3,
    sunDirection: [0, 0.2, -1],
    sunColor: [1, 0.85, 0.65],
    intensity: 3,
    ambient: 0.25,
  },
  {
    id: "distance",
    name: "Gameplay distance",
    distance: 12,
    sunDirection: [0.5, 0.8, 0.3],
    sunColor: [1, 0.96, 0.9],
    intensity: 2,
    ambient: 0.7,
  },
];
export function surfaceReviewWarnings(appearance: SurfaceAppearance): string[] {
  const warnings: string[] = [];
  if (appearance.family === "glass")
    warnings.push(
      "Glass uses tinted environment transmission; scene refraction and transparent sorting are not implemented.",
    );
  if (appearance.family === "skin" || appearance.family === "foliage")
    warnings.push("Transmission is a thin-surface approximation; verify the silhouette and backlighting.");
  if (appearance.historyScale > 20 || appearance.layers.some((layer) => layer.mask.scale > 20))
    warnings.push("Fine detail is filtered with distance; inspect at gameplay distance.");
  if (appearance.family === "fabric" && appearance.response?.anisotropy)
    warnings.push("Anisotropy follows the surface tangent; verify highlights on the final garment.");
  return warnings;
}
