import { expect, test } from "bun:test";
import type { BranchPlate } from "@wrela/model";

import { branchVisibilityApplies, compileBranchVisibility } from "./branch-visibility";

const plate = (x: number, y: number): BranchPlate => ({
  center: [x, y, 0],
  tangent: [1, 0, 0],
  bitangent: [0, 0, 1],
  halfLength: 0.1,
  halfWidth: 0.1,
});
test("whole receiver support retains possible shadows and rejects unreachable plates", () => {
  const p = compileBranchVisibility([plate(0, 0), plate(0, 1), plate(10, 1), plate(0, -1)]);
  expect(Array.from(p.candidates.slice(p.offsets[0], p.offsets[1]))).toEqual([1]);
  expect(branchVisibilityApplies(p, p.sourceKey, [1, 0.55, 0])).toBe(true);
  expect(branchVisibilityApplies(p, p.sourceKey, [1, 0.2, 0])).toBe(false);
  expect(branchVisibilityApplies(p, "edited", [1, 0.55, 0])).toBe(false);
  expect(branchVisibilityApplies(p, p.sourceKey, [0, 0, 0])).toBe(false);
});
test("conservative support includes plate extents rather than just centroids", () => {
  const a = plate(0, 0),
    b = plate(3, 1);
  b.halfLength = 2.99;
  const p = compileBranchVisibility([a, b]);
  expect(Array.from(p.candidates.slice(p.offsets[0], p.offsets[1]))).toEqual([1]);
  expect(() => compileBranchVisibility([a], 0)).toThrow();
});
