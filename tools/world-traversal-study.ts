import { createAlpineLookdevProject } from "@wrela/examples";
import {
  type Camera,
  type Project,
  sampleWorldPolyline,
  type Vec3,
  worldPathLength,
  worldPathPolyline,
} from "@wrela/model";

export type WorldTraversalStudy = {
  project: Project;
  subject: string;
  points: Vec3[];
  cameras: Camera[];
  duration: number;
  actor: { radius: number; height: number; maxStepHeight: number };
  streamingRegion: string;
};

/** A source-authored route, sampled by distance rather than array index. Camera
 * motion follows the rounded corridor used by grading and collision review. */
export function createWorldTraversalStudy(frameCount = 48): WorldTraversalStudy {
  if (!Number.isInteger(frameCount) || frameCount < 8 || frameCount > 120)
    throw new RangeError("World traversal requires 8–120 frames");
  const project = createAlpineLookdevProject();
  const world = project.documents.find((document) => document.id === project.entry);
  if (world?.kind !== "world" || !world.composition) throw new Error("Missing alpine composition");
  const ids = ["approach", "gate-branch"];
  const points = ids.flatMap((id, index) => {
    const path = world.composition?.paths.find((path) => path.id === id);
    if (!path) throw new Error(`Missing traversal path ${id}`);
    const points = worldPathPolyline(path);
    return index ? points.slice(1) : points;
  });
  // The larger grove remains the production streaming interest. This authored
  // local region tests a nearby-zone activation during the same camera journey.
  const streamingRegion = "traversal-gate-region";
  world.composition.streaming.push({
    id: streamingRegion,
    center: [-12, 1, 8],
    radius: 8,
    preloadDistance: 4,
    priority: 2,
    collision: true,
  });
  const sampled = sampleWorldPolyline(points, frameCount);
  const cameras = sampled.map(({ position }, index): Camera => {
    const ahead = sampled[Math.min(frameCount - 1, index + 4)].position;
    const target: Vec3 = index >= frameCount - 4 ? [-12, 2.6, 8] : [ahead[0], ahead[1] + 1.6, ahead[2]];
    return { position: [position[0], position[1] + 1.65, position[2]], target, fov: 62 };
  });
  return {
    project,
    subject: project.entry,
    points,
    cameras,
    duration: worldPathLength(points) / 3,
    actor: { radius: 0.35, height: 1.8, maxStepHeight: 0.3 },
    streamingRegion,
  };
}

export function traversalSummary(
  frames: readonly {
    groundReady: boolean;
    blockedBy: string[];
    complete: boolean;
    initialComplete: boolean;
    streamingReady: boolean;
    initialStreamingReady: boolean;
    streamingZoneActive: boolean;
    settleMs: number;
    measurements: { cpuMs: number; gpuMs: number | null; gpuBytes: number };
    resources: { installedBytes: number };
  }[],
) {
  const quantile = (values: (number | null)[], fraction: number) => {
    const sorted = values
      .filter((value): value is number => value !== null && Number.isFinite(value))
      .sort((a, b) => a - b);
    return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))] ?? null;
  };
  return {
    frames: frames.length,
    collisionReady: frames.every((frame) => frame.groundReady),
    blockedFrames: frames.filter((frame) => frame.blockedBy.length).length,
    completeFrames: frames.filter((frame) => frame.complete && frame.streamingReady).length,
    initialIncompleteFrames: frames.filter((frame) => !frame.initialComplete || !frame.initialStreamingReady)
      .length,
    streamingActivationObserved:
      frames.some((frame) => frame.streamingZoneActive) && frames.some((frame) => !frame.streamingZoneActive),
    totalSettleMs: frames.reduce((sum, frame) => sum + frame.settleMs, 0),
    peakGpuBytes: Math.max(0, ...frames.map((frame) => frame.measurements.gpuBytes)),
    peakInstalledBytes: Math.max(0, ...frames.map((frame) => frame.resources.installedBytes)),
    gpuTimingSamples: frames.filter((frame) => frame.measurements.gpuMs !== null).length,
    gpuMedianMs: quantile(
      frames.map((frame) => frame.measurements.gpuMs),
      0.5,
    ),
    gpuP95Ms: quantile(
      frames.map((frame) => frame.measurements.gpuMs),
      0.95,
    ),
    cpuMedianMs: quantile(
      frames.map((frame) => frame.measurements.cpuMs),
      0.5,
    ),
    cpuP95Ms: quantile(
      frames.map((frame) => frame.measurements.cpuMs),
      0.95,
    ),
  };
}
