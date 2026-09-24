import { expect, test } from "bun:test";
import { compileMaterialLattice, materialLatticeHash, sharedMaterialLattice } from "./material-lattice";

test("compiled cells preserve each analytic corner including negative and periodic boundaries", () => {
  for (const origin of [
    [-3, -2, -1],
    [510, 510, 510],
  ] as [number, number, number][]) {
    const product = compileMaterialLattice(origin, 4);
    for (let z = 0; z < 4; z++)
      for (let y = 0; y < 4; y++)
        for (let x = 0; x < 4; x++)
          for (let dz = 0; dz < 2; dz++)
            for (let dy = 0; dy < 2; dy++)
              for (let dx = 0; dx < 2; dx++) {
                const actual = product.corners[(((z * 2 + dz) * 4 + y) * 4 + x) * 4 + dy * 2 + dx];
                expect(actual).toBe(
                  materialLatticeHash(origin[0] + x + dx, origin[1] + y + dy, origin[2] + z + dz),
                );
                expect(actual).toBe(
                  materialLatticeHash(
                    origin[0] + x + dx + 1024,
                    origin[1] + y + dy - 1024,
                    origin[2] + z + dz,
                  ),
                );
              }
  }
  expect(sharedMaterialLattice()).toBe(sharedMaterialLattice());
  expect(sharedMaterialLattice().corners.byteLength).toBe(8 * 1024 * 1024);
  expect(compileMaterialLattice([0, 0, 0], 4).key).not.toBe(compileMaterialLattice([1, 0, 0], 4).key);
  expect(() => compileMaterialLattice([0, 0, 0], 65)).toThrow();
});
