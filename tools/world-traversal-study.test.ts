import { expect, test } from "bun:test";
import { parseProject, worldPathLength } from "@wrela/model";

import { createWorldTraversalStudy, traversalSummary } from "./world-traversal-study";

test("traversal follows the authored approach through an activating streaming region", () => {
  const study = createWorldTraversalStudy(24);
  expect(() => parseProject(study.project)).not.toThrow();
  expect(study.cameras).toHaveLength(24);
  expect(study.cameras[0].position).toEqual([-5, 2.15, -22]);
  for (const [axis, value] of [-12, 2.65, 6.5].entries())
    expect(study.cameras.at(-1)?.position[axis]).toBeCloseTo(value);
  expect(study.duration).toBeCloseTo(worldPathLength(study.points) / 3);
  expect(() => createWorldTraversalStudy(1000)).toThrow("8–120");
});

test("capture summary exposes transient holes and blockage even when final images settle", () => {
  const frame = {
    groundReady: true,
    blockedBy: [] as string[],
    complete: true,
    initialComplete: true,
    streamingReady: true,
    initialStreamingReady: true,
    streamingZoneActive: false,
    settleMs: 0,
    measurements: { cpuMs: 8, gpuMs: 12, gpuBytes: 100 },
    resources: { installedBytes: 200 },
  };
  const summary = traversalSummary([
    frame,
    { ...frame, initialStreamingReady: false, streamingZoneActive: true, blockedBy: ["wall"], settleMs: 20 },
  ]);
  expect(summary.initialIncompleteFrames).toBe(1);
  expect(summary.blockedFrames).toBe(1);
  expect(summary.completeFrames).toBe(2);
  expect(summary.streamingActivationObserved).toBe(true);
  expect(summary.totalSettleMs).toBe(20);
});
