import { add, type Bounds, cross, type MeshData, normalize, scale, sub, type Vec3 } from "@wrela/model";

/** Stay below the cooked mesh vertex limit, including two-sided leaf duplication. */
export const MAX_BOTANICAL_VERTICES = 240000;
export type Builder = {
  positions: number[];
  normals: number[];
  colors: number[];
  indices: number[];
  sourceIds: string[];
  materialCoordinates?: number[];
};
export const builder = (): Builder => ({
  positions: [],
  normals: [],
  colors: [],
  indices: [],
  sourceIds: [],
});
const white: Vec3 = [1, 1, 1];
export function vertex(mesh: Builder, position: Vec3, normal: Vec3, color: Vec3, source: string): number {
  const index = mesh.positions.length / 3;
  mesh.positions.push(...position);
  mesh.normals.push(...normal);
  mesh.colors.push(...color);
  mesh.sourceIds.push(source);
  return index;
}
export function stem(
  mesh: Builder,
  a: Vec3,
  b: Vec3,
  radius: number,
  tipRadius: number,
  sides: number,
  source: string,
): void {
  const direction = normalize(sub(b, a));
  const u = normalize(cross(direction, Math.abs(direction[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
  const v = cross(direction, u);
  const offset = mesh.positions.length / 3;
  for (let end = 0; end < 2; end++)
    for (let i = 0; i < sides; i++) {
      const angle = (i / sides) * Math.PI * 2;
      const normal = add(scale(u, Math.cos(angle)), scale(v, Math.sin(angle)));
      vertex(mesh, add(end ? b : a, scale(normal, end ? tipRadius : radius)), normal, white, source);
    }
  for (let i = 0; i < sides; i++) {
    const j = (i + 1) % sides;
    mesh.indices.push(
      offset + i,
      offset + j,
      offset + sides + i,
      offset + j,
      offset + sides + j,
      offset + sides + i,
    );
  }
  // Close both ends, including damage and pruning cuts.
  for (let end = 0; end < 2; end++) {
    const center = vertex(mesh, end ? b : a, scale(direction, end ? 1 : -1), white, source);
    for (let i = 0; i < sides; i++) {
      const j = (i + 1) % sides;
      const first = vertex(
        mesh,
        mesh.positions.slice((offset + end * sides + i) * 3, (offset + end * sides + i) * 3 + 3) as Vec3,
        scale(direction, end ? 1 : -1),
        white,
        source,
      );
      const second = vertex(
        mesh,
        mesh.positions.slice((offset + end * sides + j) * 3, (offset + end * sides + j) * 3 + 3) as Vec3,
        scale(direction, end ? 1 : -1),
        white,
        source,
      );
      mesh.indices.push(center, end ? first : second, end ? second : first);
    }
  }
}
/** Folded, two-sided leaf/needle geometry creates actual canopy gaps without alpha sorting. */
export function blade(
  mesh: Builder,
  base: Vec3,
  direction: Vec3,
  length: number,
  width: number,
  roll: number,
  droop: number,
  color: Vec3,
  source: string,
): void {
  const axis = normalize(direction);
  const u = normalize(cross(axis, Math.abs(axis[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0]));
  const v = cross(axis, u);
  const lateral = add(scale(u, Math.cos(roll)), scale(v, Math.sin(roll)));
  const center = add(base, scale(axis, length * 0.45));
  const tip = add(add(base, scale(axis, length)), [0, -length * droop * 0.5, 0]);
  if (width / length < 0.13) {
    const points = [add(base, scale(lateral, width * -0.5)), add(base, scale(lateral, width * 0.5)), tip];
    const normal = normalize(cross(sub(points[1], points[0]), sub(points[2], points[0])));
    for (let back = 0; back < 2; back++) {
      const ids = points.map((point) => vertex(mesh, point, scale(normal, back ? -1 : 1), color, source));
      mesh.indices.push(ids[0], ids[back ? 2 : 1], ids[back ? 1 : 2]);
    }
    return;
  }
  const points = [
    base,
    add(center, scale(lateral, width * 0.5)),
    tip,
    add(center, scale(lateral, -width * 0.5)),
    add(center, scale(v, width * 0.12)),
  ];
  for (let face = 0; face < 4; face++) {
    const a = points[face],
      b = points[(face + 1) % 4],
      c = points[4];
    const n = normalize(cross(sub(b, a), sub(c, a)));
    for (let back = 0; back < 2; back++) {
      const normal = scale(n, back ? -1 : 1);
      const indices = [
        vertex(mesh, a, normal, color, source),
        vertex(mesh, b, normal, color, source),
        vertex(mesh, c, normal, color, source),
      ];
      mesh.indices.push(indices[0], indices[back ? 2 : 1], indices[back ? 1 : 2]);
    }
  }
}
export function finish(mesh: Builder): MeshData {
  const bounds: Bounds = { min: [0, 0, 0], max: [0, 0, 0] };
  for (let i = 0; i < mesh.positions.length; i++) {
    bounds.min[i % 3] = Math.min(bounds.min[i % 3], mesh.positions[i]);
    bounds.max[i % 3] = Math.max(bounds.max[i % 3], mesh.positions[i]);
  }
  return {
    positions: new Float32Array(mesh.positions),
    normals: new Float32Array(mesh.normals),
    colors: new Float32Array(mesh.colors),
    indices: new Uint32Array(mesh.indices),
    sourceIds: mesh.sourceIds,
    ...(mesh.materialCoordinates ? { materialCoordinates: new Float32Array(mesh.materialCoordinates) } : {}),
    bounds,
  };
}
