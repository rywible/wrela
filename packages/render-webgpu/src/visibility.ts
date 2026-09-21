import type { Bounds, EvaluatedScene, RenderEnvironment, RenderSurface, Vec3 } from "@wrela/model";
import { normalizeWind, waterGeometryAttenuation } from "@wrela/model";

const emptyBounds = (): Bounds => ({
  min: [Infinity, Infinity, Infinity],
  max: [-Infinity, -Infinity, -Infinity],
});
function include(bounds: Bounds, point: Vec3) {
  for (let axis = 0; axis < 3; axis++) {
    bounds.min[axis] = Math.min(bounds.min[axis], point[axis]);
    bounds.max[axis] = Math.max(bounds.max[axis], point[axis]);
  }
}
function point(matrix: Float32Array, p: Vec3, offset = 0): Vec3 {
  return [0, 1, 2].map(
    (row) =>
      matrix[offset + row] * p[0] +
      matrix[offset + row + 4] * p[1] +
      matrix[offset + row + 8] * p[2] +
      matrix[offset + row + 12],
  ) as Vec3;
}
function corners(bounds: Bounds): Vec3[] {
  return Array.from({ length: 8 }, (_, i) => [
    bounds[i & 1 ? "max" : "min"][0],
    bounds[i & 2 ? "max" : "min"][1],
    bounds[i & 4 ? "max" : "min"][2],
  ]);
}
/** A union of transformed bind bounds contains every convex, normalized linear skin blend. */
export function surfaceBounds(surface: RenderSurface, environment: RenderEnvironment): Bounds {
  const bounds = emptyBounds();
  const bind = corners(surface.mesh.bounds);
  if (surface.skin) {
    for (let offset = 0; offset + 15 < surface.skin.matrices.length; offset += 16)
      for (const p of bind) include(bounds, point(surface.matrix, point(surface.skin.matrices, p, offset)));
  } else for (const p of bind) include(bounds, point(surface.matrix, p));
  if (surface.wind) {
    const wind = normalizeWind(environment.wind);
    const displacement =
      ((0.6 * Math.max(0, Math.min(surface.wind, 2)) * Math.min(Math.hypot(wind[0], wind[2]), 10)) / 10) *
      Math.hypot(surface.matrix[0], surface.matrix[1], surface.matrix[2]);
    for (const axis of [0, 2]) {
      bounds.min[axis] -= displacement;
      bounds.max[axis] += displacement;
    }
  }
  if (surface.water) {
    const amplitude = surface.water.waves.reduce(
      (sum, wave) =>
        sum +
        Math.abs(wave.amplitude) *
          waterGeometryAttenuation(wave.wavelength, surface.waterApproximation?.spacing ?? 0),
      0,
    );
    bounds.min[1] = surface.water.level - amplitude;
    bounds.max[1] = surface.water.level + amplitude;
  }
  return bounds;
}
/** WebGPU clip space uses -w<=x,y<=w and 0<=z<=w. Nonfinite bounds stay visible. */
export function intersectsFrustum(bounds: Bounds, matrix: Float32Array): boolean {
  if (![...bounds.min, ...bounds.max, ...matrix].every(Number.isFinite)) return true;
  const row = (index: number) => [matrix[index], matrix[index + 4], matrix[index + 8], matrix[index + 12]];
  const w = row(3),
    z = row(2);
  const planes = [z, w.map((value, i) => value - z[i])];
  for (const axis of [0, 1]) {
    const r = row(axis);
    planes.push(
      w.map((value, i) => value + r[i]),
      w.map((value, i) => value - r[i]),
    );
  }
  return planes.every(
    (plane) =>
      plane[3] +
        [0, 1, 2].reduce(
          (sum, axis) => sum + plane[axis] * bounds[plane[axis] >= 0 ? "max" : "min"][axis],
          0,
        ) >=
      -1e-5,
  );
}
export type SurfaceVisibility = { camera: boolean; shadow: boolean };
export function selectVisibility(
  scene: EvaluatedScene,
  camera: Float32Array,
  light: Float32Array,
): Map<RenderSurface, SurfaceVisibility> {
  return new Map(
    scene.surfaces.map((surface) => {
      const bounds = surfaceBounds(surface, scene.environment);
      return [
        surface,
        {
          camera: intersectsFrustum(bounds, camera),
          shadow: !surface.water && intersectsFrustum(bounds, light),
        },
      ];
    }),
  );
}
