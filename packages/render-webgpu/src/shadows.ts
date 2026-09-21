import { dot, type EvaluatedScene, normalize, type Vec3 } from "@wrela/model";
import { cameraBasis, lookAt, multiply, orthographic } from "./math";

export function shadowRadiusForScene(scene: EvaluatedScene, maximum: number): number {
  if (scene.shadowRadius !== undefined && Number.isFinite(scene.shadowRadius) && scene.shadowRadius > 0)
    return Math.max(1, Math.min(maximum, scene.shadowRadius));
  const distance = Math.hypot(
    ...scene.camera.position.map((value, axis) => value - scene.camera.target[axis]),
  );
  // Discrete coverage bands keep close subject views sharp without continuously rescaling the shadow map.
  return Math.min(maximum, 14 * 2 ** Math.ceil(Math.log2(Math.max(14, distance * 1.4) / 14)));
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
