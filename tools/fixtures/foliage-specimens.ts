import { coniferMeshes } from "@wrela/compiler/conifer-mesh";
import { alpinePineLookdevDefinition } from "@wrela/examples";
import { cross, type MeshData, normalize, sub, type Vec3 } from "@wrela/model";

/** A single tessellated, folded leaf lamina. Back faces use the renderer's real two-sided path. */
export function thinLeafMesh(flat = false): MeshData {
  const positions: number[] = [],
    normals: number[] = [],
    colors: number[] = [],
    indices: number[] = [];
  const rows = 24,
    columns = 12;
  const point = (t: number, u: number): Vec3 => [
    u * 0.105 * Math.sin(Math.PI * t) ** 0.85,
    t * 0.36,
    flat ? 0 : 0.018 * (1 - u * u) * Math.sin(Math.PI * t) + 0.012 * Math.sin(t * Math.PI * 2),
  ];
  for (let row = 0; row <= rows; row++)
    for (let column = 0; column <= columns; column++) {
      const t = 0.001 + (row / rows) * 0.998,
        u = (column / columns) * 2 - 1;
      const p = point(t, u),
        right = point(t, u + 0.001),
        upper = point(Math.min(0.9999, t + 0.0001), u);
      const normal = normalize(cross(sub(right, p), sub(upper, p)));
      positions.push(...p);
      normals.push(...normal);
      const midrib = Math.exp(-u * u * 180);
      const veins = Math.exp(-(Math.sin(t * 45 + Math.abs(u) * 3) ** 2) * 45) * (1 - midrib);
      const shade = 0.76 + t * 0.16 + midrib * 0.1 + veins * 0.06;
      colors.push(shade, shade, shade);
      if (row < rows && column < columns) {
        const a = row * (columns + 1) + column,
          b = a + 1,
          c = a + columns + 1;
        indices.push(a, b, c, b, c + 1, c);
      }
    }
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    colors: new Float32Array(colors),
    indices: new Uint32Array(indices),
    bounds: { min: [-0.11, 0, -0.02], max: [0.11, 0.36, 0.035] },
  };
}

/** Extract the explicit triangle reference from the production conifer compiler.
 * This fixture isolates the leaf material closure; coverage is reviewed separately. */
export function needleShootMesh(): MeshData {
  const source = alpinePineLookdevDefinition();
  source.height = 1;
  source.radius = 0.35;
  source.branches = 8;
  if (!source.botanical?.conifer) throw Error("Missing conifer fixture");
  source.botanical.conifer.shootsPerLimb = 3;
  source.botanical.conifer.needlesPerShoot = 48;
  const cooked = coniferMeshes(source, "review", false, "triangles");
  const branch = cooked.structure.branches.find(
    (branch) =>
      branch.level === 2 &&
      !branch.bare &&
      !branch.broken &&
      cooked.foliage.sourceIds?.some((id) => id.startsWith(`${source.id}/${branch.id}/n`)),
  );
  if (!branch) throw Error("Production conifer did not produce a reviewable needle shoot");
  const prefix = `${source.id}/${branch.id}/n`,
    mesh = cooked.foliage;
  const selected: number[] = [];
  for (let i = 0; i < mesh.indices.length; i += 3)
    if (mesh.sourceIds?.[mesh.indices[i]].startsWith(prefix))
      selected.push(mesh.indices[i], mesh.indices[i + 1], mesh.indices[i + 2]);
  const map = new Map<number, number>(),
    positions: number[] = [],
    normals: number[] = [],
    colors: number[] = [];
  const indices = selected.map((old) => {
    const known = map.get(old);
    if (known !== undefined) return known;
    const index = positions.length / 3;
    map.set(old, index);
    positions.push(...mesh.positions.slice(old * 3, old * 3 + 3));
    normals.push(...mesh.normals.slice(old * 3, old * 3 + 3));
    colors.push(...(mesh.colors?.slice(old * 3, old * 3 + 3) ?? [1, 1, 1]));
    return index;
  });
  const min: Vec3 = [Infinity, Infinity, Infinity],
    max: Vec3 = [-Infinity, -Infinity, -Infinity];
  positions.forEach((value, index) => {
    min[index % 3] = Math.min(min[index % 3], value);
    max[index % 3] = Math.max(max[index % 3], value);
  });
  const center = min.map((value, axis) => (value + max[axis]) / 2) as Vec3;
  positions.forEach((value, index) => {
    positions[index] = value - center[index % 3];
  });
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    colors: new Float32Array(colors),
    indices: new Uint32Array(indices),
    bounds: {
      min: min.map((value, axis) => value - center[axis]) as Vec3,
      max: max.map((value, axis) => value - center[axis]) as Vec3,
    },
  };
}
