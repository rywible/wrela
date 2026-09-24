import { queryWater, sampleWaterGrid, sampleWaterSpectrum } from "@wrela/compiler";
import {
  add,
  type Bounds,
  type Camera,
  cross,
  dot,
  type EvaluatedScene,
  intersectQuadric,
  inverseMatrix,
  type MeshData,
  multiplyMatrices,
  normalize,
  normalizeWind,
  quadricUnitToLocal,
  type RenderSurface,
  scale,
  sub,
  type Vec3,
  vegetationMotionOffset,
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
  accept?: (position: Vec3) => boolean,
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
    const position = add(ray.origin, scale(direction, distance));
    if (accept && !accept(position)) continue;
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
      position,
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
/** Expand only ray-relevant shared shoots on a click. Rendering retains compact
 * instances; picking returns the stable authored branch/needle identity. */
function* pickableSurfaces(scene: EvaluatedScene, ray: Ray): Generator<RenderSurface> {
  for (const surface of scene.surfaces) {
    const shoots = surface.mesh.shoots;
    if (!shoots) {
      yield surface;
      continue;
    }
    for (
      let selected = 0;
      selected < (surface.shootSelection?.indices.length ?? shoots.sourceIds.length);
      selected++
    ) {
      if (surface.shootSelection && !(surface.shootSelection.masks[selected] & 1)) continue;
      const index = surface.shootSelection?.indices[selected] ?? selected,
        at = index * 16;
      const matrix = shoots.transforms.subarray(at, at + 16);
      const bound = worldBounds({
        ...surface,
        matrix: multiplyMatrices(surface.matrix, matrix),
        mesh: { ...surface.mesh, bounds: shoots.templateBounds },
      });
      const envelope =
        ((0.885 *
          Math.min(2, Math.max(0, surface.wind ?? 0)) *
          Math.min(10, Math.hypot(...scene.environment.wind))) /
          10) *
        Math.hypot(surface.matrix[0], surface.matrix[1], surface.matrix[2]);
      for (let axis = 0; axis < 3; axis++) {
        bound.min[axis] -= envelope;
        bound.max[axis] += envelope;
      }
      if (!intersectsBounds(ray, bound, Infinity)) continue;
      const positions = new Float32Array(surface.mesh.positions.length),
        wind = new Float32Array((positions.length / 3) * 4);
      for (let vertex = 0; vertex < positions.length / 3; vertex++) {
        const p: Vec3 = [
          surface.mesh.positions[vertex * 3],
          surface.mesh.positions[vertex * 3 + 1],
          surface.mesh.positions[vertex * 3 + 2],
        ];
        const local = transform(matrix, p),
          anchor = Math.min(1, Math.max(0, local[1]) / 0.08);
        positions.set(local, vertex * 3);
        const distance = Math.hypot(...local.map((v, a) => v - shoots.anchors[index * 4 + a]));
        wind.set(
          [
            shoots.anchors[index * 4 + 3],
            Math.min(0.25, Math.max(0, distance - shoots.motion[index * 4 + 3]) ** 1.4 * 0.055) *
              shoots.motion[index * 4] *
              anchor,
            shoots.motion[index * 4 + 2],
            Math.min(0.035, Math.hypot(...p) * 0.12) * shoots.motion[index * 4 + 1] * anchor,
          ],
          vertex * 4,
        );
      }
      yield {
        ...surface,
        mesh: {
          ...surface.mesh,
          shoots: undefined,
          positions,
          wind,
          sourceIds: surface.mesh.sourceIds?.map((id) => `${shoots.sourceIds[index]}/${id}`),
        },
        shootSelection: undefined,
      };
    }
  }
}
/** Matches the renderer's skin, bounded wind and analytic wave positions.
 * Operates on a click, never as a per-frame scene traversal. */
export function pickScene(scene: EvaluatedScene, ray: Ray): SceneHit | null {
  let result: SceneHit | null = null;
  const normalized = { origin: ray.origin, direction: normalize(ray.direction) };
  for (const surface of pickableSurfaces(scene, normalized)) {
    const product = surface.selectedRenderProduct;
    if (
      product?.kind === "analytic-quadric" &&
      !surface.skin &&
      !surface.deformation &&
      !surface.wind &&
      !surface.water
    ) {
      const inverse = inverseMatrix(multiplyMatrices(surface.matrix, quadricUnitToLocal(product.primitive)));
      if (inverse) {
        const hit = intersectQuadric(
          inverse,
          normalized.origin,
          normalized.direction,
          0,
          result?.distance ?? Infinity,
        );
        if (hit)
          result = {
            distance: hit.distance,
            position: hit.position,
            triangle: -1,
            barycentric: [0, 0, 0],
            nodeId: product.primitive.nodeId,
            surfaceId: surface.id,
            documentId: surface.source,
            instanceId: surface.instanceId,
          };
        continue;
      }
    }
    if (
      !surface.skin &&
      !surface.deformation &&
      !surface.wind &&
      !surface.water &&
      !intersectsBounds(normalized, worldBounds(surface), result?.distance ?? Infinity)
    )
      continue;
    const offsets = surface.deformation?.vertexIndices
      ? new Map(Array.from(surface.deformation.vertexIndices, (vertex, offset) => [vertex, offset]))
      : undefined;
    const cached = new Map<number, Vec3>();
    const vertex = (index: number): Vec3 => {
      const existing = cached.get(index);
      if (existing) return existing;
      let p: Vec3 = [
        surface.mesh.positions[index * 3],
        surface.mesh.positions[index * 3 + 1],
        surface.mesh.positions[index * 3 + 2],
      ];
      if (surface.deformation) {
        const at = offsets ? offsets.get(index) : index;
        if (at !== undefined)
          for (let axis = 0; axis < 3; axis++) p[axis] += surface.deformation.positionDeltas[at * 3 + axis];
      }
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
        if (surface.mesh.wind) {
          const at = index * 4;
          const weights = surface.mesh.wind;
          const offset = vegetationMotionOffset(
            [weights[at], weights[at + 1], weights[at + 2], weights[at + 3]],
            world,
            scene.time,
            wind,
            surface.wind,
            scene.environment.windPhase ?? 0,
            instanceScale,
          );
          for (let axis = 0; axis < 3; axis++) world[axis] += offset[axis];
        }
      }
      if (surface.water && surface.waterState) {
        const state = surface.waterState,
          domain = state.domain,
          origin = scene.origin ?? [0, 0, 0];
        const x = world[0] + origin[0],
          z = world[2] + origin[2];
        const staticCell = domain ? sampleWaterGrid(domain, domain.cells, x, z) : undefined;
        const cell = domain && state.cells ? sampleWaterGrid(domain, state.cells, x, z) : undefined;
        const level = cell?.[0] ?? staticCell?.[1] ?? surface.water.level;
        const damp = domain ? Math.min(1, Math.max(0, level - (staticCell?.[0] ?? level)) / 0.5) : 1;
        const spacing = domain ? Math.max(...domain.spacing) : (surface.mesh.colors?.[index * 3] ?? 0);
        const wave = sampleWaterSpectrum(state.spectrum, x, z, scene.time, spacing);
        world[1] = level - origin[1] + wave.height * damp;
        if (!domain) {
          world[0] += wave.lateralX;
          world[2] += wave.lateralZ;
        }
      } else if (surface.water)
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
      surface.water && surface.waterState?.domain
        ? (position) => {
            const origin = scene.origin ?? [0, 0, 0];
            return queryWater(
              surface.water!,
              position[0] + origin[0],
              position[2] + origin[2],
              scene.time,
              surface.waterState,
            ).wet;
          }
        : undefined,
    );
    if (hit)
      result = { ...hit, surfaceId: surface.id, documentId: surface.source, instanceId: surface.instanceId };
  }
  return result;
}
