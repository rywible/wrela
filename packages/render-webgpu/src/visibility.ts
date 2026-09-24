import type { Bounds, EvaluatedScene, RenderEnvironment, RenderSurface, Vec3 } from "@wrela/model";

import {
  normalizeWind,
  quadricUnitToLocal,
  vegetationMotionEnvelope,
  waterGeometryAttenuation,
} from "@wrela/model";

import {
  analyticInnerSphere,
  hiddenFromPreparedDirection,
  hiddenFromPreparedPoint,
  type InnerSphere,
  prepareDirectionalOccluder,
  preparePointOccluder,
  sphereInsideDepth,
  type VisibilityRejection,
} from "./opaque-visibility";

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
// Binding arrays are immutable geometry products (as are the uploaded vertex
// buffers). Cache their validation, while checking each current pose palette.
const bindingValidation = new WeakMap<
  Float32Array,
  { indices: Uint16Array; paletteLength: number; error: number | null }
>();
function bindingWeightError(skin: NonNullable<RenderSurface["skin"]>): number | null {
  const { weights, jointIndices, matrices } = skin;
  const cached = bindingValidation.get(weights);
  if (cached?.indices === jointIndices && cached.paletteLength === matrices.length) return cached.error;
  let error: number | null = 0;
  if (
    matrices.length === 0 ||
    matrices.length % 16 ||
    weights.length === 0 ||
    weights.length !== jointIndices.length ||
    weights.some((value) => !Number.isFinite(value) || value < 0) ||
    jointIndices.some((index) => index * 16 + 15 >= matrices.length)
  )
    error = null;
  if (error !== null)
    for (let offset = 0; offset < weights.length; offset += 4) {
      let total = 0;
      for (let i = offset; i < Math.min(weights.length, offset + 4); i++) total += weights[i];
      if (Math.abs(total - 1) > 1e-5) {
        error = null;
        break;
      }
      error = Math.max(error, Math.abs(total - 1));
    }
  bindingValidation.set(weights, { indices: jointIndices, paletteLength: matrices.length, error });
  return error;
}
/** A union of transformed bind bounds contains every convex, normalized linear skin blend. */
export function surfaceBounds(surface: RenderSurface, environment: RenderEnvironment): Bounds {
  const bounds = emptyBounds();
  const product = surface.selectedRenderProduct;
  if (!surface.skin && product?.kind !== "analytic-quadric") {
    // An affine box transform needs its center and absolute linear extents;
    // eight corner arrays and nested mapping callbacks are unnecessary.
    const min = surface.mesh.bounds.min,
      max = surface.mesh.bounds.max,
      m = surface.matrix;
    const displacement = surface.deformation?.maxDisplacement ?? 0;
    const cx = (min[0] + max[0]) * 0.5,
      cy = (min[1] + max[1]) * 0.5,
      cz = (min[2] + max[2]) * 0.5;
    const ex = (max[0] - min[0]) * 0.5 + displacement;
    const ey = (max[1] - min[1]) * 0.5 + displacement;
    const ez = (max[2] - min[2]) * 0.5 + displacement;
    for (let row = 0; row < 3; row++) {
      const center = m[row] * cx + m[row + 4] * cy + m[row + 8] * cz + m[row + 12];
      const extent = Math.abs(m[row]) * ex + Math.abs(m[row + 4]) * ey + Math.abs(m[row + 8]) * ez;
      bounds.min[row] = center - extent;
      bounds.max[row] = center + extent;
    }
  } else {
    // An analytic silhouette may extend beyond the extracted mesh's vertex box.
    const bind =
      product?.kind === "analytic-quadric"
        ? corners({ min: [-1, -1, -1], max: [1, 1, 1] }).map((p) =>
            point(quadricUnitToLocal(product.primitive), p),
          )
        : corners({
            min: surface.mesh.bounds.min.map(
              (value) => value - (surface.deformation?.maxDisplacement ?? 0),
            ) as Vec3,
            max: surface.mesh.bounds.max.map(
              (value) => value + (surface.deformation?.maxDisplacement ?? 0),
            ) as Vec3,
          });
    if (surface.skin) {
      const weightError = bindingWeightError(surface.skin);
      if (weightError === null) return emptyBounds();
      for (let offset = 0; offset + 15 < surface.skin.matrices.length; offset += 16)
        for (const p of bind) include(bounds, point(surface.matrix, point(surface.skin.matrices, p, offset)));
      for (let axis = 0; axis < 3; axis++) {
        const padding =
          (Math.max(Math.abs(bounds.min[axis]), Math.abs(bounds.max[axis])) +
            Math.abs(surface.matrix[axis + 12])) *
          (weightError + 1e-6);
        bounds.min[axis] -= padding;
        bounds.max[axis] += padding;
      }
    } else for (const p of bind) include(bounds, point(surface.matrix, p));
  }
  if (surface.wind) {
    const wind = normalizeWind(environment.wind);
    const strength =
      ((Math.max(0, Math.min(surface.wind, 2)) * Math.min(Math.hypot(wind[0], wind[2]), 10)) / 10) *
      Math.hypot(surface.matrix[0], surface.matrix[1], surface.matrix[2]);
    const branchDisplacement =
      (surface.mesh.shoots ? 0.285 : cachedVegetationEnvelope(surface.mesh.wind)) * strength;
    for (const axis of [0, 1, 2]) {
      const displacement = branchDisplacement + (axis === 1 ? 0 : 0.6 * strength);
      bounds.min[axis] -= displacement;
      bounds.max[axis] += displacement;
    }
  }
  if (surface.waterState) {
    const state = surface.waterState,
      amplitude = state.spectrum.amplitudeBound;
    bounds.min[1] = state.minLevel + surface.matrix[13] - amplitude;
    bounds.max[1] = state.maxLevel + surface.matrix[13] + amplitude;
    for (const axis of [0, 2]) {
      bounds.min[axis] -= amplitude;
      bounds.max[axis] += amplitude;
    }
  } else if (surface.water) {
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

// Motion attributes are immutable compiler products. Cache once per buffer, not per draw/frame.
const vegetationEnvelopes = new WeakMap<Float32Array, number>();
function cachedVegetationEnvelope(wind: Float32Array | undefined): number {
  if (!wind) return 0;
  let amplitude = vegetationEnvelopes.get(wind);
  if (amplitude === undefined) {
    amplitude = vegetationMotionEnvelope(wind) / 2;
    vegetationEnvelopes.set(wind, amplitude);
  }
  return amplitude;
}
/** WebGPU clip space uses -w<=x,y<=w and 0<=z<=w. Nonfinite bounds stay visible. */
export function intersectsFrustum(bounds: Bounds, matrix: Float32Array): boolean {
  if (matrix.length !== 16) return true;
  for (let i = 0; i < 16; i++) if (!Number.isFinite(matrix[i])) return true;
  for (let i = 0; i < 3; i++)
    if (!Number.isFinite(bounds.min[i]) || !Number.isFinite(bounds.max[i]) || bounds.min[i] > bounds.max[i])
      return true;
  for (let plane = 0; plane < 6; plane++) {
    const row = plane < 2 ? 2 : plane < 4 ? 0 : 1;
    const sign = plane % 2 ? -1 : 1;
    const w = plane === 0 ? 0 : 1;
    const x = w * matrix[3] + sign * matrix[row];
    const y = w * matrix[7] + sign * matrix[row + 4];
    const z = w * matrix[11] + sign * matrix[row + 8];
    const d = w * matrix[15] + sign * matrix[row + 12];
    if (
      d +
        x * bounds[x >= 0 ? "max" : "min"][0] +
        y * bounds[y >= 0 ? "max" : "min"][1] +
        z * bounds[z >= 0 ? "max" : "min"][2] <
      -1e-5
    )
      return false;
  }
  return true;
}
export type SurfaceVisibility = { camera: boolean; shadow: boolean };
/** Ranking drops low-value occluders only; omitted proxies never remove candidates. */
export function projectedOccluderCoverage(sphere: InnerSphere, m: Float32Array): number {
  if (!sphereInsideDepth(sphere, m)) return 0;
  const [x, y, z] = sphere.center;
  const w = m[3] * x + m[7] * y + m[11] * z + m[15],
    rw = sphere.radius * Math.hypot(m[3], m[7], m[11]);
  if (!(w > rw)) return 0;
  let area = 1;
  for (let axis = 0; axis < 2; axis++) {
    const center = m[axis] * x + m[axis + 4] * y + m[axis + 8] * z + m[axis + 12],
      radius = sphere.radius * Math.hypot(m[axis], m[axis + 4], m[axis + 8]);
    const values = [
      (center - radius) / (w - rw),
      (center - radius) / (w + rw),
      (center + radius) / (w - rw),
      (center + radius) / (w + rw),
    ];
    area *= Math.max(0, Math.min(1, Math.max(...values)) - Math.max(-1, Math.min(...values)));
  }
  return area;
}
export const MAX_SEMANTIC_OCCLUDERS = 8;
export const MIN_SEMANTIC_OCCLUDER_COVERAGE = 0.0025;
export function selectVisibility(
  scene: EvaluatedScene,
  camera: Float32Array,
  light: Float32Array,
  options: {
    enabled?: boolean;
    onReject?: (surface: RenderSurface, reason: VisibilityRejection) => void;
  } = {},
): Map<RenderSurface, SurfaceVisibility> {
  const proxies = options.enabled
    ? scene.surfaces.flatMap((surface) => {
        const sphere = analyticInnerSphere(surface, (reason) => options.onReject?.(surface, reason));
        return sphere ? [{ surface, sphere }] : [];
      })
    : [];
  const rank = (matrix: Float32Array) =>
    proxies
      .map((proxy) => ({ ...proxy, area: projectedOccluderCoverage(proxy.sphere, matrix) }))
      .filter((proxy) => proxy.area >= MIN_SEMANTIC_OCCLUDER_COVERAGE)
      .sort((a, b) => b.area - a.area)
      .slice(0, MAX_SEMANTIC_OCCLUDERS);
  const cameraProxies = rank(camera).flatMap((proxy) => {
    const query = preparePointOccluder(proxy.sphere, scene.camera.position, camera);
    return query ? [{ surface: proxy.surface, query }] : [];
  });
  const lightProxies = rank(light).flatMap((proxy) => {
    const query = prepareDirectionalOccluder(proxy.sphere, scene.environment.sunDirection);
    return query ? [{ surface: proxy.surface, query }] : [];
  });
  const result = new Map<RenderSurface, SurfaceVisibility>();
  for (const surface of scene.surfaces) {
    const bounds = surfaceBounds(surface, scene.environment);
    result.set(surface, {
      camera:
        intersectsFrustum(bounds, camera) &&
        !cameraProxies.some(
          (proxy) => proxy.surface !== surface && hiddenFromPreparedPoint(bounds, proxy.query),
        ),
      shadow:
        !surface.water &&
        intersectsFrustum(bounds, light) &&
        !lightProxies.some(
          (proxy) => proxy.surface !== surface && hiddenFromPreparedDirection(bounds, proxy.query),
        ),
    });
  }
  return result;
}
