import type { MaterialDefinition } from "./documents";
import { add, cross, dot, normalize, scale, sub, type Vec3 } from "./math";
import { createSurfaceAppearance, createSurfaceLayer } from "./surface-appearance";
import {
  type SurfaceHistory,
  type SurfaceHistorySample,
  type SurfaceHistorySignals,
  sampleSurfaceHistory,
} from "./surface-history";

/** Bounded palette classes retain coherent causes without a material per masonry block. */
export type SurfaceHistoryClass = "contact" | "sheltered" | "runoff" | "exposed" | "broken";
export function surfaceHistoryClass(signals: SurfaceHistorySignals, broken = false): SurfaceHistoryClass {
  return broken
    ? "broken"
    : signals.contact > 0.48
      ? "contact"
      : signals.exposure < 0.12
        ? "sheltered"
        : signals.runoff > 0.16
          ? "runoff"
          : "exposed";
}
export function createSurfaceHistoryMaterial(
  base: MaterialDefinition,
  id: string,
  signals: SurfaceHistorySignals,
): MaterialDefinition {
  const material = structuredClone(base);
  material.id = id;
  material.name = `${base.name} · environmental history`;
  const appearance = material.appearance ?? createSurfaceAppearance();
  appearance.weathering = Math.max(appearance.weathering, signals.age * (0.1 + signals.exposure * 0.25));
  appearance.wetness = Math.max(appearance.wetness, signals.wetness * 0.72);
  appearance.dirt = Math.max(appearance.dirt, signals.dirt * 0.6);
  appearance.damage = Math.max(appearance.damage, signals.damage * 0.3);
  appearance.historyScale = 2.4;
  if (appearance.detail.kind === "mineral" && !appearance.relief)
    appearance.relief = {
      kind: "stone",
      amplitude: 0.004 + signals.damage * 0.008,
      scale: 0.13,
      seed: 73,
      targetEdgeLength: 0.035,
      direction: [0, 1, 0],
    };
  const stain = createSurfaceLayer(`${id}-deposition`);
  stain.name = "Runoff and sheltered mineral deposition";
  stain.color = [0.065, 0.062, 0.042];
  stain.coverage = Math.min(0.65, signals.runoff * 0.8 + signals.dirt * 0.4);
  stain.roughness = 0.91 - signals.wetness * 0.18;
  stain.mask = {
    ...stain.mask,
    kind: "combined",
    scale: 2.6,
    threshold: 0.47,
    softness: 0.23,
    slopeInfluence: 0.15,
    heightInfluence: 0,
  };
  const moss = createSurfaceLayer(`${id}-colonization`);
  moss.name = "Moisture-supported moss and lichen";
  moss.color = [0.045, 0.067, 0.026];
  moss.coverage = signals.moss * 0.75;
  moss.roughness = 0.94;
  moss.mask = { ...moss.mask, kind: "noise", scale: 4.3, threshold: 0.5, softness: 0.16 };
  appearance.layers = [...appearance.layers.slice(0, 2), stain, moss];
  material.appearance = appearance;
  return material;
}
/** Material prototypes are sampled in the same source frame as construction. */
export function createSurfaceHistoryPalette(
  materials: MaterialDefinition[],
  history: SurfaceHistory,
  prefix: string,
) {
  const samples: Record<SurfaceHistoryClass, SurfaceHistorySample> = {
    contact: { position: [0, history.groundHeight + 0.1, 0], normal: [0, 0, -1], shelter: 0.25 },
    sheltered: { position: [0, history.groundHeight + 1.4, 0], normal: [0, 0, 1], shelter: 0.9 },
    runoff: { position: [0.3, history.groundHeight + 1.2, 0], normal: [0, 0, -1], shelter: 0 },
    exposed: { position: [0, history.groundHeight + 2.8, 0], normal: [0, 1, 0], shelter: 0 },
    broken: { position: [0.8, history.groundHeight + 0.13, 0], normal: [0, 1, 0], shelter: 0.1 },
  };
  const up = normalize(history.up);
  const horizontal: Vec3 = Math.abs(up[0]) < 0.9 ? [1, 0, 0] : [0, 0, 1];
  const across = normalize(sub(horizontal, scale(up, dot(horizontal, up))));
  const forward = cross(across, up);
  const frame = (point: Vec3) =>
    add(add(scale(across, point[0]), scale(up, point[1])), scale(forward, point[2]));
  for (const sample of Object.values(samples)) {
    sample.position = add(history.origin, frame(sample.position));
    sample.normal = frame(sample.normal);
  }
  const documents: MaterialDefinition[] = [];
  const ids = new Map<string, string>();
  for (const material of materials)
    for (const [kind, sample] of Object.entries(samples)) {
      const id = `${prefix}-${material.id.replace(/^alpine-lookdev-/, "")}-${kind}`;
      const signals = sampleSurfaceHistory(history, sample);
      if (kind === "broken") signals.damage = Math.min(1, signals.damage + 0.35);
      documents.push(createSurfaceHistoryMaterial(material, id, signals));
      ids.set(`${material.id}:${kind}`, id);
    }
  return {
    documents,
    material: (base: string, kind: SurfaceHistoryClass) => ids.get(`${base}:${kind}`) ?? base,
  };
}
