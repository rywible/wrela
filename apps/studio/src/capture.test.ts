import { expect, test } from "bun:test";
import type { Camera } from "@wrela/model";
import { projectSegment } from "./capture";

const camera: Camera = { position: [0, 0, 5], target: [0, 0, 0], fov: 90 };
test("diagnostic projection matches centered camera, clips near plane, rejects behind camera", () => {
  const line = projectSegment({ kind: "rig", a: [0, 0, 0], b: [1, 0, 0] }, camera, 200, 100);
  expect(line?.[0]).toEqual([100, 50]);
  expect(line?.[1][0]).toBeCloseTo(110);
  expect(projectSegment({ kind: "rig", a: [0, 0, 6], b: [1, 0, 7] }, camera, 200, 100)).toBeNull();
  const clipped = projectSegment({ kind: "collider", a: [0, 0, 6], b: [1, 0, 0] }, camera, 200, 100);
  expect(clipped?.flat().every(Number.isFinite)).toBe(true);
});
