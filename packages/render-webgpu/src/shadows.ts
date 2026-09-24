import { dot, type EvaluatedScene, normalize, type Vec3 } from "@wrela/model";

import { cameraBasis, lookAt, multiply, orthographic } from "./math";

export function shadowRadiusForScene(scene: EvaluatedScene, maximum: number): number {
  if (scene.shadowRadius !== undefined && Number.isFinite(scene.shadowRadius) && scene.shadowRadius > 0)
    return Math.max(1, Math.min(maximum, scene.shadowRadius));
  const distance = Math.hypot(
    ...scene.camera.position.map((value, axis) => value - scene.camera.target[axis]),
  );
  // Discrete coverage bands keep close subject views sharp without continuously rescaling the shadow map.
  const radius = Math.min(maximum, 14 * 2 ** Math.ceil(Math.log2(Math.max(14, distance * 1.4) / 14)));
  // A small, fully bounded scene need not spend most shadow texels on empty
  // space. Certify every caster in all three light axes; retain the world band
  // when animated/displaced bounds or a large population make that uncertain.
  if (radius > 14 || !scene.surfaces.length || scene.surfaces.length > 64) return radius;
  const light = cameraBasis({
    position: normalize(scene.environment.sunDirection),
    target: [0, 0, 0],
    fov: 45,
  });
  let extent = 0;
  for (const s of scene.surfaces) {
    if (s.castsShadow === false) continue;
    if (s.skin || s.deformation || s.wind || s.mesh.wind || s.mesh.shoots || s.water || s.reliefAppearance)
      return radius;
    const b = s.mesh.bounds,
      m = s.matrix;
    if (
      !m.every(Number.isFinite) ||
      ![...b.min, ...b.max].every(Number.isFinite) ||
      b.min.some((v, a) => v > b.max[a])
    )
      return radius;
    for (let corner = 0; corner < 8; corner++) {
      const local = [0, 1, 2].map((a) => (corner & (1 << a) ? b.max[a] : b.min[a]));
      const p = [0, 1, 2].map(
        (a) =>
          m[a] * local[0] + m[4 + a] * local[1] + m[8 + a] * local[2] + m[12 + a] - scene.camera.position[a],
      ) as Vec3;
      for (const axis of [light.right, light.up, light.forward])
        extent = Math.max(extent, Math.abs(dot(p, axis)));
    }
  }
  // The forward center shift consumes at most 35% of each axis' radius.
  const fitted = 1.75 * 2 ** Math.ceil(Math.log2(Math.max(1.75, (extent + 0.1) / 0.65) / 1.75));
  return Math.min(radius, fitted);
}

/** Fixed world-space coverage and texel-snapped translation prevent orbit-distance shadow shimmer. */
export function directionalShadow(scene: EvaluatedScene, radius: number, resolution: number) {
  const sun = normalize(scene.environment.sunDirection);
  const forward = cameraBasis(scene.camera).forward;
  const center = scene.camera.position.map((value, axis) => value + forward[axis] * radius * 0.35) as Vec3;
  const basis = cameraBasis({ position: sun, target: [0, 0, 0], fov: 45 });
  const absolute = center.map((value, axis) => value + (scene.origin?.[axis] ?? 0)) as Vec3;
  const worldTexel = (2 * radius) / resolution;
  for (const axis of [basis.right, basis.up]) {
    const coordinate = dot(absolute, axis);
    const shift = Math.round(coordinate / worldTexel) * worldTexel - coordinate;
    for (let i = 0; i < 3; i++) center[i] += axis[i] * shift;
  }
  const eye = center.map((value, axis) => value + sun[axis] * radius * 2) as Vec3;
  return {
    matrix: multiply(orthographic(radius, 0.1, radius * 5), lookAt(eye, center)),
    worldTexel,
    inverseDepthRange: 1 / (radius * 5 - 0.1),
  };
}
