import type { EvaluatedScene, Vec3 } from "@wrela/model";

import {
  extractFiniteSunSphere,
  type FiniteSunSphere,
  MAX_FINITE_SUN_SPHERES,
  packFiniteSunSpheres,
} from "./finite-sun";

export type AtmosphereRuntimeTable = {
  key: string;
  data: Float32Array;
  byteLength: number;
  /** World position of the planet center, in metres. Tables use kilometres. */
  planetCenter: Vec3;
};

/** An exact-sphere control is eligible only if no other shadow blocker is lost.
 * A horizontal floor can be omitted when the whole sun and all spheres lie above it. */
export function finiteSunScene(
  scene: EvaluatedScene,
  angularRadius: number,
): {
  data: Float32Array;
  enabled: boolean;
  reason: string;
} {
  const fallback = (reason: string) => ({ data: packFiniteSunSpheres([]), enabled: false, reason });
  if (!Number.isFinite(angularRadius) || angularRadius < 0.0001 || angularRadius > 0.1)
    return fallback("sun radius outside validated kernel domain");
  const sunLength = Math.hypot(...scene.environment.sunDirection);
  if (!(sunLength > 0) || scene.environment.sunDirection[1] / sunLength <= Math.sin(angularRadius) + 0.00001)
    return fallback("sun intersects ground horizon");
  // Prove this common fallback before visiting any large terrain vertex buffers.
  if (scene.surfaces.some((surface) => !surface.water && (surface.skin || surface.wind)))
    return fallback("deformed blocker requires ordinary shadows");
  const spheres: FiniteSunSphere[] = [];
  const floors: number[] = [];
  for (const surface of scene.surfaces) {
    if (surface.water) continue; // The existing water path never casts directional shadows.
    if (surface.skin || surface.wind) return fallback("deformed blocker requires ordinary shadows");
    if (
      surface.drawRange &&
      (surface.drawRange.start !== 0 || surface.drawRange.count !== surface.mesh.indices.length)
    )
      return fallback("partial blocker requires ordinary shadows");
    const product = surface.selectedRenderProduct;
    if (product?.kind === "analytic-quadric") {
      const sphere = extractFiniteSunSphere(surface);
      if (!sphere) return fallback("ellipsoid or nonrigid sphere requires ordinary shadows");
      spheres.push(sphere);
      if (spheres.length > MAX_FINITE_SUN_SPHERES) return fallback("sphere blocker budget exceeded");
    } else {
      const p = surface.mesh.positions,
        m = surface.matrix;
      if (!p.length || !Array.from(m).every(Number.isFinite)) return fallback("invalid blocker geometry");
      let height: number | undefined;
      for (let i = 0; i < p.length; i += 3) {
        const y = m[1] * p[i] + m[5] * p[i + 1] + m[9] * p[i + 2] + m[13];
        if (!Number.isFinite(y)) return fallback("invalid blocker geometry");
        height ??= y;
        if (y !== height) return fallback("nonspherical blocker requires ordinary shadows");
      }
      floors.push(height ?? 0);
    }
  }
  if (
    floors.some((height) => height !== floors[0]) ||
    floors.some((height) => spheres.some((sphere) => sphere.center[1] - sphere.radius < height))
  )
    return fallback("ground can shadow another receiver");
  return {
    data: packFiniteSunSpheres(spheres),
    enabled: true,
    reason: "exact sphere diffuse; overlapping lenses and self intersections use PCF",
  };
}

export function validateAtmosphereTable(table: AtmosphereRuntimeTable, maxBytes: number): void {
  const data = table.data;
  if (
    !table.key ||
    !(data instanceof Float32Array) ||
    data.byteLength !== table.byteLength ||
    data.byteLength > maxBytes ||
    data.length < 24 ||
    data.length % 4 ||
    !table.planetCenter.every(Number.isFinite) ||
    !Array.from(data).every(Number.isFinite)
  )
    throw new RangeError("Invalid atmosphere runtime table");
  const [radius, top, rayleighScale, aerosolScale] = data;
  const [rows, columns, version, mistScale] = data.slice(12, 16);
  if (
    !(radius >= 1 && top > 0 && rayleighScale > 0 && aerosolScale > 0 && mistScale > 0) ||
    (version !== 2 && version !== 3) ||
    !Number.isInteger(rows) ||
    rows < 2 ||
    rows > 64 ||
    !Number.isInteger(columns) ||
    columns < 2 ||
    columns > 128 ||
    data.length !== ((version === 3 ? 6 : 4) + rows * columns) * 4 ||
    data.slice(4, 10).some((value) => value < 0 || value > 1000) ||
    Math.abs(data[10]) > 0.95 ||
    data[11] < 0.0001 ||
    data[11] > 0.1
  )
    throw new RangeError("Unsupported scattering atmosphere runtime table domain");
  const tail = (4 + rows * columns) * 4;
  for (let i = 16; i < tail; i += 4)
    if (
      data[i] < 0 ||
      data[i + 1] < 0 ||
      data[i + 2] < 0 ||
      data[i + 3] < 0 ||
      (version === 2 && data[i + 3] !== 0)
    )
      throw new RangeError("Invalid atmosphere density-integral cell");
  if (
    version === 3 &&
    (data.slice(tail, tail + 3).some((value) => value < 0 || value > 1000) ||
      data[tail + 3] !== 0 ||
      data[tail + 7] !== 0 ||
      data[tail + 4] < 0 ||
      data[tail + 5] <= data[tail + 4] ||
      data[tail + 6] <= data[tail + 5] ||
      data[tail + 6] > top)
  )
    throw new RangeError("Invalid atmosphere ozone layer");
}
