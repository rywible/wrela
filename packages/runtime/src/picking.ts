import { queryWater } from "@wrela/compiler";
import {
  add,
  type Bounds,
  type Camera,
  cross,
  dot,
  type EvaluatedScene,
  type MeshData,
  normalize,
  normalizeWind,
  type RenderSurface,
  scale,
  sub,
  type Vec3,
  waterGeometryAttenuation,
} from "@wrela/model";
export type Ray = { origin: Vec3; direction: Vec3 };
export type MeshHit = {
  distance: number;
  position: Vec3;
  triangle: number;
  barycentric: Vec3;
  nodeId?: string;
};
export type SceneHit = MeshHit & { surfaceId: string; documentId: string; instanceId?: string };
export function cameraRay(camera: Camera, x: number, y: number, width: number, height: number): Ray {
  if (!(width > 0 && height > 0)) throw new Error("Picking requires a positive viewport size");
  const forward = normalize(sub(camera.target, camera.position));
  const right = normalize(cross(forward, Math.abs(forward[1]) > 0.999 ? [0, 0, 1] : [0, 1, 0]));
  const up = cross(right, forward),
    tangent = Math.tan((camera.fov * Math.PI) / 360);
  return {
    origin: [...camera.position],
    direction: normalize(
      add(
        forward,
        add(
          scale(right, ((((x / width) * 2 - 1) * width) / height) * tangent),
          scale(up, (1 - (y / height) * 2) * tangent),
        ),
      ),
    ),
  };
}
function intersectsBounds(ray: Ray, bounds: Bounds, maximum: number): boolean {
  let near = 0,
    far = maximum;
  for (let axis = 0; axis < 3; axis++) {
    if (Math.abs(ray.direction[axis]) < 1e-12) {
      if (ray.origin[axis] < bounds.min[axis] || ray.origin[axis] > bounds.max[axis]) return false;
      continue;
    }
    let a = (bounds.min[axis] - ray.origin[axis]) / ray.direction[axis],
      b = (bounds.max[axis] - ray.origin[axis]) / ray.direction[axis];
    if (a > b) [a, b] = [b, a];
    near = Math.max(near, a);
    far = Math.min(far, b);
    if (near > far) return false;
  }
  return true;
}
/** Two-sided triangle picking on the current realized surface. Source identity
 * is selected by barycentric support, rather than exposing an arbitrary index. */
export function raycastMesh(
  mesh: MeshData,
  ray: Ray,
  maximum = Infinity,
  vertex?: (index: number) => Vec3,
  drawRange?: { start: number; count: number },
): MeshHit | null {
  const direction = normalize(ray.direction);
  if (dot(direction, direction) < 0.5) return null;
  if (!vertex && !intersectsBounds({ ...ray, direction }, mesh.bounds, maximum)) return null;
  const point =
    vertex ??
    ((index: number): Vec3 => [
      mesh.positions[index * 3],
      mesh.positions[index * 3 + 1],
      mesh.positions[index * 3 + 2],
    ]);
  let hit: MeshHit | null = null,
    closest = maximum;
  for (
    let index = drawRange?.start ?? 0;
    index <
    Math.min(mesh.indices.length, (drawRange?.start ?? 0) + (drawRange?.count ?? mesh.indices.length));
    index += 3
  ) {
    const ia = mesh.indices[index],
      ib = mesh.indices[index + 1],
      ic = mesh.indices[index + 2];
    const a = point(ia),
      b = point(ib),
      c = point(ic),
      edge1 = sub(b, a),
      edge2 = sub(c, a),
      p = cross(direction, edge2),
      det = dot(edge1, p);
    if (Math.abs(det) < 1e-10) continue;
    const inverse = 1 / det,
      offset = sub(ray.origin, a),
      u = dot(offset, p) * inverse;
    if (u < -1e-8 || u > 1 + 1e-8) continue;
    const q = cross(offset, edge1),
      v = dot(direction, q) * inverse;
    if (v < -1e-8 || u + v > 1 + 1e-8) continue;
    const distance = dot(edge2, q) * inverse;
    if (distance < 0 || distance >= closest) continue;
    closest = distance;
    const barycentric: Vec3 = [1 - u - v, u, v],
      votes = new Map<string, number>();
    [ia, ib, ic].forEach((id, corner) => {
      const source = mesh.sourceIds?.[id];
      if (source) votes.set(source, (votes.get(source) ?? 0) + barycentric[corner]);
    });
    const nodeId = [...votes].sort((a, b) => b[1] - a[1])[0]?.[0];
    hit = {
      distance,
      position: add(ray.origin, scale(direction, distance)),
      triangle: index / 3,
      barycentric,
      nodeId,
    };
  }
  return hit;
}
function transform(matrix: Float32Array, point: Vec3, offset = 0): Vec3 {
  const [x, y, z] = point;
  return [
    matrix[offset] * x + matrix[offset + 4] * y + matrix[offset + 8] * z + matrix[offset + 12],
    matrix[offset + 1] * x + matrix[offset + 5] * y + matrix[offset + 9] * z + matrix[offset + 13],
    matrix[offset + 2] * x + matrix[offset + 6] * y + matrix[offset + 10] * z + matrix[offset + 14],
  ];
}
function worldBounds(surface: RenderSurface): Bounds {
  const result: Bounds = { min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] };
  for (let corner = 0; corner < 8; corner++) {
    const p = transform(surface.matrix, [
      surface.mesh.bounds[corner & 1 ? "max" : "min"][0],
      surface.mesh.bounds[corner & 2 ? "max" : "min"][1],
      surface.mesh.bounds[corner & 4 ? "max" : "min"][2],
    ]);
    for (let axis = 0; axis < 3; axis++) {
      result.min[axis] = Math.min(result.min[axis], p[axis]);
      result.max[axis] = Math.max(result.max[axis], p[axis]);
    }
  }
  return result;
}
/** Matches the renderer's skin, bounded wind and analytic wave positions.
 * Operates on a click, never as a per-frame scene traversal. */
