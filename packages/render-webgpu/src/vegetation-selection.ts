import {
  dot,
  type EvaluatedScene,
  inverseMatrix,
  type MeshDetail,
  normalize,
  type RenderSurface,
  sub,
  type Vec3,
} from "@wrela/model";

import { surfaceBounds } from "./visibility";

/** Each pass owns exactly one crown view. Never render/fade two overlapping
 * coverage fields: independent alpha fades do not conserve occupied area. */
export function crownViewRanges(scene: EvaluatedScene, surface: RenderSurface, detail: MeshDetail) {
  const product = detail.vegetation;
  const inverse = inverseMatrix(surface.matrix);
  if (!product || !inverse) return undefined;
  const bounds = surfaceBounds(surface, scene.environment);
  const center = bounds.min.map((value, axis) => (value + bounds.max[axis]) / 2) as Vec3;
  const local = (v: Vec3): Vec3 =>
    normalize([
      inverse[0] * v[0] + inverse[4] * v[1] + inverse[8] * v[2],
      inverse[1] * v[0] + inverse[5] * v[1] + inverse[9] * v[2],
      inverse[2] * v[0] + inverse[6] * v[1] + inverse[10] * v[2],
    ]);
  const nearest = (direction: Vec3) => {
    const vector = local(direction);
    let best = product.views[0],
      score = -Infinity;
    for (const view of product.views) {
      const candidate = dot(vector, view.direction);
      if (candidate > score) {
        best = view;
        score = candidate;
      }
    }
    return best ? { start: best.firstIndex, count: best.indexCount } : undefined;
  };
  return {
    drawRange: nearest(sub(scene.camera.position, center)),
    shadowDrawRange: nearest(scene.environment.sunDirection),
  };
}

export function crownAdmissible(
  scene: EvaluatedScene,
  surface: RenderSurface,
  detail: MeshDetail,
  shadowTexelSize: number,
  allowCandidates: boolean,
): boolean {
  const product = detail.vegetation;
  if (!product) return true;
  if (product.qualification.status !== "qualified" && !allowCandidates) return false;
  if (Math.hypot(...scene.environment.wind) * (surface.wind ?? 0) > product.qualification.maxWind)
    return false;
  if (!(shadowTexelSize > 0) || !Number.isFinite(shadowTexelSize)) return false;
  // Use the complete wind-expanded diameter. A small camera image cannot excuse
  // a resolved light-space silhouette, including off-camera shadow casters.
  const bounds = surfaceBounds(surface, scene.environment);
  const diameter = Math.hypot(...bounds.max.map((value, axis) => value - bounds.min[axis]));
  if (diameter / shadowTexelSize > product.qualification.maximumPixels) return false;
  // Nonuniform/sheared transforms change the sampled angular domain.
  const m = surface.matrix;
  const axes: Vec3[] = [
    [m[0], m[1], m[2]],
    [m[4], m[5], m[6]],
    [m[8], m[9], m[10]],
  ];
  const lengths = axes.map((v) => Math.hypot(...v));
  if (Math.min(...lengths) < 1e-8 || Math.max(...lengths) / Math.min(...lengths) > 1.001) return false;
  if (
    axes.some((axis, i) =>
      axes.some((other, j) => i !== j && Math.abs(dot(axis, other)) > lengths[i] * lengths[j] * 1e-4),
    )
  )
    return false;
  return true;
}
