import { contentKey, type MaterialLattice, type Vec3 } from "@wrela/model";

/** Matches scene.wgsl's integer lattice hash, including negative coordinates. */
export function materialLatticeHash(x: number, y: number, z: number): number {
  const wrap = (n: number) => ((n % 1024) + 1024) % 1024;
  let h = Math.imul(wrap(x), 1597334677) ^ Math.imul(wrap(y), 3812015801) ^ Math.imul(wrap(z), 2798796415);
  h = Math.imul(h ^ (h >>> 16), 2246822519);
  h = Math.imul(h ^ (h >>> 13), 3266489917);
  return Math.fround(((h ^ (h >>> 16)) & 16777215) / 16777215);
}
/** Partial evaluation preserves the field, filtering and dynamic lighting exactly.
 * The product has a bounded memory cost; it is not a lower-frequency substitute. */
export function compileMaterialLattice(origin: Vec3 = [-32, -32, -32], edge = 64): MaterialLattice {
  if (
    !Number.isInteger(edge) ||
    edge < 1 ||
    edge > 64 ||
    origin.some((n) => !Number.isInteger(n) || Math.abs(n) > 512)
  )
    throw new RangeError("Material lattice requires integer bounds and an edge in [1,64]");
  const corners = new Float32Array(edge ** 3 * 8);
  for (let z = 0; z < edge; z++)
    for (let y = 0; y < edge; y++)
      for (let x = 0; x < edge; x++) {
        for (let dz = 0; dz < 2; dz++)
          for (let dy = 0; dy < 2; dy++)
            for (let dx = 0; dx < 2; dx++) {
              const texel = ((z * 2 + dz) * edge * edge + y * edge + x) * 4;
              corners[texel + dy * 2 + dx] = materialLatticeHash(
                origin[0] + x + dx,
                origin[1] + y + dy,
                origin[2] + z + dz,
              );
            }
      }
  return {
    version: "material-lattice-1",
    key: contentKey({ materialLattice: 1, origin, edge }),
    origin: [...origin],
    edge,
    corners,
  };
}
let shared: MaterialLattice | undefined;
/** Reuse one product across hosts and frames; edits to colors, lighting and poses need no rebuild. */
export function sharedMaterialLattice(): MaterialLattice {
  shared ??= compileMaterialLattice();
  return shared;
}