export function pickScene(scene: EvaluatedScene, ray: Ray): SceneHit | null {
  let result: SceneHit | null = null;
  const normalized = { origin: ray.origin, direction: normalize(ray.direction) };
  for (const surface of scene.surfaces) {
    if (
      !surface.skin &&
      !surface.wind &&
      !surface.water &&
      !intersectsBounds(normalized, worldBounds(surface), result?.distance ?? Infinity)
    )
      continue;
    const cached = new Map<number, Vec3>();
    const vertex = (index: number): Vec3 => {
      const existing = cached.get(index);
      if (existing) return existing;
      let p: Vec3 = [
        surface.mesh.positions[index * 3],
        surface.mesh.positions[index * 3 + 1],
        surface.mesh.positions[index * 3 + 2],
      ];
      if (surface.skin) {
        const posed: Vec3 = [0, 0, 0];
        for (let influence = 0; influence < 4; influence++) {
          const at = index * 4 + influence,
            weight = surface.skin.weights[at];
          if (!weight) continue;
          const position = transform(surface.skin.matrices, p, surface.skin.jointIndices[at] * 16);
          for (let axis = 0; axis < 3; axis++) posed[axis] += position[axis] * weight;
        }
        p = posed;
      }
      const world = transform(surface.matrix, p);
      if (surface.wind) {
        const wind = normalizeWind(scene.environment.wind),
          speed = Math.hypot(wind[0], wind[2]),
          strength = Math.min(speed, 10) / 10;
        const instanceScale = Math.hypot(surface.matrix[0], surface.matrix[1], surface.matrix[2]);
        const amplitude =
          Math.min(Math.max(p[1], 0) ** 2 * 0.012, 0.6) *
          Math.max(0, Math.min(surface.wind, 2)) *
          strength *
          instanceScale;
        const phase =
          scene.time * 1.4 + world[0] * 0.17 + world[2] * 0.23 + (scene.environment.windPhase ?? 0);
        if (speed > 0) {
          world[0] += (wind[0] / speed) * amplitude * Math.sin(phase);
          world[2] += (wind[2] / speed) * amplitude * Math.sin(phase);
        }
      }
      if (surface.water)
        world[1] = queryWater(
          {
            ...surface.water,
            waves: surface.water.waves.map((wave) => ({
              ...wave,
              amplitude:
                wave.amplitude *
                waterGeometryAttenuation(wave.wavelength, surface.waterApproximation?.spacing ?? 0),
            })),
          },
          world[0],
          world[2],
          scene.time,
        ).height;
      cached.set(index, world);
      return world;
    };
    const hit = raycastMesh(
      surface.mesh,
      normalized,
      result?.distance ?? Infinity,
      vertex,
      surface.drawRange,
    );
    if (hit)
      result = { ...hit, surfaceId: surface.id, documentId: surface.source, instanceId: surface.instanceId };
  }
  return result;
}
