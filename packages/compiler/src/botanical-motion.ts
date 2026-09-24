import type { MeshData, Vec3, VegetationDefinition } from "@wrela/model";

import { type BotanicalBranch, botanicalRandom } from "./botanical-branch";

/** Bind branch sway to the first woody limb, shared by its entire subtree.
 * Leaf flutter fades to zero at each leaf's attachment. Geometry-local bindings
 * survive instance reuse, source-ID remapping and every independently cooked LOD. */
export function bindBotanicalMotion(
  mesh: MeshData,
  document: VegetationDefinition,
  branches: BotanicalBranch[],
  foliage: boolean,
): MeshData {
  const motion = document.botanical?.motion;
  if (!motion) return mesh;
  const wind = new Float32Array((mesh.positions.length / 3) * 4);
  const byId = new Map(branches.map((branch) => [branch.id, branch]));
  const primary = new Map<string, BotanicalBranch>();
  for (const branch of branches) {
    let root = branch;
    while (root.parent && byId.get(root.parent)?.level !== 0) {
      const parent = byId.get(root.parent);
      if (!parent) break;
      root = parent;
    }
    if (root.level > 0) primary.set(branch.id, root);
  }
  const leafBases = new Map<string, Vec3>();
  for (let vertex = 0; vertex < mesh.positions.length / 3; vertex++) {
    const source = mesh.sourceIds?.[vertex]?.slice(document.id.length + 1) ?? "trunk";
    let organ = source;
    while (!byId.has(organ) && organ.includes("/")) organ = organ.slice(0, organ.lastIndexOf("/"));
    const branch = primary.get(organ);
    const position: Vec3 = [
      mesh.positions[vertex * 3],
      mesh.positions[vertex * 3 + 1],
      mesh.positions[vertex * 3 + 2],
    ];
    const slot = vertex * 4;
    if (branch) {
      const distance = Math.hypot(
        position[0] - branch.start[0],
        position[1] - branch.start[1],
        position[2] - branch.start[2],
      );
      // Roots remain anchored even for herbaceous species with very broad blades.
      const anchor = Math.min(1, Math.max(0, position[1]) / 0.08);
      wind[slot] = botanicalRandom(document.seed, `${branch.id}/motion`) * Math.PI * 2;
      wind[slot + 1] =
        Math.min(0.25, Math.max(0, distance - branch.radius) ** 1.4 * 0.055) * motion.branchSway * anchor;
    }
    if (foliage) {
      const base = leafBases.get(source) ?? position;
      leafBases.set(source, base);
      const distance = Math.hypot(position[0] - base[0], position[1] - base[1], position[2] - base[2]);
      wind[slot + 2] = botanicalRandom(document.seed, `${source}/flutter`) * Math.PI * 2;
      wind[slot + 3] =
        Math.min(0.035, distance * 0.12) * motion.leafFlutter * Math.min(1, Math.max(0, position[1]) / 0.08);
    }
  }
  return { ...mesh, wind };
}
