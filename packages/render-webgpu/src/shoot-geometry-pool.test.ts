import { expect, test } from "bun:test";
import { identityMatrix, type MeshData } from "@wrela/model";

import { ShootGeometryPool } from "./shoot-geometry-pool";

test("shared stream identity survives worker copies without sharing occurrence metadata", () => {
  const mesh: MeshData = {
    positions: new Float32Array([0, 0, 0]),
    normals: new Float32Array([0, 1, 0]),
    indices: new Uint32Array([0, 0, 0]),
    bounds: { min: [0, 0, 0], max: [0, 0, 0] },
    shoots: {
      transforms: identityMatrix(),
      anchors: new Float32Array(4),
      motion: new Float32Array(4),
      sourceIds: ["pine/shoot"],
      templateBounds: { min: [0, 0, 0], max: [0, 0, 0] },
    },
  };
  const pool = new ShootGeometryPool(),
    a = pool.canonical(mesh),
    copy = structuredClone(mesh);
  copy.shoots!.transforms[12] = 7;
  copy.shoots!.sourceIds = ["other/shoot"];
  expect(pool.canonical(copy)).toBe(a);
  expect(a.shoots).toBeUndefined();
  const changed = structuredClone(copy);
  changed.normals[0] = 0.1;
  expect(pool.canonical(changed)).not.toBe(a);
  const colors = structuredClone(copy);
  colors.colors = new Float32Array([1, 0, 0]);
  expect(pool.canonical(colors)).not.toBe(a);
  expect(pool.canonical(mesh)).toBe(a);
  pool.clear();
  expect(pool.canonical(copy)).not.toBe(a);
});
