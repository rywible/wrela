import type { Bounds, MeshData, Vec3 } from "@wrela/model";

export const foliageOrgan = (id: string) => id.replace(/\/(?:c\d+\/l\d+|coverage|n\d+).*$/, "");
type Group = { id: string; indices: number[]; center: Vec3; bounds: Bounds };
const empty = (): Bounds => ({ min: [Infinity, Infinity, Infinity], max: [-Infinity, -Infinity, -Infinity] });
function include(bounds: Bounds, point: Vec3) {
  for (let axis = 0; axis < 3; axis++) {
    bounds.min[axis] = Math.min(bounds.min[axis], point[axis]);
    bounds.max[axis] = Math.max(bounds.max[axis], point[axis]);
  }
}
/** Keep every source organ whole, then partition spatially. Each source triangle
 * and organ belongs to exactly one cluster; input geometry is never modified. */
export function partitionFoliage(mesh: MeshData, maximum = 4): { mesh: MeshData; sourceOrgans: string[] }[] {
  if (!Number.isInteger(maximum) || maximum < 1 || maximum > 4) throw Error("Invalid foliage cluster budget");
  const organs = new Map<string, Group>();
  for (let triangle = 0; triangle < mesh.indices.length; triangle += 3) {
    const indices = [...mesh.indices.subarray(triangle, triangle + 3)];
    const id = foliageOrgan(mesh.sourceIds?.[indices[0]] ?? `triangle-${triangle / 3}`);
    let group = organs.get(id);
    if (!group) {
      group = { id, indices: [], center: [0, 0, 0], bounds: empty() };
      organs.set(id, group);
    }
    group.indices.push(...indices);
    for (const index of indices)
      include(group.bounds, [
        mesh.positions[index * 3],
        mesh.positions[index * 3 + 1],
        mesh.positions[index * 3 + 2],
      ]);
  }
  for (const group of organs.values())
    group.center = group.bounds.min.map((v, axis) => (v + group.bounds.max[axis]) * 0.5) as Vec3;
  const partitions = organs.size ? [[...organs.values()]] : [];
  while (partitions.length < maximum) {
    let largest = -1;
    for (let i = 0; i < partitions.length; i++)
      if (partitions[i].length > 1 && (largest < 0 || partitions[i].length > partitions[largest].length))
        largest = i;
    if (largest < 0) break;
    const groups = partitions[largest],
      bounds = empty();
    for (const group of groups) include(bounds, group.center);
    let axis = 0;
    for (let i = 1; i < 3; i++)
      if (bounds.max[i] - bounds.min[i] > bounds.max[axis] - bounds.min[axis]) axis = i;
    groups.sort((a, b) => a.center[axis] - b.center[axis] || a.id.localeCompare(b.id));
    const middle = Math.ceil(groups.length / 2);
    partitions.splice(largest, 1, groups.slice(0, middle), groups.slice(middle));
  }
  return partitions.map((groups) => {
    const bounds = empty();
    for (const group of groups) {
      include(bounds, group.bounds.min);
      include(bounds, group.bounds.max);
    }
    return {
      mesh: { ...mesh, indices: Uint32Array.from(groups.flatMap((group) => group.indices)), bounds },
      sourceOrgans: groups.map((group) => group.id).sort(),
    };
  });
}
