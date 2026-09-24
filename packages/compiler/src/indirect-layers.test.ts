import { expect, test } from "bun:test";
import type { Vec3 } from "@wrela/model";
import { box } from "../../../tools/fixtures/indirect-scenes";
import { indirectGeometryLayers } from "./indirect-layers";
import { compileIndirectGeometry, traceIndirectRay, traceSkyDistance } from "./indirect-query";

test("moving query layers match rebuilt geometry, preserve hit IDs, and share the static BVH", () => {
  const wall = box("wall", [-3, -3, 4], [3, 3, 4.2], [0.4, 0.5, 0.6]);
  const fixed = compileIndirectGeometry([wall]);
  for (const x of [-2, 0, 2]) {
    const door = box("door", [x - 0.5, -1, 1], [x + 0.5, 2, 1.1], [0.1, 0.2, 0.3]);
    const moving = compileIndirectGeometry([door]);
    const layered = indirectGeometryLayers(fixed, [moving]);
    const rebuilt = compileIndirectGeometry([wall, door]);
    expect(layered.nodes).toBe(fixed.nodes);
    expect(layered.order).toBe(fixed.order);
    for (let i = 0; i < 300; i++) {
      const origin: Vec3 = [Math.sin(i * 1.71) * 4, Math.sin(i * 0.61) * 3, -1];
      const direction: Vec3 = [0, 0, 1];
      const hit = traceIndirectRay(layered, origin, direction, 10);
      // A ray on a shared edge may return either coplanar owner, depending
      // on BVH traversal. Its physical hit and global triangle table agree.
      const physical = (value: typeof hit) => (value ? { ...value, triangle: undefined } : undefined);
      expect(physical(hit)).toEqual(physical(traceIndirectRay(rebuilt, origin, direction, 10)));
      if (hit) expect(layered.triangles[hit.triangle]).toEqual(rebuilt.triangles[hit.triangle]);
      expect(traceSkyDistance(layered, origin, direction, 10)).toBe(
        traceSkyDistance(rebuilt, origin, direction, 10),
      );
      if (hit)
        expect(physical(traceIndirectRay(layered, origin, direction, 10, hit.triangle))).toEqual(
          physical(traceIndirectRay(rebuilt, origin, direction, 10, hit.triangle)),
        );
    }
  }
});

test("an empty base, nested layers, misses and finite segments remain valid", () => {
  const empty = compileIndirectGeometry([]);
  const a = compileIndirectGeometry([box("a", [-1, -1, 2], [1, 1, 3], [1, 0, 0])]);
  const b = compileIndirectGeometry([box("b", [-1, -1, 4], [1, 1, 5], [0, 1, 0])]);
  const layered = indirectGeometryLayers(indirectGeometryLayers(empty, [a]), [b]);
  expect(traceIndirectRay(layered, [0, 0, 0], [0, 0, 1])?.distance).toBe(2);
  expect(traceIndirectRay(layered, [0, 0, 0], [0, 0, 1], 1)).toBeUndefined();
  expect(traceSkyDistance(layered, [0, 0, 0], [0, 0, 1], 1)).toBe(1);
  expect(traceIndirectRay(layered, [3, 0, 0], [0, 0, 1])).toBeUndefined();
  expect(indirectGeometryLayers(empty, [])).toBe(empty);
});
