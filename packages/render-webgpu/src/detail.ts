import {
  dot,
  type EvaluatedScene,
  type MeshData,
  type RenderCompleteness,
  type RenderSurface,
  sub,
  type Vec3,
} from "@wrela/model";

import { cameraBasis } from "./math";
import { ShootSelector } from "./shoot-selection";
import { crownAdmissible, crownViewRanges } from "./vegetation-selection";
import { surfaceBounds } from "./visibility";

export function projectedDiameter(
  scene: EvaluatedScene,
  surface: RenderSurface,
  viewportHeight: number,
): number {
  const bounds = surfaceBounds(surface, scene.environment);
  const center = bounds.min.map((value, axis) => (value + bounds.max[axis]) * 0.5) as Vec3;
  const radius = Math.hypot(...bounds.max.map((value, axis) => (value - bounds.min[axis]) * 0.5));
  const distance = dot(sub(center, scene.camera.position), cameraBasis(scene.camera).forward) - radius;
  if (!(distance > 0) || !Number.isFinite(radius)) return Infinity;
  return (radius * viewportHeight) / (distance * Math.tan((scene.camera.fov * Math.PI) / 360));
}
/** Representation identity changes only outside a 15% projected-size hysteresis band. */
export class DetailSelector {
  private shoots = new ShootSelector();
  private choices = new Map<string, { base: MeshData; label: string | undefined; seen: number }>();
  private frame = 0;
  select(
    scene: EvaluatedScene,
    viewportHeight: number,
    pixelScale = 1,
    vegetation: { shadowTexelSize: number; aspect?: number; allowCandidates?: boolean } = {
      shadowTexelSize: 0,
    },
  ): { scene: EvaluatedScene; details: NonNullable<RenderCompleteness["details"]> } {
    this.frame++;
    this.shoots.begin();
    const details: NonNullable<RenderCompleteness["details"]> = [];
    const surfaces = scene.surfaces.flatMap((surface) => {
      if (surface.mesh.shoots) {
        const selected = this.shoots.select(
          scene,
          surface,
          viewportHeight / pixelScale,
          vegetation.shadowTexelSize,
          vegetation.aspect,
        );
        if (selected.some((s) => s.id.endsWith("/projected")))
          details.push({
            id: surface.id,
            label: "projected-shoot",
            selection: "projected-size",
            maxError: null,
          });
        return selected;
      }
      // Characters retain their authored realization until skin transfer and its error contract are available.
      if (surface.skin || surface.deformation || !surface.details?.length) return surface;
      const diameter = projectedDiameter(scene, surface, viewportHeight) / pixelScale;
      const previous = this.choices.get(surface.id);
      const current =
        previous?.base === surface.mesh
          ? surface.details.find((detail) => detail.label === previous.label)
          : undefined;
      const candidates = surface.details
        .filter(
          (detail) =>
            Number.isFinite(detail.maxProjectedDiameter) &&
            detail.maxProjectedDiameter > 0 &&
            crownAdmissible(
              scene,
              surface,
              detail,
              vegetation.shadowTexelSize,
              vegetation.allowCandidates ?? false,
            ),
        )
        .sort((a, b) => a.maxProjectedDiameter - b.maxProjectedDiameter);
      const selected = candidates.find(
        (detail) =>
          diameter <=
          Math.min(detail.maxProjectedDiameter, detail.vegetation?.qualification.maximumPixels ?? Infinity) *
            (detail === current && !detail.vegetation ? 1.15 : 0.85),
      );
      this.choices.set(surface.id, { base: surface.mesh, label: selected?.label, seen: this.frame });
      if (!selected) return surface;
      details.push({ id: surface.id, label: selected.label, selection: "projected-size", maxError: null });
      return {
        ...surface,
        mesh: selected.mesh,
        drawRange: selected.drawRange,
        ...(selected.vegetation ? crownViewRanges(scene, surface, selected) : {}),
        details: undefined,
        // Generated wind details and the original coarse source contain none of
        // the near mesh's geometric frequency allocation.
        ...(surface.reliefAppearance && !selected.mesh.reliefCoordinates
          ? {
              reliefAppearance: {
                ...surface.reliefAppearance,
                geometryWeights: [0, 0, 0] as Vec3,
                residualWeights: [1, 1, 1] as Vec3,
              },
            }
          : {}),
      };
    });
    this.shoots.end();
    for (const [id, choice] of this.choices) if (this.frame - choice.seen > 120) this.choices.delete(id);
    while (this.choices.size > 4096) {
      const oldest = this.choices.keys().next().value;
      if (oldest === undefined) break;
      this.choices.delete(oldest);
    }
    return { scene: { ...scene, surfaces }, details };
  }
  clear() {
    this.shoots.clear();
    this.choices.clear();
  }
}
